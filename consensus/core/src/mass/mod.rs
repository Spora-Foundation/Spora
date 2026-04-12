use crate::{
    cell_metadata::CellMetadata,
    config::params::Params,
    constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
    subnets::SUBNETWORK_ID_SIZE,
    tx::{CellEntry, CellOut, CellTx, ScriptPublicKey, Transaction, TransactionInput, TransactionOutput, VerifiableTransaction},
};
use spora_hashes::HASH_SIZE;

const LEGACY_CELL_CONST_STORAGE: u64 =
    32  // outpoint::tx_id
    + 4 // outpoint::index
    + 8 // entry amount
    + 8 // entry DAA score
    + 1 // entry is coinbase
    + 2 // entry spk version
    + 8 // entry spk len
;

const CANONICAL_CELL_CONST_STORAGE: u64 =
    32  // outpoint::tx_id
    + 4 // outpoint::index
    + 8 // entry capacity
    + 8 // entry data len
    + 32 // lock hash
    + 1 // type flag
    + 32 // data hash
    + 8 // entry DAA score
    + 1 // entry is coinbase
;

const CELL_UNIT_SIZE: u64 = 100;
const CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY: u64 =
    32  // outpoint::tx_id
    + 4 // outpoint::index
    + 8 // entry DAA score
    + 1 // entry is coinbase
;

/// Estimated serialized size of a CellTx transaction.
/// Deterministic but not necessarily accurate — only used as the size
/// component in the transaction and block mass limit calculation.
pub fn cell_tx_estimated_serialized_size(tx: &CellTx) -> u64 {
    let mut size: u64 = 0;
    size += 2; // ver (u16)

    // Inputs: each CellRef = outpoint (32+4) + since (8) = 44 bytes
    size += 8; // number of inputs
    size += tx.inputs.len() as u64 * 44;

    // Deps: each CellDep = outpoint (32+4) + dep_type (1) = 37 bytes
    size += 8; // number of deps
    size += tx.deps.len() as u64 * 37;

    // Header deps: each is a 32-byte hash
    size += 8; // number of header_deps
    size += tx.header_deps.len() as u64 * 32;

    // Outputs: each CellOut = lock script + optional type script + capacity
    size += 8; // number of outputs
    for output in &tx.outputs {
        size += 32 + 1 + 8; // lock.code_hash + lock.hash_type + len(lock.args)
        size += output.lock.args.len() as u64;
        if let Some(ref type_script) = output.type_ {
            size += 1 + 32 + 1 + 8; // flag + code_hash + hash_type + len(args)
            size += type_script.args.len() as u64;
        } else {
            size += 1; // no-type flag
        }
        size += 8; // capacity
    }

    // Outputs data
    size += 8; // number of outputs_data
    for data in &tx.outputs_data {
        size += 8; // length prefix
        size += data.len() as u64;
    }

    // Witnesses
    size += 8; // number of witnesses
    for witness in &tx.witnesses {
        size += 8; // length prefix
        size += witness.len() as u64;
    }

    size
}

// transaction_estimated_serialized_size is the estimated size of a legacy transaction in some
// serialization. This has to be deterministic, but not necessarily accurate, since
// it's only used as the size component in the transaction and block mass limit
// calculation.
pub fn transaction_estimated_serialized_size(tx: &Transaction) -> u64 {
    let mut size: u64 = 0;
    size += 2; // Tx version (u16)
    size += 8; // Number of inputs (u64)
    let inputs_size: u64 = tx.inputs.iter().map(transaction_input_estimated_serialized_size).sum();
    size += inputs_size;

    size += 8; // number of outputs (u64)
    let outputs_size: u64 = tx.outputs.iter().map(transaction_output_estimated_serialized_size).sum();
    size += outputs_size;

    size += 8; // lock time (u64)
    size += SUBNETWORK_ID_SIZE as u64;
    size += 8; // gas (u64)
    size += HASH_SIZE as u64; // payload hash

    size += 8; // length of the payload (u64)
    size += tx.payload.len() as u64;
    size
}

fn transaction_input_estimated_serialized_size(input: &TransactionInput) -> u64 {
    let mut size = 0;
    size += outpoint_estimated_serialized_size();
    size += 8; // length of signature script (u64)
    size += input.signature_script.len() as u64;
    size += 8; // sequence (u64)
    size
}

const fn outpoint_estimated_serialized_size() -> u64 {
    let mut size: u64 = 0;
    size += HASH_SIZE as u64; // previous tx id
    size += 4; // index (u32)
    size
}

