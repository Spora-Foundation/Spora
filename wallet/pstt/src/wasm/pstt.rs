use crate::pstt::{Input, PSTT as Native};
use crate::role::*;
use spora_consensus_core::network::NetworkType;
use spora_consensus_core::tx::TransactionId;

use wasm_bindgen::prelude::*;
// use js_sys::Object;
use crate::pstt::Inner;
use serde::{Deserialize, Serialize};
use spora_consensus_client::{Transaction, TransactionInput, TransactionInputT, TransactionOutput, TransactionOutputT};
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
    // this is not a Display trait intentionally
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

impl From<State> for PSTT {
    fn from(state: State) -> Self {
        PSTT { state: Arc::new(Mutex::new(Some(state))) }
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "PSTT | Transaction | string | undefined")]
    pub type CtorT;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Payload {
    data: String,
}

impl<T> TryFrom<Payload> for Native<T> {
    type Error = Error;

    fn try_from(value: Payload) -> Result<Self> {
        let Payload { data } = value;
        if data.starts_with("PSTT") {
            unimplemented!("PSTT binary serialization")
        } else {
            Ok(serde_json::from_str(&data).map_err(|err| format!("Invalid JSON: {err}"))?)
        }
    }
}

#[wasm_bindgen(inspectable)]
#[derive(Clone, CastFromJs)]
pub struct PSTT {
    state: Arc<Mutex<Option<State>>>,
}

impl TryCastFromJs for PSTT {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if JsValue::is_undefined(value.as_ref()) {
                Ok(PSTT::from(State::Creator(Native::<Creator>::default())))
            } else if let Some(data) = value.as_ref().as_string() {
                let pstt_inner: Inner = serde_json::from_str(&data).map_err(|_| Error::InvalidPayload)?;
                Ok(PSTT::from(State::NoOp(Some(pstt_inner))))
            } else if let Ok(transaction) = Transaction::try_owned_from(value) {
                let pstt_inner: Inner = transaction.try_into()?;
                Ok(PSTT::from(State::NoOp(Some(pstt_inner))))
            } else {
                Err(Error::InvalidPayload)
            }
        })
    }
}

#[wasm_bindgen]
impl PSTT {
    #[wasm_bindgen(constructor)]
    pub fn new(payload: CtorT) -> Result<PSTT> {
        PSTT::try_owned_from(payload.unchecked_into::<JsValue>().as_ref()).map_err(|err| Error::Ctor(err.to_string()))
    }

    #[wasm_bindgen(getter, js_name = "role")]
    pub fn role_getter(&self) -> String {
        self.state().as_ref().unwrap().display().to_string()
    }

    #[wasm_bindgen(getter, js_name = "payload")]
    pub fn payload_getter(&self) -> JsValue {
        let state = self.state();
        workflow_wasm::serde::to_value(state.as_ref().unwrap()).unwrap()
    }

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

    fn replace(&self, state: State) -> Result<PSTT> {
        self.state.lock().unwrap().replace(state);
        Ok(self.clone())
    }

