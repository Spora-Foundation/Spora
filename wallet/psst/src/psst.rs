//!
//! Partially Signed Spora Transaction (PSST)
//!

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use spora_bip32::{secp256k1, DerivationPath, KeyFingerprint};
use spora_consensus_core::Hash;
use std::{collections::BTreeMap, fmt::Display, fmt::Formatter, future::Future, marker::PhantomData, ops::Deref};

pub use crate::error::Error;
pub use crate::global::{Global, GlobalBuilder};
pub use crate::input::{Input, InputBuilder};
pub use crate::output::{Output, OutputBuilder};
pub use crate::role::{Combiner, Constructor, Creator, Extractor, Finalizer, Signer, Updater};
use spora_consensus_core::cell_metadata::CellMetadata;
use spora_consensus_core::config::params::Params;
use spora_consensus_core::mass::{ContextualMasses, MassCalculator};
use spora_consensus_core::{
    hashing::sighash_type::SigHashType,
    tx::{CellInput, CellOutput, CellTx, MutableTransaction, SignableTransaction, TransactionId},
};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Inner {
    /// The global map.
    pub global: Global,
    /// The corresponding key-value map for each input in the unsigned transaction.
    pub inputs: Vec<Input>,
    /// The corresponding key-value map for each output in the unsigned transaction.
    pub outputs: Vec<Output>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize_repr, Deserialize_repr)]
#[repr(u8)]
pub enum Version {
    #[default]
    Zero = 0,
    One = 1,
}

impl Display for Version {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::Zero => write!(f, "{}", Version::Zero as u8),
            Version::One => write!(f, "{}", Version::One as u8),
        }
    }
}

/// Full information on the used extended public key: fingerprint of the
/// master extended public key and a derivation path from it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct KeySource {
    #[serde(with = "spora_utils::serde_bytes_fixed")]
    pub key_fingerprint: KeyFingerprint,
    pub derivation_path: DerivationPath,
}

impl KeySource {
    pub fn new(key_fingerprint: KeyFingerprint, derivation_path: DerivationPath) -> Self {
        Self { key_fingerprint, derivation_path }
    }
}

pub type PartialSigs = BTreeMap<secp256k1::PublicKey, Signature>;

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Copy, Clone)]
#[serde(rename_all = "camelCase")]
pub enum Signature {
    ECDSA(secp256k1::ecdsa::Signature),
    Schnorr(secp256k1::schnorr::Signature),
}

impl Signature {
    pub fn into_bytes(self) -> [u8; 64] {
        match self {
            Signature::ECDSA(s) => s.serialize_compact(),
            Signature::Schnorr(s) => s.serialize(),
        }
    }
}

///
/// A Partially Signed Spora Transaction (PSST) is a standardized format
/// that allows multiple participants to collaborate in creating and signing
/// a Spora transaction. PSST enables the exchange of incomplete transaction
/// data between different wallets or entities, allowing each participant
/// to add their signature or inputs in stages. This facilitates more complex
/// transaction workflows, such as multi-signature setups or hardware wallet
/// interactions, by ensuring that sensitive data remains secure while
/// enabling cooperation across different devices or platforms without
/// exposing private keys.
///
/// Please note that due to transaction mass limits and potential of
/// a wallet aggregating large cell sets, the PSST [`Bundle`](crate::bundle::Bundle) primitive
/// is used to represent a collection of PSSTs and should be used for
/// PSST serialization and transport. PSST is an internal implementation
/// primitive that represents each transaction in the bundle.
///
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PSST<ROLE> {
    #[serde(flatten)]
    inner_psst: Inner,
    #[serde(skip_serializing, default)]
    role: PhantomData<ROLE>,
}

impl<ROLE> From<Inner> for PSST<ROLE> {
    fn from(inner_psst: Inner) -> Self {
        PSST { inner_psst, role: Default::default() }
    }
}

impl<ROLE> Clone for PSST<ROLE> {
    fn clone(&self) -> Self {
        PSST { inner_psst: self.inner_psst.clone(), role: Default::default() }
    }
}

impl<ROLE> Deref for PSST<ROLE> {
    type Target = Inner;

