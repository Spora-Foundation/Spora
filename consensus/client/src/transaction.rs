//!
//! Declares the client-side [`Transaction`] type, which represents a Spora transaction.
//!

#![allow(non_snake_case)]

use crate::cell::{CellEntry, CellEntryReference};
use crate::imports::*;
use crate::input::{TransactionInput, TransactionInputArrayAsArgT, TransactionInputArrayAsResultT};
use crate::outpoint::TransactionOutpoint;
use crate::output::{TransactionOutput, TransactionOutputArrayAsArgT, TransactionOutputArrayAsResultT};
use crate::result::Result;
use crate::serializable::{numeric, string, SerializableTransactionT};
use spora_consensus_core::mass::project_verifiable_transaction_mass;
use spora_consensus_core::network::NetworkTypeT;
use spora_consensus_core::tx::VerifiableTransaction;
use spora_utils::hex::*;

#[wasm_bindgen(typescript_custom_section)]
const TS_TRANSACTION: &'static str = r#"
/**
 * Interface defining the structure of a transaction.
 * 
 * @category Consensus
 */
export interface ITransaction {
    version: number;
    inputs: ITransactionInput[];
    outputs: ITransactionOutput[];
    payload?: HexString;
    /** Best-available one-dimensional selection mass for relay/template ranking. */
    mass?: bigint;

    /** Optional verbose data provided by RPC */
    verboseData?: ITransactionVerboseData;
}

/**
 * Optional transaction verbose data.
 * 
 * @category Node RPC
 */
export interface ITransactionVerboseData {
    transactionId : HexString;
    hash : HexString;
    computeMass : bigint;
    blockHash : HexString;
    blockTime : bigint;
}
"#;

#[wasm_bindgen]
extern "C" {
    /// WASM (TypeScript) type representing `ITransaction | Transaction`
    /// @category Consensus
    #[wasm_bindgen(typescript_type = "ITransaction | Transaction")]
    pub type TransactionT;
}

/// Inner type used by [`Transaction`]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionInner {
    pub version: u16,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
    pub payload: Vec<u8>,
    pub mass: u64,

    // A field that is used to cache the transaction ID.
    // Always use the corresponding self.id() instead of accessing this field directly
    pub id: TransactionId,
}

/// Represents a Spora transaction.
/// This is an artificial construct that includes additional
/// transaction-related data such as additional data from input cells
/// used by transaction inputs.
/// @category Consensus
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct Transaction {
    inner: Arc<Mutex<TransactionInner>>,
}

impl Transaction {
    pub fn new(
        id: Option<TransactionId>,
        version: u16,
        inputs: Vec<TransactionInput>,
        outputs: Vec<TransactionOutput>,
        payload: Vec<u8>,
        mass: u64,
    ) -> Result<Self> {
        let mut inner = TransactionInner { id: id.unwrap_or_default(), version, inputs, outputs, payload, mass };
        let cell_tx = Self::build_cell_tx(&inner)?;
        if id.is_none() {
            inner.id = TransactionId::from_bytes(cell_tx.id());
        }
        Ok(Self { inner: Arc::new(Mutex::new(inner)) })
    }

    pub fn new_with_inner(inner: TransactionInner) -> Self {
        Self { inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn inner(&self) -> MutexGuard<'_, TransactionInner> {
        self.inner.lock().unwrap()
    }

    pub fn id(&self) -> TransactionId {
        self.inner().id
    }
}

#[wasm_bindgen]
impl Transaction {
    fn is_coinbase_inner(inner: &TransactionInner) -> bool {
        inner.inputs.is_empty()
    }

    /// Determines whether or not a transaction is a coinbase transaction. A coinbase
    /// transaction is a special transaction created by miners that distributes fees and block subsidy
    /// to the previous blocks' miners, and specifies the script_pub_key that will be used to pay the current
    /// miner in future blocks.
    pub fn is_coinbase(&self) -> bool {
        Self::is_coinbase_inner(&self.inner())
    }

    /// Recompute and finalize the tx id based on updated tx fields
    pub fn finalize(&self) -> Result<TransactionId> {
        let tx = self.cell_tx()?;
        self.inner().id = TransactionId::from_bytes(tx.id());
        Ok(self.inner().id)
    }