    /// Change role to `CREATOR`
    /// #[wasm_bindgen(js_name = toCreator)]
    pub fn creator(&self) -> Result<PSTT> {
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
    pub fn constructor(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Constructor(inner.ok_or(Error::NotInitialized)?.into()),
            State::Creator(pstt) => State::Constructor(pstt.constructor()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `UPDATER`
    #[wasm_bindgen(js_name = toUpdater)]
    pub fn updater(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Updater(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(constructor) => State::Updater(constructor.updater()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `SIGNER`
    #[wasm_bindgen(js_name = toSigner)]
    pub fn signer(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Signer(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(pstt) => State::Signer(pstt.signer()),
            State::Updater(pstt) => State::Signer(pstt.signer()),
            State::Combiner(pstt) => State::Signer(pstt.signer()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `COMBINER`
    #[wasm_bindgen(js_name = toCombiner)]
    pub fn combiner(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Combiner(inner.ok_or(Error::NotInitialized)?.into()),
            State::Constructor(pstt) => State::Combiner(pstt.combiner()),
            State::Updater(pstt) => State::Combiner(pstt.combiner()),
            State::Signer(pstt) => State::Combiner(pstt.combiner()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `FINALIZER`
    #[wasm_bindgen(js_name = toFinalizer)]
    pub fn finalizer(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Finalizer(inner.ok_or(Error::NotInitialized)?.into()),
            State::Combiner(pstt) => State::Finalizer(pstt.finalizer()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    /// Change role to `EXTRACTOR`
    #[wasm_bindgen(js_name = toExtractor)]
    pub fn extractor(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::NoOp(inner) => State::Extractor(inner.ok_or(Error::NotInitialized)?.into()),
            State::Finalizer(pstt) => State::Extractor(pstt.extractor()?),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = fallbackLockTime)]
    pub fn fallback_lock_time(&self, lock_time: u64) -> Result<PSTT> {
        let state = match self.take() {
            State::Creator(pstt) => State::Creator(pstt.fallback_lock_time(lock_time)),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = inputsModifiable)]
    pub fn inputs_modifiable(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::Creator(pstt) => State::Creator(pstt.inputs_modifiable()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = outputsModifiable)]
    pub fn outputs_modifiable(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::Creator(pstt) => State::Creator(pstt.outputs_modifiable()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = noMoreInputs)]
    pub fn no_more_inputs(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::Constructor(pstt) => State::Constructor(pstt.no_more_inputs()),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = noMoreOutputs)]
    pub fn no_more_outputs(&self) -> Result<PSTT> {
        let state = match self.take() {
            State::Constructor(pstt) => State::Constructor(pstt.no_more_outputs()),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = inputAndRedeemScript)]
    pub fn input_with_redeem(&self, input: &TransactionInputT, data: &JsValue) -> Result<PSTT> {
        let obj = js_sys::Object::from(data.clone());

        let input = TransactionInput::try_owned_from(input)?;
        let mut input: Input = input.try_into()?;
        let redeem_script = js_sys::Reflect::get(&obj, &"redeemScript".into())
            .expect("Missing redeemscript field")
            .as_string()
            .expect("redeemscript must be a string");
        input.redeem_script =
            Some(hex::decode(redeem_script).map_err(|e| Error::custom(format!("Redeem script is not a hex string: {}", e)))?);
        let state = match self.take() {
            State::Constructor(pstt) => State::Constructor(pstt.input(input)),
            _ => Err(Error::expected_state("Constructor"))?,
        };

        self.replace(state)
    }

    pub fn input(&self, input: &TransactionInputT) -> Result<PSTT> {
        let input = TransactionInput::try_owned_from(input)?;
        let state = match self.take() {
            State::Constructor(pstt) => State::Constructor(pstt.input(input.try_into()?)),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    pub fn output(&self, output: &TransactionOutputT) -> Result<PSTT> {
        let output = TransactionOutput::try_owned_from(output)?;
        let state = match self.take() {
            State::Constructor(pstt) => State::Constructor(pstt.output(output.try_into()?)),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = setSequence)]
    pub fn set_sequence(&self, n: u64, input_index: usize) -> Result<PSTT> {
        let state = match self.take() {
            State::Updater(pstt) => State::Updater(pstt.set_sequence(n, input_index)?),
            _ => Err(Error::expected_state("Creator"))?,
        };

        self.replace(state)
    }

    #[wasm_bindgen(js_name = calculateId)]
    pub fn calculate_id(&self) -> Result<TransactionId> {
        let state = self.state();
        match state.as_ref().unwrap() {
            State::Signer(pstt) => Ok(pstt.calculate_id()),
            _ => Err(Error::expected_state("Creator"))?,
        }
    }

    #[wasm_bindgen(js_name = calculateMass)]
    pub fn calculate_mass(&self, data: &JsValue) -> Result<u64> {
        let obj = js_sys::Object::from(data.clone());
        let network_id = js_sys::Reflect::get(&obj, &"networkId".into())
            .map_err(|_| Error::custom("networkId is missing"))?
            .as_string()
            .ok_or_else(|| Error::custom("networkId must be a string"))?;

        let network_id = NetworkType::from_str(&network_id).map_err(|e| Error::custom(format!("Invalid networkId: {}", e)))?;

        let cloned_pstt = self.clone();

        let extractor = {
            let finalizer = cloned_pstt.finalizer()?;

            let finalizer_state = finalizer.state().clone().unwrap();

            match finalizer_state {
                State::Finalizer(pstt) => {
                    for input in pstt.inputs.iter() {
                        if input.redeem_script.is_some() {
                            return Err(Error::custom("Mass calculation is not supported for inputs with redeem scripts"));
                        }
                    }
                    let pstt = pstt
                        .finalize_sync(|inner: &Inner| -> Result<Vec<Vec<u8>>> { Ok(vec![vec![0u8, 65]; inner.inputs.len()]) })
                        .map_err(|e| Error::custom(format!("Failed to finalize PSTT: {e}")))?;
                    pstt.extractor()?
                }
                _ => panic!("Finalizer state is not valid"),
            }
        };
        let tx = extractor
            .extract_tx_unchecked(&network_id.into())
            .map_err(|e| Error::custom(format!("Failed to extract transaction: {e}")))?;
        Ok(tx.tx.mass())
    }
}
