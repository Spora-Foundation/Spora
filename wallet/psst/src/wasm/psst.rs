//! # PSST WASM Bindings
//!
//! WebAssembly bindings for the Partially Signed Spora Transaction (PSST)
//! workflow.  Exposes a role-based state machine that guides users through
//! the PSST lifecycle: Creator → Constructor → Updater → Signer →
//! Combiner → Finalizer → Extractor.

use crate::psst::{Input, Output, PSST as Native};
use crate::role::*;
use spora_consensus_core::mass::MassCalculator;
use spora_consensus_core::network::NetworkType;
use spora_consensus_core::tx::TransactionId;

use wasm_bindgen::prelude::*;
// use js_sys::Object;
use crate::psst::Inner;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::from_value;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::MutexGuard;
use std::sync::{Arc, Mutex};
use workflow_wasm::{
    convert::{Cast, CastFromJs, TryCastFromJs},
    // extensions::object::*,
    // error::Error as CastError,
};

use super::error::*;
use super::result::*;

/// Serialisable PSST role/state envelope.
///
/// Each variant wraps a native [`PSST`](crate::psst::PSST) at the
/// corresponding lifecycle stage.  The `NoOp` variant is used for PSSTs
/// that were loaded from a serialised payload and have not yet been
/// assigned a role.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "state", content = "payload")]
pub enum State {
    NoOp(Option<Inner>),
    Creator(Native<Creator>),
    Constructor(Native<Constructor>),
    Updater(Native<Updater>),
    Signer(Native<Signer>),
    Combiner(Native<Combiner>),
    Finalizer(Native<Finalizer>),
    Extractor(Native<Extractor>),
}

impl AsRef<State> for State {
    fn as_ref(&self) -> &State {
        self
    }
}

impl State {
    /// Returns a human-readable label for the current state.
    ///
    /// This is intentionally *not* a `Display` trait implementation to
    /// avoid ambiguity with other display formats.
    pub fn display(&self) -> &'static str {
        match self {
            State::NoOp(_) => "Init",
            State::Creator(_) => "Creator",
            State::Constructor(_) => "Constructor",
            State::Updater(_) => "Updater",
            State::Signer(_) => "Signer",
            State::Combiner(_) => "Combiner",
            State::Finalizer(_) => "Finalizer",
            State::Extractor(_) => "Extractor",
        }
    }
}

impl From<State> for PSST {
    fn from(state: State) -> Self {
        PSST { state: Arc::new(Mutex::new(Some(state))) }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "PSST | string | undefined")]
    pub type CtorT;
}

/// Intermediate serialisation helper for deserialising a PSST from a
/// hex-encoded or JSON string.
#[derive(Clone, Serialize, Deserialize)]
pub struct Payload {
    /// Hex-encoded (prefixed with `"PSST"`) or raw JSON string.
    data: String,
}

impl<T> TryFrom<Payload> for Native<T> {
    type Error = Error;

    fn try_from(value: Payload) -> Result<Self> {
        let Payload { data } = value;
        if data.starts_with("PSST") {
            Ok(Native::<T>::from_hex(&data)?)
        } else {
            Ok(serde_json::from_str(&data).map_err(|err| format!("Invalid JSON: {err}"))?)
        }
    }
}

/// WebAssembly-facing PSST handle.
///
/// This struct wraps the internal PSST state machine behind an `Arc<Mutex<_>>`
/// so that it can be safely shared across JavaScript promises and callbacks.
///
/// # JavaScript API
///
/// ```js
/// const psst = new PSST();             // Creator role
/// const constructor = psst.toConstructor();
/// constructor.input({ ... });
/// constructor.output({ ... });
/// const signer = constructor.toSigner();
/// ```
#[wasm_bindgen(inspectable)]
#[derive(Clone, CastFromJs)]
pub struct PSST {
    state: Arc<Mutex<Option<State>>>,
}

