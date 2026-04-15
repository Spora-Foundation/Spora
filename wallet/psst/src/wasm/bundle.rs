//! # PSSB (PSST Bundle) WASM Bindings
//!
//! Provides WebAssembly bindings for creating, serialising, and deserialising
//! bundles of Partially Signed Spora Transactions.

use std::ops::Deref;

use super::error::*;
use super::result::*;
use crate::bundle::Bundle as Inner;
use crate::psst::Inner as PSSTInner;
use crate::wasm::psst::*;
use crate::wasm::utils::sau_to_spora_string_with_suffix;
use spora_consensus_core::network::{NetworkId, NetworkIdT};
use wasm_bindgen::prelude::*;
use workflow_wasm::convert::TryCastFromJs;

/// A **PSST Bundle** (PSSB) – an ordered collection of PSSTs that can be
/// serialised as a single hex-encoded blob and transferred between signers.
///
/// # JavaScript API
///
/// ```js
/// const bundle = new PSSB();
/// bundle.add(psst1);
/// bundle.add(psst2);
/// const hex = bundle.serialize();
/// const restored = PSSB.deserialize(hex);
/// ```
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug)]
pub struct PSSB(Inner);

impl Clone for PSSB {
    fn clone(&self) -> Self {
        PSSB(Inner(self.0 .0.clone()))
    }
}

#[wasm_bindgen]
impl PSSB {
    /// Create a new, empty PSSB bundle.
    ///
    /// # Errors
    ///
    /// Currently infallible; the `Result` is reserved for future validation.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<PSSB> {
        let bundle = Inner::new();
        Ok(PSSB(bundle))
    }

    /// Serialise the bundle to a hex-encoded string prefixed with `"PSSB"`.
    ///
    /// The resulting string can be passed to [`PSSB::deserialize`] to
    /// reconstruct the bundle.
    ///
    /// # Errors
    ///
    /// Returns an error if serialisation of any inner PSST fails.
    #[wasm_bindgen]
    pub fn serialize(&self) -> Result<String> {
        self.0.serialize().map_err(Error::from)
    }

    /// Format the bundle contents as a human-readable string using the
    /// given network for currency suffix (e.g. `"SPORA"` / `"TSPORA"`).
    ///
    /// # Arguments
    ///
    /// * `network_id` – A network identifier (`NetworkId` or compatible JS value).
    ///
    /// # Errors
    ///
    /// Returns an error if the network ID cannot be parsed.
    #[wasm_bindgen(js_name = "displayFormat")]
    pub fn display_format(&self, network_id: &NetworkIdT) -> Result<String> {
        let network_id = NetworkId::try_cast_from(network_id).map_err(|err| Error::Custom(err.to_string()))?.into_owned();
        Ok(self.0.display_format(network_id, sau_to_spora_string_with_suffix))
    }

    /// Deserialise a bundle from a hex-encoded string previously produced
    /// by [`PSSB::serialize`].
    ///
    /// # Arguments
    ///
    /// * `hex_data` – Hex string prefixed with `"PSSB"`.
    ///
    /// # Errors
    ///
    /// Returns an error if the hex data is malformed or any inner PSST
    /// cannot be parsed.
    #[wasm_bindgen]
    pub fn deserialize(hex_data: &str) -> Result<PSSB> {
        let bundle = Inner::deserialize(hex_data).map_err(Error::from)?;
        Ok(PSSB(bundle))
    }

    /// Returns the number of PSSTs in the bundle.
    #[wasm_bindgen(getter, js_name = "length")]
    pub fn length(&self) -> usize {
        self.0 .0.len()
    }

    /// Append a PSST to the bundle.
    ///
    /// The method accepts a PSST in *any* role state and extracts its inner
    /// payload for storage.
    ///
    /// # Arguments
    ///
    /// * `psst` – Reference to a [`PSST`] instance.
    ///
    /// # Errors
    ///
    /// Returns an error if the PSST payload cannot be deserialised or is in
    /// an unsupported state.
    pub fn add(&mut self, psst: &PSST) -> Result<()> {
        let payload = psst.payload_getter();

        let payload_str: String = payload.as_string().unwrap_or_else(|| "Unable to convert to string".to_string());
        println!("{}", payload_str);

        // Try multiple deserialization methods
        if let Ok(inner) = serde_wasm_bindgen::from_value::<PSSTInner>(payload.clone()) {
            self.0.add_inner(inner);
            Ok(())
        } else if let Ok(state) = serde_wasm_bindgen::from_value::<State>(payload) {
            match state {
                State::Creator(native_psst) => {
                    self.0.add_inner(native_psst.deref().clone());
                    Ok(())
                }
                State::Combiner(native_psst) => {
                    self.0.add_inner(native_psst.deref().clone());
                    Ok(())
                }
                State::Constructor(native_psst) => {
                    self.0.add_inner(native_psst.deref().clone());
                    Ok(())
                }
                State::Signer(native_psst) => {
                    self.0.add_inner(native_psst.deref().clone());
                    Ok(())
                }
                State::Extractor(native_psst) => {
                    self.0.add_inner(native_psst.deref().clone());
                    Ok(())
                }
                State::Finalizer(native_psst) => {
                    self.0.add_inner(native_psst.deref().clone());
                    Ok(())
                }
                _ => Err(Error::Custom("Unsupported PSST state".into())),
            }
        } else {
            Err(Error::Custom("Failed to deserialize PSST payload".into()))
        }
    }
}