pub fn transaction_output_estimated_serialized_size(output: &TransactionOutput) -> u64 {
    let mut size: u64 = 0;
    size += 8; // value (u64)
    size += 2; // output.ScriptPublicKey.Version (u16)
    size += 8; // length of script public key (u64)
    size += output.script_public_key.script().len() as u64;
    size
}

/// Returns the cell storage plurality for this script public key.
/// i.e., how many 100-byte "storage units" it occupies.
/// The choice of 100 bytes per unit ensures that all standard SPKs have a plurality of 1.
pub fn cell_plurality(spk: &ScriptPublicKey) -> u64 {
    // The base (63 bytes) plus the max standard public key length (33 bytes) fits into one 100-byte unit.
    // Hence, all standard SPKs end up with a plurality of 1.
    // Using CANONICAL_CELL_CONST_STORAGE (126 bytes) as the base for cell model compatibility
    (CANONICAL_CELL_CONST_STORAGE + spk.script().len() as u64).div_ceil(CELL_UNIT_SIZE)
}

fn canonical_cell_storage_bytes(entry: &CellEntry) -> Option<u64> {
    entry
        .embedded_cell_metadata()
        .map(|metadata| CANONICAL_CELL_CONST_STORAGE + u64::from(metadata.type_hash.is_some()) * 32 + metadata.data_bytes)
}

fn canonical_cell_metadata_storage_bytes(metadata: &CellMetadata) -> u64 {
    CANONICAL_CELL_CONST_STORAGE + u64::from(metadata.type_hash.is_some()) * 32 + metadata.data_bytes
}

fn canonical_output_storage_bytes(output: &TransactionOutput) -> Option<u64> {
    crate::cell_metadata::parse_cell_metadata_placeholder_script_public_key(&output.script_public_key)
        .map(|metadata| CANONICAL_CELL_CONST_STORAGE + u64::from(metadata.type_hash.is_some()) * 32 + metadata.data_bytes)
}



pub fn cell_entry_plurality(entry: &CellEntry) -> u64 {
    // CellMeta always carries canonical cell metadata
    canonical_cell_storage_bytes(entry).expect("CellMeta always has embedded cell metadata").div_ceil(CELL_UNIT_SIZE)
}

pub fn cell_out_storage_bytes(output: &CellOut, data_len: usize) -> u64 {
    CELL_ENTRY_OVERHEAD_EXCLUDING_OUTPUT_BODY + output.occupied_capacity(data_len)
}

pub fn cell_out_plurality(output: &CellOut, data_len: usize) -> u64 {
    cell_out_storage_bytes(output, data_len).div_ceil(CELL_UNIT_SIZE)
}

pub trait CellPlurality {
    /// Returns the cell storage plurality for the script public key associated with this object.
    fn plurality(&self) -> u64;
}

impl CellPlurality for ScriptPublicKey {
    fn plurality(&self) -> u64 {
        cell_plurality(self)
    }
}

impl CellPlurality for CellEntry {
    fn plurality(&self) -> u64 {
        cell_entry_plurality(self)
    }
}

impl CellPlurality for TransactionOutput {
    fn plurality(&self) -> u64 {
        canonical_output_storage_bytes(self)
            .unwrap_or_else(|| LEGACY_CELL_CONST_STORAGE + self.script_public_key.script().len() as u64)
            .div_ceil(CELL_UNIT_SIZE)
    }
}





/// An abstract storage cell.
///
/// # Plurality
///
/// Each `CellMass` now has a `plurality` field reflecting how many 100-byte "storage units"
/// this cell effectively occupies. This generalizes KIP-0009 to support cell entries with
/// script public keys larger than the standard 33-byte limit. For a cell of byte-size
/// `entry.size`, we define:
///
/// ```ignore
/// p := ceil(entry.size / CELL_UNIT)
/// ```
///
/// Conceptually, we treat a large cell as `p` sub-entries each holding `entry.amount / p`,
/// preserving the total locked amount but increasing the "count" proportionally to script size.
///
/// Refer to the KIP-0009 specification for more details.
#[derive(Clone, Copy)]
pub struct CellMass {
    /// The plurality (number of "storage units") for this cell
    pub plurality: u64,
    /// The amount of SPORA (in saus) locked in this cell
    pub amount: u64,
}