    fn deref(&self) -> &Self::Target {
        &self.inner_psst
    }
}

impl<R> PSST<R> {
    fn unsigned_tx(&self) -> SignableTransaction {
        let inputs = self
            .inputs
            .iter()
            .map(|Input { previous_outpoint, since, .. }| CellInput::new(*previous_outpoint, since.unwrap_or_default()))
            .collect();
        let outputs = self.outputs.iter().map(cell_out_from_psst_output).collect::<Vec<_>>();
        let outputs_data = self.outputs.iter().map(|output| output.output_data.clone().unwrap_or_default()).collect::<Vec<_>>();
        let mut witnesses = vec![vec![]; self.inputs.len()];
        if self.global.version >= Version::One {
            if let Some(payload) = self.global.payload.clone().filter(|payload| !payload.is_empty()) {
                // Preserve PSST payload in an extra witness so canonical CellTx keeps it hash-committed
                // without overloading per-input witnesses.
                witnesses.push(payload);
            }
        }
        let tx =
            CellTx::new(inputs, vec![], outputs, outputs_data, witnesses).expect("psst unsigned transaction must be constructible");
        let entries = self.inputs.iter().map(|Input { cell_entry, .. }| cell_entry.clone()).collect::<Vec<_>>();
        if entries.iter().all(Option::is_some) {
            SignableTransaction::with_entries(tx, entries.into_iter().flatten().collect())
        } else {
            SignableTransaction::new(tx)
        }
    }

    fn calculate_id_internal(&self) -> TransactionId {
        self.unsigned_tx().tx.id().into()
    }

    pub fn to_hex(&self) -> Result<String, Error> {
        Ok(format!("PSST{}", hex::encode(serde_json::to_string(self)?)))
    }

    pub fn from_hex(hex_data: &str) -> Result<Self, Error> {
        if let Some(hex_data) = hex_data.strip_prefix("PSST") {
            Ok(serde_json::from_slice(hex::decode(hex_data)?.as_slice())?)
        } else {
            Err(Error::PSSTPrefixError)
        }
    }
}

fn cell_out_from_psst_output(output: &Output) -> CellOutput {
    CellOutput { lock: output.lock_script.clone(), type_: output.type_script.clone(), capacity: output.capacity }
}

impl Default for PSST<Creator> {
    fn default() -> Self {
        PSST { inner_psst: Default::default(), role: Default::default() }
    }
}

impl PSST<Creator> {
    /// Sets the PSST version.
    pub fn set_version(mut self, version: Version) -> Self {
        self.inner_psst.global.version = version;
        self
    }

    // todo generic const
    /// Sets the inputs modifiable bit in the transaction modifiable flags.
    pub fn inputs_modifiable(mut self) -> Self {
        self.inner_psst.global.inputs_modifiable = true;
        self
    }
    // todo generic const
    /// Sets the outputs modifiable bit in the transaction modifiable flags.
    pub fn outputs_modifiable(mut self) -> Self {
        self.inner_psst.global.outputs_modifiable = true;
        self
    }

    pub fn constructor(self) -> PSST<Constructor> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }
}

impl PSST<Constructor> {
    // todo generic const
    /// Marks that the `PSST` can not have any more inputs added to it.
    pub fn no_more_inputs(mut self) -> Self {
        self.inner_psst.global.inputs_modifiable = false;
        self
    }
    // todo generic const
    /// Marks that the `PSST` can not have any more outputs added to it.
    pub fn no_more_outputs(mut self) -> Self {
        self.inner_psst.global.outputs_modifiable = false;
        self
    }

    /// Adds an input to the PSST.
    pub fn input(mut self, input: Input) -> Self {
        self.inner_psst.inputs.push(input);
        self.inner_psst.global.input_count += 1;
        self
    }

    /// Adds an output to the PSST.
    pub fn output(mut self, output: Output) -> Self {
        self.inner_psst.outputs.push(output);
        self.inner_psst.global.output_count += 1;
        self
    }

    pub fn payload(mut self, payload: Option<Vec<u8>>) -> Result<Self, Error> {
        // Only allow setting payload if version is One or greater
        if payload.is_some() && self.inner_psst.global.version < Version::One {
            return Err(Error::PayloadRequiresVersion1(self.inner_psst.global.version));
        }
        self.inner_psst.global.payload = payload;
        Ok(self)
    }

