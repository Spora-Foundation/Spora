//!
//! Implementation of the client-side [`TransactionInput`] struct used by the client-side [`Transaction`] struct.
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::result::Result;
use crate::standard_script::pay_to_address_lock_script;
use crate::CellEntryReference;
use crate::TransactionOutpoint;
use spora_utils::hex::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION: &'static str = r#"
/**
 * Interface defines the structure of a transaction input.
 * 
 * @category Consensus
 */
export interface ITransactionInput {
    previousOutpoint: ITransactionOutpoint;
    witness?: HexString;
    since: bigint;
    cellEntry?: CellEntryReference;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionInputVerboseData;
}

/**
 * Option transaction input verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionInputVerboseData { }

"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing `ITransactionInput | TransactionInput`
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "ITransactionInput | TransactionInput")]
    pub type TransactionInputT;
    /// WASM (TypeScript) type representing `ITransactionInput[] | TransactionInput[]`
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "(ITransactionInput | TransactionInput)[]")]
    pub type TransactionInputArrayAsArgT;
    /// WASM (TypeScript) type representing `TransactionInput[]`
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "TransactionInput[]")]
    pub type TransactionInputArrayAsResultT;
}

/// Inner type used by [`TransactionInput`]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInputInner {
    pub previous_outpoint: TransactionOutpoint,
    pub witness: Option<Vec<u8>>,
    pub since: u64,
    pub cell_entry: Option<CellEntryReference>,
}

impl TransactionInputInner {
    pub fn new(
        previous_outpoint: TransactionOutpoint,
        witness: Option<Vec<u8>>,
        since: u64,
        cell_entry: Option<CellEntryReference>,
    ) -> Self {
        Self { previous_outpoint, witness, since, cell_entry }
    }
}

/// Represents a Spora transaction input
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct TransactionInput {
    inner: Arc<Mutex<TransactionInputInner>>,
}

impl TransactionInput {
    pub fn new(
        previous_outpoint: TransactionOutpoint,
        witness: Option<Vec<u8>>,
        since: u64,
        cell_entry: Option<CellEntryReference>,
    ) -> Self {
        let inner = TransactionInputInner::new(previous_outpoint, witness, since, cell_entry);
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn new_with_inner(inner: TransactionInputInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionInputInner> {
        self.inner.lock().unwrap()
    }

    pub fn witness_length(&self) -> usize {
        self.inner().witness.as_ref().map(|witness| witness.len()).unwrap_or_default()
    }

    pub fn cell_entry(&self) -> Option<CellEntryReference> {
        self.inner().cell_entry.clone()
    }
}

#[wasm_bindgen]
impl TransactionInput {
    #[wasm_bindgen(constructor)]
    pub fn constructor(value: &TransactionInputT) -> Result<TransactionInput> {
        Self::try_owned_from(value)
    }

    #[wasm_bindgen(getter = previousOutpoint)]
    pub fn get_previous_outpoint(&self) -> TransactionOutpoint {
        self.inner().previous_outpoint.clone()
    }

    #[wasm_bindgen(setter = previousOutpoint)]
    pub fn set_previous_outpoint(&mut self, js_value: &JsValue) -> Result<()> {
        match js_value.try_into() {
            Ok(outpoint) => {
                self.inner().previous_outpoint = outpoint;
                Ok(())
            }
            Err(_) => Err(Error::custom("invalid outpoint script".to_string())),
        }
    }

    #[wasm_bindgen(getter = witness)]
    pub fn get_witness_as_hex(&self) -> Option<String> {
        self.inner().witness.as_ref().map(|witness| witness.to_hex())
    }

    #[wasm_bindgen(setter = witness)]
    pub fn set_witness_from_js_value(&mut self, js_value: JsValue) -> Result<()> {
        match js_value.try_as_vec_u8() {
            Ok(witness) => {
                self.set_witness(witness);
                Ok(())
            }
            Err(_) => Err(Error::custom("invalid witness".to_string())),
        }
    }

    #[wasm_bindgen(getter = since)]
    pub fn get_since(&self) -> u64 {
        self.inner().since
    }

    #[wasm_bindgen(setter = since)]
    pub fn set_since(&mut self, since: u64) {
        self.inner().since = since;
    }

    #[wasm_bindgen(getter = cellEntry)]
    pub fn get_cell_entry(&self) -> Option<CellEntryReference> {
        self.inner().cell_entry.clone()
    }
}

impl TransactionInput {
    pub fn set_witness(&self, witness: Vec<u8>) {
        self.inner().witness.replace(witness);
    }

    pub fn lock_script_args(&self) -> Option<Vec<u8>> {
        self.cell_entry().and_then(|cell_ref| cell_ref.cell.address.as_ref().map(|address| pay_to_address_lock_script(address).args))
    }
}

impl AsRef<TransactionInput> for TransactionInput {
    fn as_ref(&self) -> &TransactionInput {
        self
    }
}

impl TryCastFromJs for TransactionInput {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve_cast(value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                let previous_outpoint: TransactionOutpoint = object.get_value("previousOutpoint")?.as_ref().try_into()?;
                let witness = object.get_vec_u8("witness").ok();
                let since = object.get_u64("since")?;
                let cell_entry = object.try_cast_into::<CellEntryReference>("cellEntry")?;
                Ok(TransactionInput::new(previous_outpoint, witness, since, cell_entry).into())
            } else {
                Err("TransactionInput must be an object".into())
            }
        })
    }
}