impl CellMass {
    pub fn new(plurality: u64, amount: u64) -> Self {
        Self { plurality, amount }
    }
}

impl From<&CellEntry> for CellMass {
    fn from(entry: &CellEntry) -> Self {
        Self::new(entry.plurality(), entry.capacity())
    }
}

impl From<&CellMetadata> for CellMass {
    fn from(metadata: &CellMetadata) -> Self {
        Self::new(canonical_cell_metadata_storage_bytes(metadata).div_ceil(CELL_UNIT_SIZE), metadata.capacity)
    }
}

impl From<&TransactionOutput> for CellMass {
    fn from(output: &TransactionOutput) -> Self {
        Self::new(output.plurality(), output.value)
    }
}



impl From<(&CellOut, usize)> for CellMass {
    fn from((output, data_len): (&CellOut, usize)) -> Self {
        Self::new(cell_out_plurality(output, data_len), output.capacity)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct NonContextualMasses {
    /// Compute mass
    pub compute_mass: u64,

    /// Transient storage mass
    pub transient_mass: u64,
}

impl NonContextualMasses {
    pub fn new(compute_mass: u64, transient_mass: u64) -> Self {
        Self { compute_mass, transient_mass }
    }

    /// Returns the maximum over all non-contextual masses (currently compute and transient). This
    /// max value has no consensus meaning and should only be used for mempool-level simplification
    /// such as obtaining a one-dimensional mass value when composing blocks templates.  
    pub fn max(&self) -> u64 {
        self.compute_mass.max(self.transient_mass)
    }
}

impl std::fmt::Display for NonContextualMasses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "compute: {}, transient: {}", self.compute_mass, self.transient_mass)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ContextualMasses {
    /// Persistent storage mass
    pub storage_mass: u64,
}

impl ContextualMasses {
    pub fn new(storage_mass: u64) -> Self {
        Self { storage_mass }
    }

    /// Returns the maximum over *all masses* (currently compute, transient and storage). This max
    /// value has no consensus meaning and should only be used for mempool-level simplification such
    /// as obtaining a one-dimensional mass value when composing blocks templates.  
    pub fn max(&self, non_contextual_masses: NonContextualMasses) -> u64 {
        self.storage_mass.max(non_contextual_masses.max())
    }
}

impl std::fmt::Display for ContextualMasses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "storage: {}", self.storage_mass)
    }
}

impl std::cmp::PartialEq<u64> for ContextualMasses {
    fn eq(&self, other: &u64) -> bool {
        self.storage_mass.eq(other)
    }
}

pub type Mass = (NonContextualMasses, ContextualMasses);

pub trait MassOps {
    fn max(&self) -> u64;
}

impl MassOps for Mass {
    fn max(&self) -> u64 {
        self.1.max(self.0)
    }
}

// Note: consensus mass calculator operates on signed transactions.
// To calculate mass for unsigned transactions, please use
// `spora_wallet_core::tx::mass::MassCalculator`
#[derive(Clone)]
pub struct MassCalculator {
    mass_per_tx_byte: u64,
    mass_per_script_pub_key_byte: u64,
    mass_per_sig_op: u64,
    storage_mass_parameter: u64,
}

impl MassCalculator {
    pub fn new(mass_per_tx_byte: u64, mass_per_script_pub_key_byte: u64, mass_per_sig_op: u64, storage_mass_parameter: u64) -> Self {
        Self { mass_per_tx_byte, mass_per_script_pub_key_byte, mass_per_sig_op, storage_mass_parameter }
    }

    pub fn new_with_consensus_params(consensus_params: &Params) -> Self {
        Self {
            mass_per_tx_byte: consensus_params.mass_per_tx_byte,
            mass_per_script_pub_key_byte: consensus_params.mass_per_script_pub_key_byte,
            mass_per_sig_op: consensus_params.mass_per_sig_op,
            storage_mass_parameter: consensus_params.storage_mass_parameter,
        }
    }