    /// Returns the transaction ID
    #[wasm_bindgen(getter, js_name = id)]
    pub fn id_string(&self) -> String {
        self.inner().id.to_string()
    }

    #[wasm_bindgen(constructor)]
    pub fn constructor(js_value: &TransactionT) -> std::result::Result<Transaction, JsError> {
        Ok(js_value.try_into_owned()?)
    }

    #[wasm_bindgen(getter = inputs)]
    pub fn get_inputs_as_js_array(&self) -> TransactionInputArrayAsResultT {
        let inputs = self.inner.lock().unwrap().inputs.clone().into_iter().map(JsValue::from);
        Array::from_iter(inputs).unchecked_into()
    }

    /// Returns a list of unique addresses used by transaction inputs.
    /// This method can be used to determine addresses used by transaction inputs
    /// in order to select private keys needed for transaction signing.
    pub fn addresses(&self, network_type: &NetworkTypeT) -> Result<spora_addresses::AddressArrayT> {
        let _ = network_type;
        let mut list = std::collections::HashSet::new();
        for input in &self.inner.lock().unwrap().inputs {
            if let Some(cell_entry) = input.get_cell_entry() {
                if let Some(address) = &cell_entry.cell.address {
                    list.insert(address.clone());
                }
            }
        }
        Ok(Array::from_iter(list.into_iter().map(JsValue::from)).unchecked_into())
    }

    #[wasm_bindgen(setter = inputs)]
    pub fn set_inputs_from_js_array(&mut self, js_value: &TransactionInputArrayAsArgT) {
        let inputs = Array::from(js_value)
            .iter()
            .map(|js_value| {
                TransactionInput::try_owned_from(&js_value).unwrap_or_else(|err| panic!("invalid transaction input: {err}"))
            })
            .collect::<Vec<_>>();
        self.inner().inputs = inputs;
    }

    #[wasm_bindgen(getter = outputs)]
    pub fn get_outputs_as_js_array(&self) -> TransactionOutputArrayAsResultT {
        let outputs = self.inner.lock().unwrap().outputs.clone().into_iter().map(JsValue::from);
        Array::from_iter(outputs).unchecked_into()
    }

    #[wasm_bindgen(setter = outputs)]
    pub fn set_outputs_from_js_array(&mut self, js_value: &TransactionOutputArrayAsArgT) {
        let outputs = Array::from(js_value)
            .iter()
            .map(|js_value| TryCastFromJs::try_owned_from(&js_value).unwrap_or_else(|err| panic!("invalid transaction output: {err}")))
            .collect::<Vec<_>>();
        self.inner().outputs = outputs;
    }

    #[wasm_bindgen(getter, js_name = version)]
    pub fn get_version(&self) -> u16 {
        self.inner().version
    }

    #[wasm_bindgen(setter, js_name = version)]
    pub fn set_version(&self, v: u16) {
        self.inner().version = v;
    }

    #[wasm_bindgen(getter = payload)]
    pub fn get_payload_as_hex_string(&self) -> String {
        self.inner().payload.to_hex()
    }

    #[wasm_bindgen(setter = payload)]
    pub fn set_payload_from_js_value(&mut self, js_value: JsValue) {
        self.inner.lock().unwrap().payload = js_value.try_as_vec_u8().unwrap_or_else(|err| panic!("payload value error: {err}"));
    }

    #[wasm_bindgen(getter = mass)]
    pub fn get_mass(&self) -> u64 {
        self.inner().mass
    }

