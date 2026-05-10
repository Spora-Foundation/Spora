//!
//! Transaction generator module used for creating multi-stage transactions
//! optimized for parallelized DAG processing.
//!
//! The [`Generator`] intakes a set of cell entries and accumulates them as
//! inputs into a single transaction. If transaction hits mass boundaries
//! before 1) desired amount is reached or 2) all cells are consumed, the
//! transaction is yielded and a "relay" transaction is created.
//!
//! If "relay" transactions are created, the [`Generator`] will aggregate
//! such transactions into a single transaction and repeat the process
//! until 1) desired amount is reached or 2) all cells are consumed.
//!
//! This processing results in a creation of a transaction tree where
//! each level (stage) of this tree is submitted to the network in parallel.
//!
//!```text
//!
//! Tx1 Tx2 Tx3 Tx4 Tx5 Tx6     | stage 0 (relays to stage 1)
//!  |   |   |   |   |   |      |
//!  +---+   +---+   +---+      |
//!    |       |       |        |
//!   Tx7     Tx8     Tx9       | stage 1 (relays to stage 2)
//!    |       |       |        |
//!    +-------+-------+        |
//!            |                |
//!           Tx10              | stage 2 (final outbound transaction)
//!
//!```
//!
//! The generator will produce transactions in the following order:
//! Tx1, Tx2, Tx3, Tx4, Tx5, Tx6, Tx7, Tx8, Tx9, Tx10
//!
//! Transactions within a single stage are independent of one another
//! and as such can be processed in parallel.
//!
//! The [`Generator`] acts as a transaction iterator, yielding transactions
//! for each iteration. These transactions can be obtained via an iterator
//! interface or via an async Stream interface.
//!
//! Q: Why is this not implemented as a single loop?
//!
//! A: There are a number of requirements that need to be handled:
//!
//! 1. Cell entry consumption while creating inputs may result in
//!    additional fees, requiring additional cell entries to cover
//!    the fees. Goto 1. (this is a classic issue, can be solved using padding)
//!
//! 2. The overall design strategy for this processor is to allow
//!    concurrent processing of a large number of transactions and cells.
//!    This implementation avoids in-memory aggregation of all
//!    transactions that may result in OOM conditions.
//!
//! 3. If used with a large tracked-cell set, the transaction generation process
//!    needs to be asynchronous to avoid blocking the main thread. In the
//!    context of WASM32 SDK, not doing that while working with large
//!    large cell sets will result in a browser UI freezing.
//!

use crate::cell::{CellContext, CellEntryReference, NetworkParams};
use crate::imports::*;
use crate::result::Result;
use crate::tx::{
    mass::*, CellScriptTypedCellOutput, CellScriptTypedCellResolvedCell, CellScriptTypedCellSchedulerAccessPlan,
    CellScriptTypedCellSchedulerPlan, Fees, GeneratorSettings, GeneratorSummary, PaymentDestination, PaymentOutput,
    PendingTransaction, PendingTransactionIterator, PendingTransactionStream,
};
use spora_consensus_client::{pay_to_address_lock_script, CellEntry, TransactionInput};
use spora_consensus_core::block::CellScriptSchedulerAccessList;
use spora_consensus_core::constants::UNACCEPTED_DAA_SCORE;
use spora_consensus_core::tx::{TransactionId, TransactionOutpoint};
use spora_exec::{
    celltx::{
        compute_conflict_hash, compute_typed_data_hash, encode_cellscript_scheduler_witness_molecule,
        encode_conflict_key_value_composite, CellScriptSchedulerAccessWitness, CellScriptSchedulerWitness,
        CELLSCRIPT_SCHEDULER_EFFECT_CREATING, CELLSCRIPT_SCHEDULER_EFFECT_DESTROYING, CELLSCRIPT_SCHEDULER_EFFECT_MUTATING,
        CELLSCRIPT_SCHEDULER_EFFECT_PURE, CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY, CELLSCRIPT_SCHEDULER_OP_CONSUME,
        CELLSCRIPT_SCHEDULER_OP_CREATE, CELLSCRIPT_SCHEDULER_OP_DESTROY, CELLSCRIPT_SCHEDULER_OP_READ_REF,
        CELLSCRIPT_SCHEDULER_OP_TRANSFER, CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP, CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
        CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT, CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
    },
    ckb_apply_type_id_script_to_output_molecule, CellDep, CellInput, CellTx, Script,
};
use spora_rpc_core::RpcTransactionOutput;
use std::collections::VecDeque;

use super::SignerT;

/// Options for Child-Pays-for-Parent (CPFP) fee bumping.
///
/// CPFP allows a child transaction to pay a higher fee to incentivize miners
/// to confirm both the parent (stuck/low-fee) transaction and the child
/// transaction together.
///
/// # Fee Calculation
///
/// The child fee is calculated so that the *combined* fee rate of the
/// parent + child package meets `target_fee_rate`:
///
/// ```text
/// child_fee = target_fee_rate * (parent_mass + child_mass) - parent_fee
/// ```
///
/// If the parent already meets the target rate, no additional fee is added.
#[derive(Debug, Clone)]
pub struct CpfpOptions {
    /// Target fee rate (fees per unit of mass) for the combined parent+child package.
    pub target_fee_rate: f64,
    /// Transaction ID of the unconfirmed parent transaction to accelerate.
    pub parent_transaction_id: TransactionId,
    /// Total mass of the parent transaction.
    pub parent_transaction_mass: u64,
    /// Fee already paid by the parent transaction (in SAU).
    pub parent_transaction_fee: u64,
}

// fee reduction - when a transactions has some storage mass
// and the total mass is below this threshold (as well as
// other conditions), we attempt to accumulate additional
// inputs to reduce storage mass/fees
const TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION: u64 = MAXIMUM_STANDARD_TRANSACTION_MASS / 5 * 4;
// optimization boundary - when aggregating inputs,
// we don't perform any checks until we reach this mass
// or the aggregate input amount reaches the requested
// output amount
const TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION: u64 = MAXIMUM_STANDARD_TRANSACTION_MASS / 5 * 4;

/// Mutable [`Generator`] state used to track the current transaction generation process.
struct Context {
    /// iterator containing cell entries available for transaction generation
    cell_source_iterator: Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>,
    /// List of priority cell entries, that are consumed before polling the iterator
    priority_cell_entries: Option<VecDeque<CellEntryReference>>,
    /// HashSet containing priority cell entries, used for filtering
    /// for potential duplicates from the iterator
    priority_cell_entry_filter: Option<HashSet<CellEntryReference>>,
    /// total number of cells consumed by the single generator instance
    aggregated_cells: usize,
    /// total fees of all transactions issued by
    /// the single generator instance
    aggregate_fees: u64,
    /// total mass of all transactions issued by the single generator instance
    aggregate_mass: u64,
    /// number of generated transactions
    number_of_transactions: usize,
    /// Number of generated stages. Stage represents multiple transactions
    /// executed in parallel. Each stage is a tree level in the transaction
    /// tree. When calculating time for submission of transactions, the estimated
    /// time per transaction (either as DAA score or a fee-rate based estimate)
    /// should be multiplied by the number of stages.
    number_of_stages: usize,
    /// current tree stage
    stage: Option<Box<Stage>>,
    /// Rejected or "stashed" cell entries that are consumed before polling
    /// the iterator. This store is used in edge cases when a cell entry from the
    /// iterator has been consumed but was rejected due to mass constraints or
    /// other conditions.
    cell_stash: VecDeque<CellEntryReference>,
    /// final transaction id
    final_transaction_id: Option<TransactionId>,
    /// signifies that the generator is finished
    /// no more items will be produced in the
    /// iterator or a stream
    is_done: bool,
}

/// [`Generator`] stage. A "tree level" processing stage, used to track
/// transactions processed during a stage.
#[derive(Default)]
struct Stage {
    /// iterator containing cell entries from the previous tree stage
    cell_iterator: Option<Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static>>,
    /// cell entries generated during this stage
    cell_accumulator: Vec<CellEntryReference>,
    /// Total aggregate value of all inputs consumed during this stage
    aggregate_input_value: u64,
    /// Total aggregate value of all fees incurred during this stage
    aggregate_fees: u64,
    /// Total number of transactions generated during this stage
    number_of_transactions: usize,
}

impl Stage {
    fn new(previous: Stage) -> Stage {
        let cell_iterator: Box<dyn Iterator<Item = CellEntryReference> + Send + Sync + 'static> =
            Box::new(previous.cell_accumulator.into_iter());

        Stage {
            cell_iterator: Some(cell_iterator),
            cell_accumulator: vec![],
            aggregate_input_value: 0,
            aggregate_fees: 0,
            number_of_transactions: 0,
        }
    }
}

impl std::fmt::Debug for Stage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stage")
            .field("aggregate_input_value", &self.aggregate_input_value)
            .field("aggregate_fees", &self.aggregate_fees)
            .field("number_of_transactions", &self.number_of_transactions)
            .finish()
    }
}

///
///  Indicates the type of data yielded by the generator
///
#[derive(Debug, Copy, Clone)]
pub enum DataKind {
    /// No operation should be performed (abort)
    /// Used for handling exceptions, such as rejecting
    /// to produce dust outputs during sweep transactions.
    NoOp,
    /// A "tree node" or "relay" transaction meant for multi-stage
    /// operations. This transaction combines multiple cell entries
    /// into a single transaction to the supplied change address.
    Node,
    /// A "tree edge" transaction meant for multi-stage
    /// processing. Signifies completion of the tree level (stage).
    /// This operation will create a new tree level (stage).
    Edge,
    /// Final transaction combining the entire aggregated cell set
    /// into a single set of supplied outputs.
    Final,
}

impl DataKind {
    pub fn is_final(&self) -> bool {
        matches!(self, DataKind::Final)
    }
    pub fn is_stage_node(&self) -> bool {
        matches!(self, DataKind::Node)
    }
    pub fn is_stage_edge(&self) -> bool {
        matches!(self, DataKind::Edge)
    }
}

///
/// Single transaction data accumulator.  This structure is used to accumulate
/// and track all necessary transaction data and is then used to create
/// an actual transaction.
///
#[derive(Debug)]
struct Data {
    /// Transaction inputs accumulated during processing
    inputs: Vec<TransactionInput>,
    /// Cell entries referenced by transaction inputs
    cell_entry_references: Vec<CellEntryReference>,
    /// Addresses referenced by transaction inputs
    addresses: HashSet<Address>,
    /// Aggregate transaction mass
    aggregate_mass: u64,
    /// Transaction fees based on the aggregate mass
    transaction_fees: u64,
    /// Aggregate value of all inputs
    aggregate_input_value: u64,
    /// Optional change output value
    change_output_value: Option<u64>,
}

impl Data {
    fn new(calc: &MassCalculator) -> Self {
        let aggregate_mass = calc.blank_transaction_compute_mass();

        Data {
            inputs: vec![],
            cell_entry_references: vec![],
            addresses: HashSet::default(),
            aggregate_mass,
            transaction_fees: 0,
            aggregate_input_value: 0,
            change_output_value: None,
        }
    }
}

/// Helper struct for passing around transaction value
#[derive(Debug)]
struct FinalTransaction {
    /// Total output value required for the final transaction
    value_no_fees: u64,
    /// Total output value required for the final transaction + priority fees
    value_with_priority_fee: u64,
}

/// Helper struct for obtaining properties related to
/// transaction mass calculations.
struct MassDisposition {
    /// Transaction mass derived from compute and storage mass
    transaction_mass: u64,
    /// Calculated storage mass
    storage_mass: u64,
    /// Calculated transaction fees
    transaction_fees: u64,
    /// Flag signaling that computed values require change to be absorbed to fees.
    /// This occurs when the change is dust or the change is below the fees
    /// produced by the storage mass.
    absorb_change_to_fees: bool,
}

///
///  Internal Generator settings and references
///
struct Inner {
    // Atomic abortable trigger that will cause the processing to halt with `Error::Aborted`
    abortable: Option<Abortable>,
    // Optional signer that is passed on to the [`PendingTransaction`] allowing [`PendingTransaction`] to expose signing functions for convenience.
    signer: Option<Arc<dyn SignerT>>,
    // Internal mass calculator (pre-configured with network params)
    mass_calculator: MassCalculator,
    // Current network id
    network_id: NetworkId,
    // Current network params
    network_params: &'static NetworkParams,

