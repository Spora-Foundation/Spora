use crate::error::Error;
use crate::prelude::*;
use crate::psst::{Inner as PSSTInner, PSST};
// use crate::wasm::result;

use spora_addresses::{Address, Prefix};
// use spora_bip32::Prefix;
use hex;
use serde::{Deserialize, Serialize};
use spora_consensus_client::{extract_address_from_lock_script, pay_to_address_lock_script, pay_to_script_hash_lock_script};
use spora_consensus_core::cell_diff::CellMeta;
use spora_consensus_core::constants::UNACCEPTED_DAA_SCORE;
use spora_consensus_core::network::{NetworkId, NetworkType};
use spora_consensus_core::tx::{ScriptRef, TransactionOutpoint};
use std::ops::Deref;

///
/// Bundle is a [`PSST`] bundle - a sequence of PSST transactions
/// meant for batch processing and transport as a
/// single serialized payload.
///
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bundle(pub Vec<PSSTInner>);

impl<ROLE> From<PSST<ROLE>> for Bundle {
    fn from(psst: PSST<ROLE>) -> Self {
        Bundle(vec![psst.deref().clone()])
    }
}

impl<ROLE> From<Vec<PSST<ROLE>>> for Bundle {
    fn from(pssts: Vec<PSST<ROLE>>) -> Self {
        let inner_list = pssts.into_iter().map(|psst| psst.deref().clone()).collect();
        Bundle(inner_list)
    }
}

impl Bundle {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Adds an Inner instance to the bundle
    pub fn add_inner(&mut self, inner: PSSTInner) {
        self.0.push(inner);
    }

    /// Adds a PSST instance to the bundle
    pub fn add_psst<ROLE>(&mut self, psst: PSST<ROLE>) {
        self.0.push(psst.deref().clone());
    }

    /// Merges another bundle into the current bundle
    pub fn merge(&mut self, other: Bundle) {
        for inner in other.0 {
            self.0.push(inner);
        }
    }

    /// Iterator over the inner PSST instances
    pub fn iter(&self) -> std::slice::Iter<'_, PSSTInner> {
        self.0.iter()
    }

    pub fn serialize(&self) -> Result<String, Error> {
        Ok(format!("PSSB{}", hex::encode(serde_json::to_string(self)?)))
    }

    pub fn deserialize(hex_data: &str) -> Result<Self, Error> {
        if let Some(hex_data) = hex_data.strip_prefix("PSSB") {
            Ok(serde_json::from_slice(hex::decode(hex_data)?.as_slice())?)
        } else {
            Err(Error::PSSBPrefixError)
        }
    }

    pub fn display_format<F>(&self, network_id: NetworkId, sau_formatter: F) -> String
    where
        F: Fn(u64, &NetworkType) -> String,
    {
        let mut result = "".to_string();

        for (psst_index, bundle_inner) in self.0.iter().enumerate() {
            let psst: PSST<Signer> = PSST::<Signer>::from(bundle_inner.to_owned());

            result.push_str(&format!("\r\nPSST #{:02}\r\n", psst_index + 1));

            for (key_inner, input) in psst.clone().inputs.iter().enumerate() {
                result.push_str(&format!("Input #{:02}\r\n", key_inner + 1));

                if let Some(cell_entry) = &input.cell_entry {
                    result.push_str(&format!("  amount: {}\r\n", sau_formatter(cell_entry.amount(), &NetworkType::from(network_id))));
                    result.push_str("  address: <unavailable>\r\n");
                }
            }

            result.push_str("---\r\n");

            for (key_inner, output) in psst.clone().outputs.iter().enumerate() {
                result.push_str(&format!("Output #{:02}\r\n", key_inner + 1));
                result.push_str(&format!("  amount: {}\r\n", sau_formatter(output.capacity, &NetworkType::from(network_id))));
            }
        }
        result
    }
}

impl AsRef<[PSSTInner]> for Bundle {
    fn as_ref(&self) -> &[PSSTInner] {
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
            Err(e) => Err(Error::PSSBSerializeError(e.to_string())),
        }
    }
}

