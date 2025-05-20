use crate::constants::{MAX_SOMPI, SEQUENCE_LOCK_TIME_DISABLED, SEQUENCE_LOCK_TIME_MASK};
use tondi_consensus_core::{
    hashing::sighash::{SigHashReusedValuesSync, SigHashReusedValuesUnsync},
    tx::{TransactionInput, VerifiableTransaction},
};
use tondi_txscript::{caches::Cache, get_sig_op_count_upper_bound, SigCacheKey, TxScriptEngine};
use tondi_txscript_errors::TxScriptError;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use rayon::ThreadPool;
use std::marker::Sync;
use secp256k1::{Secp256k1, Message};
use rand::rngs::OsRng;
use blake3;

use super::{
    errors::{TxResult, TxRuleError},
    TransactionValidator,
};

/// The threshold above which we apply parallelism to input script processing
const CHECK_SCRIPTS_PARALLELISM_THRESHOLD: usize = 1;

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
                // Note: in the Tondid codebase there's a use in i64 in order to use the -1 value
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
    use crate::params::MAINNET_PARAMS;
    use std::str::FromStr;
    use tondi_consensus_core::{
        subnets::SubnetworkId,
        tx::{PopulatedTransaction, ScriptPublicKey,
             Transaction, TransactionId, TransactionInput, TransactionOutpoint,
             TransactionOutput, UtxoEntry,
        },
    };
    use smallvec::SmallVec;

    use super::super::errors::TxRuleError;

    /// Helper function to duplicate the last input
    /// This is used to test transaction validation with multiple inputs
    /// It creates a copy of the last input and its corresponding UTXO entry
    /// This is useful for testing scenarios where we need to verify multiple inputs
    fn duplicate_input(tx: &Transaction, entries: &[UtxoEntry]) -> (Transaction, Vec<UtxoEntry>) {
        let mut tx2 = tx.clone();
        let mut entries2 = entries.to_owned();
        
        // 创建一个新的输入，使用不同的 outpoint 但相同的签名
        let mut new_input = tx2.inputs.last().unwrap().clone();
        
        // 修改 outpoint 的 index，使其与原始输入不同
        new_input.previous_outpoint.index = new_input.previous_outpoint.index + 1;
        
        tx2.inputs.push(new_input);
        entries2.push(entries2.last().unwrap().clone());
        
        (tx2, entries2)
    }

    #[test]
    fn print_valid_signature_and_pubkey() {
        use secp256k1::{Secp256k1, Message};
        use rand::rngs::OsRng;
        use blake3;

        // 1. 生成密钥对
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);

        // 2. 构造交易内容
        let tx_bytes = b"test transaction bytes";

        // 3. 用 BLAKE3 计算 sighash
        let sighash = blake3::hash(tx_bytes);

        // 4. 用私钥签名
        let msg = Message::from_digest_slice(sighash.as_bytes()).unwrap();
        let sig = secp.sign_ecdsa(&msg, &secret_key);

        // 5. 构造 signature_script
        let mut sig_der = sig.serialize_der().to_vec();
        sig_der.push(0x01); // SIGHASH_ALL

        // 构造 push 操作码
        let mut signature_script = Vec::new();
        let push_len = sig_der.len() as u8;
        signature_script.push(push_len);
        signature_script.extend_from_slice(&sig_der);

        // 6. 构造 script_pub_key
        let pubkey_bytes = public_key.serialize();
        let mut script_pub_key = Vec::new();
        script_pub_key.push(0x20); // 32字节压缩公钥
        script_pub_key.extend_from_slice(&pubkey_bytes[1..33]); // 跳过第一个字节（0x02或0x03）
        script_pub_key.push(0xac); // OP_CHECKSIG

        // 7. 打印 hex 字符串
        println!("signature_script: {}", hex::encode(&signature_script));
        println!("script_pub_key: {}", hex::encode(&script_pub_key));
        println!("signature_script bytes: {}", signature_script.len());
    }

    #[test]
    fn check_signature_test() {
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

        let prev_tx_id = TransactionId::from_slice(&[1u8; 32]);

        // 生成密钥对和签名
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);
        let tx_bytes = b"test transaction bytes";
        let sighash = blake3::hash(tx_bytes);
        let msg = Message::from_digest_slice(sighash.as_bytes()).unwrap();
        let sig = secp.sign_ecdsa(&msg, &secret_key);

        // 构造 signature_script
        let mut sig_der = sig.serialize_der().to_vec();
        println!("Raw DER signature length: {}", sig_der.len());
        println!("Raw DER signature: {}", hex::encode(&sig_der));
        
        sig_der.push(0x01); // SIGHASH_ALL
        println!("DER + SIGHASH length: {}", sig_der.len());
        println!("DER + SIGHASH: {}", hex::encode(&sig_der));
        
        let mut signature_script = Vec::new();
        let push_len = sig_der.len() as u8;
        signature_script.push(push_len);
        signature_script.extend_from_slice(&sig_der);
        println!("Final signature script length: {}", signature_script.len());
        println!("Final signature script: {}", hex::encode(&signature_script));
        println!("Push opcode value: {}", push_len);
        
        // 构造 script_pub_key
        let pubkey_bytes = public_key.serialize();
        let mut script_pub_key = Vec::new();
        script_pub_key.push(0x21); // Push 33 bytes
        script_pub_key.extend_from_slice(&pubkey_bytes);
        script_pub_key.push(0xad); // OP_CHECKSIGECDSA
        let script_pub_key = SmallVec::from(script_pub_key);
        println!("Script pub key length: {}", script_pub_key.len());
        println!("Script pub key: {}", hex::encode(&script_pub_key));

        // 构造交易
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: signature_script.clone(),
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![TransactionOutput { value: 300, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) }],
            1615462089000,
            SubnetworkId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        // 构造 UTXO
        let utxo_entry = UtxoEntry {
            amount: 100,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()),
            block_daa_score: 0,
            is_coinbase: false,
        };

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![utxo_entry.clone()],
        );

        // 校验签名应通过
        let result = tv.check_scripts(&populated_tx, u64::MAX);
        println!("Validation result: {:?}", result);
        assert!(result.is_ok(), "Signature check failed: {:?}", result);

        // 测试重复输入
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        // 校验应失败
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
    }

    #[test]
    fn check_multi_signature_test() {
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

        let prev_tx_id = TransactionId::from_slice(&[2u8; 32]);

        // 生成密钥对和签名
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);
        let tx_bytes = b"test transaction bytes";
        let sighash = blake3::hash(tx_bytes);
        let msg = Message::from_digest_slice(sighash.as_bytes()).unwrap();
        let sig = secp.sign_ecdsa(&msg, &secret_key);

        // 构造 signature_script
        let mut sig_der = sig.serialize_der().to_vec();
        sig_der.push(0x01); // SIGHASH_ALL
        let mut signature_script = Vec::new();
        let push_len = sig_der.len() as u8;
        signature_script.push(push_len);
        signature_script.extend_from_slice(&sig_der);

        // 构造 script_pub_key
        let pubkey_bytes = public_key.serialize();
        let mut script_pub_key = Vec::new();
        script_pub_key.push(0x21); // Push 33 bytes
        script_pub_key.extend_from_slice(&pubkey_bytes);
        script_pub_key.push(0xad); // OP_CHECKSIGECDSA
        let script_pub_key = SmallVec::from(script_pub_key);

        // 构造交易
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) }],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        // 构造 UTXO
        let utxo_entry = UtxoEntry {
            amount: 12793000000000,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![utxo_entry.clone()],
        );

        // 校验签名应通过
        let result = tv.check_scripts(&populated_tx, u64::MAX);
        assert!(result.is_ok(), "Signature check failed: {:?}", result);

        // 测试重复输入
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        // 校验应失败
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
    }

    #[test]
    fn check_incorrect_signature_test() {
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

        let prev_tx_id = TransactionId::from_slice(&[3u8; 32]);

        // 生成密钥对和签名
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);
        let tx_bytes = b"test transaction bytes";
        let sighash = blake3::hash(tx_bytes);
        let msg = Message::from_digest_slice(sighash.as_bytes()).unwrap();
        let sig = secp.sign_ecdsa(&msg, &secret_key);

        // 构造 signature_script
        let mut sig_der = sig.serialize_der().to_vec();
        sig_der.push(0x01); // SIGHASH_ALL
        let mut signature_script = Vec::new();
        let push_len = sig_der.len() as u8;
        signature_script.push(push_len);
        signature_script.extend_from_slice(&sig_der);

        // 构造 script_pub_key
        let pubkey_bytes = public_key.serialize();
        let mut script_pub_key = Vec::new();
        script_pub_key.push(0x21); // Push 33 bytes
        script_pub_key.extend_from_slice(&pubkey_bytes);
        script_pub_key.push(0xad); // OP_CHECKSIGECDSA
        let script_pub_key = SmallVec::from(script_pub_key);

        // 构造交易
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) }],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        // 构造 UTXO
        let utxo_entry = UtxoEntry {
            amount: 12793000000000,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![utxo_entry.clone()],
        );

        // 校验签名应失败
        assert!(tv.check_scripts(&populated_tx, u64::MAX).is_err(), "Expecting signature check to fail");

        // 测试重复输入
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX).expect_err("Expecting signature check to fail");
    }

    #[test]
    fn check_last_sig_incorrect_multi_signature_test() {
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

        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        // 生成密钥对和签名
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);
        let tx_bytes = b"test transaction bytes";
        let sighash = blake3::hash(tx_bytes);
        let msg = Message::from_digest_slice(sighash.as_bytes()).unwrap();
        let sig = secp.sign_ecdsa(&msg, &secret_key);

        // 构造 signature_script
        let mut sig_der = sig.serialize_der().to_vec();
        sig_der.push(0x01); // SIGHASH_ALL
        let mut signature_script = Vec::new();
        let push_len = sig_der.len() as u8;
        signature_script.push(push_len);
        signature_script.extend_from_slice(&sig_der);

        // 构造 script_pub_key
        let pubkey_bytes = public_key.serialize();
        let mut script_pub_key = Vec::new();
        script_pub_key.push(0x21); // Push 33 bytes
        script_pub_key.extend_from_slice(&pubkey_bytes);
        script_pub_key.push(0xad); // OP_CHECKSIGECDSA
        let script_pub_key = SmallVec::from(script_pub_key);

        // 构造交易
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![
                TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) },
                TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        // 构造 UTXO
        let utxo_entry = UtxoEntry {
            amount: 12793000000000,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![utxo_entry.clone()],
        );

        assert_eq!(tv.check_scripts(&populated_tx, u64::MAX), Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse)));

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
    }

    #[test]
    fn check_first_sig_incorrect_multi_signature_test() {
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

        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        // 生成密钥对和签名
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);
        let tx_bytes = b"test transaction bytes";
        let sighash = blake3::hash(tx_bytes);
        let msg = Message::from_digest_slice(sighash.as_bytes()).unwrap();
        let sig = secp.sign_ecdsa(&msg, &secret_key);

        // 构造 signature_script
        let mut sig_der = sig.serialize_der().to_vec();
        sig_der.push(0x01); // SIGHASH_ALL
        let mut signature_script = Vec::new();
        let push_len = sig_der.len() as u8;
        signature_script.push(push_len);
        signature_script.extend_from_slice(&sig_der);

        // 构造 script_pub_key
        let pubkey_bytes = public_key.serialize();
        let mut script_pub_key = Vec::new();
        script_pub_key.push(0x21); // Push 33 bytes
        script_pub_key.extend_from_slice(&pubkey_bytes);
        script_pub_key.push(0xad); // OP_CHECKSIGECDSA
        let script_pub_key = SmallVec::from(script_pub_key);

        // 构造交易
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![
                TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) },
                TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        // 构造 UTXO
        let utxo_entry = UtxoEntry {
            amount: 12793000000000,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![utxo_entry.clone()],
        );

        assert_eq!(tv.check_scripts(&populated_tx, u64::MAX), Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse)));

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
    }

    #[test]
    fn check_empty_incorrect_multi_signature_test() {
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

        let prev_tx_id = TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap();

        // 生成密钥对和签名
        let secp = Secp256k1::new();
        let mut rng = OsRng;
        let (secret_key, public_key) = secp.generate_keypair(&mut rng);
        let tx_bytes = b"test transaction bytes";
        let sighash = blake3::hash(tx_bytes);
        let msg = Message::from_digest_slice(sighash.as_bytes()).unwrap();
        let sig = secp.sign_ecdsa(&msg, &secret_key);

        // 构造 signature_script
        let mut sig_der = sig.serialize_der().to_vec();
        sig_der.push(0x01); // SIGHASH_ALL
        let mut signature_script = Vec::new();
        let push_len = sig_der.len() as u8;
        signature_script.push(push_len);
        signature_script.extend_from_slice(&sig_der);

        // 构造 script_pub_key
        let pubkey_bytes = public_key.serialize();
        let mut script_pub_key = Vec::new();
        script_pub_key.push(0x21); // Push 33 bytes
        script_pub_key.extend_from_slice(&pubkey_bytes);
        script_pub_key.push(0xad); // OP_CHECKSIGECDSA
        let script_pub_key = SmallVec::from(script_pub_key);

        // 构造交易
        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script,
                sequence: 0,
                sig_op_count: 1,
            }],
            vec![
                TransactionOutput { value: 10000000000000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) },
                TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()) },
            ],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        // 构造 UTXO
        let utxo_entry = UtxoEntry {
            amount: 12793000000000,
            script_public_key: ScriptPublicKey::new(0, script_pub_key.clone().into()),
            block_daa_score: 36151168,
            is_coinbase: false,
        };

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![utxo_entry.clone()],
        );

        assert_eq!(tv.check_scripts(&populated_tx, u64::MAX), Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse)));

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::EvalFalse))
        );
    }

    #[test]
    fn check_non_push_only_script_sig_test() {
        // We test a situation where the script itself is valid, but the script signature is not push only
        let params = MAINNET_PARAMS.clone();
        let tv = TransactionValidator::new_for_tests(
            params.prior_max_tx_inputs,
            params.prior_max_tx_outputs,
            params.prior_max_signature_script_len,
            params.prior_max_script_public_key_len,
            params.coinbase_payload_script_public_key_max_len,
            params.prior_coinbase_maturity,
            Default::default(),
        );

        let prev_tx_id = TransactionId::from_str("1111111111111111111111111111111111111111111111111111111111111111").unwrap();

        let mut bytes = [0u8; 2];
        faster_hex::hex_decode("5175".as_bytes(), &mut bytes).unwrap(); // OP_TRUE OP_DROP
        let signature_script = bytes.to_vec();

        let mut bytes = [0u8; 1];
        faster_hex::hex_decode("51".as_bytes(), &mut bytes) // OP_TRUE
            .unwrap();
        let script_pub_key_1 = SmallVec::from(bytes.to_vec());

        let tx = Transaction::new(
            0,
            vec![TransactionInput {
                previous_outpoint: TransactionOutpoint { transaction_id: prev_tx_id, index: 0 },
                signature_script: signature_script.clone(),
                sequence: 0,
                sig_op_count: 4,
            }],
            vec![TransactionOutput { value: 2792999990000, script_public_key: ScriptPublicKey::new(0, script_pub_key_1.clone()) }],
            0,
            SubnetworkId::from_bytes([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            0,
            vec![],
        );

        let populated_tx = PopulatedTransaction::new(
            &tx,
            vec![UtxoEntry {
                amount: 12793000000000,
                script_public_key: ScriptPublicKey::new(0, script_pub_key_1),
                block_daa_score: 36151168,
                is_coinbase: false,
            }],
        );

        assert_eq!(
            tv.check_scripts(&populated_tx, u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::SignatureScriptNotPushOnly))
        );

        // Test a tx with 2 inputs to cover parallelism split points in inner script checking code
        let (tx2, entries2) = duplicate_input(&tx, &populated_tx.entries);
        assert_eq!(
            tv.check_scripts(&PopulatedTransaction::new(&tx2, entries2), u64::MAX),
            Err(TxRuleError::SignatureInvalid(TxScriptError::SignatureScriptNotPushOnly))
        );
    }
}