    /// Returns a PSST [`Updater`] once construction is completed.
    pub fn updater(self) -> PSST<Updater> {
        let psst = self.no_more_inputs().no_more_outputs();
        PSST { inner_psst: psst.inner_psst, role: Default::default() }
    }

    pub fn signer(self) -> PSST<Signer> {
        self.updater().signer()
    }

    pub fn combiner(self) -> PSST<Combiner> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }
}

impl PSST<Updater> {
    pub fn set_since(mut self, n: u64, input_index: usize) -> Result<Self, Error> {
        self.inner_psst.inputs.get_mut(input_index).ok_or(Error::OutOfBounds)?.since = Some(n);
        Ok(self)
    }

    pub fn signer(self) -> PSST<Signer> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }

    pub fn combiner(self) -> PSST<Combiner> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }
}

impl PSST<Signer> {
    // todo use iterator instead of vector
    pub fn pass_signature_sync<SignFn, E>(mut self, sign_fn: SignFn) -> Result<Self, E>
    where
        E: Display,
        SignFn: FnOnce(SignableTransaction, Vec<SigHashType>) -> Result<Vec<SignInputOk>, E>,
    {
        let unsigned_tx = self.unsigned_tx();
        let sighashes = self.inputs.iter().map(|input| input.sighash_type).collect();
        self.inner_psst.inputs.iter_mut().zip(sign_fn(unsigned_tx, sighashes)?).for_each(
            |(input, SignInputOk { signature, pub_key, key_source })| {
                input.bip32_derivations.insert(pub_key, key_source);
                input.partial_sigs.insert(pub_key, signature);
            },
        );

        Ok(self)
    }
    // todo use iterator instead of vector
    pub async fn pass_signature<SignFn, Fut, E>(mut self, sign_fn: SignFn) -> Result<Self, E>
    where
        E: Display,
        Fut: Future<Output = Result<Vec<SignInputOk>, E>>,
        SignFn: FnOnce(SignableTransaction, Vec<SigHashType>) -> Fut,
    {
        let unsigned_tx = self.unsigned_tx();
        let sighashes = self.inputs.iter().map(|input| input.sighash_type).collect();
        self.inner_psst.inputs.iter_mut().zip(sign_fn(unsigned_tx, sighashes).await?).for_each(
            |(input, SignInputOk { signature, pub_key, key_source })| {
                input.bip32_derivations.insert(pub_key, key_source);
                input.partial_sigs.insert(pub_key, signature);
            },
        );
        Ok(self)
    }

    pub fn calculate_id(&self) -> TransactionId {
        self.calculate_id_internal()
    }

    pub fn finalizer(self) -> PSST<Finalizer> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }

    pub fn combiner(self) -> PSST<Combiner> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }

    // Unorphan batch transaction cell.
    pub fn set_input_prev_transaction_id(self, transaction_id: Hash) -> PSST<Signer> {
        let mut new_inputs = self.inner_psst.inputs.clone();

        new_inputs.iter_mut().for_each(|input| {
            input.previous_outpoint.tx_hash = transaction_id.as_bytes();
        });

        let mut updated_inner = self.inner_psst.clone();
        updated_inner.inputs = new_inputs;

        PSST { inner_psst: updated_inner, role: Default::default() }
    }
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignInputOk {
    pub signature: Signature,
    pub pub_key: secp256k1::PublicKey,
    pub key_source: Option<KeySource>,
}

impl<R> std::ops::Add<PSST<R>> for PSST<Combiner> {
    type Output = Result<Self, CombineError>;