impl Default for Bundle {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lock_script_sig_templating(payload: String, pubkey_bytes: Option<&[u8]>) -> Result<Vec<u8>, Error> {
    let payload_bytes: Vec<u8> = hex::decode(payload)?;
    lock_script_sig_templating_bytes(payload_bytes.to_vec(), pubkey_bytes)
}

pub fn lock_script_sig_templating_bytes(payload: Vec<u8>, pubkey_bytes: Option<&[u8]>) -> Result<Vec<u8>, Error> {
    let mut payload_bytes = payload;

    if let Some(pubkey) = pubkey_bytes {
        let placeholder = b"{{pubkey}}";

        // Search for the placeholder in payload bytes to be replaced by public key.
        if let Some(pos) = payload_bytes.windows(placeholder.len()).position(|window| window == placeholder) {
            payload_bytes.splice(pos..pos + placeholder.len(), pubkey.iter().cloned());
        }
    }
    Ok(payload_bytes)
}

pub fn script_sig_to_address(script_sig: &[u8], prefix: spora_addresses::Prefix) -> Result<Address, Error> {
    address_from_lock_script(&pay_to_script_hash_lock_script(script_sig), prefix)
}

pub fn unlock_cells_as_pssb(
    cell_references: Vec<(CellMeta, TransactionOutpoint)>,
    recipient: &Address,
    script_sig: Vec<u8>,
    priority_fee_sau_per_transaction: u64,
) -> Result<Bundle, Error> {
    // Fee per transaction.
    // Check if each cell amount can cover the priority fee.
    cell_references
        .iter()
        .map(|(entry, _)| {
            if entry.amount() <= priority_fee_sau_per_transaction {
                return Err(Error::ExcessUnlockFeeError);
            }
            Ok(())
        })
        .collect::<Result<Vec<_>, _>>()?;

    let recipient_lock = pay_to_address_lock_script(recipient);
    let (successes, errors): (Vec<_>, Vec<_>) = cell_references
        .into_iter()
        .map(|(cell_entry, outpoint)| {
            unlock_cell(&cell_entry, &outpoint, &recipient_lock, &script_sig, priority_fee_sau_per_transaction)
        })
        .partition(Result::is_ok);

    let successful_bundles: Vec<_> = successes.into_iter().filter_map(Result::ok).collect();
    let error_list: Vec<_> = errors.into_iter().filter_map(Result::err).collect();

    if !error_list.is_empty() {
        return Err(Error::MultipleUnlockCellError(error_list));
    }

    let merged_bundle = successful_bundles.into_iter().fold(None, |acc: Option<Bundle>, bundle| match acc {
        Some(mut merged_bundle) => {
            merged_bundle.merge(bundle);
            Some(merged_bundle)
        }
        None => Some(bundle),
    });

    match merged_bundle {
        None => Err("Generating an empty pssb".into()),
        Some(bundle) => Ok(bundle),
    }
}

pub fn unlock_cell(
    cell_entry: &CellMeta,
    outpoint: &TransactionOutpoint,
    lock_script: &ScriptRef,
    script_sig: &[u8],
    _priority_fee_sau: u64,
) -> Result<Bundle, Error> {
    let input = InputBuilder::default()
        .cell_entry(cell_entry.to_owned())
        .previous_outpoint(outpoint.to_owned())
        .redeem_script(script_sig.to_vec())
        .build()?;

    let output =
        OutputBuilder::default().capacity(cell_entry.amount() - _priority_fee_sau).lock_script(lock_script.clone()).build()?;

    let psst: PSST<Constructor> = PSST::<Creator>::default().constructor().input(input).output(output);
    Ok(psst.into())
}

// Build a cell-spending PSSB with custom input and multiple outputs
// to be used in atomic transaction batch.
pub fn unlock_cell_outputs_as_batch_transaction_pssb(
    amount: u64,
    start_address: &Address,
    script_sig: &[u8],
    destination_outputs: Vec<(Address, u64)>,
) -> Result<Bundle, Error> {
    let origin_lock = pay_to_address_lock_script(start_address);
    let cell_entry = direct_cell_meta_from_script(amount, origin_lock, UNACCEPTED_DAA_SCORE, false);

    let input = InputBuilder::default().cell_entry(cell_entry.to_owned()).redeem_script(script_sig.to_vec()).build()?;

    let outputs: Vec<Output> = destination_outputs
        .iter()
        .filter_map(|(address, amount)| {
            OutputBuilder::default().capacity(*amount).lock_script(pay_to_address_lock_script(address)).build().ok()
        })
        .collect();

    let psst: PSST<Constructor> =
        outputs.into_iter().fold(PSST::<Creator>::default().constructor().input(input), |psst, output| psst.output(output));
    Ok(psst.into())
}

fn direct_cell_meta_from_script(amount: u64, lock_script: ScriptRef, block_daa_score: u64, is_coinbase: bool) -> CellMeta {
    CellMeta::from_cell_metadata(amount, 0, lock_script.hash(), None, [0; 32], block_daa_score, is_coinbase)
}

fn address_from_lock_script(lock_script: &ScriptRef, prefix: Prefix) -> Result<Address, Error> {
    extract_address_from_lock_script(lock_script.args.as_slice(), prefix).map_err(|err| Error::Custom(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;
    use crate::role::Creator;
    use crate::role::*;
    use secp256k1::Secp256k1;
    use secp256k1::{rand::thread_rng, Keypair};
    use spora_consensus_core::tx::{multisig_redeem_script, outpoint_from_id, TransactionId};
    use std::str::FromStr;
    use std::sync::LazyLock;

    static CONTEXT: LazyLock<Box<([Keypair; 2], Vec<u8>)>> = LazyLock::new(|| {
        let kps = [Keypair::new(&Secp256k1::new(), &mut thread_rng()), Keypair::new(&Secp256k1::new(), &mut thread_rng())];
        let redeem_script: Vec<u8> =
            multisig_redeem_script(kps.iter().map(|pk| pk.x_only_public_key().0.serialize()), 2).expect("Test multisig redeem script");

        Box::new((kps, redeem_script))
    });

    fn mock_context() -> &'static ([Keypair; 2], Vec<u8>) {
        CONTEXT.as_ref()
    }

    // Mock multisig PSST from example
    fn mock_psst_constructor() -> PSST<Constructor> {
        let (_, redeem_script) = mock_context();
        let psst = PSST::<Creator>::default().inputs_modifiable().outputs_modifiable();
        let redeem_spk = pay_to_script_hash_lock_script(redeem_script);
        let input_0 = InputBuilder::default()
            .cell_entry(direct_cell_entry_from_script(12793000000000, redeem_spk.clone(), 36151168, false))
            .previous_outpoint(outpoint_from_id(
                TransactionId::from_str("63020db736215f8b1105a9281f7bcbb6473d965ecc45bb2fb5da59bd35e6ff84").unwrap(),
                0,
            ))
            .redeem_script(redeem_script.to_owned())
            .build()
            .expect("Mock psst constructor");

        psst.constructor().input(input_0)
    }

    #[test]
    fn test_pssb_serialization() {
        let constructor = mock_psst_constructor();
        let bundle = Bundle::from(constructor.clone());

        println!("Bundle: {}", serde_json::to_string(&bundle).unwrap());

        // Serialize Bundle
        let serialized = bundle.serialize().map_err(|err| format!("Unable to serialize bundle: {err}")).unwrap();
        println!("Serialized: {}", serialized);

        assert!(!bundle.0.is_empty());

        match Bundle::deserialize(&serialized) {
            Ok(bundle_constructor_deser) => {
                println!("Deserialized: {:?}", bundle_constructor_deser);
                let psst_constructor_deser: Option<PSST<Constructor>> =
                    bundle_constructor_deser.0.first().map(|inner| PSST::from(inner.clone()));
                match psst_constructor_deser {
                    Some(_) => println!("psst<Constructor> deserialized successfully"),
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
    fn test_pssb_bundle_creation() {
        let bundle = Bundle::new();
        assert!(bundle.0.is_empty());
    }

    #[test]
    fn test_pssb_new_with_psst() {
        let psst = PSST::<Creator>::default();
        let bundle = Bundle::from(psst);
        assert_eq!(bundle.0.len(), 1);
    }

    #[test]
    fn test_pssb_add_psst() {
        let mut bundle = Bundle::new();
        let psst = PSST::<Creator>::default();
        bundle.add_psst(psst);
        assert_eq!(bundle.0.len(), 1);
    }

    #[test]
    fn test_pssb_merge_bundles() {
        let mut bundle1 = Bundle::new();
        let mut bundle2 = Bundle::new();

        let inner1 = PSSTInner::default();
        let inner2 = PSSTInner::default();

        bundle1.add_inner(inner1.clone());
        bundle2.add_inner(inner2.clone());

        bundle1.merge(bundle2);

        assert_eq!(bundle1.0.len(), 2);
    }
}
