//!
//! Transaction mass calculator.
//!

use crate::error::Error;
use crate::result::Result;
use crate::tx::PaymentOutput;
use spora_consensus_client as kcc;
use spora_consensus_client::TransactionInput;
use spora_consensus_client::{pay_to_address_lock_script, CellEntryReference};
use spora_consensus_core::mass::{
    calc_storage_mass as consensus_calc_storage_mass, cell_tx_estimated_serialized_size, CellMass,
    MassCalculator as ConsensusMassCalculator,
};
use spora_consensus_core::tx::CellTx;
use spora_consensus_core::{config::params::Params, constants::*};
use spora_hashes::HASH_SIZE;

const STANDARD_LOCK_ARGS_SIZE: u64 = 32;
const CKB_TYPE_ID_ARGS_SIZE: u64 = 32;
const CKB_TYPE_ID_SCRIPT_SIZE: u64 = 32 + 1 + CKB_TYPE_ID_ARGS_SIZE;
const CELL_OUTPUT_TYPE_SCRIPT_SERIALIZED_DELTA: u64 = 32 + 1 + 8 + CKB_TYPE_ID_ARGS_SIZE;

// pub const ECDSA_SIGNATURE_SIZE: u64 = 64;
// pub const SCHNORR_SIGNATURE_SIZE: u64 = 64;
pub const SIGNATURE_SIZE: u64 = 1 + 64 + 1; //1 byte for OP_DATA_65 + 64 (length of signature) + 1 byte for sig hash type

/// MINIMUM_RELAY_TRANSACTION_FEE specifies the minimum transaction fee for a transaction to be accepted to
/// the mempool and relayed. It is specified in sau per 1kg (or 1000 grams) of transaction mass.
pub(crate) const MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;

/// MAXIMUM_STANDARD_TRANSACTION_MASS is the maximum mass allowed for transactions that
/// are considered standard and will therefore be relayed and considered for mining.
pub const MAXIMUM_STANDARD_TRANSACTION_MASS: u64 = 500_000;

/// minimum_required_transaction_relay_fee returns the minimum transaction fee required
/// for a transaction with the passed mass to be accepted into the mempool and relayed.
pub fn calc_minimum_required_transaction_relay_fee(mass: u64) -> u64 {
    // Calculate the minimum fee for a transaction to be allowed into the
    // mempool and relayed by scaling the base fee. MinimumRelayTransactionFee is in
    // sau/kg so multiply by mass (which is in grams) and divide by 1000 to get
    // minimum saus.
    let mut minimum_fee = (mass * MINIMUM_RELAY_TRANSACTION_FEE) / 1000;

    if minimum_fee == 0 {
        minimum_fee = MINIMUM_RELAY_TRANSACTION_FEE;
    }

    // Set the minimum fee to the maximum possible value if the calculated
    // fee is not in the valid range for monetary amounts.
    minimum_fee = minimum_fee.min(MAX_SAU);

    minimum_fee
}

// The most common scripts are pay-to-pubkey, and as per the above
// breakdown, the minimum size of a p2pk input script is 148 bytes. So
// that figure is used.
pub const STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE: u64 = transaction_standard_output_serialized_byte_size() + 148;
pub const STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X: u64 = STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE * 3;

// pub fn is_standard_output_amount_dust(value: u64) -> bool {
//     match value.checked_mul(1000) {
//         Some(value_1000) => value_1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE,
//         None => (value as u128 * 1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X as u128) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
//     }
// }

// pub fn is_standard_output_amount_dust(network_params: &NetworkParams, value: u64) -> bool {
// pub fn is_dust(_network_params: &NetworkParams, value: u64) -> bool {
//     // if let Some(dust_threshold_sau) = network_params.dust_threshold_sau {
//     //     return value < dust_threshold_sau;
//     // } else {
//     match value.checked_mul(1000) {
//         Some(value_1000) => value_1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE,
//         None => (value as u128 * 1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X as u128) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
//     }
//     // }
// }

// transaction_estimated_serialized_size is the estimated size of a transaction in some
// serialization. This has to be deterministic, but not necessarily accurate, since
// it's only used as the size component in the transaction and block mass limit
// calculation.
pub fn transaction_serialized_byte_size(tx: &CellTx) -> u64 {
    cell_tx_estimated_serialized_size(tx)
}

