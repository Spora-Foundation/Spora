//!
//! Implementation of the client-side [`TransactionOutput`] used by the [`Transaction`] struct.
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION_OUTPUT: &'static str = r#"
/**
 * Interface defining the structure of a canonical Cell output.
 *
 * @category Consensus
 */
export interface ITransactionOutput {
    capacity: bigint;
    lockScript: Script;
    typeScript?: Script;
    outputData?: HexString;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionOutputVerboseData;
}

/**
 * TransactionOutput verbose data.
 *
 * @category Node RPC
 */
export interface ITransactionOutputVerboseData {
    lockScriptType?: string;
    lockScriptAddress?: string;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing `ITransactionOutput | TransactionOutput`
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "ITransactionOutput | TransactionOutput")]
    pub type TransactionOutputT;
    /// WASM (TypeScript) type representing `ITransactionOutput[] | TransactionOutput[]`
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "(ITransactionOutput | TransactionOutput)[]")]
    pub type TransactionOutputArrayAsArgT;
    /// WASM (TypeScript) type representing `TransactionOutput[]`
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "TransactionOutput[]")]
    pub type TransactionOutputArrayAsResultT;
}

/// Inner type used by [`TransactionOutput`]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionOutputInner {
    pub capacity: u64,
    pub lock_script: cctx::Script,
    #[serde(default)]
    pub type_script: Option<cctx::Script>,
    #[serde(with = "spora_utils::serde_bytes_optional")]
    #[serde(default)]
    pub output_data: Option<Vec<u8>>,
}

/// Represents a canonical client-side Cell output.
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct TransactionOutput {
    inner: Arc<Mutex<TransactionOutputInner>>,
}

impl TransactionOutput {
    pub fn new(capacity: u64, lock_script: cctx::Script) -> TransactionOutput {
        Self { inner: Arc::new(Mutex::new(TransactionOutputInner { capacity, lock_script, type_script: None, output_data: None })) }
    }

    pub fn new_with_inner(inner: TransactionOutputInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionOutputInner> {
        self.inner.lock().unwrap()
    }

    pub fn lock_script_length(&self) -> usize {
        self.inner().lock_script.args.len()
    }
}

#[wasm_bindgen]
impl TransactionOutput {
    #[wasm_bindgen(constructor)]
    /// TransactionOutput constructor
    pub fn ctor(capacity: u64, lock_script: JsValue) -> Result<TransactionOutput> {
        let lock_script: cctx::Script = workflow_wasm::serde::from_value(lock_script)?;
        Ok(Self::new(capacity, lock_script))
    }

    #[wasm_bindgen(getter, js_name = capacity)]
    pub fn capacity(&self) -> u64 {
        self.inner().capacity
    }

    #[wasm_bindgen(setter, js_name = capacity)]
    pub fn set_capacity(&self, v: u64) {
        self.inner().capacity = v;
    }

    #[wasm_bindgen(getter, js_name = lockScript)]
    pub fn get_lock_script(&self) -> Result<JsValue> {
        Ok(workflow_wasm::serde::to_value(&self.inner().lock_script)?)
    }

    #[wasm_bindgen(setter, js_name = lockScript)]
    pub fn set_lock_script(&self, v: JsValue) -> Result<()> {
        self.inner().lock_script = workflow_wasm::serde::from_value(v)?;
        Ok(())
    }

    #[wasm_bindgen(getter, js_name = typeScript)]
    pub fn get_type_script(&self) -> Result<JsValue> {
        Ok(workflow_wasm::serde::to_value(&self.inner().type_script)?)
    }

    #[wasm_bindgen(setter, js_name = typeScript)]
    pub fn set_type_script(&self, v: JsValue) -> Result<()> {
        self.inner().type_script = if v.is_undefined() || v.is_null() { None } else { Some(workflow_wasm::serde::from_value(v)?) };
        Ok(())
    }

    #[wasm_bindgen(getter, js_name = outputData)]
    pub fn get_output_data(&self) -> Option<String> {
        self.inner().output_data.as_ref().map(hex::encode)
    }

    #[wasm_bindgen(setter, js_name = outputData)]
    pub fn set_output_data(&self, v: Option<String>) -> Result<()> {
        self.inner().output_data = v.map(|value| hex::decode(value).map_err(|error| Error::custom(error.to_string()))).transpose()?;
        Ok(())
    }
}

impl AsRef<TransactionOutput> for TransactionOutput {
    fn as_ref(&self) -> &TransactionOutput {
        self
    }
}

impl From<&cctx::CellOutput> for TransactionOutput {
    fn from(cell_out: &cctx::CellOutput) -> Self {
        Self::new_with_inner(TransactionOutputInner {
            capacity: cell_out.capacity,
            lock_script: cell_out.lock.clone(),
            type_script: cell_out.type_.clone(),
            output_data: None,
        })
    }
}

impl TryCastFromJs for TransactionOutput {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve_cast(value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                let capacity = object.get_u64("capacity")?;
                let lock_script: cctx::Script = workflow_wasm::serde::from_value(object.get_value("lockScript")?)?;
                let type_script = match object.try_get_value("typeScript")? {
                    Some(value) if !value.is_null() && !value.is_undefined() => Some(workflow_wasm::serde::from_value(value)?),
                    _ => None,
                };
                let output_data = object
                    .get_string("outputData")
                    .ok()
                    .map(|value| hex::decode(value).map_err(|error| Error::custom(error.to_string())))
                    .transpose()?;
                Ok(TransactionOutput::new_with_inner(TransactionOutputInner { capacity, lock_script, type_script, output_data })
                    .into())
            } else {
                Err("TransactionOutput must be an object".into())
            }
        })
    }
}
