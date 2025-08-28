use crate::error::Error;
use crate::prelude::*;
use crate::pstt::{Inner as PSTTInner, PSTT};
// use crate::wasm::result;

use tondi_addresses::{Address, Prefix};
// use tondi_bip32::Prefix;
use tondi_consensus_core::network::{NetworkId, NetworkType};
use tondi_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};

use hex;
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use tondi_txscript::{extract_script_pub_key_address, pay_to_address_script, pay_to_script_hash_script};

///
/// Bundle is a [`PSTT`] bundle - a sequence of PSTT transactions
/// meant for batch processing and transport as a
/// single serialized payload.
///
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle(pub Vec<PSTTInner>);

impl<ROLE> From<PSTT<ROLE>> for Bundle {
    fn from(pstt: PSTT<ROLE>) -> Self {
        Bundle(vec![pstt.deref().clone()])
    }
}

impl<ROLE> From<Vec<PSTT<ROLE>>> for Bundle {
    fn from(pstts: Vec<PSTT<ROLE>>) -> Self {
        let inner_list = pstts.into_iter().map(|pstt| pstt.deref().clone()).collect();
        Bundle(inner_list)
    }
}

impl Bundle {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Adds an Inner instance to the bundle
    pub fn add_inner(&mut self, inner: PSTTInner) {
        self.0.push(inner);
    }

    /// Adds a PSTT instance to the bundle
    pub fn add_pstt<ROLE>(&mut self, pstt: PSTT<ROLE>) {
        self.0.push(pstt.deref().clone());
    }

    /// Merges another bundle into the current bundle
    pub fn merge(&mut self, other: Bundle) {
        for inner in other.0 {
            self.0.push(inner);
        }
    }

    /// Iterator over the inner PSTT instances
    pub fn iter(&self) -> std::slice::Iter<'_, PSTTInner> {
        self.0.iter()
    }

    pub fn serialize(&self) -> Result<String, Error> {
        Ok(format!("PSTB{}", hex::encode(serde_json::to_string(self)?)))
    }

    pub fn deserialize(hex_data: &str) -> Result<Self, Error> {
        if let Some(hex_data) = hex_data.strip_prefix("PSTB") {
            Ok(serde_json::from_slice(hex::decode(hex_data)?.as_slice())?)
        } else {
            Err(Error::PSTBPrefixError)
        }
    }

    pub fn display_format<F>(&self, network_id: NetworkId, sau_formatter: F) -> String
    where
        F: Fn(u64, &NetworkType) -> String,
    {
        let mut result = "".to_string();

        for (pstt_index, bundle_inner) in self.0.iter().enumerate() {
            let pstt: PSTT<Signer> = PSTT::<Signer>::from(bundle_inner.to_owned());

            result.push_str(&format!("\r\nPSTT #{:02}\r\n", pstt_index + 1));

            for (key_inner, input) in pstt.clone().inputs.iter().enumerate() {
                result.push_str(&format!("Input #{:02}\r\n", key_inner + 1));

                if let Some(utxo_entry) = &input.utxo_entry {
                    result.push_str(&format!("  amount: {}\r\n", sau_formatter(utxo_entry.amount, &NetworkType::from(network_id))));
                    result.push_str(&format!(
                        "  address: {}\r\n",
                        extract_script_pub_key_address(&utxo_entry.script_public_key, Prefix::from(network_id))
                            .expect("Input address")
                    ));
                }
            }

            result.push_str("---\r\n");

            for (key_inner, output) in pstt.clone().outputs.iter().enumerate() {
                result.push_str(&format!("Output #{:02}\r\n", key_inner + 1));
                result.push_str(&format!("  amount: {}\r\n", sau_formatter(output.amount, &NetworkType::from(network_id))));
                result.push_str(&format!(
                    "  address: {}\r\n",
                    extract_script_pub_key_address(&output.script_public_key, Prefix::from(network_id)).expect("Input address")
                ));
            }
        }
        result
    }
}

impl AsRef<[PSTTInner]> for Bundle {
    fn as_ref(&self) -> &[PSTTInner] {
        self.0.as_slice()
    }
}