impl TryCastFromJs for PSST {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if JsValue::is_undefined(value.as_ref()) {
                Ok(PSST::from(State::Creator(Native::<Creator>::default())))
            } else if let Some(data) = value.as_ref().as_string() {
                if data.starts_with("PSST") {
                    let native_psst = Native::<Creator>::from_hex(&data)?;
                    Ok(PSST::from(State::NoOp(Some(native_psst.deref().clone()))))
                } else if let Ok(state) = serde_json::from_str::<State>(&data) {
                    Ok(PSST::from(state))
                } else {
                    let psst_inner: Inner = serde_json::from_str(&data).map_err(|_| Error::InvalidPayload)?;
                    Ok(PSST::from(State::NoOp(Some(psst_inner))))
                }
            } else {
                Err(Error::InvalidPayload)
            }
        })
    }
}

#[wasm_bindgen]
impl PSST {
    /// Create a new PSST instance.
    ///
    /// # Arguments
    ///
    /// * `payload` - One of:
    ///   - `undefined` – creates an empty PSST in the **Creator** role.
    ///   - A hex string starting with `"PSST"` – deserialised from the
    ///     portable PSST format.
    ///   - A JSON string – deserialised from a JSON payload.
    ///   - Another `PSST` instance – cloned.
    ///
    /// # Errors
    ///
    /// Returns an error if the payload cannot be parsed.
    #[wasm_bindgen(constructor)]
    pub fn new(payload: CtorT) -> Result<PSST> {
        PSST::try_owned_from(payload.unchecked_into::<JsValue>().as_ref()).map_err(|err| Error::Ctor(err.to_string()))
    }

    /// Returns the current role name (e.g. `"Creator"`, `"Signer"`).
    #[wasm_bindgen(getter, js_name = "role")]
    pub fn role_getter(&self) -> String {
        self.state().as_ref().unwrap().display().to_string()
    }

    /// Returns the underlying state as a JavaScript value suitable for
    /// serialisation or transfer between WASM workers.
    #[wasm_bindgen(getter, js_name = "payload")]
    pub fn payload_getter(&self) -> JsValue {
        let state = self.state();
        workflow_wasm::serde::to_value(state.as_ref().unwrap()).unwrap()
    }

    /// Serialise the current PSST state to a JSON string.
    pub fn serialize(&self) -> String {
        let state = self.state();
        serde_json::to_string(state.as_ref().unwrap()).unwrap()
    }

