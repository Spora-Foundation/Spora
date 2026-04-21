use crate::error::Error;
use crate::prelude::*;
use crate::psst::{Inner as PSSTInner, PSST};
// use crate::wasm::result;

use spora_addresses::Address;
// use spora_bip32::Prefix;
use hex;
use serde::{Deserialize, Serialize};
use spora_consensus_client::pay_to_address_lock_script;
use spora_consensus_core::cell_diff::CellMeta;
use spora_consensus_core::constants::UNACCEPTED_DAA_SCORE;
use spora_consensus_core::network::{NetworkId, NetworkType};
use spora_consensus_core::tx::Script;
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

// Build a cell-spending PSSB with custom input and multiple outputs
// to be used in atomic transaction batch.
pub fn unlock_cell_outputs_as_batch_transaction_pssb(
    amount: u64,
    start_address: &Address,
    witness_template: &[u8],
    destination_outputs: Vec<(Address, u64)>,
) -> Result<Bundle, Error> {
    let origin_lock = pay_to_address_lock_script(start_address);
    let cell_entry = direct_cell_meta_from_script(amount, origin_lock, UNACCEPTED_DAA_SCORE, false);

    let input = InputBuilder::default().cell_entry(cell_entry.to_owned()).witness_template(witness_template.to_vec()).build()?;

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

fn direct_cell_meta_from_script(amount: u64, lock_script: Script, block_daa_score: u64, is_coinbase: bool) -> CellMeta {
    CellMeta::from_cell_metadata(amount, 0, lock_script.hash(), None, [0; 32], block_daa_score, is_coinbase).with_resolved_metadata(
        Some(lock_script),
        None,
        Some(Vec::new()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::role::Creator;

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