    // Source cell context (used for source cell entry aggregation)
    source_cell_context: Option<CellContext>,
    // Destination cell context (used only during transfer transactions)
    destination_cell_context: Option<CellContext>,
    // Event multiplexer
    multiplexer: Option<Multiplexer<Box<Events>>>,
    // number of minimum signatures required to sign the transaction
    minimum_signatures: u16,
    // change address
    change_address: Address,
    standard_change_output_compute_mass: u64,
    // signature mass per input
    signature_mass_per_input: u64,
    // fee rate
    fee_rate: Option<f64>,
    // CPFP (Child-Pays-for-Parent) options
    cpfp: Option<CpfpOptions>,
    // final transaction amount and fees
    // `None` is used for sweep transactions
    final_transaction: Option<FinalTransaction>,
    // applies only to the final transaction
    final_transaction_priority_fee: Fees,
    // issued only in the final transaction
    final_transaction_outputs: Vec<PaymentOutput>,
    // pre-calculated partial harmonic for user outputs (does not include change)
    final_transaction_outputs_harmonic: u64,
    // mass of the final transaction
    final_transaction_outputs_compute_mass: u64,
    // final transaction payload
    final_transaction_payload: Vec<u8>,
    // final transaction payload mass
    final_transaction_payload_mass: u64,
    // compiled CellScript scheduler witness for the final transaction
    final_cellscript_compiled_scheduler_witness: Option<Vec<u8>>,
    final_cellscript_compiled_scheduler_witness_mass: u64,
    // parsed CellScript typed-cell scheduler plan for live witness construction
    cellscript_typed_cell_scheduler_plan: Option<CellScriptTypedCellSchedulerPlan>,
    // resolved sidecars for live typed-cell scheduler witness construction
    cellscript_typed_cell_resolved_cells: Vec<CellScriptTypedCellResolvedCell>,
    // Cell dependencies included in each generated transaction.
    cell_deps: Vec<CellDep>,
    // Header dependencies included in each generated transaction.
    header_deps: Vec<[u8; 32]>,
    // Final-transaction user output indexes that should receive CKB TYPE_ID scripts.
    ckb_type_id_output_indexes: Vec<usize>,
    // Final-transaction user outputs that should receive typed-cell type script and data.
    cellscript_typed_cell_outputs: Vec<CellScriptTypedCellOutput>,
    // execution context
    context: Mutex<Context>,
}

impl std::fmt::Debug for Inner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inner")
            .field("network_id", &self.network_id)
            .field("network_params", &self.network_params)
            // .field("source_cell_context", &self.source_cell_context)
            // .field("destination_cell_context", &self.destination_cell_context)
            // .field("multiplexer", &self.multiplexer)
            .field("minimum_signatures", &self.minimum_signatures)
            .field("change_address", &self.change_address)
            .field("standard_change_output_compute_mass", &self.standard_change_output_compute_mass)
            .field("signature_mass_per_input", &self.signature_mass_per_input)
            // .field("final_transaction", &self.final_transaction)
            .field("fee_rate", &self.fee_rate)
            .field("cpfp", &self.cpfp)
            .field("final_transaction_priority_fee", &self.final_transaction_priority_fee)
            .field("final_transaction_outputs", &self.final_transaction_outputs)
            .field("final_transaction_outputs_harmonic", &self.final_transaction_outputs_harmonic)
            .field("final_transaction_outputs_compute_mass", &self.final_transaction_outputs_compute_mass)
            .field("final_transaction_payload", &self.final_transaction_payload)
            .field("final_transaction_payload_mass", &self.final_transaction_payload_mass)
            .field(
                "final_cellscript_compiled_scheduler_witness_mass",
                &self.final_cellscript_compiled_scheduler_witness_mass,
            )
            .field("cellscript_typed_cell_scheduler_plan", &self.cellscript_typed_cell_scheduler_plan.is_some())
            .field("cellscript_typed_cell_resolved_cells", &self.cellscript_typed_cell_resolved_cells.len())
            .field("cell_deps", &self.cell_deps)
            .field("header_deps", &self.header_deps)
            .field("ckb_type_id_output_indexes", &self.ckb_type_id_output_indexes)
            .field("cellscript_typed_cell_outputs", &self.cellscript_typed_cell_outputs.len())
            // .field("context", &self.context)
            .finish()
    }
}

///
/// Transaction generator
///
#[derive(Clone)]
pub struct Generator {
    inner: Arc<Inner>,
}

impl Generator {
    /// Create a new [`Generator`] instance using [`GeneratorSettings`].
    pub fn try_new(settings: GeneratorSettings, signer: Option<Arc<dyn SignerT>>, abortable: Option<&Abortable>) -> Result<Self> {
        let GeneratorSettings {
            network_id,
            multiplexer,
            cell_iterator,
            source_cell_context,
            priority_cell_entries,
            minimum_signatures,
            change_address,
            fee_rate,
            final_transaction_priority_fee,
            final_transaction_destination,
            final_transaction_payload,
            final_cellscript_compiled_scheduler_witness,
            cell_deps,
            header_deps,
            ckb_type_id_output_indexes,
            cellscript_typed_cell_outputs,
            cellscript_typed_cell_scheduler_plan,
            cellscript_typed_cell_resolved_cells,
            destination_cell_context,
        } = settings;

        let network_type = NetworkType::from(network_id);
        let network_params = NetworkParams::from(network_id);
        let mass_calculator = MassCalculator::new(&network_id.into());

        let (final_transaction_outputs, final_transaction_amount) = match final_transaction_destination {
            PaymentDestination::Change => {
                if !final_transaction_priority_fee.is_none() {
                    return Err(Error::GeneratorFeesInSweepTransaction);
                }

                (vec![], None)
            }
            PaymentDestination::PaymentOutputs(outputs) => {
                // sanity checks
                if final_transaction_priority_fee.is_none() {
                    return Err(Error::GeneratorNoFeesForFinalTransaction);
                }

                for output in outputs.iter() {
                    if NetworkType::try_from(output.address.prefix)? != network_type {
                        return Err(Error::GeneratorPaymentOutputNetworkTypeMismatch);
                    }
                    if output.amount == 0 {
                        return Err(Error::GeneratorPaymentOutputZeroAmount);
                    }
                }

                (outputs.outputs.clone(), Some(outputs.amount()))
            }
        };
        validate_ckb_type_id_output_indexes(&ckb_type_id_output_indexes, final_transaction_outputs.len())?;
        validate_cellscript_typed_cell_scheduler_configuration(
            cellscript_typed_cell_scheduler_plan.as_ref(),
            &cellscript_typed_cell_outputs,
            &cellscript_typed_cell_resolved_cells,
            final_transaction_outputs.len(),
            &ckb_type_id_output_indexes,
        )?;

        if final_transaction_outputs.is_empty() && matches!(final_transaction_priority_fee, Fees::ReceiverPays(_)) {
            return Err(Error::GeneratorIncludeFeesRequiresOneOutput);
        }

        // sanity check
        if NetworkType::try_from(change_address.prefix)? != network_type {
            return Err(Error::GeneratorChangeAddressNetworkTypeMismatch);
        }

        let standard_change_output_mass =
            mass_calculator.calc_compute_mass_for_payment_output(&PaymentOutput::new(change_address.clone(), 0));
        let signature_mass_per_input = mass_calculator.calc_compute_mass_for_signature(minimum_signatures);
        let final_transaction_outputs_compute_mass = mass_calculator.calc_compute_mass_for_payment_outputs(&final_transaction_outputs)
            + mass_calculator.calc_compute_mass_for_ckb_type_id_output_scripts(ckb_type_id_output_indexes.len())
            + mass_calculator.calc_compute_mass_for_cellscript_typed_cell_outputs(&cellscript_typed_cell_outputs);
        let final_transaction_payload = final_transaction_payload.unwrap_or_default();
        let final_transaction_payload_mass = mass_calculator.calc_compute_mass_for_payload(final_transaction_payload.len());
        let final_cellscript_compiled_scheduler_witness_mass =
            if let Some(witness) = final_cellscript_compiled_scheduler_witness.as_ref() {
                mass_calculator.calc_compute_mass_for_payload(witness.len())
            } else if let Some(plan) = cellscript_typed_cell_scheduler_plan.as_ref() {
                typed_cell_scheduler_witness_mass_for_plan(plan, &mass_calculator)?
            } else {
                0
            };
        let final_transaction_outputs_harmonic = mass_calculator
            .calc_storage_mass_payment_output_harmonic(&final_transaction_outputs)
            .ok_or(Error::MassCalculationError)?;

        // reject transactions where the payload and outputs are more than 2/3rds of the maximum tx mass
        let final_transaction = final_transaction_amount.map(|amount| FinalTransaction {
            value_no_fees: amount,
            value_with_priority_fee: amount + final_transaction_priority_fee.additional(),
        });

        let mass_sanity_check = standard_change_output_mass
            + final_transaction_outputs_compute_mass
            + final_transaction_payload_mass
            + final_cellscript_compiled_scheduler_witness_mass;
        if mass_sanity_check > MAXIMUM_STANDARD_TRANSACTION_MASS / 5 * 4 {
            return Err(Error::GeneratorTransactionOutputsAreTooHeavy { mass: mass_sanity_check, kind: "compute mass" });
        }

        let priority_cell_entry_filter = priority_cell_entries.as_ref().map(|entries| entries.iter().cloned().collect());
        // remap to VecDeque as this list gets drained
        let priority_cell_entries = priority_cell_entries.map(|entries| entries.into_iter().collect::<VecDeque<_>>());

        let context = Mutex::new(Context {
            cell_source_iterator: cell_iterator,
            priority_cell_entries,
            priority_cell_entry_filter,
            number_of_stages: 0,
            number_of_transactions: 0,
            aggregated_cells: 0,
            aggregate_fees: 0,
            aggregate_mass: 0,
            stage: Some(Box::default()),
            cell_stash: VecDeque::default(),
            final_transaction_id: None,
            is_done: false,
        });

        let inner = Inner {
            network_id,
            network_params,
            multiplexer,
            context,
            signer,
            abortable: abortable.cloned(),
            mass_calculator,
            source_cell_context,
            minimum_signatures,
            change_address,
            standard_change_output_compute_mass: standard_change_output_mass,
            signature_mass_per_input,
            fee_rate,
            cpfp: None,
            final_transaction,
            final_transaction_priority_fee,
            final_transaction_outputs,
            final_transaction_outputs_harmonic,
            final_transaction_outputs_compute_mass,
            final_transaction_payload,
            final_transaction_payload_mass,
            final_cellscript_compiled_scheduler_witness,
            final_cellscript_compiled_scheduler_witness_mass,
            cellscript_typed_cell_scheduler_plan,
            cellscript_typed_cell_resolved_cells,
            cell_deps,
            header_deps,
            ckb_type_id_output_indexes,
            cellscript_typed_cell_outputs,
            destination_cell_context,
        };

        Ok(Self { inner: Arc::new(inner) })
    }

    /// Create a new [`Generator`] with CPFP (Child-Pays-for-Parent) fee bumping enabled.
    ///
    /// This is a convenience builder that configures the generator to calculate
    /// additional fees so that the combined parent+child transaction package meets
    /// the desired fee rate.
    ///
    /// # Arguments
    /// * `settings` - Standard generator settings.
    /// * `signer` - Optional transaction signer.
    /// * `abortable` - Optional abort trigger.
    /// * `cpfp` - CPFP options specifying the parent transaction details and target fee rate.
    ///
    /// # Example
    /// ```ignore
    /// let cpfp = CpfpOptions {
    ///     target_fee_rate: 2.0,
    ///     parent_transaction_id: parent_tx_id,
    ///     parent_transaction_mass: 2000,
    ///     parent_transaction_fee: 500,
    /// };
    /// let generator = Generator::with_cpfp(settings, None, None, cpfp)?;
    /// ```
    pub fn with_cpfp(
        settings: GeneratorSettings,
        signer: Option<Arc<dyn SignerT>>,
        abortable: Option<&Abortable>,
        cpfp: CpfpOptions,
    ) -> Result<Self> {
        let mut gen = Self::try_new(settings, signer, abortable)?;
        // Safety: we just created `gen` and hold the only Arc reference.
        Arc::get_mut(&mut gen.inner).expect("generator inner is uniquely owned at construction time").cpfp = Some(cpfp);
        Ok(gen)
    }

    /// Calculate the additional fee that the child transaction must pay to meet
    /// the CPFP target fee rate for the combined parent+child package.
    ///
    /// Returns `0` if no CPFP options are configured or if the parent already
    /// meets the target rate.
    ///
    /// The formula is:
    /// ```text
    /// child_extra = target_rate * (parent_mass + child_mass) - parent_fee - base_child_fee
    /// ```
    pub fn calc_cpfp_additional_fee(&self, child_mass: u64, base_child_fee: u64) -> u64 {
        if let Some(cpfp) = &self.inner.cpfp {
            let combined_mass = cpfp.parent_transaction_mass.saturating_add(child_mass);
            let required_combined_fee = (cpfp.target_fee_rate * combined_mass as f64) as u64;
            // The child must make up the deficit between required combined fee
            // and what is already covered by parent_fee + base_child_fee.
            let already_covered = cpfp.parent_transaction_fee.saturating_add(base_child_fee);
            required_combined_fee.saturating_sub(already_covered)
        } else {
            0
        }
    }

    /// Returns the [`CpfpOptions`] if CPFP is configured for this generator.
    pub fn cpfp_options(&self) -> Option<&CpfpOptions> {
        self.inner.cpfp.as_ref()
    }

    /// Check whether any of the current transaction inputs originate from an
    /// unconfirmed parent transaction.  A cell entry with `block_daa_score ==
    /// UNACCEPTED_DAA_SCORE` is treated as unconfirmed.
    pub fn has_unconfirmed_parent_inputs(&self, entries: &[CellEntryReference]) -> bool {
        entries.iter().any(|e| e.cell.block_daa_score == UNACCEPTED_DAA_SCORE)
    }

    /// Returns the current [`NetworkType`]
    #[inline(always)]
    pub fn network_type(&self) -> NetworkType {
        self.inner.network_id.into()
    }

    /// Returns the current [`NetworkId`]
    #[inline(always)]
    pub fn network_id(&self) -> NetworkId {
        self.inner.network_id
    }

    /// Returns current [`NetworkParams`]
    #[inline(always)]
    pub fn network_params(&self) -> &NetworkParams {
        self.inner.network_params
    }