pub fn blank_transaction_serialized_byte_size() -> u64 {
    cell_tx_estimated_serialized_size(&CellTx::new(vec![], vec![], vec![], vec![], vec![]).expect("empty CellTx must be valid"))
}

/// Serialized-size estimate for the client-side input wrapper.
fn transaction_input_serialized_byte_size(input: &TransactionInput) -> u64 {
    let inner = input.inner();
    let mut size = 0;
    size += outpoint_estimated_serialized_size();

    size += 8; // length of witness (u64)
    size += inner.witness.as_ref().map(|witness| witness.len()).unwrap_or(0) as u64;

    size += 8; // since (u64)
    size
}

const fn outpoint_estimated_serialized_size() -> u64 {
    let mut size: u64 = 0;
    size += HASH_SIZE as u64; // Previous tx ID
    size += 4; // Index (u32)
    size
}

pub fn payment_output_serialized_byte_size(output: &PaymentOutput) -> u64 {
    let lock_script = pay_to_address_lock_script(&output.address);
    let mut size: u64 = 0;
    size += 8; // value (u64)
    size += 32; // output.lock.code_hash ([u8; 32])
    size += 1; // output.lock.hash_type (u8)
    size += 8; // length of output.lock.args (u64)
    size += lock_script.args.len() as u64;
    size
}

pub const fn transaction_standard_output_serialized_byte_size() -> u64 {
    let mut size: u64 = 0;
    size += 8; // value (u64)
    size += 32; // output.lock.code_hash ([u8; 32])
    size += 1; // output.lock.hash_type (u8)
    size += 8; // length of output.lock.args (u64)
    size += STANDARD_LOCK_ARGS_SIZE;
    size
}

pub struct MassCalculator {
    mass_per_tx_byte: u64,
    mass_per_script_pub_key_byte: u64,
    mass_per_sig_op: u64,
    storage_mass_parameter: u64,
}

impl MassCalculator {
    pub fn new(consensus_params: &Params) -> Self {
        Self {
            mass_per_tx_byte: consensus_params.mass_per_tx_byte,
            mass_per_script_pub_key_byte: consensus_params.mass_per_script_pub_key_byte,
            mass_per_sig_op: consensus_params.mass_per_sig_op,
            storage_mass_parameter: consensus_params.storage_mass_parameter,
        }
    }

    pub fn is_dust(&self, value: u64) -> bool {
        match value.checked_mul(1000) {
            Some(value_1000) => value_1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X < MINIMUM_RELAY_TRANSACTION_FEE,
            None => (value as u128 * 1000 / STANDARD_OUTPUT_SIZE_PLUS_INPUT_SIZE_3X as u128) < MINIMUM_RELAY_TRANSACTION_FEE as u128,
        }
    }

    fn consensus_mass_calculator(&self) -> ConsensusMassCalculator {
        ConsensusMassCalculator::new(
            self.mass_per_tx_byte,
            self.mass_per_script_pub_key_byte,
            self.mass_per_sig_op,
            self.storage_mass_parameter,
        )
    }

    pub fn calc_compute_mass_for_signed_consensus_transaction(&self, tx: &CellTx) -> u64 {
        self.consensus_mass_calculator().calc_non_contextual_masses_cell(tx).compute_mass
    }