    fn add(mut self, mut rhs: PSST<R>) -> Self::Output {
        self.inner_psst.global = (self.inner_psst.global + rhs.inner_psst.global)?;
        macro_rules! combine {
            ($left:expr, $right:expr, $err: ty) => {
                if $left.len() > $right.len() {
                    $left.iter_mut().zip($right.iter_mut()).try_for_each(|(left, right)| -> Result<(), $err> {
                        *left = (std::mem::take(left) + std::mem::take(right))?;
                        Ok(())
                    })?;
                    $left
                } else {
                    $right.iter_mut().zip($left.iter_mut()).try_for_each(|(left, right)| -> Result<(), $err> {
                        *left = (std::mem::take(left) + std::mem::take(right))?;
                        Ok(())
                    })?;
                    $right
                }
            };
        }
        // todo add sort to build deterministic combination
        self.inner_psst.inputs = combine!(self.inner_psst.inputs, rhs.inner_psst.inputs, crate::input::CombineError);
        self.inner_psst.outputs = combine!(self.inner_psst.outputs, rhs.inner_psst.outputs, crate::output::CombineError);
        Ok(self)
    }
}

impl PSST<Combiner> {
    pub fn signer(self) -> PSST<Signer> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }
    pub fn finalizer(self) -> PSST<Finalizer> {
        PSST { inner_psst: self.inner_psst, role: Default::default() }
    }
}

impl PSST<Finalizer> {
    pub fn finalize_sync<E: Display>(
        self,
        final_sig_fn: impl FnOnce(&Inner) -> Result<Vec<Vec<u8>>, E>,
    ) -> Result<Self, FinalizeError<E>> {
        let sigs = final_sig_fn(&self);
        self.finalize_internal(sigs)
    }

    pub async fn finalize<F, Fut, E>(self, final_sig_fn: F) -> Result<Self, FinalizeError<E>>
    where
        E: Display,
        F: FnOnce(&Inner) -> Fut,
        Fut: Future<Output = Result<Vec<Vec<u8>>, E>>,
    {
        let sigs = final_sig_fn(&self).await;
        self.finalize_internal(sigs)
    }

    pub fn id(&self) -> Option<TransactionId> {
        self.global.id
    }

    pub fn extractor(self) -> Result<PSST<Extractor>, TxNotFinalized> {
        if self.global.id.is_none() {
            Err(TxNotFinalized {})
        } else {
            Ok(PSST { inner_psst: self.inner_psst, role: Default::default() })
        }
    }

    fn finalize_internal<E: Display>(mut self, sigs: Result<Vec<Vec<u8>>, E>) -> Result<Self, FinalizeError<E>> {
        let sigs = sigs?;
        if sigs.len() != self.inputs.len() {
            return Err(FinalizeError::WrongFinalizedSigsCount { expected: self.inputs.len(), actual: sigs.len() });
        }
        self.inner_psst.inputs.iter_mut().enumerate().zip(sigs).try_for_each(|((idx, input), sig)| {
            if sig.is_empty() {
                return Err(FinalizeError::EmptySignature(idx));
            }
            input.since = Some(input.since.unwrap_or_default());
            input.final_witness = Some(sig);
            Ok(())
        })?;
        self.inner_psst.global.id = Some(self.calculate_id_internal());
        Ok(self)
    }
}

impl PSST<Extractor> {
    pub fn extract_tx_unchecked(self, params: &Params) -> Result<SignableTransaction, TxNotFinalized> {
        let tx = self.unsigned_tx();
        let entries = tx.entries;
        let resolved_cell_metadata = entries.iter().map(|entry| entry.as_ref().map(|entry| CellMetadata::from(entry))).collect();
        let mut tx = tx.tx;
        tx.witnesses.iter_mut().zip(self.inner_psst.inputs).try_for_each(|(dest, src)| {
            *dest = src.final_witness.ok_or(TxNotFinalized {})?;
            Ok(())
        })?;
        let mut tx = MutableTransaction {
            tx,
            entries,
            resolved_cell_metadata,
            calculated_fee: None,
            calculated_non_contextual_masses: None,
            calculated_contextual_masses: None,
            verified_cycles: None,
        };
        let calculator = MassCalculator::new_with_consensus_params(params);
        let storage_mass = calculator.calc_contextual_masses(&tx.as_verifiable()).map(|mass| mass.storage_mass).unwrap_or_default();
        let non_contextual_masses = calculator.calc_non_contextual_masses_cell(&tx.tx);
        let _mass = storage_mass.max(non_contextual_masses.compute_mass).max(non_contextual_masses.transient_mass);
        tx.calculated_non_contextual_masses = Some(non_contextual_masses);
        tx.calculated_contextual_masses = Some(ContextualMasses::new(storage_mass));
        Ok(tx)
    }