    /// Returns owned mass calculator instance (bound to [`NetworkParams`] of the [`Generator`])
    #[inline(always)]
    pub fn mass_calculator(&self) -> &MassCalculator {
        &self.inner.mass_calculator
    }

    /// The underlying [`CellContext`] (if available).
    #[inline(always)]
    pub fn source_cell_context(&self) -> &Option<CellContext> {
        &self.inner.source_cell_context
    }

    /// Signifies that the transaction is a transfer between accounts
    #[inline(always)]
    pub fn destination_cell_context(&self) -> &Option<CellContext> {
        &self.inner.destination_cell_context
    }

    /// Core [`Multiplexer<Events>`] (if available)
    #[inline(always)]
    pub fn multiplexer(&self) -> &Option<Multiplexer<Box<Events>>> {
        &self.inner.multiplexer
    }

    /// Mutable context used by the generator to track state
    #[inline(always)]
    fn context(&self) -> MutexGuard<'_, Context> {
        self.inner.context.lock().unwrap()
    }

    /// Returns the underlying instance of the [Signer](SignerT)
    #[inline(always)]
    pub(crate) fn signer(&self) -> &Option<Arc<dyn SignerT>> {
        &self.inner.signer
    }

    /// The total amount of fees in SAU consumed during the transaction generation process.
    #[inline(always)]
    pub fn aggregate_fees(&self) -> u64 {
        self.context().aggregate_fees
    }

    /// The total number of cell entries consumed during the transaction generation process.
    #[inline(always)]
    pub fn aggregate_cells(&self) -> usize {
        self.context().aggregated_cells
    }

    /// The final transaction amount (if available).
    #[inline(always)]
    pub fn final_transaction_value_no_fees(&self) -> Option<u64> {
        self.inner.final_transaction.as_ref().map(|final_transaction| final_transaction.value_no_fees)
    }

    /// Returns the final transaction id if the generator has finished successfully.
    #[inline(always)]
    pub fn final_transaction_id(&self) -> Option<TransactionId> {
        self.context().final_transaction_id
    }

    /// Returns an async Stream causes the [Generator] to produce
    /// transaction for each stream item request. NOTE: transactions
    /// are generated only when each stream item is polled.
    #[inline(always)]
    pub fn stream(&self) -> impl Stream<Item = Result<PendingTransaction>> {
        Box::pin(PendingTransactionStream::new(self))
    }

    /// Returns an iterator that causes the [Generator] to produce
    /// transaction for each iterator poll request. NOTE: transactions
    /// are generated only when the returned iterator is iterated.
    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = Result<PendingTransaction>> {
        PendingTransactionIterator::new(self)
    }

    /// Adds a [`CellEntryReference`] to the cell-entry stash. This stash
    /// is the first source consulted during input selection.
    pub fn stash(&self, into_iter: impl IntoIterator<Item = CellEntryReference>) {
        self.context().cell_stash.extend(into_iter);
    }

    /// Get next cell entry. This function obtains entries in the following order:
    /// 1. From the stash (used to store entries that were consumed during previous transaction generation but were rejected due to various conditions, such as mass overflow)
    /// 2. From the current stage
    /// 3. From priority cell entries
    /// 4. From the source iterator (while filtering against priority entries)
    fn get_cell_entry(&self, context: &mut Context, stage: &mut Stage) -> Option<CellEntryReference> {
        context
            .cell_stash
            .pop_front()
            .or_else(|| stage.cell_iterator.as_mut().and_then(|cell_stage_iterator| cell_stage_iterator.next()))
            .or_else(|| context.priority_cell_entries.as_mut().and_then(|entries| entries.pop_front()))
            .or_else(|| loop {
                let cell_entry = context.cell_source_iterator.next()?;

                if let Some(filter) = context.priority_cell_entry_filter.as_ref() {
                    if filter.contains(&cell_entry) {
                        // skip the entry from the iterator intake
                        // if it has been supplied as a priority entry
                        continue;
                    }
                }

                break Some(cell_entry);
            })
    }

    /// Calculate relay transaction mass for the current transaction `data`
    #[inline(always)]
    fn calc_relay_transaction_mass(&self, data: &Data) -> u64 {
        data.aggregate_mass + self.inner.standard_change_output_compute_mass
    }

    /// Calculate relay transaction fees for the current transaction `data`
    #[inline(always)]
    fn calc_relay_transaction_compute_fees(&self, data: &Data) -> u64 {
        let mass = self.calc_relay_transaction_mass(data);
        self.inner.mass_calculator.calc_minimum_transaction_fee_from_mass(mass).max(self.calc_fee_rate(mass))
    }

    fn calc_fees_from_mass(&self, mass: u64) -> u64 {
        self.inner.mass_calculator.calc_minimum_transaction_fee_from_mass(mass).max(self.calc_fee_rate(mass))
    }

    /// Main cell-entry processing loop. This function sources inputs from [`Generator::get_cell_entry()`] and
    /// accumulates consumed entry data within the [`Context`], [`Stage`] and [`Data`] structures.
    ///
    /// The general processing pattern can be described as follows:
    ///
    /**
    loop {
       1. Obtain a cell entry from [`Generator::get_cell_entry()`]
       2. Check if entries have been depleted, if so, handle sweep processing.
       3. Create a new Input for the transaction from the cell entry.
       4. Check if the transaction mass threshold has been reached, if so, yield the transaction.
       5. Register input with the [`Data`] structures.
       6. Check if the final transaction amount has been reached, if so, yield the transaction.

    }
    */
    fn generate_transaction_data(&self, context: &mut Context, stage: &mut Stage) -> Result<(DataKind, Data)> {
        let calc = &self.inner.mass_calculator;
        let mut data = Data::new(calc);

        loop {
            if let Some(abortable) = self.inner.abortable.as_ref() {
                abortable.check()?;
            }

            let cell_entry_reference = if let Some(cell_entry_reference) = self.get_cell_entry(context, stage) {
                cell_entry_reference
            } else {
                // entry sources are depleted
                if let Some(final_transaction) = &self.inner.final_transaction {
                    // reject transaction
                    return Err(Error::InsufficientFunds {
                        additional_needed: final_transaction.value_with_priority_fee.saturating_sub(stage.aggregate_input_value),
                        origin: "accumulator",
                    });
                } else {
                    // finish sweep processing
                    return self.finish_relay_stage_processing(context, stage, data);
                }
            };

            if let Some(node) = self.aggregate_cell_entry(context, calc, stage, &mut data, cell_entry_reference) {
                return Ok((node, data));
            }

            if let Some(final_transaction) = &self.inner.final_transaction {
                // try finish a stage or produce a final transaction with target value
                // use basic condition checks to avoid unnecessary processing
                if data.aggregate_mass > TRANSACTION_MASS_BOUNDARY_FOR_STAGE_INPUT_ACCUMULATION
                    || (self.inner.final_transaction_priority_fee.sender_pays()
                        && stage.aggregate_input_value >= final_transaction.value_with_priority_fee)
                    || (self.inner.final_transaction_priority_fee.receiver_pays()
                        && stage.aggregate_input_value >= final_transaction.value_no_fees.saturating_sub(context.aggregate_fees))
                {
                    if let Some(kind) = self.try_finish_standard_stage_processing(context, stage, &mut data, final_transaction)? {
                        return Ok((kind, data));
                    }
                }
            }
        }
    }

    /// Test if the current state has additional entries. Use with caution as this
    /// function polls the iterator and relocates the entry into the stash.
    fn has_cell_entries(&self, context: &mut Context, stage: &mut Stage) -> bool {
        if let Some(cell_entry_reference) = self.get_cell_entry(context, stage) {
            context.cell_stash.push_back(cell_entry_reference);
            true
        } else {
            false
        }
    }

    /// Add a single input (cell entry) to the transaction accumulator.
    fn aggregate_cell_entry(
        &self,
        context: &mut Context,
        calc: &MassCalculator,
        stage: &mut Stage,
        data: &mut Data,
        cell_entry_reference: CellEntryReference,
    ) -> Option<DataKind> {
        let cell = &cell_entry_reference.cell;

        let input = TransactionInput::new(cell.outpoint.clone().into(), None, 0, Some(cell_entry_reference.clone()));
        let input_amount = cell.amount();
        let input_compute_mass = calc.calc_compute_mass_for_client_transaction_input(&input) + self.inner.signature_mass_per_input;

        // NOTE: relay transactions have no storage mass
        // mass threshold reached, yield transaction
        if data.aggregate_mass
            + input_compute_mass
            + self.inner.standard_change_output_compute_mass
            + self.inner.network_params.additional_compound_transaction_mass()
            > MAXIMUM_STANDARD_TRANSACTION_MASS
        {
            // note, we've used input for mass boundary calc and now abandon it
            // while preserving the entry reference to be used in the next iteration

            context.cell_stash.push_back(cell_entry_reference);
            data.aggregate_mass +=
                self.inner.standard_change_output_compute_mass + self.inner.network_params.additional_compound_transaction_mass();
            data.transaction_fees = self.calc_relay_transaction_compute_fees(data);
            stage.aggregate_fees += data.transaction_fees;
            context.aggregate_fees += data.transaction_fees;
            Some(DataKind::Node)
        } else {
            context.aggregated_cells += 1;
            stage.aggregate_input_value += input_amount;
            data.aggregate_input_value += input_amount;
            data.aggregate_mass += input_compute_mass;
            data.cell_entry_references.push(cell_entry_reference.clone());
            data.inputs.push(input);
            cell.address.as_ref().map(|address| data.addresses.insert(address.clone()));
            None
        }
    }

    /// Check current state and either 1) initiate a new stage or 2) finish stage accumulation processing
    fn finish_relay_stage_processing(&self, context: &mut Context, stage: &mut Stage, mut data: Data) -> Result<(DataKind, Data)> {
        data.transaction_fees = self.calc_relay_transaction_compute_fees(&data);
        stage.aggregate_fees += data.transaction_fees;
        context.aggregate_fees += data.transaction_fees;

        if context.aggregated_cells < 2 {
            Ok((DataKind::NoOp, data))
        } else if stage.number_of_transactions > 0 {
            data.aggregate_mass += self.inner.standard_change_output_compute_mass;
            Ok((DataKind::Edge, data))
        } else if data.aggregate_input_value < data.transaction_fees {
            Err(Error::InsufficientFunds { additional_needed: data.transaction_fees - data.aggregate_input_value, origin: "relay" })
        } else {
            let change_output_value = data.aggregate_input_value - data.transaction_fees;

            if self.inner.mass_calculator.is_dust(change_output_value) {
                // sweep transaction resulting in dust output
                Ok((DataKind::NoOp, data))
            } else {
                data.aggregate_mass += self.inner.standard_change_output_compute_mass;
                data.change_output_value = Some(change_output_value);
                Ok((DataKind::Final, data))
            }
        }
    }

    /// Calculate storage mass using inputs from `Data`
    /// and `output_harmonics` supplied by the user
    fn calc_storage_mass(&self, data: &Data, output_harmonics: u64) -> u64 {
        let calc = &self.inner.mass_calculator;
        calc.calc_storage_mass(output_harmonics, data.aggregate_input_value, data.inputs.len() as u64)
    }

    fn calc_fee_rate(&self, mass: u64) -> u64 {
        self.inner.fee_rate.map(|fee_rate| (fee_rate * mass as f64) as u64).unwrap_or(0)
    }
    /// Check if the current state has sufficient funds for the final transaction,
    /// initiate new stage if necessary, or finish stage processing creating the
    /// final transaction.
    fn try_finish_standard_stage_processing(
        &self,
        context: &mut Context,
        stage: &mut Stage,
        data: &mut Data,
        final_transaction: &FinalTransaction,
    ) -> Result<Option<DataKind>> {
        let calc = &self.inner.mass_calculator;

        // calculate storage mass
        let MassDisposition { transaction_mass, storage_mass, transaction_fees, absorb_change_to_fees } =
            self.calculate_mass(stage, data, final_transaction.value_with_priority_fee)?;

        let total_stage_value_needed = if self.inner.final_transaction_priority_fee.sender_pays() {
            final_transaction.value_with_priority_fee + stage.aggregate_fees + transaction_fees
        } else {
            final_transaction.value_with_priority_fee
        };

        let reject = match self.inner.final_transaction_priority_fee {
            Fees::SenderPays(_) => stage.aggregate_input_value < total_stage_value_needed,
            Fees::ReceiverPays(_) => stage.aggregate_input_value + context.aggregate_fees < total_stage_value_needed,
            Fees::None => unreachable!("Fees::None can not occur for final transaction"),
        };

        if reject {
            // need more value, reject finalization (try adding more inputs)
            Ok(None)
        } else if transaction_mass > MAXIMUM_STANDARD_TRANSACTION_MASS || stage.number_of_transactions > 0 {
            self.generate_edge_transaction(context, stage, data)
        } else {
            // ---
            // attempt to aggregate additional cells in an effort to have more inputs and lower storage mass
            // TODO - discuss:
            // this is of questionable value as this can result in both positive and negative impact,
            // also doing this can result in reduction of the wallet cell set, which later results
            // in additional fees for the user.
            if storage_mass > 0
                && data.inputs.len() < self.inner.final_transaction_outputs.len() * 2
                && transaction_mass < TRANSACTION_MASS_BOUNDARY_FOR_ADDITIONAL_INPUT_ACCUMULATION
            {
                // fetch a cell from the iterator and if it exists, make it available on the next iteration via cell_stash.
                if self.has_cell_entries(context, stage) {
                    return Ok(None);
                }
            }
            // ---

            let (mut transaction_fees, change_output_value) = match self.inner.final_transaction_priority_fee {
                Fees::SenderPays(priority_fees) => {
                    let transaction_fees = transaction_fees + priority_fees;
                    let change_output_value = data.aggregate_input_value - final_transaction.value_no_fees - transaction_fees;
                    (transaction_fees, change_output_value)
                }
                // TODO - currently unreachable at the API level
                Fees::ReceiverPays(priority_fees) => {
                    let transaction_fees = transaction_fees + priority_fees;
                    let change_output_value = data.aggregate_input_value.saturating_sub(final_transaction.value_no_fees);
                    (transaction_fees, change_output_value)
                }
                Fees::None => unreachable!("Fees::None is not allowed for final transactions"),
            };

            // checks output dust threshold in network params
            // if is_dust(&self.inner.network_params, change_output_value) {
            if absorb_change_to_fees || change_output_value == 0 {
                transaction_fees += change_output_value;

                // as we might absorb an input as a part of the receiver
                // pays fee reduction, we should update the mass to make
                // sure internal metrics and unit tests check out.
                let compute_mass = data.aggregate_mass
                    + self.inner.final_transaction_outputs_compute_mass
                    + self.inner.final_transaction_payload_mass;
                let storage_mass = self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic);

                data.aggregate_mass = calc.combine_mass(compute_mass, storage_mass);

                transaction_fees += change_output_value;
                data.transaction_fees = transaction_fees;
                stage.aggregate_fees += transaction_fees;
                context.aggregate_fees += transaction_fees;

                Ok(Some(DataKind::Final))
            } else {
                data.aggregate_mass = transaction_mass;
                data.transaction_fees = transaction_fees;
                stage.aggregate_fees += transaction_fees;
                context.aggregate_fees += transaction_fees;
                data.change_output_value = Some(change_output_value);

                Ok(Some(DataKind::Final))
            }
        }
    }

    fn calculate_mass(&self, stage: &Stage, data: &Data, transaction_target_value: u64) -> Result<MassDisposition> {
        let calc = &self.inner.mass_calculator;

        let mut absorb_change_to_fees = false;

        let compute_mass_with_change = data.aggregate_mass
            + self.inner.standard_change_output_compute_mass
            + self.inner.final_transaction_outputs_compute_mass
            + self.inner.final_transaction_payload_mass;

        let storage_mass = if stage.number_of_transactions > 0 {
            // calculate for edge transaction boundaries
            // we know that stage.number_of_transactions > 0 will trigger stage generation
            let edge_compute_mass = data.aggregate_mass + self.inner.standard_change_output_compute_mass; //self.inner.final_transaction_outputs_compute_mass + self.inner.final_transaction_payload_mass;
            let edge_fees = self.calc_fees_from_mass(edge_compute_mass);
            let edge_output_value = data.aggregate_input_value.saturating_sub(edge_fees);
            if edge_output_value != 0 {
                let edge_output_harmonic = calc.calc_storage_mass_output_harmonic_single(edge_output_value);
                self.calc_storage_mass(data, edge_output_harmonic)
            } else {
                0
            }
        } else if data.aggregate_input_value <= transaction_target_value {
            // calculate for final transaction boundaries
            self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic)
        } else {
            // calculate for final transaction boundaries
            let change_value = data.aggregate_input_value - transaction_target_value;

            if self.inner.mass_calculator.is_dust(change_value) {
                absorb_change_to_fees = true;
                self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic)
            } else {
                let output_harmonic_with_change =
                    calc.calc_storage_mass_output_harmonic_single(change_value) + self.inner.final_transaction_outputs_harmonic;
                let storage_mass_with_change = self.calc_storage_mass(data, output_harmonic_with_change);

                // TODO - review and potentially simplify:
                // this profiles the storage mass with change and without change
                // and decides which one to use based on the fees
                if storage_mass_with_change == 0 || (storage_mass_with_change < compute_mass_with_change) {
                    0
                } else {
                    let storage_mass_no_change = self.calc_storage_mass(data, self.inner.final_transaction_outputs_harmonic);
                    if storage_mass_with_change < storage_mass_no_change {
                        storage_mass_with_change
                    } else {
                        let fees_with_change = calc.calc_fee_for_mass(storage_mass_with_change);
                        let fees_no_change = calc.calc_fee_for_mass(storage_mass_no_change);
                        let difference = fees_with_change.saturating_sub(fees_no_change);

                        if difference > change_value {
                            absorb_change_to_fees = true;
                            storage_mass_no_change
                        } else {
                            storage_mass_with_change
                        }
                    }
                }
            }
        };

        if storage_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            Err(Error::StorageMassExceedsMaximumTransactionMass { storage_mass })
        } else {
            let transaction_mass = calc.combine_mass(compute_mass_with_change, storage_mass);
            let transaction_fees = self.calc_fees_from_mass(transaction_mass);

            Ok(MassDisposition { transaction_mass, transaction_fees, storage_mass, absorb_change_to_fees })
        }
    }

    /// Generate an `Edge` transaction. This function is called when the transaction
    /// processing has aggregated sufficient inputs to match requested outputs.
    fn generate_edge_transaction(&self, context: &mut Context, stage: &mut Stage, data: &mut Data) -> Result<Option<DataKind>> {
        let calc = &self.inner.mass_calculator;

        let compute_mass = data.aggregate_mass
            + self.inner.standard_change_output_compute_mass
            + self.inner.network_params.additional_compound_transaction_mass();
        let compute_fees = self.calc_fees_from_mass(compute_mass);

        // TODO - consider removing this as calculated storage mass should produce `0` value
        let edge_output_harmonic =
            calc.calc_storage_mass_output_harmonic_single(data.aggregate_input_value.saturating_sub(compute_fees));
        let storage_mass = self.calc_storage_mass(data, edge_output_harmonic);
        let transaction_mass = calc.combine_mass(compute_mass, storage_mass);

        if transaction_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
            // transaction mass is too high... if we have additional
            // cells, reject and try to accumulate more inputs...
            if self.has_cell_entries(context, stage) {
                Ok(None)
            } else {
                // otherwise we have insufficient funds
                Err(Error::GeneratorTransactionIsTooHeavy)
            }
        } else {
            data.aggregate_mass = transaction_mass;
            data.transaction_fees = self.calc_fees_from_mass(transaction_mass);
            stage.aggregate_fees += data.transaction_fees;
            context.aggregate_fees += data.transaction_fees;
            Ok(Some(DataKind::Edge))
        }
    }

    /// Generates a single transaction by draining the supplied cell iterator.
    /// This function is used by the by the available async Stream and Iterator
    /// implementations to generate a stream of transactions.
    ///
    /// This function returns `None` once the supplied cell iterator is depleted.
    ///
    /// This function runs a continuous loop by ingesting inputs from the cell
    /// iterator, analyzing the resulting transaction mass, and either producing
    /// an intermediate "batch" transaction sending funds to the change address
    /// or creating a final transaction with the requested set of outputs and the
    /// payload.
    pub fn generate_transaction(&self) -> Result<Option<PendingTransaction>> {
        let mut context = self.context();

        if context.is_done {
            return Ok(None);
        }

        let mut stage = context.stage.take().unwrap();
        let (kind, data) = self.generate_transaction_data(&mut context, &mut stage)?;
        context.stage.replace(stage);

        match (kind, data) {
            (DataKind::NoOp, _) => {
                context.is_done = true;
                context.stage.take();
                Ok(None)
            }
            (DataKind::Final, data) => {
                context.is_done = true;
                context.stage.take();

                let Data {
                    inputs,
                    cell_entry_references,
                    addresses,
                    aggregate_input_value,
                    change_output_value,
                    aggregate_mass: _,
                    transaction_fees,
                    ..
                } = data;

                let change_output_value = change_output_value.unwrap_or(0);

                let mut final_outputs = self.inner.final_transaction_outputs.clone();

                if self.inner.final_transaction_priority_fee.receiver_pays() {
                    let output = final_outputs.get_mut(0).expect("include fees requires one output");
                    if aggregate_input_value < output.amount {
                        output.amount = aggregate_input_value - transaction_fees;
                    } else {
                        output.amount -= transaction_fees;
                    }
                }

                let change_output_index = if change_output_value > 0 {
                    let change_output_index = Some(final_outputs.len());
                    final_outputs.push(PaymentOutput::new(self.inner.change_address.clone(), change_output_value));
                    change_output_index
                } else {
                    None
                };

                let aggregate_output_value = final_outputs.iter().map(|output| output.amount).sum::<u64>();
                // TODO - validate that this is still correct
                // `Fees::ReceiverPays` processing can result in outputs being larger than inputs
                if aggregate_output_value > aggregate_input_value {
                    return Err(Error::InsufficientFunds {
                        additional_needed: aggregate_output_value - aggregate_input_value,
                        origin: "final",
                    });
                }

                let mut tx =
                    self.build_unsigned_cell_transaction(inputs, final_outputs, self.inner.final_transaction_payload.clone())?;
                apply_cellscript_typed_cell_outputs(&mut tx, &self.inner.cellscript_typed_cell_outputs)?;
                apply_ckb_type_id_output_scripts(&mut tx, &self.inner.ckb_type_id_output_indexes)?;
                let cellscript_scheduler_accesses = if let Some(plan) = self.inner.cellscript_typed_cell_scheduler_plan.as_ref() {
                    Some(attach_cellscript_typed_cell_scheduler_witness(
                        &mut tx,
                        plan,
                        &self.inner.cellscript_typed_cell_resolved_cells,
                    )?)
                } else {
                    attach_cellscript_compiled_scheduler_witness(
                        &mut tx,
                        self.inner.final_cellscript_compiled_scheduler_witness.clone(),
                    )?
                };

                let transaction_mass = self.inner.mass_calculator.calc_overall_mass_for_unsigned_consensus_transaction(
                    &tx,
                    &cell_entry_references,
                    self.inner.minimum_signatures,
                )?;
                if transaction_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
                    // this should never occur as we should not produce transactions higher than the mass limit
                    return Err(Error::MassCalculationError);
                }
                context.aggregate_mass += transaction_mass;
                context.final_transaction_id = Some(tx.id().into());
                context.number_of_stages += 1;
                context.number_of_transactions += 1;

                Ok(Some(PendingTransaction::try_new(
                    self,
                    tx,
                    cell_entry_references,
                    addresses.into_iter().collect(),
                    self.final_transaction_value_no_fees(),
                    change_output_index,
                    change_output_value,
                    aggregate_input_value,
                    aggregate_output_value,
                    self.inner.minimum_signatures,
                    transaction_mass,
                    transaction_fees,
                    kind,
                    cellscript_scheduler_accesses,
                )?))
            }
            (kind, data) => {
                let Data {
                    inputs,
                    cell_entry_references,
                    addresses,
                    aggregate_input_value,
                    aggregate_mass: _,
                    transaction_fees,
                    change_output_value,
                    ..
                } = data;

                assert_eq!(change_output_value, None);

                if aggregate_input_value <= transaction_fees {
                    return Err(Error::TransactionFeesAreTooHigh);
                }

                let output_value = aggregate_input_value.saturating_sub(transaction_fees);
                let output = PaymentOutput::new(self.inner.change_address.clone(), output_value);
                let tx = self.build_unsigned_cell_transaction(inputs, vec![output], vec![])?;

                let mut transaction_mass = self.inner.mass_calculator.calc_overall_mass_for_unsigned_consensus_transaction(
                    &tx,
                    &cell_entry_references,
                    self.inner.minimum_signatures,
                )?;
                transaction_mass = transaction_mass.saturating_add(self.inner.network_params.additional_compound_transaction_mass());
                if transaction_mass > MAXIMUM_STANDARD_TRANSACTION_MASS {
                    // this should never occur as we should not produce transactions higher than the mass limit
                    return Err(Error::MassCalculationError);
                }
                context.aggregate_mass += transaction_mass;
                context.number_of_transactions += 1;

                let previous_batch_cell_entry_reference =
                    Self::create_batch_cell_entry_reference(tx.id().into(), output_value, &self.inner.change_address);

                match kind {
                    DataKind::Node => {
                        // store the resulting cell in the current stage
                        let stage = context.stage.as_mut().unwrap();
                        stage.cell_accumulator.push(previous_batch_cell_entry_reference);
                        stage.number_of_transactions += 1;
                    }
                    DataKind::Edge => {
                        // store the resulting cell in the current stage and create a new stage
                        let mut stage = context.stage.take().unwrap();
                        stage.cell_accumulator.push(previous_batch_cell_entry_reference);
                        stage.number_of_transactions += 1;
                        context.number_of_stages += 1;
                        context.stage.replace(Box::new(Stage::new(*stage)));
                    }
                    _ => unreachable!(),
                }

                Ok(Some(PendingTransaction::try_new(
                    self,
                    tx,
                    cell_entry_references,
                    addresses.into_iter().collect(),
                    self.final_transaction_value_no_fees(),
                    None,
                    output_value,
                    aggregate_input_value,
                    output_value,
                    self.inner.minimum_signatures,
                    transaction_mass,
                    transaction_fees,
                    kind,
                    None,
                )?))
            }
        }
    }

    fn build_unsigned_cell_transaction(
        &self,
        inputs: Vec<TransactionInput>,
        outputs: Vec<PaymentOutput>,
        payload: Vec<u8>,
    ) -> Result<CellTx> {
        let inputs = inputs
            .into_iter()
            .map(|input| {
                let inner = input.inner();
                let witness = inner.witness.clone().unwrap_or_default();
                let previous_outpoint = inner.previous_outpoint.clone();
                let since = inner.since;
                drop(inner);
                (CellInput::new(previous_outpoint.into(), since), witness)
            })
            .collect::<Vec<_>>();
        let witnesses = inputs.iter().map(|(_, witness)| witness.clone()).collect::<Vec<_>>();
        let inputs = inputs.into_iter().map(|(input, _)| input).collect::<Vec<_>>();
        let outputs = outputs.iter().map(cell_out_from_payment_output).collect::<Vec<_>>();
        let outputs_data = vec![vec![]; outputs.len()];
        let mut witnesses = witnesses;
        if !payload.is_empty() {
            // Preserve opaque payload bytes without reintroducing a top-level payload field.
            witnesses.push(payload);
        }
        CellTx::new_with_header_deps(
            inputs,
            self.inner.cell_deps.clone(),
            self.inner.header_deps.clone(),
            outputs,
            outputs_data,
            witnesses,
        )
        .map_err(|err| Error::custom(format!("failed to build canonical CellTx: {err}")))
    }

    fn create_batch_cell_entry_reference(txid: TransactionId, amount: u64, address: &Address) -> CellEntryReference {
        let outpoint = TransactionOutpoint::new(txid.as_bytes(), 0);
        let lock_script = pay_to_address_lock_script(address);
        let cell = CellEntry {
            address: Some(address.clone()),
            outpoint: outpoint.into(),
            amount,
            capacity: Some(amount),
            data_bytes: Some(0),
            lock_hash: Some(lock_script.hash().into()),
            type_hash: None,
            data_hash: Some(TransactionId::from([0; 32])),
            block_daa_score: UNACCEPTED_DAA_SCORE,
            is_coinbase: false, // entry
        };
        CellEntryReference { cell: Arc::new(cell) }
    }

    /// Produces [`GeneratorSummary`] for the current state of the generator.
    /// This method is useful for creation of transaction estimations.
    pub fn summary(&self) -> GeneratorSummary {
        let context = self.context();

        GeneratorSummary {
            network_id: self.inner.network_id,
            aggregated_cells: context.aggregated_cells,
            aggregated_fees: context.aggregate_fees,
            aggregated_mass: context.aggregate_mass,
            final_transaction_amount: self.final_transaction_value_no_fees(),
            final_transaction_id: context.final_transaction_id,
            number_of_generated_transactions: context.number_of_transactions,
            number_of_generated_stages: context.number_of_stages,
        }
    }
}