    pub(crate) fn blank_transaction_compute_mass(&self) -> u64 {
        blank_transaction_serialized_byte_size() * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_payload(&self, payload_byte_size: usize) -> u64 {
        payload_byte_size as u64 * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_payment_outputs(&self, outputs: &[PaymentOutput]) -> u64 {
        outputs.iter().map(|output| self.calc_compute_mass_for_payment_output(output)).sum()
    }

    pub(crate) fn calc_compute_mass_for_payment_output(&self, output: &PaymentOutput) -> u64 {
        let lock_script = pay_to_address_lock_script(&output.address);
        self.mass_per_script_pub_key_byte * (33 + lock_script.args.len() as u64)
            + payment_output_serialized_byte_size(output) * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_ckb_type_id_output_scripts(&self, count: usize) -> u64 {
        let per_output = CKB_TYPE_ID_SCRIPT_SIZE
            .saturating_mul(self.mass_per_script_pub_key_byte)
            .saturating_add(CELL_OUTPUT_TYPE_SCRIPT_SERIALIZED_DELTA.saturating_mul(self.mass_per_tx_byte));
        per_output.saturating_mul(count as u64)
    }

    /// Client-side input mass based on serialized input bytes only.
    /// Cell-model implicit sigops are accounted for on the consensus side.
    pub(crate) fn calc_compute_mass_for_client_transaction_input(&self, input: &TransactionInput) -> u64 {
        transaction_input_serialized_byte_size(input) * self.mass_per_tx_byte
    }

    pub(crate) fn calc_compute_mass_for_signature(&self, minimum_signatures: u16) -> u64 {
        SIGNATURE_SIZE * self.mass_per_tx_byte * minimum_signatures.max(1) as u64
    }

    pub fn calc_signature_compute_mass_for_inputs(&self, number_of_inputs: usize, minimum_signatures: u16) -> u64 {
        SIGNATURE_SIZE * self.mass_per_tx_byte * minimum_signatures.max(1) as u64 * number_of_inputs as u64
    }

    fn estimated_signed_transaction_serialized_byte_size(&self, tx: &CellTx, minimum_signatures: u16) -> u64 {
        let existing_witnesses = tx.witnesses.len() as u64;
        let input_count = tx.inputs.len() as u64;
        let added_witness_slots = input_count.saturating_sub(existing_witnesses);
        let estimated_witness_size = SIGNATURE_SIZE * minimum_signatures.max(1) as u64;

        cell_tx_estimated_serialized_size(tx)
            .saturating_add(added_witness_slots * 8)
            .saturating_add(input_count * estimated_witness_size)
    }

    pub fn calc_transient_mass_for_signed_consensus_transaction(&self, tx: &CellTx) -> u64 {
        cell_tx_estimated_serialized_size(tx) * TRANSIENT_BYTE_TO_MASS_FACTOR
    }

    pub fn calc_transient_mass_for_unsigned_consensus_transaction(&self, tx: &CellTx, minimum_signatures: u16) -> u64 {
        self.estimated_signed_transaction_serialized_byte_size(tx, minimum_signatures) * TRANSIENT_BYTE_TO_MASS_FACTOR
    }

    pub fn calc_minimum_transaction_fee_from_mass(&self, mass: u64) -> u64 {
        calc_minimum_required_transaction_relay_fee(mass)
    }

    pub fn calc_compute_mass_for_unsigned_consensus_transaction(&self, tx: &CellTx, minimum_signatures: u16) -> u64 {
        self.calc_compute_mass_for_signed_consensus_transaction(tx)
            + self.calc_signature_compute_mass_for_inputs(tx.inputs.len(), minimum_signatures)
    }

    #[inline(always)]
    pub fn calc_fee_for_mass(&self, mass: u64) -> u64 {
        self.calc_minimum_transaction_fee_from_mass(mass)
    }

    pub fn combine_mass(&self, compute_mass: u64, storage_mass: u64) -> u64 {
        compute_mass.max(storage_mass)
    }

    pub fn combine_mass_components(&self, compute_mass: u64, transient_mass: u64, storage_mass: u64) -> u64 {
        self.combine_mass(compute_mass, storage_mass).max(transient_mass)
    }

    /// Calculates the overall mass of this transaction, combining compute, transient, and storage masses.
    pub fn calc_overall_mass_for_unsigned_client_transaction(&self, tx: &kcc::Transaction, minimum_signatures: u16) -> Result<u64> {
        let cctx = tx.cell_tx()?;
        let storage_mass = self.calc_storage_mass_for_transaction(tx)?.ok_or(Error::MassCalculationError)?;
        let compute_mass = self.calc_compute_mass_for_unsigned_consensus_transaction(&cctx, minimum_signatures);
        let transient_mass = self.calc_transient_mass_for_unsigned_consensus_transaction(&cctx, minimum_signatures);
        Ok(self.combine_mass_components(compute_mass, transient_mass, storage_mass))
    }

    pub fn calc_overall_mass_for_unsigned_consensus_transaction(
        &self,
        tx: &CellTx,
        cells: &[CellEntryReference],
        minimum_signatures: u16,
    ) -> Result<u64> {
        let storage_mass = self
            .calc_storage_mass_for_cell_transaction_parts(cells, &tx.outputs, &tx.outputs_data)
            .ok_or(Error::MassCalculationError)?;
        let compute_mass = self.calc_compute_mass_for_unsigned_consensus_transaction(tx, minimum_signatures);
        let transient_mass = self.calc_transient_mass_for_unsigned_consensus_transaction(tx, minimum_signatures);
        Ok(self.combine_mass_components(compute_mass, transient_mass, storage_mass))
    }

    pub fn calc_storage_mass_for_transaction(&self, tx: &kcc::Transaction) -> Result<Option<u64>> {
        let cells = tx.cell_entry_references()?;
        let cell_tx = tx.cell_tx()?;
        Ok(self.calc_storage_mass_for_cell_transaction_parts(&cells, &cell_tx.outputs, &cell_tx.outputs_data))
    }

    pub fn calc_storage_mass_for_cell_transaction_parts(
        &self,
        inputs: &[CellEntryReference],
        outputs: &[spora_exec::celltx::CellOutput],
        outputs_data: &[Vec<u8>],
    ) -> Option<u64> {
        consensus_calc_storage_mass(
            false,
            inputs.iter().map(|entry| entry.into()),
            outputs
                .iter()
                .enumerate()
                .map(|(index, output)| CellMass::from((output, outputs_data.get(index).map(Vec::len).unwrap_or_default()))),
            self.storage_mass_parameter,
        )
    }

    pub fn calc_storage_mass_payment_output_harmonic(&self, outputs: &[PaymentOutput]) -> Option<u64> {
        outputs
            .iter()
            .map(|out| self.storage_mass_parameter.checked_div(out.amount))
            .try_fold(0u64, |total, current| current.and_then(|current| total.checked_add(current)))
    }

    pub fn calc_storage_mass_output_harmonic_single(&self, output_value: u64) -> u64 {
        self.storage_mass_parameter / output_value
    }

    pub fn calc_storage_mass_input_mean_arithmetic(&self, total_input_value: u64, number_of_inputs: u64) -> u64 {
        let mean_input_value = total_input_value / number_of_inputs;
        number_of_inputs.saturating_mul(self.storage_mass_parameter / mean_input_value)
    }

    pub fn calc_storage_mass(&self, output_harmonic: u64, total_input_value: u64, number_of_inputs: u64) -> u64 {
        let input_arithmetic = self.calc_storage_mass_input_mean_arithmetic(total_input_value, number_of_inputs);
        output_harmonic.saturating_sub(input_arithmetic)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spora_consensus_core::{
        config::params::MAINNET_PARAMS,
        tx::{CellInput, CellOutput, CellTx, Script, TransactionOutpoint},
    };

    fn test_lock_script(arg_len: usize) -> Script {
        Script::new([7; 32], 0, vec![1; arg_len])
    }

    fn test_tx(arg_len: usize) -> CellTx {
        let input = CellInput::new(TransactionOutpoint::new([3; 32], 0), 0);
        let output = CellOutput { capacity: 1_000, lock: test_lock_script(arg_len), type_: None };
        CellTx::new(vec![input], vec![], vec![output], vec![vec![]], vec![vec![]]).expect("test tx must be valid")
    }

    #[test]
    fn calc_fee_for_mass_matches_relay_floor() {
        let calculator = MassCalculator::new(&MAINNET_PARAMS);
        assert_eq!(calculator.calc_fee_for_mass(1), 1);
        assert_eq!(calculator.calc_fee_for_mass(2_500), calc_minimum_required_transaction_relay_fee(2_500));
    }

    #[test]
    fn combine_mass_includes_transient_mass() {
        let calculator = MassCalculator::new(&MAINNET_PARAMS);
        assert_eq!(calculator.combine_mass_components(100, 250, 200), 250);
    }

    #[test]
    fn unsigned_overall_mass_accounts_for_signature_expanded_transient_mass() {
        let calculator = MassCalculator::new(&MAINNET_PARAMS);
        let tx = test_tx(8_000);

        let compute_mass = calculator.calc_compute_mass_for_unsigned_consensus_transaction(&tx, 1);
        let signed_transient_mass = calculator.calc_transient_mass_for_signed_consensus_transaction(&tx);
        let transient_mass = calculator.calc_transient_mass_for_unsigned_consensus_transaction(&tx, 1);
        let cells = [CellEntryReference::simulated(1_000)];
        let storage_mass = calculator
            .calc_storage_mass_for_cell_transaction_parts(&cells, &tx.outputs, &tx.outputs_data)
            .expect("storage mass should be computable");
        let overall_mass = calculator.calc_overall_mass_for_unsigned_consensus_transaction(&tx, &cells, 1).unwrap();

        assert!(transient_mass > signed_transient_mass);
        assert_eq!(overall_mass, compute_mass.max(transient_mass).max(storage_mass));
    }
}