impl From<Inner> for PSSB {
    fn from(bundle: Inner) -> Self {
        PSSB(bundle)
    }
}

impl From<PSSB> for Inner {
    fn from(bundle: PSSB) -> Self {
        bundle.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::psst::Finalizer;
    use crate::psst::PSST as Native;
    use console_log;
    use serde_json::json;
    use spora_consensus_core::tx::Script;
    use wasm_bindgen_test::wasm_bindgen_test;
    use wasm_bindgen_test::*;
    #[wasm_bindgen_test]
    fn _test_psst_bundle_creation() {
        let bundle = PSSB::new().expect("Failed to create PSSB");
        assert_eq!(bundle.length(), 0, "New bundle should have zero PSSTs");
    }

    #[wasm_bindgen_test]
    fn _test_psst_bundle_serialize_deserialize() {
        // Create a bundle
        let mut bundle = PSSB::new().expect("Failed to create PsstBundle");

        let pssts: Vec<PSST> =
            (0..3).map(|_| PSST::new(CtorT::from(JsValue::UNDEFINED)).expect("creator").constructor().expect("constructor")).collect();
        pssts.iter().for_each(|psst| bundle.add(psst).expect("added to bundle"));

        let serialized = bundle.serialize().expect("Failed to serialize bundle");

        // Deserialize the bundle
        let deserialized_bundle = PSSB::deserialize(&serialized).expect("Failed to deserialize bundle");

        // Check that the deserialized bundle has the same number of PSSTs
        assert_eq!(deserialized_bundle.length(), 3, "Deserialized bundle should have length of 3");
        assert_eq!(bundle.length(), deserialized_bundle.length(), "Deserialized bundle should have same PSST count");
    }

    #[wasm_bindgen_test]
    fn _test_bundle_add() {
        let psst_json = json!({
            "global": {
                "version": 0,
                "txVersion": 0,
                "inputsModifiable": false,
                "outputsModifiable": false,
                "inputCount": 0,
                "outputCount": 0,
                "xpubs": {},
                "id": null,
                "proprietaries": {}
            },
            "inputs": [
                {
                    "cellEntry": {
                        "amount": 468928887,
                        "scriptPublicKey": "0000202d8a1414e62e081fb6bcf644e648c18061c2855575cac722f86324cad91dd0faac",
                        "blockDaaScore": 84981186,
                        "isCoinbase": false
                    },
                    "previousOutpoint": {
                        "transactionId": "69155d0e3380e8816dffe2671294ad104f0b3776f35bce1a22f0c21b1f908500",
                        "index": 0
                    },
                    "since": null,
                    "partialSigs": {},
                    "sighashType": 1,
                    "witnessTemplate": null,
                    "bip32Derivations": {},
                    "finalWitness": null,
                    "proprietaries": {}
                }
            ],
            "outputs": [
                {
                    "capacity": 1500000000,
                    "lockScript": {
                        "codeHash": "0000000000000000000000000000000000000000000000000000000000000000",
                        "hashType": 0,
                        "args": []
                    },
                    "typeScript": null,
                    "outputData": null,
                    "witnessTemplate": null,
                    "bip32Derivations": {},
                    "proprietaries": {}
                }
            ]
        });
        let json_str = psst_json.to_string();
        let inner: PSSTInner = serde_json::from_str(&json_str).expect("Failed to parse JSON");

        let native = Native::<Finalizer>::from(inner);

        let wasm_psst = PSST::from(State::Finalizer(native));

        // Create a bundle
        let mut bundle = PSSB::new().expect("Failed to create PsstBundle");
        bundle.add(&wasm_psst).expect("add psst");

        // Serialize the bundle
        let serialized = bundle.serialize().expect("Failed to serialize bundle");
        console_log!("{}", serialized);

        // Deserialize the bundle
        let deserialized_bundle = PSSB::deserialize(&serialized).expect("Failed to deserialize bundle");

        // Check that the deserialized bundle has the same number of PSSTs
        assert_eq!(bundle.length(), deserialized_bundle.length(), "Deserialized bundle should have same PSST count");
    }

    #[wasm_bindgen_test]
    fn _test_deser() {
        let pssb = "PSSB5b7b22676c6f62616c223a7b2276657273696f6e223a302c22747856657273696f6e223a302c2266616c6c6261636b4c6f636b54696d65223a6e756c6c2c22696e707574734d6f6469666961626c65223a66616c73652c226f7574707574734d6f6469666961626c65223a66616c73652c22696e707574436f756e74223a302c226f7574707574436f756e74223a302c227870756273223a7b7d2c226964223a6e756c6c2c2270726f70726965746172696573223a7b7d7d2c22696e70757473223a5b7b227574786f456e747279223a7b22616d6f756e74223a3436383932383838372c227363726970745075626c69634b6579223a22303030303230326438613134313465363265303831666236626366363434653634386331383036316332383535353735636163373232663836333234636164393164643066616163222c22626c6f636b44616153636f7265223a38343938313138362c226973436f696e62617365223a66616c73657d2c2270726576696f75734f7574706f696e74223a7b227472616e73616374696f6e4964223a2236393135356430653333383065383831366466666532363731323934616431303466306233373736663335626365316132326630633231623166393038353030222c22696e646578223a307d2c2273657175656e6365223a6e756c6c2c226d696e54696d65223a6e756c6c2c227061727469616c53696773223a7b7d2c227369676861736854797065223a312c2272656465656d536372697074223a6e756c6c2c227369674f70436f756e74223a312c22626970333244657269766174696f6e73223a7b7d2c2266696e616c536372697074536967223a6e756c6c2c2270726f70726965746172696573223a7b7d7d5d2c226f757470757473223a5b7b22616d6f756e74223a313530303030303030302c227363726970745075626c69634b6579223a2230303030222c2272656465656d536372697074223a6e756c6c2c22626970333244657269766174696f6e73223a7b7d2c2270726f70726965746172696573223a7b7d7d5d7d5d";
        // Deserialize the bundle
        let deserialized_bundle = PSSB::deserialize(pssb).expect("Failed to deserialize bundle");
        assert_eq!(deserialized_bundle.length(), 1, "Should be length 1");
        let inner = deserialized_bundle.0 .0.first().expect("psst after deserialize");
        assert_eq!(inner.inputs.len(), 1);
        let input_01 = inner.inputs.first().expect("first input");
        assert_eq!(input_01.clone().cell_entry.expect("cell entry").amount(), 468928887);
        assert_eq!(inner.outputs.first().expect("output").lock_script, Script::new([0; 32], 0, vec![]));
    }
}