fn cell_out_from_payment_output(output: &PaymentOutput) -> spora_exec::CellOutput {
    let lock_script = pay_to_address_lock_script(&output.address);
    spora_exec::CellOutput { lock: lock_script, type_: None, capacity: output.amount }
}

fn validate_ckb_type_id_output_indexes(output_indexes: &[usize], output_count: usize) -> Result<()> {
    if output_indexes.is_empty() {
        return Ok(());
    }
    let mut sorted = output_indexes.to_vec();
    sorted.sort_unstable();
    for output_index in &sorted {
        if *output_index >= output_count {
            return Err(Error::custom(format!(
                "CKB TYPE_ID output index {} is outside final user output count {}",
                output_index, output_count
            )));
        }
    }
    for pair in sorted.windows(2) {
        if pair[0] == pair[1] {
            return Err(Error::custom(format!("duplicate CKB TYPE_ID output index {}", pair[0])));
        }
    }
    Ok(())
}

fn validate_cellscript_typed_cell_outputs(
    typed_outputs: &[CellScriptTypedCellOutput],
    output_count: usize,
    ckb_type_id_output_indexes: &[usize],
) -> Result<()> {
    if typed_outputs.is_empty() {
        return Ok(());
    }
    let mut output_indexes = typed_outputs.iter().map(|output| output.output_index).collect::<Vec<_>>();
    output_indexes.sort_unstable();
    for output_index in &output_indexes {
        if *output_index >= output_count {
            return Err(Error::custom(format!(
                "CellScript typed-cell output index {} is outside final user output count {}",
                output_index, output_count
            )));
        }
        if ckb_type_id_output_indexes.contains(output_index) {
            return Err(Error::custom(format!(
                "CellScript typed-cell output index {} conflicts with CKB TYPE_ID output configuration",
                output_index
            )));
        }
    }
    for pair in output_indexes.windows(2) {
        if pair[0] == pair[1] {
            return Err(Error::custom(format!("duplicate CellScript typed-cell output index {}", pair[0])));
        }
    }
    Ok(())
}

