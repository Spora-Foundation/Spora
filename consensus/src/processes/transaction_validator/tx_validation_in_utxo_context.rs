use tondi_consensus_core::{
    constants::{MAX_SOMPI, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK},
    errors::tx::TxRuleError,
    hashing::sighash::{SigHashReusedValuesSync, SigHashReusedValuesUnsync},
    tx::{TransactionInput, VerifiableTransaction},
};

use tondi_txscript::{
    caches::Cache,
    get_sig_op_count_upper_bound,
    SigCacheKey,
    TxScriptEngine,
};
use tondi_txscript_errors::TxScriptError;
use rayon::{iter::{IntoParallelIterator, ParallelIterator}, ThreadPool};

use crate::processes::transaction_validator::{errors::TxResult, TransactionValidator};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TxValidationFlags {
    /// Perform full validation including script verification
    Full,

    /// Perform fee and sequence/maturity validations but skip script checks. This is usually
    /// an optimization to be applied when it is known that scripts were already checked
    SkipScriptChecks,

    /// When validating mempool transactions, we just set this value ourselves
    SkipMassCheck,
}

/// The threshold above which we apply parallelism to input script processing
pub const CHECK_SCRIPTS_PARALLELISM_THRESHOLD: usize = 100;

/// A trait for types that can be used to validate transactions in a UTXO context
pub trait TransactionValidatorInUtxoContext {
    /// Validates a transaction in a UTXO context
    fn validate_in_utxo_context<T: VerifiableTransaction + Sync>(&self, tx: &T) -> TxResult<()>;
}

impl TransactionValidatorInUtxoContext for TransactionValidator {
    fn validate_in_utxo_context<T: VerifiableTransaction + Sync>(&self, tx: &T) -> TxResult<()> {
        self.validate_populated_transaction_and_get_fee(tx, u64::MAX, u64::MAX, TxValidationFlags::Full, None)?;
        Ok(())
    }
}

impl TransactionValidator {
    pub fn validate_populated_transaction_and_get_fee(
        &self,
        tx: &(impl VerifiableTransaction + Sync),
        pov_daa_score: u64,
        block_daa_score: u64,
        flags: TxValidationFlags,
        mass_and_feerate_threshold: Option<(u64, f64)>,
    ) -> TxResult<u64> {
        self.check_transaction_coinbase_maturity(tx, pov_daa_score, block_daa_score)?;
        let total_in = self.check_transaction_input_amounts(tx)?;
        let total_out = Self::check_transaction_output_values(tx, total_in)?;
        let fee = total_in - total_out;
        if flags != TxValidationFlags::SkipMassCheck && self.crescendo_activation.is_active(block_daa_score) {
            // Storage mass hardfork was activated
            self.check_mass_commitment(tx)?;
        }
        Self::check_sequence_lock(tx, pov_daa_score)?;

        // The following call is not a consensus check (it could not be one in the first place since it uses a floating number)
        // but rather a mempool Replace by Fee validation rule. It is placed here purposely for avoiding unneeded script checks.
        Self::check_feerate_threshold(fee, mass_and_feerate_threshold)?;

        match flags {
            TxValidationFlags::Full | TxValidationFlags::SkipMassCheck => {
                if !self.crescendo_activation.is_active(block_daa_score) {
                    Self::check_sig_op_counts(tx)?;
                }
                self.check_scripts(tx, block_daa_score)?;
            }
            TxValidationFlags::SkipScriptChecks => {}
        }
        Ok(fee)
    }

    fn check_feerate_threshold(fee: u64, mass_and_feerate_threshold: Option<(u64, f64)>) -> TxResult<()> {
        // An actual check can only occur if some mass and threshold are provided,
        // otherwise, the check does not verify anything and exits successfully.
        if let Some((contextual_mass, feerate_threshold)) = mass_and_feerate_threshold {
            assert!(contextual_mass > 0);
            if fee as f64 / contextual_mass as f64 <= feerate_threshold {
                return Err(TxRuleError::FeerateTooLow);
            }
        }
        Ok(())
    }

    fn check_transaction_coinbase_maturity(
        &self,
        tx: &impl VerifiableTransaction,
        pov_daa_score: u64,
        block_daa_score: u64,
    ) -> TxResult<()> {
        if let Some((index, (input, entry))) = tx.populated_inputs().enumerate().find(|(_, (_, entry))| {
            entry.is_coinbase && entry.block_daa_score + self.coinbase_maturity.get(block_daa_score) > pov_daa_score
        }) {
            return Err(TxRuleError::ImmatureCoinbaseSpend(
                index,
                input.previous_outpoint,
                entry.block_daa_score,
                pov_daa_score,
                self.coinbase_maturity.get(block_daa_score),
            ));
        }

        Ok(())
    }