    /// Calculates non-contextual masses for a CellTx transaction.
    ///
    /// In Cell model:
    /// - Lock script size replaces script_public_key size for compute mass
    /// - Each input implicitly has 1 sigop
    /// - CellTx serialized size includes deps, header_deps, outputs_data, witnesses
    pub fn calc_non_contextual_masses_cell(&self, tx: &CellTx) -> NonContextualMasses {
        if tx.is_coinbase() {
            return NonContextualMasses::new(0, 0);
        }

        let size = cell_tx_estimated_serialized_size(tx);
        let compute_mass_for_size = size * self.mass_per_tx_byte;

        // Lock script mass (replaces script_public_key mass)
        let total_lock_script_size: u64 = tx
            .outputs
            .iter()
            .map(|output| {
                let mut script_size = 32 + 1 + output.lock.args.len() as u64; // code_hash + hash_type + args
                if let Some(ref type_script) = output.type_ {
                    script_size += 32 + 1 + type_script.args.len() as u64;
                }
                script_size
            })
            .sum();
        let total_lock_script_mass = total_lock_script_size * self.mass_per_script_pub_key_byte;

        // Each input implicitly has 1 sigop in Cell model
        let total_sigops = tx.inputs.len() as u64;
        let total_sigops_mass = total_sigops * self.mass_per_sig_op;

        let compute_mass = compute_mass_for_size + total_lock_script_mass + total_sigops_mass;
        let transient_mass = size * TRANSIENT_BYTE_TO_MASS_FACTOR;

        NonContextualMasses::new(compute_mass, transient_mass)
    }

    /// Calculates the contextual masses for this populated transaction.
    /// Assumptions which must be verified before this call:
    ///     1. All output values are non-zero
    ///     2. At least one input (unless coinbase)
    ///
    /// Otherwise this function should never fail.
    pub fn calc_contextual_masses(&self, tx: &(impl VerifiableTransaction + ?Sized)) -> Option<ContextualMasses> {
        let input_masses = tx
            .inputs()
            .iter()
            .enumerate()
            .map(|(index, _)| {
                tx.cell_metadata(index).map(|metadata| CellMass::from(&metadata)).or_else(|| tx.cell_entry(index).map(CellMass::from))
            })
            .collect::<Option<Vec<_>>>()?;

        calc_storage_mass(
            tx.is_coinbase(),
            input_masses.into_iter(),
            tx.outputs().iter().enumerate().map(|(i, out)| {
                let data_len = tx.tx().outputs_data.get(i).map(Vec::len).unwrap_or_default();
                CellMass::from((out, data_len))
            }),
            self.storage_mass_parameter,
        )
        .map(ContextualMasses::new)
    }
}