fn validate_cellscript_typed_cell_resolved_cells(resolved_cells: &[CellScriptTypedCellResolvedCell]) -> Result<()> {
    if resolved_cells.is_empty() {
        return Ok(());
    }
    let mut bindings = resolved_cells.iter().map(|cell| (cell.source.as_str(), cell.index)).collect::<Vec<_>>();
    bindings.sort_unstable();
    for (source, index) in &bindings {
        match *source {
            "Input" | "CellDep" => {}
            other => {
                return Err(Error::custom(format!("CellScript typed-cell resolved sidecar {other}#{index} has unsupported source")));
            }
        }
    }
    for pair in bindings.windows(2) {
        if pair[0] == pair[1] {
            return Err(Error::custom(format!("duplicate CellScript typed-cell resolved sidecar {}#{}", pair[0].0, pair[0].1)));
        }
    }
    Ok(())
}

fn validate_cellscript_typed_cell_scheduler_configuration(
    plan: Option<&CellScriptTypedCellSchedulerPlan>,
    typed_outputs: &[CellScriptTypedCellOutput],
    resolved_cells: &[CellScriptTypedCellResolvedCell],
    output_count: usize,
    ckb_type_id_output_indexes: &[usize],
) -> Result<()> {
    validate_cellscript_typed_cell_outputs(typed_outputs, output_count, ckb_type_id_output_indexes)?;
    validate_cellscript_typed_cell_resolved_cells(resolved_cells)?;

    let Some(plan) = plan else {
        if !typed_outputs.is_empty() || !resolved_cells.is_empty() {
            return Err(Error::custom("CellScript typed-cell output/sidecar configuration requires a typed-cell scheduler plan"));
        }
        return Ok(());
    };

    for access in &plan.accesses {
        match access.source.as_str() {
            "Output" => {
                if access.index >= output_count {
                    return Err(Error::custom(format!(
                        "CellScript typed-cell scheduler access {} Output#{} is outside final user output count {}",
                        access.binding, access.index, output_count
                    )));
                }
                let typed_output = typed_outputs.iter().find(|output| output.output_index == access.index).ok_or_else(|| {
                    Error::custom(format!(
                        "CellScript typed-cell scheduler access {} Output#{} requires typed output type script and data configuration",
                        access.binding, access.index
                    ))
                })?;
                extract_typed_cell_conflict_key_value(access, &typed_output.data)?;
            }
            "Input" | "CellDep" => {
                let resolved_cell = resolved_cells
                    .iter()
                    .find(|cell| cell.source == access.source && cell.index == access.index)
                    .ok_or_else(|| {
                        Error::custom(format!(
                            "CellScript typed-cell scheduler access {} {}#{} requires resolved type script and data sidecar",
                            access.binding, access.source, access.index
                        ))
                    })?;
                extract_typed_cell_conflict_key_value(access, &resolved_cell.data)?;
            }
            other => {
                return Err(Error::custom(format!(
                    "CellScript typed-cell scheduler access {} has unsupported source {other}",
                    access.binding
                )));
            }
        }
    }

    for typed_output in typed_outputs {
        if !plan.accesses.iter().any(|access| access.source == "Output" && access.index == typed_output.output_index) {
            return Err(Error::custom(format!(
                "CellScript typed-cell output index {} is not referenced by the typed-cell scheduler plan",
                typed_output.output_index
            )));
        }
    }

    for resolved_cell in resolved_cells {
        if !plan.accesses.iter().any(|access| access.source == resolved_cell.source && access.index == resolved_cell.index) {
            return Err(Error::custom(format!(
                "CellScript typed-cell resolved sidecar {}#{} is not referenced by the typed-cell scheduler plan",
                resolved_cell.source, resolved_cell.index
            )));
        }
    }

    Ok(())
}

fn apply_ckb_type_id_output_scripts(tx: &mut CellTx, output_indexes: &[usize]) -> Result<Vec<[u8; 32]>> {
    output_indexes
        .iter()
        .map(|output_index| {
            ckb_apply_type_id_script_to_output_molecule(tx, *output_index)
                .map_err(|err| Error::custom(format!("failed to apply CKB TYPE_ID script to output {output_index}: {err}")))
        })
        .collect()
}

fn apply_cellscript_typed_cell_outputs(tx: &mut CellTx, typed_outputs: &[CellScriptTypedCellOutput]) -> Result<()> {
    for typed_output in typed_outputs {
        let output = tx.outputs.get_mut(typed_output.output_index).ok_or_else(|| {
            Error::custom(format!(
                "CellScript typed-cell output index {} is outside final transaction outputs",
                typed_output.output_index
            ))
        })?;
        if output.type_.is_some() {
            return Err(Error::custom(format!(
                "CellScript typed-cell output index {} already has a type script",
                typed_output.output_index
            )));
        }
        let output_data = tx.outputs_data.get_mut(typed_output.output_index).ok_or_else(|| {
            Error::custom(format!(
                "CellScript typed-cell output index {} is outside final transaction output data",
                typed_output.output_index
            ))
        })?;
        output.type_ = Some(typed_output.type_script.clone());
        *output_data = typed_output.data.clone();
    }
    Ok(())
}

fn attach_cellscript_compiled_scheduler_witness(
    tx: &mut CellTx,
    compiled_scheduler_witness: Option<Vec<u8>>,
) -> Result<Option<CellScriptSchedulerAccessList>> {
    let Some(compiled_scheduler_witness) = compiled_scheduler_witness else {
        return Ok(None);
    };

    tx.push_cellscript_compiled_scheduler_witness(compiled_scheduler_witness)
        .map(Some)
        .map_err(|err| Error::custom(format!("invalid CellScript scheduler witness for generated transaction: {err}")))
}

fn typed_cell_scheduler_witness_mass_for_plan(
    plan: &CellScriptTypedCellSchedulerPlan,
    mass_calculator: &MassCalculator,
) -> Result<u64> {
    let witness = typed_cell_scheduler_shape_witness_for_plan(plan)?;
    Ok(mass_calculator.calc_compute_mass_for_payload(witness.len()))
}