    pub fn extract_tx(self, params: &Params) -> Result<SignableTransaction, ExtractError> {
        self.extract_tx_unchecked(params).map_err(Into::into)
    }
}

/// Error combining psst.
#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum CombineError {
    #[error(transparent)]
    Global(#[from] crate::global::CombineError),
    #[error(transparent)]
    Inputs(#[from] crate::input::CombineError),
    #[error(transparent)]
    Outputs(#[from] crate::output::CombineError),
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum FinalizeError<E> {
    #[error("Signatures count mismatch")]
    WrongFinalizedSigsCount { expected: usize, actual: usize },
    #[error("Signatures at index: {0} is empty")]
    EmptySignature(usize),
    #[error(transparent)]
    FinalaziCb(#[from] E),
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
    #[error(transparent)]
    TxNotFinalized(#[from] TxNotFinalized),
    #[error("Missing cell entry for input {0}")]
    MissingCellEntry(usize),
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
#[error("Transaction is not finalized")]
pub struct TxNotFinalized {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::InputBuilder;
    use crate::output::OutputBuilder;
    use crate::role::Creator;
    use secp256k1::{rand::thread_rng, Keypair, Secp256k1};
    use spora_addresses::{Address, Prefix};
    use spora_consensus_core::{
        cell_diff::CellMeta,
        config::params::TESTNET_PARAMS,
        hashing::sighash::{calc_schnorr_signature_hash, SigHashReusedValuesUnsync},
        tx::{
            outpoint_from_id, pay_to_address_lock_script, push_data_script, CellInput, CellOutput, CellTx, Script, TransactionId,
            TransactionOutpoint,
        },
    };
    use std::str::FromStr;

    fn test_cell_meta_from_lock_script(amount: u64, lock_script: Script, block_daa_score: u64, is_coinbase: bool) -> CellMeta {
        CellMeta::from_cell_metadata(amount, 0, lock_script.code_hash, None, [0; 32], block_daa_score, is_coinbase)
            .with_resolved_metadata(Some(lock_script), None, Some(Vec::new()))
    }

    #[test]
    fn test_payload_version_zero() {
        // Test that payload cannot be set on Version::Zero
        let psst = PSST::<Creator>::default().set_version(Version::Zero).constructor();

        let result = psst.payload(Some(vec![1, 2, 3]));
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::PayloadRequiresVersion1(version) => {
                assert_eq!(version, Version::Zero);
            }
            _ => panic!("Expected PayloadRequiresVersion1 error"),
        }
    }

    #[test]
    fn test_payload_version_one() {
        // Test that payload can be set on Version::One
        let psst = PSST::<Creator>::default().set_version(Version::One).constructor();

        let payload_data = vec![1, 2, 3, 4, 5];
        let result = psst.payload(Some(payload_data.clone()));
        assert!(result.is_ok());

        let psst = result.unwrap();
        assert_eq!(psst.global.payload, Some(payload_data));
    }

    #[test]
    fn test_payload_none() {
        // Test that None payload works on any version
        let psst = PSST::<Creator>::default().set_version(Version::Zero).constructor();

        let result = psst.payload(None);
        assert!(result.is_ok());

        let psst = result.unwrap();
        assert_eq!(psst.global.payload, None);
    }

    #[test]
    fn test_payload_combination_same() {
        // Test combining PSSTs with same payload
        let payload_data = vec![1, 2, 3];

        let psst1 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone())).unwrap();

        let psst2 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone())).unwrap();

        let combiner1 = psst1.combiner();
        let result = combiner1 + psst2;
        assert!(result.is_ok());

        let combined = result.unwrap();
        assert_eq!(combined.global.payload, Some(payload_data));
    }

    #[test]
    fn test_payload_combination_different() {
        // Test combining PSSTs with different payloads should fail
        let payload1 = vec![1, 2, 3];
        let payload2 = vec![4, 5, 6];

        let psst1 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload1)).unwrap();

        let psst2 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload2)).unwrap();

        let combiner1 = psst1.combiner();
        let result = combiner1 + psst2;
        assert!(result.is_err());
    }

    #[test]
    fn test_payload_combination_one_none() {
        // Test combining PSST with payload and PSST without payload
        let payload_data = vec![1, 2, 3];

        let psst1 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(Some(payload_data.clone())).unwrap();

        let psst2 = PSST::<Creator>::default().set_version(Version::One).constructor().payload(None).unwrap();

        let combiner1 = psst1.combiner();
        let result = combiner1 + psst2;
        assert!(result.is_ok());

        let combined = result.unwrap();
        assert_eq!(combined.global.payload, Some(payload_data));
    }

    #[test]
    fn extract_tx_validates_direct_schnorr_input_with_local_validator() {
        let secp = Secp256k1::new();
        let signer = Keypair::new(&secp, &mut thread_rng());
        let signer_address = Address::new_std_single(Prefix::Testnet, &signer.x_only_public_key().0.serialize()).unwrap();
        let recipient = Address::new_std_single(Prefix::Testnet, &[0x22; 32]).unwrap();
        let signer_script = pay_to_address_lock_script(&signer_address);

        let input = InputBuilder::default()
            .cell_entry(test_cell_meta_from_lock_script(12793000000000, signer_script.clone(), 36151168, false))
            .previous_outpoint(outpoint_from_id(
                TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap(),
                0,
            ))
            .build()
            .unwrap();
        let recipient_script = pay_to_address_lock_script(&recipient);
        let output = OutputBuilder::default().capacity(12792999900000).lock_script(recipient_script).build().unwrap();

        let signer_psst = PSST::<Creator>::default().constructor().input(input).output(output).signer();
        let reused_values = SigHashReusedValuesUnsync::new();
        let signed = signer_psst
            .pass_signature_sync(|tx, sighash| -> Result<Vec<SignInputOk>, String> {
                let hash = calc_schnorr_signature_hash(&tx.as_verifiable(), 0, sighash[0], &reused_values);
                let msg = secp256k1::Message::from_digest_slice(&hash.as_bytes()).map_err(|e| e.to_string())?;
                Ok(vec![SignInputOk {
                    signature: Signature::Schnorr(signer.sign_schnorr(msg)),
                    pub_key: signer.public_key(),
                    key_source: None,
                }])
            })
            .unwrap();
        let finalized = signed
            .finalizer()
            .finalize_sync(|inner| -> Result<Vec<Vec<u8>>, String> {
                let mut witness = Vec::new();
                let mut sig = Vec::from(inner.inputs[0].partial_sigs.values().next().unwrap().into_bytes());
                sig.push(inner.inputs[0].sighash_type.to_u8());
                witness.extend(push_data_script(&sig).map_err(|e| e.to_string())?);
                Ok(vec![witness])
            })
            .unwrap();

        let extracted = finalized.extractor().unwrap().extract_tx(&TESTNET_PARAMS).unwrap();
        assert_eq!(extracted.tx.inputs.len(), 1);
    }

    #[test]
    fn unsigned_tx_prefers_native_cell_output_fields() {
        let lock = Script::new([7u8; 32], 1, vec![1, 2, 3, 4]);
        let type_script = Script::new([8u8; 32], 0, vec![5, 6, 7]);
        let output_data = vec![9, 10, 11];
        let cell_tx = CellTx::new(
            vec![CellInput::new(TransactionOutpoint::new([3u8; 32], 1), 42)],
            vec![],
            vec![CellOutput { lock: lock.clone(), type_: Some(type_script.clone()), capacity: 1234 }],
            vec![output_data.clone()],
            vec![vec![]],
        )
        .unwrap();

        let inner = Inner::try_from(cell_tx).unwrap();
        let psst = PSST::<Creator>::from(inner);
        let unsigned = psst.unsigned_tx();

        assert_eq!(unsigned.tx.inputs[0].since, 42);
        assert_eq!(unsigned.tx.outputs[0].lock, lock);
        assert_eq!(unsigned.tx.outputs[0].type_, Some(type_script));
        assert_eq!(unsigned.tx.outputs_data[0], output_data);
    }
}