    fn check_transaction_input_amounts(&self, tx: &impl VerifiableTransaction) -> TxResult<u64> {
        let mut total: u64 = 0;
        for (_, entry) in tx.populated_inputs() {
            if let Some(new_total) = total.checked_add(entry.amount) {
                total = new_total
            } else {
                return Err(TxRuleError::InputAmountOverflow);
            }

            if total > MAX_SOMPI {
                return Err(TxRuleError::InputAmountTooHigh);
            }
        }

        Ok(total)
    }

    fn check_transaction_output_values(tx: &impl VerifiableTransaction, total_in: u64) -> TxResult<u64> {
        // There's no need to check for overflow here because it was already checked by check_transaction_output_value_ranges
        let total_out: u64 = tx.outputs().iter().map(|out| out.value).sum();
        if total_in < total_out {
            return Err(TxRuleError::SpendTooHigh(total_out, total_in));
        }

        Ok(total_out)
    }

    fn check_mass_commitment(&self, tx: &impl VerifiableTransaction) -> TxResult<()> {
        let calculated_contextual_mass =
            self.mass_calculator.calc_contextual_masses(tx).ok_or(TxRuleError::MassIncomputable)?.storage_mass;
        let committed_contextual_mass = tx.tx().mass();
        if committed_contextual_mass != calculated_contextual_mass {
            return Err(TxRuleError::WrongMass(calculated_contextual_mass, committed_contextual_mass));
        }
        Ok(())
    }

    fn check_sequence_lock(tx: &impl VerifiableTransaction, pov_daa_score: u64) -> TxResult<()> {
        let pov_daa_score: i64 = pov_daa_score as i64;
        if tx.populated_inputs().filter(|(input, _)| input.sequence & SEQUENCE_LOCK_TIME_DISABLED != SEQUENCE_LOCK_TIME_DISABLED).any(
            |(input, entry)| {
                // Given a sequence number, we apply the relative time lock
                // mask in order to obtain the time lock delta required before
                // this input can be spent.
                let relative_lock = (input.sequence & SEQUENCE_LOCK_TIME_MASK) as i64;

                // The relative lock-time for this input is expressed
                // in blocks so we calculate the relative offset from
                // the input's DAA score as its converted absolute
                // lock-time. We subtract one from the relative lock in
                // order to maintain the original lockTime semantics.
                //
                // Note: in the tondi codebase there's a use in i64 in order to use the -1 value
                // as None. Here it's not needed, but we still use it to avoid breaking consensus.
                let lock_daa_score = entry.block_daa_score as i64 + relative_lock - 1;

                lock_daa_score >= pov_daa_score
            },
        ) {
            return Err(TxRuleError::SequenceLockConditionsAreNotMet);
        }
        Ok(())
    }

    fn check_sig_op_counts<T: VerifiableTransaction>(tx: &T) -> TxResult<()> {
        for (i, (input, entry)) in tx.populated_inputs().enumerate() {
            let calculated =
                get_sig_op_count_upper_bound::<T, SigHashReusedValuesUnsync>(&input.signature_script, &entry.script_public_key);
            if calculated != input.sig_op_count as u64 {
                return Err(TxRuleError::WrongSigOpCount(i, input.sig_op_count as u64, calculated));
            }
        }
        Ok(())
    }

    pub fn check_scripts(&self, tx: &(impl VerifiableTransaction + Sync), block_daa_score: u64) -> TxResult<()> {
        check_scripts(
            &self.sig_cache,
            tx,
            self.crescendo_activation.is_active(block_daa_score),
            self.crescendo_activation.is_active(block_daa_score),
        )
    }
}

pub fn check_scripts(
    sig_cache: &Cache<SigCacheKey, bool>,
    tx: &(impl VerifiableTransaction + Sync),
    kip10_enabled: bool,
    runtime_sig_op_counting: bool,
) -> TxResult<()> {
    if tx.inputs().len() > CHECK_SCRIPTS_PARALLELISM_THRESHOLD {
        check_scripts_par_iter(sig_cache, tx, kip10_enabled, runtime_sig_op_counting)
    } else {
        check_scripts_sequential(sig_cache, tx, kip10_enabled, runtime_sig_op_counting)
    }
}