    #[wasm_bindgen(setter = mass)]
    pub fn set_mass(&self, v: u64) {
        self.inner().mass = v;
    }
}

impl TryCastFromJs for Transaction {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> std::result::Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve_cast(value, || {
            if let Some(object) = Object::try_from(value.as_ref()) {
                if let Some(tx) = object.try_get_value("tx")? {
                    Transaction::try_captured_cast_from(tx)
                } else {
                    let id = object.try_cast_into::<TransactionId>("id")?;
                    let version = object.get_u16("version")?;
                    if object.try_get_value("lockTime")?.is_some() {
                        return Err(Error::Custom("ITransaction.lockTime was removed from the canonical client schema".to_string()));
                    }
                    if object.try_get_value("gas")?.is_some() {
                        return Err(Error::Custom("ITransaction.gas was removed from the canonical client schema".to_string()));
                    }
                    let payload = object.try_get_value("payload")?.map(|value| value.try_as_vec_u8()).transpose()?.unwrap_or_default();
                    // mass field is optional
                    let mass = object.get_u64("mass").unwrap_or_default();
                    let inputs = object
                        .get_vec("inputs")?
                        .iter()
                        .map(TryCastFromJs::try_owned_from)
                        .collect::<std::result::Result<Vec<TransactionInput>, Error>>()?;
                    if object.try_get_value("subnetworkId")?.is_some() {
                        return Err(Error::Custom(
                            "ITransaction.subnetworkId was removed from the canonical client schema".to_string(),
                        ));
                    }
                    let outputs: Vec<TransactionOutput> = object
                        .get_vec("outputs")?
                        .iter()
                        .map(TryCastFromJs::try_owned_from)
                        .collect::<std::result::Result<Vec<TransactionOutput>, Error>>()?;
                    Transaction::new(id, version, inputs, outputs, payload, mass).map(Into::into)
                }
            } else {
                Err("Transaction must be an object".into())
            }
        })
        // Transaction::try_from(value)
    }
}

impl Transaction {
    fn build_cell_tx(inner: &TransactionInner) -> Result<cctx::CellTx> {
        let is_coinbase = Self::is_coinbase_inner(inner);
        if !is_coinbase && !inner.payload.is_empty() {
            return Err(Error::Custom("transaction-level payload is only supported for coinbase in Cell model".to_string()));
        }

        let inputs = inner
            .inputs
            .iter()
            .map(|input| {
                let input = input.inner();
                cctx::CellRef::new((&input.previous_outpoint).into(), input.since)
            })
            .collect::<Vec<_>>();

        let mut witnesses = inner.inputs.iter().map(|input| input.inner().witness.clone().unwrap_or_default()).collect::<Vec<_>>();

        let outputs = inner
            .outputs
            .iter()
            .map(|output| {
                let output = output.inner();
                cctx::CellOut { lock: output.lock_script.clone(), type_: output.type_script.clone(), capacity: output.capacity }
            })
            .collect::<Vec<_>>();

        let mut outputs_data =
            inner.outputs.iter().map(|output| output.inner().output_data.clone().unwrap_or_default()).collect::<Vec<_>>();
        if is_coinbase && !inner.payload.is_empty() {
            if let Some(first_output_data) = outputs_data.first_mut() {
                *first_output_data = inner.payload.clone();
            } else {
                witnesses = vec![inner.payload.clone()];
            }
        }

        cctx::CellTx::new(inputs, vec![], outputs, outputs_data, witnesses).map_err(Error::custom)
    }

    pub fn cell_tx(&self) -> Result<cctx::CellTx> {
        let inner = self.inner();
        Self::build_cell_tx(&inner)
    }

    pub fn signable_transaction(&self) -> Result<cctx::SignableTransaction> {
        let cell_tx = self.cell_tx()?;
        let entries = self
            .inner()
            .inputs
            .iter()
            .map(|input| input.get_cell_entry().ok_or(Error::MissingCellEntry).map(|entry| cctx::CellEntry::from(&entry)))
            .collect::<Result<Vec<_>>>()?;
        Ok(cctx::SignableTransaction::with_entries(cell_tx, entries))
    }

    pub fn from_cell_tx(tx: &cctx::CellTx) -> Self {
        let signable_tx = cctx::SignableTransaction::with_entries(tx.clone(), vec![]);
        Self::from_signable_transaction(&signable_tx)
    }