impl TryFrom<String> for Bundle {
    type Error = Error;
    fn try_from(value: String) -> Result<Self, Error> {
        Bundle::deserialize(&value)
    }
}

impl TryFrom<&str> for Bundle {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Error> {
        Bundle::deserialize(value)
    }
}
impl TryFrom<Bundle> for String {
    type Error = Error;
    fn try_from(value: Bundle) -> Result<String, Error> {
        match Bundle::serialize(&value) {
            Ok(output) => Ok(output.to_owned()),
            Err(e) => Err(Error::PSTBSerializeError(e.to_string())),
        }
    }
}

impl Default for Bundle {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lock_script_sig_templating(payload: String, pubkey_bytes: Option<&[u8]>) -> Result<Vec<u8>, Error> {
    let mut payload_bytes: Vec<u8> = hex::decode(payload)?;

    if let Some(pubkey) = pubkey_bytes {
        let placeholder = b"{{pubkey}}";

        // Search for the placeholder in payload bytes to be replaced by public key.
        if let Some(pos) = payload_bytes.windows(placeholder.len()).position(|window| window == placeholder) {
            payload_bytes.splice(pos..pos + placeholder.len(), pubkey.iter().cloned());
        }
    }
    Ok(payload_bytes)
}

pub fn script_sig_to_address(script_sig: &[u8], prefix: tondi_addresses::Prefix) -> Result<Address, Error> {
    extract_script_pub_key_address(&pay_to_script_hash_script(script_sig), prefix).map_err(Error::P2SHExtractError)
}

pub fn unlock_utxos_as_pstb(
    utxo_references: Vec<(UtxoEntry, TransactionOutpoint)>,
    recipient: &Address,
    script_sig: Vec<u8>,
    priority_fee_sau_per_transaction: u64,
) -> Result<Bundle, Error> {
    // Fee per transaction.
    // Check if each UTXO's amounts can cover priority fee.
    utxo_references
        .iter()
        .map(|(entry, _)| {
            if entry.amount <= priority_fee_sau_per_transaction {
                return Err(Error::ExcessUnlockFeeError);
            }
            Ok(())
        })
        .collect::<Result<Vec<_>, _>>()?;

    let recipient_spk = pay_to_address_script(recipient);
    let (successes, errors): (Vec<_>, Vec<_>) = utxo_references
        .into_iter()
        .map(|(utxo_entry, outpoint)| {
            unlock_utxo(&utxo_entry, &outpoint, &recipient_spk, &script_sig, priority_fee_sau_per_transaction)
        })
        .partition(Result::is_ok);

    let successful_bundles: Vec<_> = successes.into_iter().filter_map(Result::ok).collect();
    let error_list: Vec<_> = errors.into_iter().filter_map(Result::err).collect();

    if !error_list.is_empty() {
        return Err(Error::MultipleUnlockUtxoError(error_list));
    }

    let merged_bundle = successful_bundles.into_iter().fold(None, |acc: Option<Bundle>, bundle| match acc {
        Some(mut merged_bundle) => {
            merged_bundle.merge(bundle);
            Some(merged_bundle)
        }
        None => Some(bundle),
    });

    match merged_bundle {
        None => Err("Generating an empty pstb".into()),
        Some(bundle) => Ok(bundle),
    }
}

pub fn unlock_utxo(
    utxo_entry: &UtxoEntry,
    outpoint: &TransactionOutpoint,
    script_public_key: &ScriptPublicKey,
    script_sig: &[u8],
    priority_fee_sau: u64,
) -> Result<Bundle, Error> {
    let input = InputBuilder::default()
        .utxo_entry(utxo_entry.to_owned())
        .previous_outpoint(outpoint.to_owned())
        .sig_op_count(1)
        .redeem_script(script_sig.to_vec())
        .build()?;

    let output = OutputBuilder::default()
        .amount(utxo_entry.amount - priority_fee_sau)
        .script_public_key(script_public_key.clone())
        .build()?;

    let pstt: PSTT<Constructor> = PSTT::<Creator>::default().constructor().input(input).output(output);
    Ok(pstt.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::role::Creator;
    use crate::role::*;
    use secp256k1::Secp256k1;
    use secp256k1::{rand::thread_rng, Keypair};
    use std::str::FromStr;
    use std::sync::LazyLock;
    use tondi_consensus_core::tx::{TransactionId, TransactionOutpoint, UtxoEntry};
    use tondi_txscript::{multisig_redeem_script, pay_to_script_hash_script};

    static CONTEXT: LazyLock<Box<([Keypair; 2], Vec<u8>)>> = LazyLock::new(|| {
        let kps = [Keypair::new(&Secp256k1::new(), &mut thread_rng()), Keypair::new(&Secp256k1::new(), &mut thread_rng())];
        let redeem_script: Vec<u8> =
            multisig_redeem_script(kps.iter().map(|pk| pk.x_only_public_key().0.serialize()), 2).expect("Test multisig redeem script");

        Box::new((kps, redeem_script))
    });

    fn mock_context() -> &'static ([Keypair; 2], Vec<u8>) {
        CONTEXT.as_ref()
    }

    // Mock multisig PSTT from example
    fn mock_pstt_constructor() -> PSTT<Constructor> {
        let (_, redeem_script) = mock_context();
        let pstt = PSTT::<Creator>::default().inputs_modifiable().outputs_modifiable();
        let input_0 = InputBuilder::default()
            .utxo_entry(UtxoEntry {
                amount: 12793000000000,
                script_public_key: pay_to_script_hash_script(redeem_script),
                block_daa_score: 36151168,
                is_coinbase: false,
            })
            .previous_outpoint(TransactionOutpoint {
                transaction_id: TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap(),
                index: 0,
            })
            .sig_op_count(2)
            .redeem_script(redeem_script.to_owned())
            .build()
            .expect("Mock pstt constructor");

        pstt.constructor().input(input_0)
    }

    #[test]
    fn test_pstb_serialization() {
        let constructor = mock_pstt_constructor();
        let bundle = Bundle::from(constructor.clone());

        println!("Bundle: {}", serde_json::to_string(&bundle).unwrap());

        // Serialize Bundle
        let serialized = bundle.serialize().map_err(|err| format!("Unable to serialize bundle: {err}")).unwrap();
        println!("Serialized: {}", serialized);

        assert!(!bundle.0.is_empty());

        match Bundle::deserialize(&serialized) {
            Ok(bundle_constructor_deser) => {
                println!("Deserialized: {:?}", bundle_constructor_deser);
                let pstt_constructor_deser: Option<PSTT<Constructor>> =
                    bundle_constructor_deser.0.first().map(|inner| PSTT::from(inner.clone()));
                match pstt_constructor_deser {
                    Some(_) => println!("pstt<Constructor> deserialized successfully"),
                    None => println!("No elements in the inner list to deserialize"),
                }
            }
            Err(e) => {
                eprintln!("Failed to deserialize: {}", e);
                panic!()
            }
        }
    }

    #[test]
    fn test_pstb_bundle_creation() {
        let bundle = Bundle::new();
        assert!(bundle.0.is_empty());
    }

    #[test]
    fn test_pstb_new_with_pstt() {
        let pstt = PSTT::<Creator>::default();
        let bundle = Bundle::from(pstt);
        assert_eq!(bundle.0.len(), 1);
    }

    #[test]
    fn test_pstb_add_pstt() {
        let mut bundle = Bundle::new();
        let pstt = PSTT::<Creator>::default();
        bundle.add_pstt(pstt);
        assert_eq!(bundle.0.len(), 1);
    }

    #[test]
    fn test_pstb_merge_bundles() {
        let mut bundle1 = Bundle::new();
        let mut bundle2 = Bundle::new();

        let inner1 = PSTTInner::default();
        let inner2 = PSTTInner::default();

        bundle1.add_inner(inner1.clone());
        bundle2.add_inner(inner2.clone());

        bundle1.merge(bundle2);

        assert_eq!(bundle1.0.len(), 2);
    }
}