pub fn check_scripts_sequential(
    sig_cache: &Cache<SigCacheKey, bool>,
    tx: &impl VerifiableTransaction,
    kip10_enabled: bool,
    runtime_sig_op_counting: bool,
) -> TxResult<()> {
    let reused_values = SigHashReusedValuesUnsync::new();
    for (i, (input, entry)) in tx.populated_inputs().enumerate() {
        TxScriptEngine::from_transaction_input(tx, input, i, entry, &reused_values, sig_cache, kip10_enabled, runtime_sig_op_counting)
            .execute()
            .map_err(|err| map_script_err(err, input))?;
    }
    Ok(())
}

pub fn check_scripts_par_iter(
    sig_cache: &Cache<SigCacheKey, bool>,
    tx: &(impl VerifiableTransaction + Sync),
    kip10_enabled: bool,
    runtime_sig_op_counting: bool,
) -> TxResult<()> {
    let reused_values = SigHashReusedValuesSync::new();
    (0..tx.inputs().len()).into_par_iter().try_for_each(|idx| {
        let (input, utxo) = tx.populated_input(idx);
        TxScriptEngine::from_transaction_input(tx, input, idx, utxo, &reused_values, sig_cache, kip10_enabled, runtime_sig_op_counting)
            .execute()
            .map_err(|err| map_script_err(err, input))
    })
}

pub fn check_scripts_par_iter_pool(
    sig_cache: &Cache<SigCacheKey, bool>,
    tx: &(impl VerifiableTransaction + Sync),
    pool: &ThreadPool,
    kip10_enabled: bool,
    runtime_sig_op_counting: bool,
) -> TxResult<()> {
    pool.install(|| check_scripts_par_iter(sig_cache, tx, kip10_enabled, runtime_sig_op_counting))
}

fn map_script_err(script_err: TxScriptError, input: &TransactionInput) -> TxRuleError {
    if input.signature_script.is_empty() {
        TxRuleError::SignatureEmpty(script_err)
    } else {
        TxRuleError::SignatureInvalid(script_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::processes::transaction_validator::TransactionValidator;

    /// Helper function to duplicate the last input
    fn duplicate_input(tx: &Transaction, entries: &[UtxoEntry]) -> (Transaction, Vec<UtxoEntry>) {
        let mut new_tx = tx.clone();
        let mut new_entries = entries.to_vec();
        let last_input = new_tx.inputs.last().unwrap().clone();
        let last_entry = entries.last().unwrap().clone();
        new_tx.inputs.push(last_input);
        new_entries.push(last_entry);
        (new_tx, new_entries)
    }

    #[test]
    fn check_signature_test() -> Result<(), Box<dyn Error>> {
        let mut params = MAINNET_PARAMS.clone();
        params.prior_max_tx_inputs = 10;
        params.prior_max_tx_outputs = 15;
        let tv = TransactionValidator::new_for_tests(
            params.prior_max_tx_inputs,
            params.prior_max_tx_outputs,
            params.prior_max_signature_script_len,
            params.prior_max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.prior_coinbase_maturity,
            Default::default(),
        );

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_str("63b0e5d57d5e5a5e5a5e5a5e5a5e5a5e5a5e5a5e5a5e5a5e5a5e5a5e5a5e5a5e").unwrap();
        let keypair = Keypair::from_seckey_slice(&secp, &secret_key.secret_bytes()).unwrap();
        let (public_key, _) = keypair.x_only_public_key();

        let script_pub_key = once(0x20).chain(public_key.serialize()).chain(once(0xac)).collect_vec();
        let script_pub_key = ScriptVec::from_slice(&script_pub_key);

        let prev_tx_id = TransactionId::from_str("746915c8dfc5e1550eacbe1d87625a105750cf1a65aaddd1baa60f8bcf7e953c").unwrap();

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 1 },
                signature_script: vec![],
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![
                TransactionOutput { value: 10360487799, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
                TransactionOutput { value: 10518958752, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone()) },
            ],
            0,
            SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );

        let tx_clone = tx.clone();
        let populated_tx = PopulatedTransaction::new(
            &tx_clone,
            vec![UtxoEntry {
                amount: 20879456551,
                script_public_key: ScriptPublicKey::new(0, script_pub_key),
                block_daa_score: 32022768,
                is_coinbase: false,
            }],
        );

        let mut_tx = MutableTransaction::with_entries(tx, populated_tx.entries.clone());
        let signed_tx = sign(mut_tx, keypair);
        let populated_tx = signed_tx.as_verifiable();

        tv.check_scripts(&populated_tx, u64::MAX).expect("Signature check failed");

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let entries: Vec<_> = populated_tx.populated_inputs().map(|(_, entry)| entry.clone()).collect();
        let (tx2, entries2) = duplicate_input(populated_tx.tx(), &entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
        Ok(())
    }

    // ... rest of the tests ...
}