/// Calculates the storage mass (KIP-0009) for a given set of inputs and outputs.
///
/// This function has been generalized for cell entries that may exceed
/// the max standard 33-byte script public key size. Each `CellMass::plurality` indicates
/// how many 100-byte "storage units" that cell occupies.
///
/// # Formula Overview
///
/// The core formula is:
///
/// ```ignore
///     max(0, C · (|O| / H(O) - |I| / A(I)))
/// ```
///
/// where:
///
/// - `C` is the storage mass parameter (`storm_param`).
/// - `|O|` and `|I|` are the total pluralities of outputs and inputs, respectively.
/// - `H(O)` is the harmonic mean of the outputs' amounts, generalized to account for per-cell
///   `plurality`.
///
///   In standard KIP-0009, one has:
///
///   ```ignore
///   |O| / H(O) = Σ (1 / o)
///   ```
///
///   Here, each cell that occupies `p` storage units is treated as `p` sub-entries,
///   each holding `amount / p`. This effectively converts `1 / o` into `p^2 / amount`.
///   Consequently, the code accumulates:
///
///   ```ignore
///   Σ [C · p(o)^2 / amount(o)]
///   ```
///
/// - `A(I)` is the arithmetic mean of the inputs' amounts, similarly scaled by `|I|`,
///   while the sum of amounts remains unchanged.
///
/// Under the “relaxed formula” conditions (`|O| = 1`, `|I| = 1`, or `|O| = |I| = 2`),
/// we compute the harmonic mean for inputs as well; otherwise, we use the arithmetic
/// approach for inputs.
///
/// Refer to KIP-0009 for more details.
///
/// Assumptions which must be verified before this call:
///   1. All input/output values are non-zero
///   2. At least one input (unless coinbase)
///
/// If these assumptions hold, this function should never fail. A `None` return
/// indicates that the mass is incomputable and can be considered too high.
pub fn calc_storage_mass(
    is_coinbase: bool,
    inputs: impl ExactSizeIterator<Item = CellMass> + Clone,
    mut outputs: impl Iterator<Item = CellMass>,
    storm_param: u64,
) -> Option<u64> {
    if is_coinbase {
        return Some(0);
    }

    /*
        In KIP-0009 terms, the canonical formula is:
            max(0, C * (|O|/H(O) - |I|/A(I))).

        We first calculate the harmonic portion for outputs in a single pass,
        accumulating:
            1) outs_plurality = Σ p(o)
            2) harmonic_outs  = Σ [C * p(o)^2 / amount(o)]
    */
    let (outs_plurality, harmonic_outs) = outputs.try_fold(
        (0u64, 0u64), // (accumulated plurality, accumulated harmonic)
        |(acc_plurality, acc_harm), CellMass { plurality, amount }| {
            Some((
                acc_plurality + plurality, // represents in-memory bytes, cannot overflow
                acc_harm.checked_add(storm_param.checked_mul(plurality)?.checked_mul(plurality)? / amount)?,
            ))
        },
    )?;

    /*
        KIP-0009 defines a relaxed formula for the cases:
            |O| = 1  or  |O| <= |I| <= 2

        The relaxed formula is:
            max(0, C · (|O| / H(O) - |I| / H(I)))

        If |I| = 1, the harmonic and arithmetic approaches coincide, so the conditions can be expressed as:
            |O| = 1 or |I| = 1 or |O| = |I| = 2
    */
    let relaxed_formula_path = {
        if outs_plurality == 1 {
            true // |O| = 1
        } else if inputs.len() > 2 {
            false // since element plurality always >= 1 => ins_plurality > 2 => skip harmonic path
        } else {
            // For <= 2 inputs, we can afford to clone and sum the pluralities
            let ins_plurality = inputs.clone().map(|cell| cell.plurality).sum::<u64>();
            ins_plurality == 1 || (outs_plurality == 2 && ins_plurality == 2)
        }
    };

    if relaxed_formula_path {
        // Each input i contributes C · p(i)^2 / amount(i)
        let harmonic_ins = inputs
            .map(|CellMass { plurality, amount }| storm_param * plurality * plurality / amount) // we assume no overflow (see verify_cell_plurality_limits)
            .fold(0u64, |total, current| total.saturating_add(current));

        // max(0, harmonic_outs - harmonic_ins)
        return Some(harmonic_outs.saturating_sub(harmonic_ins));
    }

    // Otherwise, we calculate the arithmetic portion for inputs:
    // (ins_plurality, sum_ins) =>  (Σ plurality, Σ amounts)
    let (ins_plurality, sum_ins) =
        inputs.fold((0u64, 0u64), |(acc_plur, acc_amt), CellMass { plurality, amount }| (acc_plur + plurality, acc_amt + amount));

    // mean_ins = (Σ amounts) / (Σ plurality)
    let mean_ins = sum_ins / ins_plurality;

    // arithmetic_ins:  C · (|I| / A(I)) = |I| · (C / mean_ins)
    let arithmetic_ins = ins_plurality.saturating_mul(storm_param / mean_ins);

    // max(0, harmonic_outs - arithmetic_ins)
    Some(harmonic_outs.saturating_sub(arithmetic_ins))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        constants::{SAU_PER_SPORA, STORAGE_MASS_PARAMETER},
        network::NetworkType,
        tx::*,
    };
    use std::str::FromStr;

    #[test]
    fn verify_cell_plurality_limits() {
        /*
           Verify that for all networks, existing cell entries can never overflow the product C·P^2 used
           for harmonic_ins within calc_storage_mass
        */
        for net in NetworkType::iter() {
            let params: Params = net.into();
            let max_spk_len = (params.max_script_public_key_len() as u64)
                .min(params.max_block_mass.div_ceil(params.mass_per_script_pub_key_byte));
            let max_plurality = (LEGACY_CELL_CONST_STORAGE + max_spk_len).div_ceil(CELL_UNIT_SIZE); // see cell_plurality
            let product = params.storage_mass_parameter.checked_mul(max_plurality).and_then(|x| x.checked_mul(max_plurality));
            // verify C·P^2 can never overflow
            assert!(product.is_some());
        }

        // verify P >= 1 also when the script is empty
        assert!(cell_plurality(&ScriptPublicKey::new(0, ScriptVec::from_slice(&[]))) == 1);
        // Assert the CANONICAL_CELL_CONST_STORAGE=126, CELL_UNIT_SIZE=100 constants
        // Note: With canonical storage (126 bytes), even empty script gives plurality = 2 (ceil(126/100))
        assert!(cell_plurality(&ScriptPublicKey::from_vec(0, vec![1; (CELL_UNIT_SIZE * 2 - CANONICAL_CELL_CONST_STORAGE) as usize])) == 2);
        assert!(
            cell_plurality(&ScriptPublicKey::from_vec(0, vec![1; (CELL_UNIT_SIZE * 2 - CANONICAL_CELL_CONST_STORAGE + 1) as usize])) == 3
        );
    }

    #[derive(Debug)]
    struct PluralityTestCase {
        /// Test name
        name: &'static str,

        /// Amounts for the first transaction's inputs
        inputs_tx1: &'static [u64],
        /// Amounts for the first transaction's outputs
        outputs_tx1: &'static [u64],

        /// Amounts for the second transaction's inputs
        inputs_tx2: &'static [u64],
        /// Amounts for the second transaction's outputs
        outputs_tx2: &'static [u64],

        /// (Optional) index of the input/output in tx2 whose script we want to override
        plurality_index: Option<usize>,
        /// Desired plurality for that cell's script
        desired_plurality: Option<u64>,
        /// Whether to override an output and not an input
        override_output: bool,

        /// Mass calculator parameters
        storage_mass_parameter: u64,
    }

    impl PluralityTestCase {
        /// Runs the test and asserts that the masses are equal
        fn run(&self) {
            // Sanity
            assert!(
                self.inputs_tx1.iter().sum::<u64>() >= self.outputs_tx1.iter().sum::<u64>(),
                "Test \"{}\": tx1 outs > ins",
                self.name
            );
            assert!(
                self.inputs_tx2.iter().sum::<u64>() >= self.outputs_tx2.iter().sum::<u64>(),
                "Test \"{}\": tx2 outs > ins",
                self.name
            );

            // Generate
            let tx1 = generate_tx_from_amounts(self.inputs_tx1, self.outputs_tx1);
            let mut tx2 = generate_tx_from_amounts(self.inputs_tx2, self.outputs_tx2);

            // If specified, override plurality in tx2.
            if let (Some(index), Some(plur)) = (self.plurality_index, self.desired_plurality) {
                if self.override_output {
                    // Adjust outputs_data length to achieve desired plurality
                    // cell_out_storage_bytes = 86 + data_len, plurality = ceil((86+data_len)/100)
                    let target_data_len = ((plur - 1) * CELL_UNIT_SIZE).saturating_sub(85) as usize;
                    tx2.tx.outputs_data[index] = vec![0; target_data_len];
                } else {
                    // For CellMeta entries, adjust data_bytes to achieve desired plurality
                    let entry = tx2.entries[index].as_mut().unwrap();
                    let base = CANONICAL_CELL_CONST_STORAGE + u64::from(entry.type_hash.is_some()) * 32;
                    let target = plur * CELL_UNIT_SIZE;
                    entry.data_bytes = target.saturating_sub(base);
                    // Update resolved metadata to match the modified entry
                    tx2.resolved_cell_metadata[index] = Some(CellMetadata::from(&*entry));
                }
            }

            let mc = MassCalculator::new(0, 0, 0, self.storage_mass_parameter);

            let mass1 = mc.calc_contextual_masses(&tx1.as_verifiable());
            let mass2 = mc.calc_contextual_masses(&tx2.as_verifiable());

            assert_ne!(mass1, Some(ContextualMasses::new(0)), "Test \"{}\": avoid running meaningless test cases", self.name);
            assert_eq!(mass1, mass2, "Test \"{}\" failed: mass1 = {:?}, mass2 = {:?}", self.name, mass1, mass2);
        }
    }

    #[test]
    fn test_storage_mass_pluralities() {
        /*
            Tests pluralities by comparing transactions with all inputs/outputs with plurality 1
            with transactions with a super entry (plurality > 1) which replaces several entries
            in the plurality 1 tx (with equal total value and equal total plurality)
        */
        let test_cases = vec![
            PluralityTestCase {
                name: "3:4; input index=1, plurality=4",
                inputs_tx1: &[300, 200, 200],
                outputs_tx1: &[200, 200, 200, 100],
                inputs_tx2: &[300, 400],
                outputs_tx2: &[200, 200, 200, 100],
                plurality_index: Some(1),
                desired_plurality: Some(4),
                override_output: false,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "2:3; output index=1, plurality=4",
                inputs_tx1: &[350, 400],
                outputs_tx1: &[300, 200, 200],
                inputs_tx2: &[350, 400],
                outputs_tx2: &[300, 400],
                plurality_index: Some(1),
                desired_plurality: Some(4),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:2; output index=0, plurality=4",
                inputs_tx1: &[500],
                outputs_tx1: &[200, 200],
                inputs_tx2: &[500],
                outputs_tx2: &[400],
                plurality_index: Some(0),
                desired_plurality: Some(4),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:3; output index=1, plurality=4",
                inputs_tx1: &[1000],
                outputs_tx1: &[200, 200, 200],
                inputs_tx2: &[1000],
                outputs_tx2: &[200, 400],
                plurality_index: Some(1),
                desired_plurality: Some(4),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:3; output index=1, plurality=4; spora units",
                inputs_tx1: &[1000 * SAU_PER_SPORA],
                outputs_tx1: &[200 * SAU_PER_SPORA, 200 * SAU_PER_SPORA, 200 * SAU_PER_SPORA],
                inputs_tx2: &[1000 * SAU_PER_SPORA],
                outputs_tx2: &[200 * SAU_PER_SPORA, 400 * SAU_PER_SPORA],
                plurality_index: Some(1),
                desired_plurality: Some(4),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "1:2; output index=0, plurality=4; spora units",
                inputs_tx1: &[1000 * SAU_PER_SPORA],
                outputs_tx1: &[200 * SAU_PER_SPORA, 200 * SAU_PER_SPORA],
                inputs_tx2: &[1000 * SAU_PER_SPORA],
                outputs_tx2: &[400 * SAU_PER_SPORA],
                plurality_index: Some(0),
                desired_plurality: Some(4),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "2:2; output index=0, plurality=4; spora units",
                inputs_tx1: &[350 * SAU_PER_SPORA, 500 * SAU_PER_SPORA],
                outputs_tx1: &[200 * SAU_PER_SPORA, 200 * SAU_PER_SPORA],
                inputs_tx2: &[350 * SAU_PER_SPORA, 500 * SAU_PER_SPORA],
                outputs_tx2: &[400 * SAU_PER_SPORA],
                plurality_index: Some(0),
                desired_plurality: Some(4),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
            PluralityTestCase {
                name: "4:6; output index=0, plurality=6; spora units",
                inputs_tx1: &[350 * SAU_PER_SPORA, 500 * SAU_PER_SPORA, 350 * SAU_PER_SPORA, 500 * SAU_PER_SPORA],
                outputs_tx1: &[
                    200 * SAU_PER_SPORA,
                    200 * SAU_PER_SPORA,
                    400 * SAU_PER_SPORA,
                    250 * SAU_PER_SPORA,
                    250 * SAU_PER_SPORA,
                    250 * SAU_PER_SPORA,
                ],
                inputs_tx2: &[350 * SAU_PER_SPORA, 500 * SAU_PER_SPORA, 350 * SAU_PER_SPORA, 500 * SAU_PER_SPORA],
                outputs_tx2: &[200 * SAU_PER_SPORA, 200 * SAU_PER_SPORA, 400 * SAU_PER_SPORA, 750 * SAU_PER_SPORA],
                plurality_index: Some(3),
                desired_plurality: Some(6),
                override_output: true,
                storage_mass_parameter: 10_u64.pow(12),
            },
        ];

        for tc in test_cases {
            tc.run();
        }
    }

    /// Calculate the outputs_data entry length needed to achieve
    /// a desired plurality (number of 100-byte storage units) for a CellOut.
    /// cell_out_storage_bytes = CELL_ENTRY_OVERHEAD(45) + occupied_capacity
    /// occupied_capacity = 8 + 33 + data_len = 41 + data_len (standard lock, no type script)
    /// So: cell_out_storage_bytes = 86 + data_len, plurality = ceil((86+data_len)/100)
    fn _data_len_for_plurality(desired_plurality: u64) -> usize {
        ((desired_plurality - 1) * CELL_UNIT_SIZE).saturating_sub(85) as usize
    }

    #[test]
    fn test_storage_mass() {
        // Tx with less outs than ins
        let mut tx = generate_tx_from_amounts(&[100, 200, 300], &[300, 300]);

        //
        // Assert the formula: max( 0 , C·( |O|/H(O) - |I|/A(I) ) )
        //

        let storage_mass = MassCalculator::new(0, 0, 0, 10u64.pow(12)).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 0); // Compounds from 3 to 2, with symmetric outputs and no fee, should be zero

        // Create asymmetry
        tx.tx.outputs[0].capacity = 50;
        tx.tx.outputs[1].capacity = 550;
        let storage_mass_parameter = 10u64.pow(12);
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        // With CellMeta entries (p=2) and canonical outputs (p=2):
        // harmonic_outs = 4C/50 + 4C/550, arithmetic_ins = 6*(C/100)
        assert_eq!(
            storage_mass,
            4 * storage_mass_parameter / 50 + 4 * storage_mass_parameter / 550 - 6 * (storage_mass_parameter / 100)
        );

        // Create a tx with more outs than ins
        let base_value = 10_000 * SAU_PER_SPORA;
        let mut tx = generate_tx_from_amounts(&[base_value, base_value, base_value * 2], &[base_value; 4]);
        let storage_mass_parameter = STORAGE_MASS_PARAMETER;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 10); // With p=2: harmonic_outs=16, arithmetic_ins=6

        let mut tx2 = tx.clone();
        tx2.tx.outputs[0].capacity = 10 * SAU_PER_SPORA;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx2.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 4006);

        // Increase values over the lim
        for out in tx.tx.outputs.iter_mut() {
            out.capacity += 1
        }
        tx.entries[0].as_mut().unwrap().capacity += tx.tx.outputs.len() as u64;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 6); // With p=2 the threshold shifts, so slightly-above-C values still produce non-zero mass

        // Now create 2:2 transaction
        // Assert the formula: max( 0 , C·( |O|/H(O) - |I|/H(I) ) )
        let mut tx = generate_tx_from_amounts(&[100, 200], &[50, 250]);
        let storage_mass_parameter = 10u64.pow(12);

        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        // With p=2, 2:2 tx uses arithmetic path (ins_p=4 != 2), not relaxed harmonic
        assert_eq!(
            storage_mass,
            4 * storage_mass_parameter / 50 + 4 * storage_mass_parameter / 250 - 4 * (storage_mass_parameter / 75)
        );

        // Set outputs to be equal to inputs
        tx.tx.outputs[0].capacity = 100;
        tx.tx.outputs[1].capacity = 200;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        // With p=2 and arithmetic path, even equal amounts produce non-zero mass due to rounding
        assert_eq!(
            storage_mass,
            4 * storage_mass_parameter / 100 + 4 * storage_mass_parameter / 200 - 4 * (storage_mass_parameter / 75)
        );

        // Remove an output and make sure the other is small enough to make storage mass greater than zero
        tx.tx.outputs.pop();
        tx.tx.outputs_data.pop();
        tx.tx.outputs[0].capacity = 50;
        let storage_mass = MassCalculator::new(0, 0, 0, storage_mass_parameter).calc_contextual_masses(&tx.as_verifiable()).unwrap();
        assert_eq!(storage_mass, 4 * storage_mass_parameter / 50 - 4 * (storage_mass_parameter / 75));
    }

    fn generate_tx_from_amounts(ins: &[u64], outs: &[u64]) -> MutableTransaction<CellTx> {
        let lock_hash = [0u8; 32];
        let lock = ScriptRef::new(lock_hash, 0, vec![]);
        let prev_tx_id = TransactionId::from_str("880eb9819a31821d9d2399e2f35e2433b72637e393d71ecc9b8d0250f49153c3").unwrap();

        let inputs: Vec<CellRef> = (0..ins.len())
            .map(|i| CellRef::new(outpoint_from_id(prev_tx_id, i as u32), 0))
            .collect();

        let outputs: Vec<CellOut> = outs.iter()
            .copied()
            .map(|out_amount| CellOut {
                lock: lock.clone(),
                type_: None,
                capacity: out_amount,
            })
            .collect();

        // Use data_len=15 for each output so cell_out_plurality = ceil((86+15)/100) = 2
        // This matches CellMeta entries which also have plurality 2 (canonical_cell_storage=126)
        let outputs_data: Vec<Vec<u8>> = (0..outs.len()).map(|_| vec![0; 15]).collect();

        let tx = CellTx {
            ver: 0,
            inputs,
            deps: vec![],
            header_deps: vec![],
            outputs,
            outputs_data,
            witnesses: vec![],
        };

        let entries = ins
            .iter()
            .copied()
            .map(|in_amount| CellEntry {
                out_point: TransactionOutpoint::default(),
                capacity: in_amount,
                data_bytes: 0,
                lock_hash,
                type_hash: None,
                data_hash: [0; 32],
                block_daa_score: 0,
                is_cellbase: false,
            })
            .collect();

        MutableTransaction::with_entries(tx, entries)
    }
}