    pub fn from_signable_transaction(tx: &cctx::SignableTransaction) -> Self {
        let verifiable_tx = tx.as_verifiable();
        let transaction = tx.as_ref();
        let inputs = transaction
            .inputs
            .iter()
            .enumerate()
            .map(|(index, input)| {
                let previous_outpoint = TransactionOutpoint::from(input.out_point);
                let cell_entry = verifiable_tx.cell_entry(index).map(|entry| {
                    let entry = CellEntry::from_consensus_entry(None, previous_outpoint.clone(), entry);
                    CellEntryReference::from(entry)
                });
                let witness = transaction.witnesses.get(index).cloned().filter(|witness| !witness.is_empty());
                TransactionInput::new(previous_outpoint, witness, input.since, cell_entry)
            })
            .collect::<Vec<_>>();

        let outputs = transaction.outputs.iter().map(|output| TransactionOutput::from(output)).collect::<Vec<_>>();

        let payload = transaction.payload().map(|payload| payload.to_vec()).unwrap_or_default();

        Self::new_with_inner(TransactionInner {
            id: TransactionId::from_bytes(transaction.id()),
            version: transaction.version(),
            inputs,
            outputs,
            payload,
            mass: project_verifiable_transaction_mass(&verifiable_tx, None).selection_mass,
        })
    }

    pub fn cell_entry_references(&self) -> Result<Vec<CellEntryReference>> {
        let inner = self.inner();
        let cell_entry_references = inner
            .inputs
            .clone()
            .into_iter()
            .map(|input| input.get_cell_entry().ok_or(Error::MissingCellEntry))
            .collect::<Result<Vec<CellEntryReference>>>()?;
        Ok(cell_entry_references)
    }

    pub fn set_witness(&self, input_index: usize, witness: Vec<u8>) -> Result<()> {
        if self.inner().inputs.len() <= input_index {
            return Err(Error::Custom("Input index is invalid".to_string()));
        }
        self.inner().inputs[input_index].set_witness(witness);
        Ok(())
    }

    pub fn payload(&self) -> Vec<u8> {
        self.inner().payload.clone()
    }

    pub fn payload_len(&self) -> usize {
        self.inner().payload.len()
    }
}

#[wasm_bindgen]
impl Transaction {
    /// Serializes the transaction to a pure JavaScript Object.
    /// The schema of the JavaScript object is defined by {@link ISerializableTransaction}.
    /// @see {@link ISerializableTransaction}
    #[wasm_bindgen(js_name = "serializeToObject")]
    pub fn serialize_to_object(&self) -> Result<SerializableTransactionT> {
        Ok(numeric::SerializableTransaction::from_signable_transaction(&self.signable_transaction()?)?.serialize_to_object()?.into())
    }

    /// Serializes the transaction to a JSON string.
    /// The schema of the JSON is defined by {@link ISerializableTransaction}.
    #[wasm_bindgen(js_name = "serializeToJSON")]
    pub fn serialize_to_json(&self) -> Result<String> {
        numeric::SerializableTransaction::from_signable_transaction(&self.signable_transaction()?)?.serialize_to_json()
    }

    /// Serializes the transaction to a "Safe" JSON schema where it converts all `bigint` values to `string` to avoid potential client-side precision loss.
    #[wasm_bindgen(js_name = "serializeToSafeJSON")]
    pub fn serialize_to_json_safe(&self) -> Result<String> {
        string::SerializableTransaction::from_signable_transaction(&self.signable_transaction()?)?.serialize_to_json()
    }

    /// Deserialize the {@link Transaction} Object from a pure JavaScript Object.
    #[wasm_bindgen(js_name = "deserializeFromObject")]
    pub fn deserialize_from_object(js_value: &JsValue) -> Result<Transaction> {
        numeric::SerializableTransaction::deserialize_from_object(js_value.clone())?.try_into()
    }

    /// Deserialize the {@link Transaction} Object from a JSON string.
    #[wasm_bindgen(js_name = "deserializeFromJSON")]
    pub fn deserialize_from_json(json: &str) -> Result<Transaction> {
        numeric::SerializableTransaction::deserialize_from_json(json)?.try_into()
    }

    /// Deserialize the {@link Transaction} Object from a "Safe" JSON schema where all `bigint` values are represented as `string`.
    #[wasm_bindgen(js_name = "deserializeFromSafeJSON")]
    pub fn deserialize_from_safe_json(json: &str) -> Result<Transaction> {
        string::SerializableTransaction::deserialize_from_json(json)?.try_into()
    }
}