fn typed_cell_scheduler_shape_witness_for_plan(plan: &CellScriptTypedCellSchedulerPlan) -> Result<Vec<u8>> {
    let accesses = plan
        .accesses
        .iter()
        .map(|access| {
            Ok(CellScriptSchedulerAccessWitness {
                operation: cellscript_scheduler_operation_id(&access.operation)?,
                source: cellscript_scheduler_source_id(&access.source)?,
                index: u32::try_from(access.index).map_err(|_| {
                    Error::custom(format!("CellScript typed-cell scheduler access index {} is too large", access.index))
                })?,
                conflict_hash: [0u8; 32],
                typed_data_hash: [0u8; 32],
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(encode_cellscript_scheduler_witness_molecule(&CellScriptSchedulerWitness {
        magic: 0xCE11,
        version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
        effect_class: cellscript_scheduler_effect_class_id(&plan.effect_class)?,
        parallelizable: plan.parallelizable,
        estimated_cycles: plan.estimated_cycles,
        access_count: accesses.len() as u32,
        accesses,
    }))
}

pub fn build_cellscript_typed_cell_scheduler_witness_for_tx(
    tx: &CellTx,
    plan: &CellScriptTypedCellSchedulerPlan,
    resolved_cells: &[CellScriptTypedCellResolvedCell],
) -> Result<Vec<u8>> {
    let accesses = plan
        .accesses
        .iter()
        .map(|access| {
            let operation = cellscript_scheduler_operation_id(&access.operation)?;
            let source = cellscript_scheduler_source_id(&access.source)?;
            let index = u32::try_from(access.index)
                .map_err(|_| Error::custom(format!("CellScript typed-cell scheduler access index {} is too large", access.index)))?;
            let resolved = resolve_typed_cell_scheduler_access(tx, access, resolved_cells)?;
            let conflict_key_value = extract_typed_cell_conflict_key_value(access, &resolved.data)?;
            Ok(CellScriptSchedulerAccessWitness {
                operation,
                source,
                index,
                conflict_hash: compute_conflict_hash(&resolved.type_script, &conflict_key_value),
                typed_data_hash: compute_typed_data_hash(&resolved.type_script, &resolved.data),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(encode_cellscript_scheduler_witness_molecule(&CellScriptSchedulerWitness {
        magic: 0xCE11,
        version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
        effect_class: cellscript_scheduler_effect_class_id(&plan.effect_class)?,
        parallelizable: plan.parallelizable,
        estimated_cycles: plan.estimated_cycles,
        access_count: accesses.len() as u32,
        accesses,
    }))
}

pub fn attach_cellscript_typed_cell_scheduler_witness(
    tx: &mut CellTx,
    plan: &CellScriptTypedCellSchedulerPlan,
    resolved_cells: &[CellScriptTypedCellResolvedCell],
) -> Result<CellScriptSchedulerAccessList> {
    let witness = build_cellscript_typed_cell_scheduler_witness_for_tx(tx, plan, resolved_cells)?;
    tx.push_cellscript_compiled_scheduler_witness(witness)
        .map_err(|err| Error::custom(format!("invalid live CellScript typed-cell scheduler witness for generated transaction: {err}")))
}

pub async fn resolve_cellscript_typed_cell_resolved_cells_from_rpc(
    rpc: &Arc<DynRpcApi>,
    tx: &CellTx,
    plan: &CellScriptTypedCellSchedulerPlan,
) -> Result<Vec<CellScriptTypedCellResolvedCell>> {
    let mut resolved_cells = Vec::new();
    let mut resolved_bindings = HashSet::new();

    for access in &plan.accesses {
        let Some(outpoint) = typed_cell_scheduler_access_outpoint(tx, access)? else {
            continue;
        };
        if !resolved_bindings.insert((access.source.clone(), access.index)) {
            continue;
        }
        let resolved = resolve_typed_cell_scheduler_access_from_rpc(rpc, access, outpoint).await?;
        let resolved_cell = match access.source.as_str() {
            "Input" => CellScriptTypedCellResolvedCell::input(access.index, resolved.type_script, resolved.data),
            "CellDep" => CellScriptTypedCellResolvedCell::cell_dep(access.index, resolved.type_script, resolved.data),
            other => {
                return Err(Error::custom(format!(
                    "CellScript typed-cell scheduler access {} has unsupported sidecar source {other}",
                    access.binding
                )));
            }
        };
        resolved_cells.push(resolved_cell);
    }

    Ok(resolved_cells)
}

struct ResolvedTypedCellAccess {
    type_script: Script,
    data: Vec<u8>,
}

fn resolve_typed_cell_scheduler_access(
    tx: &CellTx,
    access: &CellScriptTypedCellSchedulerAccessPlan,
    resolved_cells: &[CellScriptTypedCellResolvedCell],
) -> Result<ResolvedTypedCellAccess> {
    match access.source.as_str() {
        "Output" => {
            let output = tx.outputs.get(access.index).ok_or_else(|| {
                Error::custom(format!(
                    "CellScript typed-cell scheduler access {} Output#{} is outside transaction outputs",
                    access.binding, access.index
                ))
            })?;
            let type_script = output.type_.clone().ok_or_else(|| {
                Error::custom(format!(
                    "CellScript typed-cell scheduler access {} Output#{} has no type script",
                    access.binding, access.index
                ))
            })?;
            let data = tx.outputs_data.get(access.index).cloned().ok_or_else(|| {
                Error::custom(format!(
                    "CellScript typed-cell scheduler access {} Output#{} has no output data",
                    access.binding, access.index
                ))
            })?;
            Ok(ResolvedTypedCellAccess { type_script, data })
        }
        "Input" | "CellDep" => resolved_cells
            .iter()
            .find(|resolved| resolved.source == access.source && resolved.index == access.index)
            .map(|resolved| ResolvedTypedCellAccess { type_script: resolved.type_script.clone(), data: resolved.data.clone() })
            .ok_or_else(|| {
                Error::custom(format!(
                    "CellScript typed-cell scheduler access {} {}#{} requires resolved type script and data sidecar",
                    access.binding, access.source, access.index
                ))
            }),
        other => {
            Err(Error::custom(format!("CellScript typed-cell scheduler access {} has unsupported source {other}", access.binding)))
        }
    }
}

fn typed_cell_scheduler_access_outpoint(
    tx: &CellTx,
    access: &CellScriptTypedCellSchedulerAccessPlan,
) -> Result<Option<TransactionOutpoint>> {
    match access.source.as_str() {
        "Output" => Ok(None),
        "Input" => tx.inputs.get(access.index).map(|input| Some(input.previous_output)).ok_or_else(|| {
            Error::custom(format!(
                "CellScript typed-cell scheduler access {} Input#{} is outside transaction inputs",
                access.binding, access.index
            ))
        }),
        "CellDep" => tx.cell_deps.get(access.index).map(|cell_dep| Some(cell_dep.out_point)).ok_or_else(|| {
            Error::custom(format!(
                "CellScript typed-cell scheduler access {} CellDep#{} is outside transaction cell deps",
                access.binding, access.index
            ))
        }),
        other => {
            Err(Error::custom(format!("CellScript typed-cell scheduler access {} has unsupported source {other}", access.binding)))
        }
    }
}

async fn resolve_typed_cell_scheduler_access_from_rpc(
    rpc: &Arc<DynRpcApi>,
    access: &CellScriptTypedCellSchedulerAccessPlan,
    outpoint: TransactionOutpoint,
) -> Result<ResolvedTypedCellAccess> {
    let transaction = rpc.get_transaction(TransactionId::from_bytes(outpoint.tx_hash)).await?;
    let output = transaction.outputs.get(outpoint.index as usize).ok_or_else(|| {
        Error::custom(format!(
            "CellScript typed-cell scheduler access {} {}#{} points to missing source output {}#{}",
            access.binding,
            access.source,
            access.index,
            TransactionId::from_bytes(outpoint.tx_hash),
            outpoint.index
        ))
    })?;
    resolve_typed_cell_scheduler_access_from_rpc_output(access, outpoint, output)
}

fn resolve_typed_cell_scheduler_access_from_rpc_output(
    access: &CellScriptTypedCellSchedulerAccessPlan,
    outpoint: TransactionOutpoint,
    output: &RpcTransactionOutput,
) -> Result<ResolvedTypedCellAccess> {
    let type_script = output.type_script.clone().map(Script::from).ok_or_else(|| {
        Error::custom(format!(
            "CellScript typed-cell scheduler access {} {}#{} source output {}#{} has no type script",
            access.binding,
            access.source,
            access.index,
            TransactionId::from_bytes(outpoint.tx_hash),
            outpoint.index
        ))
    })?;
    let data = output.output_data.clone().unwrap_or_default();
    if output.data_bytes.unwrap_or(0) as usize != data.len() {
        return Err(Error::custom(format!(
            "CellScript typed-cell scheduler access {} {}#{} source output {}#{} has incomplete RPC output data",
            access.binding,
            access.source,
            access.index,
            TransactionId::from_bytes(outpoint.tx_hash),
            outpoint.index
        )));
    }
    extract_typed_cell_conflict_key_value(access, &data)?;
    Ok(ResolvedTypedCellAccess { type_script, data })
}

fn extract_typed_cell_conflict_key_value(access: &CellScriptTypedCellSchedulerAccessPlan, data: &[u8]) -> Result<Vec<u8>> {
    if access.conflict_key.is_none() {
        return Err(Error::custom(format!("CellScript typed-cell scheduler access {} has no conflict_key metadata", access.binding)));
    }
    let fields = access
        .conflict_key_field_slices
        .iter()
        .map(|field| {
            let end = field
                .offset
                .checked_add(field.size)
                .ok_or_else(|| Error::custom(format!("CellScript typed-cell conflict key field {} offset overflows", field.field)))?;
            data.get(field.offset..end).ok_or_else(|| {
                Error::custom(format!(
                    "CellScript typed-cell conflict key field {} needs bytes [{}..{}), but cell data has {} bytes",
                    field.field,
                    field.offset,
                    end,
                    data.len()
                ))
            })
        })
        .collect::<Result<Vec<_>>>()?;

    match access.conflict_key_encoding.as_deref() {
        Some("single-field-fixed-bytes-v1") if fields.len() == 1 => Ok(fields[0].to_vec()),
        Some("composite-fixed-bytes-v1") if !fields.is_empty() => Ok(encode_conflict_key_value_composite(&fields)),
        Some(encoding) => Err(Error::custom(format!(
            "CellScript typed-cell scheduler access {} has unsupported conflict_key_encoding {encoding}",
            access.binding
        ))),
        None => {
            Err(Error::custom(format!("CellScript typed-cell scheduler access {} is missing conflict_key_encoding", access.binding)))
        }
    }
}

fn cellscript_scheduler_effect_class_id(effect_class: &str) -> Result<u8> {
    match effect_class {
        "Pure" => Ok(CELLSCRIPT_SCHEDULER_EFFECT_PURE),
        "ReadOnly" => Ok(CELLSCRIPT_SCHEDULER_EFFECT_READ_ONLY),
        "Mutating" => Ok(CELLSCRIPT_SCHEDULER_EFFECT_MUTATING),
        "Creating" => Ok(CELLSCRIPT_SCHEDULER_EFFECT_CREATING),
        "Destroying" => Ok(CELLSCRIPT_SCHEDULER_EFFECT_DESTROYING),
        other => Err(Error::custom(format!("unsupported CellScript scheduler effect_class {other}"))),
    }
}

fn cellscript_scheduler_operation_id(operation: &str) -> Result<u8> {
    match operation {
        "consume" => Ok(CELLSCRIPT_SCHEDULER_OP_CONSUME),
        "transfer" => Ok(CELLSCRIPT_SCHEDULER_OP_TRANSFER),
        "destroy" => Ok(CELLSCRIPT_SCHEDULER_OP_DESTROY),
        "read_ref" => Ok(CELLSCRIPT_SCHEDULER_OP_READ_REF),
        "create" => Ok(CELLSCRIPT_SCHEDULER_OP_CREATE),
        other => Err(Error::custom(format!("unsupported CellScript typed-cell scheduler operation {other}"))),
    }
}

fn cellscript_scheduler_source_id(source: &str) -> Result<u8> {
    match source {
        "Input" => Ok(CELLSCRIPT_SCHEDULER_SOURCE_INPUT),
        "CellDep" => Ok(CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP),
        "Output" => Ok(CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT),
        other => Err(Error::custom(format!("unsupported CellScript typed-cell scheduler source {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::PaymentOutputs;
    use spora_exec::celltx::{
        compute_conflict_hash, compute_typed_data_hash, decode_cellscript_scheduler_witness,
        encode_cellscript_scheduler_witness_molecule, CellScriptSchedulerAccessWitness, CellScriptSchedulerWitness,
        CELLSCRIPT_SCHEDULER_EFFECT_CREATING, CELLSCRIPT_SCHEDULER_EFFECT_MUTATING, CELLSCRIPT_SCHEDULER_OP_CONSUME,
        CELLSCRIPT_SCHEDULER_OP_CREATE, CELLSCRIPT_SCHEDULER_OP_READ_REF, CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP,
        CELLSCRIPT_SCHEDULER_SOURCE_INPUT, CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT, CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
    };
    use spora_exec::{
        ckb_type_id_args, CellDep, CellOutput, CkbSecp256k1Blake160SighashAllLockConfig, DepType, OutPoint, Script,
        CKB_SCRIPT_HASH_TYPE_TYPE, CKB_TYPE_ID_CODE_HASH,
    };
    use spora_rpc_core::RpcTransaction;

    #[test]
    fn test_attach_cellscript_compiled_scheduler_witness_returns_trusted_summary() {
        let mut tx = CellTx::new(
            vec![],
            vec![],
            vec![CellOutput { lock: Script::new([0u8; 32], 0, vec![]), type_: None, capacity: 1000 }],
            vec![vec![]],
            vec![],
        )
        .unwrap();
        let access = CellScriptSchedulerAccessWitness {
            operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
            source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
            index: 0,
            conflict_hash: [0x4a; 32],
            typed_data_hash: [0x00; 32],
        };
        let witness = encode_cellscript_scheduler_witness_molecule(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,

            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![access.clone()],
        });

        let summary = attach_cellscript_compiled_scheduler_witness(&mut tx, Some(witness.clone())).unwrap();

        assert_eq!(summary.as_ref().map(|summary| summary.accesses.as_slice()), Some([access].as_slice()));
        assert_eq!(tx.witnesses.last().map(Vec::as_slice), Some(witness.as_slice()));
    }

    #[test]
    fn test_attach_cellscript_compiled_scheduler_witness_none_is_noop() {
        let mut tx = CellTx::new(vec![], vec![], vec![], vec![], vec![]).unwrap();

        let summary = attach_cellscript_compiled_scheduler_witness(&mut tx, None).unwrap();

        assert_eq!(summary, None);
        assert!(tx.witnesses.is_empty());
    }

    #[test]
    fn test_attach_cellscript_compiled_scheduler_witness_rejects_shape_mismatch_without_append() {
        let mut tx = CellTx::new(vec![], vec![], vec![], vec![], vec![]).unwrap();
        let witness = encode_cellscript_scheduler_witness_molecule(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_CREATING,
            parallelizable: false,

            estimated_cycles: 64,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CREATE,
                source: CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
                index: 0,
                conflict_hash: [0x4b; 32],
                typed_data_hash: [0x00; 32],
            }],
        });

        let error = attach_cellscript_compiled_scheduler_witness(&mut tx, Some(witness)).unwrap_err();

        assert!(error.to_string().contains("invalid CellScript scheduler witness"));
        assert!(tx.witnesses.is_empty());
    }

    fn typed_cell_scheduler_plan(operation: &str, source: &str, index: usize) -> CellScriptTypedCellSchedulerPlan {
        CellScriptTypedCellSchedulerPlan {
            abi: "spora-typed-cell-scheduler-plan-v1".to_string(),
            conflict_hash_domain: "spora-typed-cell/conflict-hash/v1".to_string(),
            typed_data_hash_domain: "spora-typed-cell/typed-data-hash/v1".to_string(),
            effect_class: "Mutating".to_string(),
            parallelizable: false,
            estimated_cycles: 500,
            accesses: vec![typed_cell_scheduler_access(operation, source, index, "invoice")],
        }
    }

    fn typed_cell_scheduler_access(
        operation: &str,
        source: &str,
        index: usize,
        binding: &str,
    ) -> CellScriptTypedCellSchedulerAccessPlan {
        CellScriptTypedCellSchedulerAccessPlan {
            operation: operation.to_string(),
            source: source.to_string(),
            index,
            binding: binding.to_string(),
            ty: "Invoice".to_string(),
            conflict_key: Some("field(invoice_id)".to_string()),
            conflict_key_fields: vec!["invoice_id".to_string()],
            conflict_key_encoding: Some("single-field-fixed-bytes-v1".to_string()),
            conflict_key_field_slices: vec![crate::tx::CellScriptTypedCellFieldSlice {
                field: "invoice_id".to_string(),
                offset: 0,
                size: 32,
            }],
            conflict_key_value_source: format!("transaction-{}-data-conflict-key-fields", source.to_ascii_lowercase()),
            typed_data_source: format!("transaction-{}-data", source.to_ascii_lowercase()),
        }
    }

    fn bytes_to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn typed_cell_input_scheduler_witness_with_hash(conflict_hash: [u8; 32]) -> Vec<u8> {
        encode_cellscript_scheduler_witness_molecule(&CellScriptSchedulerWitness {
            magic: 0xCE11,
            version: CELLSCRIPT_SCHEDULER_WITNESS_VERSION,
            effect_class: CELLSCRIPT_SCHEDULER_EFFECT_MUTATING,
            parallelizable: false,
            estimated_cycles: 500,
            access_count: 1,
            accesses: vec![CellScriptSchedulerAccessWitness {
                operation: CELLSCRIPT_SCHEDULER_OP_CONSUME,
                source: CELLSCRIPT_SCHEDULER_SOURCE_INPUT,
                index: 0,
                conflict_hash,
                typed_data_hash: [0xDD; 32],
            }],
        })
    }

    fn typed_cell_input_action_metadata_json(witness_hex: &str) -> String {
        format!(
            r#"{{
  "target_profile": {{ "name": "typed-cell" }},
  "types": [
    {{
      "name": "Invoice",
      "fields": [
        {{
          "name": "invoice_id",
          "ty": "Hash",
          "offset": 0,
          "encoded_size": 32,
          "fixed_width": true
        }}
      ]
    }}
  ],
  "actions": [
    {{
      "name": "settle_invoice",
      "effect_class": "Mutating",
      "parallelizable": false,
      "estimated_cycles": 500,
      "scheduler_witness_abi": "molecule",
      "scheduler_witness_hex": "{witness_hex}",
      "typed_cell_scheduler_plan": {{
        "abi": "spora-typed-cell-scheduler-plan-v1",
        "conflict_hash_domain": "spora-typed-cell/conflict-hash/v1",
        "typed_data_hash_domain": "spora-typed-cell/typed-data-hash/v1",
        "accesses": [
          {{
            "operation": "consume",
            "source": "Input",
            "index": 0,
            "binding": "invoice",
            "ty": "Invoice",
            "conflict_key": "field(invoice_id)",
            "conflict_key_fields": ["invoice_id"],
            "conflict_key_encoding": "single-field-fixed-bytes-v1",
            "conflict_key_value_source": "transaction-input-data-conflict-key-fields",
            "typed_data_source": "transaction-input-data"
          }}
        ]
      }}
    }}
  ]
}}"#
        )
    }

    fn typed_cell_generator_settings(seed: u8) -> GeneratorSettings {
        let source_address = Address::new_std_single(Prefix::Testnet, &[seed; 32]).unwrap();
        let change_address = Address::new_std_single(Prefix::Testnet, &[seed.wrapping_add(1); 32]).unwrap();
        let recipient = Address::new_std_single(Prefix::Testnet, &[seed.wrapping_add(2); 32]).unwrap();
        let cell = CellEntryReference::simulated_with_address(20_000_000_000, &source_address);
        let outputs = PaymentOutputs { outputs: vec![PaymentOutput::new(recipient, 5_000_000_000)] };
        GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(vec![cell].into_iter()),
            None,
            change_address,
            1,
            PaymentDestination::PaymentOutputs(outputs),
            None,
            Fees::SenderPays(0),
            None,
            None,
        )
        .unwrap()
    }

    fn rpc_transaction_with_typed_output(output_index: usize, type_script: Script, data: Vec<u8>) -> RpcTransaction {
        let outputs = (0..=output_index)
            .map(|index| {
                let output_type_script = (index == output_index).then(|| type_script.clone());
                let output_data = if index == output_index { data.as_slice() } else { &[] };
                RpcTransactionOutput::from_cell_output(
                    &CellOutput { lock: Script::new([0x51; 32], 0, vec![]), type_: output_type_script, capacity: 1000 },
                    output_data,
                )
            })
            .collect();
        RpcTransaction {
            version: 0,
            inputs: vec![],
            cell_deps: vec![],
            header_deps: vec![],
            outputs,
            payload: vec![],
            mass: 0,
            verbose_data: None,
        }
    }

    #[test]
    fn build_typed_cell_scheduler_witness_uses_output_data_field_slices() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-script-args".to_vec());
        let mut data = vec![0xA1; 32];
        data.extend_from_slice(b"invoice-state:issued:amount=1250000");
        let tx = CellTx::new(
            vec![],
            vec![],
            vec![CellOutput { lock: Script::new([0x51; 32], 0, vec![]), type_: Some(type_script.clone()), capacity: 1000 }],
            vec![data.clone()],
            vec![],
        )
        .unwrap();
        let plan = typed_cell_scheduler_plan("create", "Output", 0);

        let witness_bytes = build_cellscript_typed_cell_scheduler_witness_for_tx(&tx, &plan, &[]).unwrap();
        let witness = decode_cellscript_scheduler_witness(&witness_bytes).unwrap();

        assert_eq!(witness.effect_class, CELLSCRIPT_SCHEDULER_EFFECT_MUTATING);
        assert_eq!(witness.estimated_cycles, 500);
        assert_eq!(witness.access_count, 1);
        assert_eq!(witness.accesses[0].operation, CELLSCRIPT_SCHEDULER_OP_CREATE);
        assert_eq!(witness.accesses[0].source, CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT);
        assert_eq!(witness.accesses[0].index, 0);
        assert_eq!(witness.accesses[0].conflict_hash, compute_conflict_hash(&type_script, &[0xA1; 32]));
        assert_eq!(witness.accesses[0].typed_data_hash, compute_typed_data_hash(&type_script, &data));
    }

    #[test]
    fn attach_typed_cell_scheduler_witness_uses_resolved_input_sidecar() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-script-args".to_vec());
        let mut data = vec![0xB2; 32];
        data.extend_from_slice(b"invoice-state:funded:amount=900000");
        let mut tx = CellTx::new(vec![CellInput::new(OutPoint::new([0x90; 32], 0), 0)], vec![], vec![], vec![], vec![]).unwrap();
        let plan = typed_cell_scheduler_plan("consume", "Input", 0);

        let summary = attach_cellscript_typed_cell_scheduler_witness(
            &mut tx,
            &plan,
            &[CellScriptTypedCellResolvedCell::input(0, type_script.clone(), data.clone())],
        )
        .unwrap();

        assert_eq!(summary.effect_class, CELLSCRIPT_SCHEDULER_EFFECT_MUTATING);
        assert_eq!(summary.accesses.len(), 1);
        assert_eq!(summary.accesses[0].operation, CELLSCRIPT_SCHEDULER_OP_CONSUME);
        assert_eq!(summary.accesses[0].source, CELLSCRIPT_SCHEDULER_SOURCE_INPUT);
        assert_eq!(summary.accesses[0].conflict_hash, compute_conflict_hash(&type_script, &[0xB2; 32]));
        assert_eq!(summary.accesses[0].typed_data_hash, compute_typed_data_hash(&type_script, &data));
        assert_eq!(tx.witnesses.len(), 1);
    }

    #[test]
    fn build_typed_cell_scheduler_witness_uses_resolved_cell_dep_sidecar() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-script-args".to_vec());
        let mut data = vec![0xC3; 32];
        data.extend_from_slice(b"invoice-state:read-only");
        let tx = CellTx::new(
            vec![],
            vec![CellDep { out_point: OutPoint::new([0x77; 32], 1), dep_type: DepType::Code }],
            vec![],
            vec![],
            vec![],
        )
        .unwrap();
        let plan = typed_cell_scheduler_plan("read_ref", "CellDep", 0);

        let witness_bytes = build_cellscript_typed_cell_scheduler_witness_for_tx(
            &tx,
            &plan,
            &[CellScriptTypedCellResolvedCell::cell_dep(0, type_script.clone(), data.clone())],
        )
        .unwrap();
        let witness = decode_cellscript_scheduler_witness(&witness_bytes).unwrap();

        assert_eq!(witness.accesses[0].operation, CELLSCRIPT_SCHEDULER_OP_READ_REF);
        assert_eq!(witness.accesses[0].source, CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP);
        assert_eq!(witness.accesses[0].conflict_hash, compute_conflict_hash(&type_script, &[0xC3; 32]));
        assert_eq!(witness.accesses[0].typed_data_hash, compute_typed_data_hash(&type_script, &data));
    }

    #[test]
    fn build_typed_cell_scheduler_witness_rejects_missing_input_sidecar() {
        let tx = CellTx::new(vec![CellInput::new(OutPoint::new([0x90; 32], 0), 0)], vec![], vec![], vec![], vec![]).unwrap();
        let plan = typed_cell_scheduler_plan("consume", "Input", 0);

        let err = build_cellscript_typed_cell_scheduler_witness_for_tx(&tx, &plan, &[]).unwrap_err();

        assert!(err.to_string().contains("requires resolved type script and data sidecar"), "unexpected error: {err}");
    }

    #[test]
    fn build_typed_cell_scheduler_witness_rejects_short_output_data() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-script-args".to_vec());
        let tx = CellTx::new(
            vec![],
            vec![],
            vec![CellOutput { lock: Script::new([0x51; 32], 0, vec![]), type_: Some(type_script), capacity: 1000 }],
            vec![vec![0xA1; 31]],
            vec![],
        )
        .unwrap();
        let plan = typed_cell_scheduler_plan("create", "Output", 0);

        let err = build_cellscript_typed_cell_scheduler_witness_for_tx(&tx, &plan, &[]).unwrap_err();

        assert!(err.to_string().contains("needs bytes [0..32)"), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn resolve_typed_cell_scheduler_sidecars_from_rpc_uses_input_and_cell_dep_outpoints() {
        let rpc_core = Arc::new(crate::tests::RpcCoreMock::new());
        let rpc: Arc<DynRpcApi> = rpc_core.clone();
        let input_type_script = Script::new([0x42; 32], 1, b"invoice-input-script-args".to_vec());
        let dep_type_script = Script::new([0x43; 32], 1, b"invoice-dep-script-args".to_vec());
        let mut input_data = vec![0x11; 32];
        input_data.extend_from_slice(b"invoice-state:funded");
        let mut dep_data = vec![0x22; 32];
        dep_data.extend_from_slice(b"invoice-state:registry");
        rpc_core.insert_transaction(
            TransactionId::from_bytes([0xA1; 32]),
            rpc_transaction_with_typed_output(0, input_type_script.clone(), input_data.clone()),
        );
        rpc_core.insert_transaction(
            TransactionId::from_bytes([0xB2; 32]),
            rpc_transaction_with_typed_output(1, dep_type_script.clone(), dep_data.clone()),
        );
        let tx = CellTx::new(
            vec![CellInput::new(OutPoint::new([0xA1; 32], 0), 0)],
            vec![CellDep { out_point: OutPoint::new([0xB2; 32], 1), dep_type: DepType::Code }],
            vec![],
            vec![],
            vec![],
        )
        .unwrap();
        let mut plan = typed_cell_scheduler_plan("consume", "Input", 0);
        plan.accesses.push(typed_cell_scheduler_access("read_ref", "CellDep", 0, "invoice_registry"));

        let resolved = resolve_cellscript_typed_cell_resolved_cells_from_rpc(&rpc, &tx, &plan).await.unwrap();

        assert_eq!(
            resolved,
            vec![
                CellScriptTypedCellResolvedCell::input(0, input_type_script, input_data),
                CellScriptTypedCellResolvedCell::cell_dep(0, dep_type_script, dep_data),
            ]
        );
    }

    #[test]
    fn generator_attaches_live_typed_cell_scheduler_witness_from_metadata_plan() {
        let source_address = Address::new_std_single(Prefix::Testnet, &[0x21; 32]).unwrap();
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x22; 32]).unwrap();
        let recipient = Address::new_std_single(Prefix::Testnet, &[0x23; 32]).unwrap();
        let cell = CellEntryReference::simulated_with_address(20_000_000_000, &source_address);
        let outputs = PaymentOutputs { outputs: vec![PaymentOutput::new(recipient, 5_000_000_000)] };
        let stale_compiled_witness = typed_cell_input_scheduler_witness_with_hash([0xEE; 32]);
        let metadata = typed_cell_input_action_metadata_json(&bytes_to_hex(&stale_compiled_witness));
        let type_script = Script::new([0x42; 32], 1, b"invoice-script-args".to_vec());
        let mut data = vec![0xD4; 32];
        data.extend_from_slice(b"invoice-state:settle-input");
        let settings = GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(vec![cell].into_iter()),
            None,
            change_address,
            1,
            PaymentDestination::PaymentOutputs(outputs),
            None,
            Fees::SenderPays(0),
            None,
            None,
        )
        .unwrap()
        .with_cellscript_action_metadata_json(&metadata, "settle_invoice")
        .unwrap()
        .with_cellscript_typed_cell_resolved_cell(CellScriptTypedCellResolvedCell::input(
            0,
            type_script.clone(),
            data.clone(),
        ));
        let generator = Generator::try_new(settings, None, None).unwrap();

        let pending = generator.generate_transaction().unwrap().expect("final transaction");
        let tx = pending.transaction();
        let witness = decode_cellscript_scheduler_witness(tx.witnesses.last().expect("scheduler witness")).unwrap();

        assert_eq!(witness.effect_class, CELLSCRIPT_SCHEDULER_EFFECT_MUTATING);
        assert_eq!(witness.estimated_cycles, 500);
        assert_eq!(witness.access_count, 1);
        assert_eq!(witness.accesses[0].operation, CELLSCRIPT_SCHEDULER_OP_CONSUME);
        assert_eq!(witness.accesses[0].source, CELLSCRIPT_SCHEDULER_SOURCE_INPUT);
        assert_eq!(witness.accesses[0].conflict_hash, compute_conflict_hash(&type_script, &[0xD4; 32]));
        assert_eq!(witness.accesses[0].typed_data_hash, compute_typed_data_hash(&type_script, &data));
        assert_ne!(witness.accesses[0].conflict_hash, [0xEE; 32]);
    }

    #[test]
    fn generator_rejects_typed_cell_plan_without_resolved_input_sidecar() {
        let stale_compiled_witness = typed_cell_input_scheduler_witness_with_hash([0xEE; 32]);
        let metadata = typed_cell_input_action_metadata_json(&bytes_to_hex(&stale_compiled_witness));
        let settings = typed_cell_generator_settings(0x24).with_cellscript_action_metadata_json(&metadata, "settle_invoice").unwrap();

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("typed-cell scheduler plan without resolved input sidecar should fail at generator init"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("requires resolved type script and data sidecar"), "unexpected error: {err}");
    }

    #[test]
    fn generator_attaches_live_typed_cell_scheduler_witness_from_typed_output_config() {
        let source_address = Address::new_std_single(Prefix::Testnet, &[0x27; 32]).unwrap();
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x28; 32]).unwrap();
        let recipient = Address::new_std_single(Prefix::Testnet, &[0x29; 32]).unwrap();
        let cell = CellEntryReference::simulated_with_address(20_000_000_000, &source_address);
        let outputs = PaymentOutputs { outputs: vec![PaymentOutput::new(recipient, 5_000_000_000)] };
        let type_script = Script::new([0x42; 32], 1, b"invoice-output-type".to_vec());
        let mut data = vec![0xE5; 32];
        data.extend_from_slice(b"invoice-state:created-output");
        let settings = GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(vec![cell].into_iter()),
            None,
            change_address,
            1,
            PaymentDestination::PaymentOutputs(outputs),
            None,
            Fees::SenderPays(0),
            None,
            None,
        )
        .unwrap()
        .with_cellscript_typed_cell_scheduler_plan(typed_cell_scheduler_plan("create", "Output", 0))
        .unwrap()
        .with_cellscript_typed_cell_output(CellScriptTypedCellOutput::new(0, type_script.clone(), data.clone()));
        let generator = Generator::try_new(settings, None, None).unwrap();

        let pending = generator.generate_transaction().unwrap().expect("final transaction");
        let tx = pending.transaction();
        let witness = decode_cellscript_scheduler_witness(tx.witnesses.last().expect("scheduler witness")).unwrap();

        assert_eq!(tx.outputs[0].type_.as_ref(), Some(&type_script));
        assert_eq!(tx.outputs_data[0], data);
        assert_eq!(witness.access_count, 1);
        assert_eq!(witness.accesses[0].operation, CELLSCRIPT_SCHEDULER_OP_CREATE);
        assert_eq!(witness.accesses[0].source, CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT);
        assert_eq!(witness.accesses[0].conflict_hash, compute_conflict_hash(&type_script, &[0xE5; 32]));
        assert_eq!(witness.accesses[0].typed_data_hash, compute_typed_data_hash(&type_script, &data));
    }

    #[test]
    fn generator_rejects_output_plan_without_typed_output_config() {
        let settings = typed_cell_generator_settings(0x2A)
            .with_cellscript_typed_cell_scheduler_plan(typed_cell_scheduler_plan("create", "Output", 0))
            .unwrap();

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("typed-cell output scheduler plan without output type/data should fail at generator init"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("Output#0 requires typed output type script and data"), "unexpected error: {err}");
    }

    #[test]
    fn generator_rejects_typed_cell_output_without_scheduler_plan() {
        let settings = typed_cell_generator_settings(0x30).with_cellscript_typed_cell_output(CellScriptTypedCellOutput::new(
            0,
            Script::new([0x42; 32], 1, b"invoice-output-type".to_vec()),
            vec![0xF0; 32],
        ));

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("typed-cell output configuration without scheduler plan should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("requires a typed-cell scheduler plan"), "unexpected error: {err}");
    }

    #[test]
    fn generator_rejects_typed_cell_output_with_short_conflict_key_data() {
        let settings = typed_cell_generator_settings(0x32)
            .with_cellscript_typed_cell_scheduler_plan(typed_cell_scheduler_plan("create", "Output", 0))
            .unwrap()
            .with_cellscript_typed_cell_output(CellScriptTypedCellOutput::new(
                0,
                Script::new([0x42; 32], 1, b"invoice-output-type".to_vec()),
                vec![0xF1; 31],
            ));

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("typed-cell output with short conflict key data should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("needs bytes [0..32)"), "unexpected error: {err}");
    }

    #[test]
    fn generator_rejects_duplicate_typed_cell_resolved_sidecars() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-input-type".to_vec());
        let settings = typed_cell_generator_settings(0x33)
            .with_cellscript_typed_cell_scheduler_plan(typed_cell_scheduler_plan("consume", "Input", 0))
            .unwrap()
            .with_cellscript_typed_cell_resolved_cells(vec![
                CellScriptTypedCellResolvedCell::input(0, type_script.clone(), vec![0xF1; 32]),
                CellScriptTypedCellResolvedCell::input(0, type_script, vec![0xF2; 32]),
            ]);

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("duplicate typed-cell sidecars should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("duplicate CellScript typed-cell resolved sidecar Input#0"), "unexpected error: {err}");
    }

    #[test]
    fn generator_rejects_typed_cell_resolved_sidecar_with_short_conflict_key_data() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-input-type".to_vec());
        let settings = typed_cell_generator_settings(0x35)
            .with_cellscript_typed_cell_scheduler_plan(typed_cell_scheduler_plan("consume", "Input", 0))
            .unwrap()
            .with_cellscript_typed_cell_resolved_cell(CellScriptTypedCellResolvedCell::input(0, type_script, vec![0xF2; 31]));

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("typed-cell sidecar with short conflict key data should fail"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("needs bytes [0..32)"), "unexpected error: {err}");
    }

    #[test]
    fn generator_rejects_unreferenced_typed_cell_output_config() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-input-type".to_vec());
        let settings = typed_cell_generator_settings(0x36)
            .with_cellscript_typed_cell_scheduler_plan(typed_cell_scheduler_plan("consume", "Input", 0))
            .unwrap()
            .with_cellscript_typed_cell_resolved_cell(CellScriptTypedCellResolvedCell::input(0, type_script.clone(), vec![0xF3; 32]))
            .with_cellscript_typed_cell_output(CellScriptTypedCellOutput::new(0, type_script, vec![0xF4; 32]));

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("unreferenced typed-cell output configuration should fail"),
            Err(err) => err,
        };

        assert!(
            err.to_string().contains("CellScript typed-cell output index 0 is not referenced by the typed-cell scheduler plan"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn generator_rejects_unreferenced_typed_cell_resolved_sidecar() {
        let type_script = Script::new([0x42; 32], 1, b"invoice-output-type".to_vec());
        let settings = typed_cell_generator_settings(0x39)
            .with_cellscript_typed_cell_scheduler_plan(typed_cell_scheduler_plan("create", "Output", 0))
            .unwrap()
            .with_cellscript_typed_cell_output(CellScriptTypedCellOutput::new(0, type_script.clone(), vec![0xF5; 32]))
            .with_cellscript_typed_cell_resolved_cell(CellScriptTypedCellResolvedCell::input(0, type_script, vec![0xF6; 32]));

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("unreferenced typed-cell sidecar should fail"),
            Err(err) => err,
        };

        assert!(
            err.to_string()
                .contains("CellScript typed-cell resolved sidecar Input#0 is not referenced by the typed-cell scheduler plan"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn generator_rejects_typed_cell_output_ckb_type_id_overlap() {
        let source_address = Address::new_std_single(Prefix::Testnet, &[0x2D; 32]).unwrap();
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x2E; 32]).unwrap();
        let recipient = Address::new_std_single(Prefix::Testnet, &[0x2F; 32]).unwrap();
        let cell = CellEntryReference::simulated_with_address(20_000_000_000, &source_address);
        let outputs = PaymentOutputs { outputs: vec![PaymentOutput::new(recipient, 5_000_000_000)] };
        let settings = GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(vec![cell].into_iter()),
            None,
            change_address,
            1,
            PaymentDestination::PaymentOutputs(outputs),
            None,
            Fees::SenderPays(0),
            None,
            None,
        )
        .unwrap()
        .with_ckb_type_id_output_index(0)
        .with_cellscript_typed_cell_output(CellScriptTypedCellOutput::new(
            0,
            Script::new([0x42; 32], 1, b"invoice-output-type".to_vec()),
            vec![0xE6; 32],
        ));

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("typed-cell output must not overlap CKB TYPE_ID output configuration"),
            Err(err) => err,
        };

        assert!(err.to_string().contains("conflicts with CKB TYPE_ID output configuration"), "unexpected error: {err}");
    }

    #[test]
    fn generator_settings_cell_and_header_deps_are_included_in_unsigned_transactions() {
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x11; 32]).unwrap();
        let dep_group_out_point = OutPoint::new([0x42; 32], 7);
        let ckb_lock_config = CkbSecp256k1Blake160SighashAllLockConfig::with_dep_group([0x51; 32], dep_group_out_point);
        let expected_dep = CellDep { out_point: dep_group_out_point, dep_type: DepType::DepGroup };
        let header_dep = [0x62; 32];
        let settings = GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(std::iter::empty()),
            None,
            change_address.clone(),
            1,
            PaymentDestination::Change,
            None,
            Fees::None,
            None,
            None,
        )
        .unwrap()
        .with_ckb_secp256k1_blake160_sighash_all_lock_dep(&ckb_lock_config)
        .with_header_dep(header_dep);
        let generator = Generator::try_new(settings, None, None).unwrap();

        let tx = generator.build_unsigned_cell_transaction(vec![], vec![PaymentOutput::new(change_address, 1000)], vec![]).unwrap();

        assert_eq!(tx.cell_deps, vec![expected_dep]);
        assert_eq!(tx.header_deps, vec![header_dep]);
    }

    #[test]
    fn generator_installs_ckb_type_id_scripts_on_configured_final_outputs() {
        let source_address = Address::new_std_single(Prefix::Testnet, &[0x12; 32]).unwrap();
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x13; 32]).unwrap();
        let recipient = Address::new_std_single(Prefix::Testnet, &[0x14; 32]).unwrap();
        let cell = CellEntryReference::simulated_with_address(20_000_000_000, &source_address);
        let outputs = PaymentOutputs { outputs: vec![PaymentOutput::new(recipient, 5_000_000_000)] };
        let settings = GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(vec![cell].into_iter()),
            None,
            change_address,
            1,
            PaymentDestination::PaymentOutputs(outputs),
            None,
            Fees::SenderPays(0),
            None,
            None,
        )
        .unwrap()
        .with_ckb_type_id_output_index(0);
        let generator = Generator::try_new(settings, None, None).unwrap();

        let pending = generator.generate_transaction().unwrap().expect("final transaction");
        let tx = pending.transaction();
        let type_script = tx.outputs[0].type_.as_ref().expect("CKB TYPE_ID type script");

        assert_eq!(type_script.code_hash, CKB_TYPE_ID_CODE_HASH);
        assert_eq!(type_script.hash_type, CKB_SCRIPT_HASH_TYPE_TYPE);
        assert_eq!(type_script.args, ckb_type_id_args(&tx.inputs[0], 0).unwrap().to_vec());
    }

    #[test]
    fn generator_rejects_invalid_ckb_type_id_output_indexes() {
        let change_address = Address::new_std_single(Prefix::Testnet, &[0x15; 32]).unwrap();
        let recipient = Address::new_std_single(Prefix::Testnet, &[0x16; 32]).unwrap();
        let outputs = PaymentOutputs { outputs: vec![PaymentOutput::new(recipient, 5_000_000_000)] };
        let settings = GeneratorSettings::try_new_with_iterator(
            NetworkId::with_suffix(NetworkType::Testnet, 10),
            Box::new(std::iter::empty()),
            None,
            change_address,
            1,
            PaymentDestination::PaymentOutputs(outputs),
            None,
            Fees::SenderPays(0),
            None,
            None,
        )
        .unwrap()
        .with_ckb_type_id_output_indexes(vec![0, 0]);

        let err = match Generator::try_new(settings, None, None) {
            Ok(_) => panic!("duplicate CKB TYPE_ID output index must be rejected"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("duplicate CKB TYPE_ID output index"), "unexpected error: {err}");
    }
}