    fn state(&self) -> MutexGuard<'_, Option<State>> {
        self.state.lock().unwrap()
    }

    fn take(&self) -> State {
        self.state.lock().unwrap().take().unwrap()
    }

    fn replace(&self, state: State) -> Result<PSST> {
        self.state.lock().unwrap().replace(state);
        Ok(self.clone())
    }

    /// Change role to `CREATOR`
    /// #[wasm_bindgen(js_name = toCreator)]
    pub fn creator(&self) -> Result<PSST> {
        let state = match self.take() {
            State::NoOp(inner) => match inner {
                None => State::Creator(Native::default()),
                Some(_) => Err(Error::CreateNotAllowed)?,
            },
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `CONSTRUCTOR`
    #[wasm_bindgen(js_name = toConstructor)]
    pub fn constructor(&self) -> Result<PSST> {
        let state = match self.take() {
            State::NoOp(inner) => State::Constructor(inner.ok_or(Error::NotInitialized)?.into()),
            State::Creator(psst) => State::Constructor(psst.constructor()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `UPDATER`
    #[wasm_bindgen(js_name = toUpdater)]
    pub fn updater(&self) -> Result<PSST> {
        let state = match self.take() {
            State::NoOp(inner) => State::Updater(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(constructor) => State::Updater(constructor.updater()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `SIGNER`
    #[wasm_bindgen(js_name = toSigner)]
    pub fn signer(&self) -> Result<PSST> {
        let state = match self.take() {
            State::NoOp(inner) => State::Signer(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(psst) => State::Signer(psst.signer()),
            State::Updater(psst) => State::Signer(psst.signer()),
            State::Combiner(psst) => State::Signer(psst.signer()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `COMBINER`
    #[wasm_bindgen(js_name = toCombiner)]
    pub fn combiner(&self) -> Result<PSST> {
        let state = match self.take() {
            State::NoOp(inner) => State::Combiner(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(psst) => State::Combiner(psst.combiner()),
            State::Updater(psst) => State::Combiner(psst.combiner()),
            State::Signer(psst) => State::Combiner(psst.combiner()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `FINALIZER`
    #[wasm_bindgen(js_name = toFinalizer)]
    pub fn finalizer(&self) -> Result<PSST> {
        let state = match self.take() {
            State::NoOp(inner) => State::Finalizer(inner.ok_or(Error::NotInitialized)?.into()),
            State::Combiner(psst) => State::Finalizer(psst.finalizer()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `EXTRACTOR`
    #[wasm_bindgen(js_name = toExtractor)]
    pub fn extractor(&self) -> Result<PSST> {
        let state = match self.take() {
            State::NoOp(inner) => State::Extractor(inner.ok_or(Error::NotInitialized)?.into()),
            State::Finalizer(psst) => State::Extractor(psst.extractor()?),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Mark inputs as modifiable (Creator role only).
    ///
    /// # Errors
    ///
    /// Returns an error if the PSST is not in the Creator state.
    #[wasm_bindgen(js_name = inputsModifiable)]
    pub fn inputs_modifiable(&self) -> Result<PSST> {
        let state = match self.take() {
            State::Creator(psst) => State::Creator(psst.inputs_modifiable()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Mark outputs as modifiable (Creator role only).
    ///
    /// # Errors
    ///
    /// Returns an error if the PSST is not in the Creator state.
    #[wasm_bindgen(js_name = outputsModifiable)]
    pub fn outputs_modifiable(&self) -> Result<PSST> {
        let state = match self.take() {
            State::Creator(psst) => State::Creator(psst.outputs_modifiable()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Signal that no more inputs will be added (Constructor role only).
    ///
    /// # Errors
    ///
    /// Returns an error if the PSST is not in the Constructor state.
    #[wasm_bindgen(js_name = noMoreInputs)]
    pub fn no_more_inputs(&self) -> Result<PSST> {
        let state = match self.take() {
            State::Constructor(psst) => State::Constructor(psst.no_more_inputs()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Signal that no more outputs will be added (Constructor role only).
    ///
    /// # Errors
    ///
    /// Returns an error if the PSST is not in the Constructor state.
    #[wasm_bindgen(js_name = noMoreOutputs)]
    pub fn no_more_outputs(&self) -> Result<PSST> {
        let state = match self.take() {
            State::Constructor(psst) => State::Constructor(psst.no_more_outputs()),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    /// Add an input together with a witness template (Constructor role only).
    ///
    /// # Arguments
    ///
    /// * `input` – Input JS object (see [`Input`]).
    /// * `data`  – JS object with a `witnessTemplate` hex string field.
    ///
    /// # Errors
    ///
    /// Returns an error if the witness template is not valid hex or the
    /// PSST is not in the Constructor state.
    #[wasm_bindgen(js_name = inputAndWitnessTemplate)]
    pub fn input_with_witness_template(&self, input: &JsValue, data: &JsValue) -> Result<PSST> {
        let obj = js_sys::Object::from(data.clone());

        let mut input: Input = from_value(input.clone())?;
        let witness_template = js_sys::Reflect::get(&obj, &"witnessTemplate".into())
            .expect("Missing witnessTemplate field")
            .as_string()
            .expect("witnessTemplate must be a string");
        input.witness_template =
            Some(hex::decode(witness_template).map_err(|e| Error::custom(format!("witnessTemplate is not a hex string: {}", e)))?);
        let state = match self.take() {
            State::Constructor(psst) => State::Constructor(psst.input(input)),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    /// Add an input to the PSST (Constructor role only).
    ///
    /// # Arguments
    ///
    /// * `input` – A JS object conforming to the [`Input`] schema.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialisation fails or the PSST is not in the
    /// Constructor state.
    pub fn input(&self, input: &JsValue) -> Result<PSST> {
        let input: Input = from_value(input.clone())?;
        let state = match self.take() {
            State::Constructor(psst) => State::Constructor(psst.input(input)),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Add an output to the PSST (Constructor role only).
    ///
    /// # Arguments
    ///
    /// * `output` – A JS object conforming to the [`Output`] schema.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialisation fails or the PSST is not in the
    /// Constructor state.
    pub fn output(&self, output: &JsValue) -> Result<PSST> {
        let output: Output = from_value(output.clone())?;
        let state = match self.take() {
            State::Constructor(psst) => State::Constructor(psst.output(output)),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Set the `since` (relative timelock) value on a specific input
    /// (Updater role only).
    ///
    /// # Arguments
    ///
    /// * `n` – The `since` value.
    /// * `input_index` – Zero-based index of the target input.
    ///
    /// # Errors
    ///
    /// Returns an error if the index is out of range or the PSST is not in
    /// the Updater state.
    #[wasm_bindgen(js_name = setSince)]
    pub fn set_since(&self, n: u64, input_index: usize) -> Result<PSST> {
        let state = match self.take() {
            State::Updater(psst) => State::Updater(psst.set_since(n, input_index)?),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Calculate the transaction ID without finalising (Signer role only).
    ///
    /// # Errors
    ///
    /// Returns an error if the PSST is not in the Signer state.
    #[wasm_bindgen(js_name = calculateId)]
    pub fn calculate_id(&self) -> Result<TransactionId> {
        let state = self.state();
        match state.as_ref().unwrap() {
            State::Signer(psst) => Ok(psst.calculate_id()),
            _ => Err(Error::expected_state("Creator"))?,
        }
    }

    /// Estimate the transaction mass for the current PSST.
    ///
    /// The method internally finalises a **clone** of the PSST (filling in
    /// dummy witnesses) and then computes the mass using the network-specific
    /// [`MassCalculator`].
    ///
    /// # Arguments
    ///
    /// * `data` – A JS object with a `networkId` string field (e.g.
    ///   `{ networkId: "mainnet" }`).
    ///
    /// # Returns
    ///
    /// The estimated transaction mass as `u64`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `networkId` is missing or invalid.
    /// - Any input contains a witness template (not supported for mass
    ///   estimation).
    /// - Finalisation or extraction fails.
    #[wasm_bindgen(js_name = calculateMass)]
    pub fn calculate_mass(&self, data: &JsValue) -> Result<u64> {
        let obj = js_sys::Object::from(data.clone());
        let network_id = js_sys::Reflect::get(&obj, &"networkId".into())
            .map_err(|_| Error::custom("networkId is missing"))?
            .as_string()
            .ok_or_else(|| Error::custom("networkId must be a string"))?;

        let network_id = NetworkType::from_str(&network_id).map_err(|e| Error::custom(format!("Invalid networkId: {}", e)))?;

        let cloned_psst = self.clone();

        let extractor = {
            let finalizer = cloned_psst.finalizer()?;

            let finalizer_state = finalizer.state().clone().unwrap();

            match finalizer_state {
                State::Finalizer(psst) => {
                    for input in psst.inputs.iter() {
                        if input.witness_template.is_some() {
                            return Err(Error::custom("Mass calculation is not supported for inputs with witness templates"));
                        }
                    }
                    let psst = psst
                        .finalize_sync(|inner: &Inner| -> Result<Vec<Vec<u8>>> { Ok(vec![vec![0u8, 65]; inner.inputs.len()]) })
                        .map_err(|e| Error::custom(format!("Failed to finalize PSST: {e}")))?;
                    psst.extractor()?
                }
                _ => panic!("Finalizer state is not valid"),
            }
        };
        let tx = extractor
            .extract_tx_unchecked(&network_id.into())
            .map_err(|e| Error::custom(format!("Failed to extract transaction: {e}")))?;
        let calculator = MassCalculator::new_with_consensus_params(&network_id.into());
        let storage_mass = calculator.calc_contextual_masses(&tx.as_verifiable()).map(|mass| mass.storage_mass).unwrap_or_default();
        let non_contextual = tx.calculated_non_contextual_masses.unwrap_or_else(|| calculator.calc_non_contextual_masses_cell(&tx.tx));
        Ok(storage_mass.max(non_contextual.compute_mass).max(non_contextual.transient_mass))
    }
}
