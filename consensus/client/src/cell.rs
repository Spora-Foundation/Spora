//!
//! # Cell client-side data structures.
//!
//! This module provides client-side data structures for Cell tracking.
//! The canonical Rust names are [`CellEntry`] and [`CellEntryReference`].
//! Client-side Cell entries are represented using canonical Cell metadata.
//!

#![allow(non_snake_case)]

use crate::imports::*;
use crate::outpoint::{TransactionOutpoint, TransactionOutpointInner};
use crate::result::Result;
use crate::standard_script::pay_to_address_lock_script;
use spora_addresses::Address;
use spora_consensus_core::cell_metadata::EmbeddedCellMetadata;
use spora_consensus_core::mass::CellMass;

#[wasm_bindgen(typescript_custom_section)]
const TS_CELL_ENTRY: &'static str = r#"
/**
 * Interface defines the structure of a Cell entry.
 * 
 * @category Consensus
 */
export interface ICellEntry {
    /** @readonly */
    address?: Address;
    /** @readonly */
    outpoint: ITransactionOutpoint;
    /** @readonly */
    amount : bigint;
    /** @readonly */
    capacity?: bigint;
    /** @readonly */
    dataBytes?: bigint;
    /** @readonly */
    lockHash?: HexString;
    /** @readonly */
    typeHash?: HexString;
    /** @readonly */
    dataHash?: HexString;
    /** @readonly */
    blockDaaScore: bigint;
    /** @readonly */
    isCoinbase: boolean;
}

"#;

#[wasm_bindgen]
extern "C" {
    /// WASM type representing an array of [`CellEntryReference`] objects.
    #[wasm_bindgen(extends = Array, typescript_type = "CellEntryReference[]")]
    pub type CellEntryReferenceArrayT;
    /// WASM type representing a Cell entry interface.
    #[wasm_bindgen(typescript_type = "ICellEntry")]
    pub type ICellEntry;
    /// WASM type representing an array of Cell entries.
    #[wasm_bindgen(typescript_type = "ICellEntry[]")]
    pub type ICellEntryArray;
}

/// A Cell entry id is a unique identifier defined by `txid + output_index`.
pub type CellEntryId = TransactionOutpointInner;

/// [`CellEntry`] represents a client-side Cell entry.
///
/// @category Wallet SDK
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[serde(rename_all = "camelCase")]
#[wasm_bindgen(inspectable)]
pub struct CellEntry {
    #[wasm_bindgen(getter_with_clone)]
    pub address: Option<Address>,
    #[wasm_bindgen(getter_with_clone)]
    pub outpoint: TransactionOutpoint,
    pub amount: u64,
    #[serde(default)]
    #[wasm_bindgen(skip)]
    pub capacity: Option<u64>,
    #[serde(default)]
    #[wasm_bindgen(skip)]
    pub data_bytes: Option<u64>,
    #[serde(default)]
    #[wasm_bindgen(skip)]
    pub lock_hash: Option<TransactionId>,
    #[serde(default)]
    #[wasm_bindgen(skip)]
    pub type_hash: Option<TransactionId>,
    #[serde(default)]
    #[wasm_bindgen(skip)]
    pub data_hash: Option<TransactionId>,
    #[wasm_bindgen(js_name = blockDaaScore)]
    pub block_daa_score: u64,
    #[wasm_bindgen(js_name = isCoinbase)]
    pub is_coinbase: bool,
}

#[wasm_bindgen]
impl CellEntry {
    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> Result<js_sys::JsString> {
        //SerializableCellEntry::from(self).serialize_to_json()
        Ok(js_sys::JSON::stringify(&self.to_js_object()?.into())?)
    }
}

impl CellEntry {
    #[inline(always)]
    pub fn amount(&self) -> u64 {
        self.capacity()
    }
    #[inline(always)]
    pub fn capacity(&self) -> u64 {
        self.capacity.unwrap_or(self.amount)
    }
    #[inline(always)]
    pub fn block_daa_score(&self) -> u64 {
        self.block_daa_score
    }

    #[inline(always)]
    pub fn is_coinbase(&self) -> bool {
        self.is_coinbase
    }

    pub fn with_cell_metadata(
        mut self,
        capacity: u64,
        data_bytes: u64,
        lock_hash: TransactionId,
        type_hash: Option<TransactionId>,
        data_hash: TransactionId,
    ) -> Self {
        self.amount = capacity;
        self.capacity = Some(capacity);
        self.data_bytes = Some(data_bytes);
        self.lock_hash = Some(lock_hash);
        self.type_hash = type_hash;
        self.data_hash = Some(data_hash);
        self
    }

    pub fn embedded_cell_metadata(&self) -> Option<EmbeddedCellMetadata> {
        match (self.lock_hash, self.data_hash) {
            (Some(lock_hash), Some(data_hash)) => Some(EmbeddedCellMetadata {
                lock_hash: lock_hash.as_bytes(),
                type_hash: self.type_hash.map(|hash| hash.as_bytes()),
                data_hash: data_hash.as_bytes(),
                data_bytes: self.data_bytes.unwrap_or_default(),
            }),
            _ => None,
        }
    }

    pub fn from_consensus_entry(address: Option<Address>, outpoint: TransactionOutpoint, entry: &cctx::CellEntry) -> Self {
        let mut cell_entry = Self {
            address,
            outpoint,
            amount: entry.amount(),
            capacity: None,
            data_bytes: None,
            lock_hash: None,
            type_hash: None,
            data_hash: None,
            block_daa_score: entry.block_daa_score,
            is_coinbase: entry.is_cellbase,
        };

        if let Some(metadata) = entry.embedded_cell_metadata() {
            cell_entry = cell_entry.with_cell_metadata(
                entry.capacity(),
                metadata.data_bytes,
                metadata.lock_hash.into(),
                metadata.type_hash.map(Into::into),
                metadata.data_hash.into(),
            );
        }

        cell_entry
    }

    fn to_js_object(&self) -> Result<js_sys::Object> {
        let obj = js_sys::Object::new();
        if let Some(address) = &self.address {
            obj.set("address", &address.to_string().into())?;
        }

        let outpoint = js_sys::Object::new();
        outpoint.set("transactionId", &self.outpoint.transaction_id().to_string().into())?;
        outpoint.set("index", &self.outpoint.index().into())?;

        obj.set("amount", &self.amount.to_string().into())?;
        obj.set("capacity", &self.capacity().to_string().into())?;
        obj.set("outpoint", &outpoint.into())?;
        obj.set("blockDaaScore", &self.block_daa_score.to_string().into())?;
        obj.set("isCoinbase", &self.is_coinbase.into())?;
        if let Some(metadata) = self.embedded_cell_metadata() {
            obj.set("dataBytes", &metadata.data_bytes.to_string().into())?;
            obj.set("lockHash", &TransactionId::from(metadata.lock_hash).to_string().into())?;
            if let Some(type_hash) = metadata.type_hash {
                obj.set("typeHash", &TransactionId::from(type_hash).to_string().into())?;
            }
            obj.set("dataHash", &TransactionId::from(metadata.data_hash).to_string().into())?;
        }

        Ok(obj)
    }
}

impl AsRef<CellEntry> for CellEntry {
    fn as_ref(&self) -> &CellEntry {
        self
    }
}

impl From<&CellEntry> for cctx::CellEntry {
    fn from(cell: &CellEntry) -> Self {
        let metadata = cell.embedded_cell_metadata().expect("client CellEntry requires canonical Cell metadata");
        cctx::CellEntry::from_cell_metadata(
            cell.capacity(),
            metadata.data_bytes,
            metadata.lock_hash,
            metadata.type_hash,
            metadata.data_hash,
            cell.block_daa_score,
            cell.is_coinbase,
        )
    }
}

/// [`Arc`] reference to a [`CellEntry`] used by the wallet subsystems.
///
/// @category Wallet SDK
#[derive(Clone, Debug, Serialize, Deserialize, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct CellEntryReference {
    #[wasm_bindgen(skip)]
    pub cell: Arc<CellEntry>,
}

#[wasm_bindgen]
impl CellEntryReference {
    #[wasm_bindgen(js_name = toString)]
    pub fn js_to_string(&self) -> Result<js_sys::JsString> {
        //let entry = workflow_wasm::serde::to_value(&SerializableCellEntry::from(self))?;
        let object = js_sys::Object::new();
        object.set("entry", &self.cell.to_js_object()?.into())?;
        Ok(js_sys::JSON::stringify(&object)?)
    }

    #[wasm_bindgen(getter)]
    pub fn entry(&self) -> CellEntry {
        self.as_ref().clone()
    }

    #[wasm_bindgen(getter)]
    pub fn outpoint(&self) -> TransactionOutpoint {
        self.cell.outpoint.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn address(&self) -> Option<Address> {
        self.cell.address.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn amount(&self) -> u64 {
        self.cell.amount()
    }

    #[wasm_bindgen(getter, js_name = "isCoinbase")]
    pub fn is_coinbase(&self) -> bool {
        self.cell.is_coinbase
    }

    #[wasm_bindgen(getter, js_name = "blockDaaScore")]
    pub fn block_daa_score(&self) -> u64 {
        self.cell.block_daa_score
    }
}

pub trait TryIntoCellEntryReferences {
    fn try_into_cell_entry_references(&self) -> Result<Vec<CellEntryReference>>;
}

impl TryIntoCellEntryReferences for JsValue {
    fn try_into_cell_entry_references(&self) -> Result<Vec<CellEntryReference>> {
        Array::from(self).iter().map(CellEntryReference::try_owned_from).collect()
    }
}

impl CellEntryReference {
    #[inline(always)]
    pub fn id(&self) -> CellEntryId {
        self.cell.outpoint.inner().clone()
    }

    #[inline(always)]
    pub fn id_as_ref(&self) -> &CellEntryId {
        self.cell.outpoint.inner()
    }

    #[inline(always)]
    pub fn amount_as_ref(&self) -> &u64 {
        &self.cell.amount
    }

    #[inline(always)]
    pub fn transaction_id(&self) -> TransactionId {
        self.cell.outpoint.transaction_id()
    }

    #[inline(always)]
    pub fn transaction_id_as_ref(&self) -> &TransactionId {
        self.cell.outpoint.transaction_id_as_ref()
    }
}

impl std::hash::Hash for CellEntryReference {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl AsRef<CellEntry> for CellEntryReference {
    fn as_ref(&self) -> &CellEntry {
        &self.cell
    }
}

impl From<CellEntryReference> for CellEntry {
    fn from(value: CellEntryReference) -> Self {
        (*value.cell).clone()
    }
}

impl From<&CellEntryReference> for cctx::CellEntry {
    fn from(value: &CellEntryReference) -> Self {
        value.cell.as_ref().into()
        // (*value.cell).clone()
    }
}

impl From<CellEntry> for CellEntryReference {
    fn from(entry: CellEntry) -> Self {
        Self { cell: Arc::new(entry) }
    }
}

impl From<&CellEntryReference> for CellMass {
    fn from(entry: &CellEntryReference) -> Self {
        let entry: cctx::CellEntry = entry.into();
        Self::from(&entry)
    }
}

impl Eq for CellEntryReference {}

impl PartialEq for CellEntryReference {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Ord for CellEntryReference {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(&other.id())
    }
}

impl PartialOrd for CellEntryReference {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.id().cmp(&other.id()))
    }
}

impl TryCastFromJs for CellEntry {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Ok(Self::try_ref_from_js_value_as_cast(value)?)
    }
}

/// A simple collection of Cell entries. This struct is used to
/// retain a set of entries in WASM memory for faster
/// processing. This struct keeps a list of entries represented
/// by `CellEntryReference` struct. This data structure is used
/// internally by the framework, but is exposed for convenience.
/// Please consider using `CellContext` instead.
/// @category Wallet SDK
#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[wasm_bindgen(inspectable)]
pub struct CellEntries(Arc<Vec<CellEntryReference>>);

impl CellEntries {
    pub fn contains(&self, entry: &CellEntryReference) -> bool {
        self.0.contains(entry)
    }

    pub fn iter(&self) -> impl Iterator<Item = &CellEntryReference> {
        self.0.iter()
    }
}

#[wasm_bindgen]
impl CellEntries {
    /// Create a new `CellEntries` struct with a set of entries.
    #[wasm_bindgen(constructor)]
    pub fn js_ctor(js_value: JsValue) -> Result<CellEntries> {
        js_value.try_into()
    }

    #[wasm_bindgen(getter = items)]
    pub fn get_items_as_js_array(&self) -> JsValue {
        let items = self.0.as_ref().clone().into_iter().map(<CellEntryReference as Into<JsValue>>::into);
        Array::from_iter(items).into()
    }

    #[wasm_bindgen(setter = items)]
    pub fn set_items_from_js_array(&mut self, js_value: &JsValue) {
        let items = Array::from(js_value)
            .iter()
            .map(|js_value| {
                CellEntryReference::try_owned_from(&js_value).unwrap_or_else(|err| panic!("invalid CellEntryReference: {err}"))
            })
            .collect::<Vec<_>>();
        self.0 = Arc::new(items);
    }

    /// Sort the contained entries by amount. Please note that
    /// this function is not intended for use with large cell sets
    /// as it duplicates the whole contained set while sorting.
    pub fn sort(&mut self) {
        let mut items = (*self.0).clone();
        items.sort_by_key(|e| e.amount());
        self.0 = Arc::new(items);
    }

    pub fn amount(&self) -> u64 {
        self.0.iter().map(|e| e.amount()).sum()
    }
}

impl CellEntries {
    pub fn items(&self) -> Arc<Vec<CellEntryReference>> {
        self.0.clone()
    }
}

impl From<CellEntries> for Vec<Option<CellEntry>> {
    fn from(value: CellEntries) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.as_ref().clone())).collect::<Vec<_>>()
    }
}

impl From<Vec<CellEntry>> for CellEntries {
    fn from(items: Vec<CellEntry>) -> Self {
        Self(Arc::new(items.into_iter().map(CellEntryReference::from).collect::<_>()))
    }
}

impl From<CellEntries> for Vec<Option<cctx::CellEntry>> {
    fn from(value: CellEntries) -> Self {
        value.0.as_ref().iter().map(|entry| Some(entry.cell.as_ref().into())).collect::<Vec<_>>()
    }
}

impl TryFrom<Vec<Option<CellEntry>>> for CellEntries {
    type Error = Error;
    fn try_from(value: Vec<Option<CellEntry>>) -> std::result::Result<Self, Self::Error> {
        let mut list = vec![];
        for entry in value.into_iter() {
            list.push(entry.ok_or(Error::Custom("Unable to cast `Vec<Option<CellEntry>>` into `CellEntries`.".to_string()))?.into());
        }

        Ok(Self(Arc::new(list)))
    }
}

impl From<Vec<CellEntryReference>> for CellEntries {
    fn from(list: Vec<CellEntryReference>) -> Self {
        Self(Arc::new(list))
    }
}

impl TryFrom<JsValue> for CellEntries {
    type Error = Error;
    fn try_from(js_value: JsValue) -> std::result::Result<Self, Self::Error> {
        if !js_value.is_array() {
            return Err("Data type supplied to CellEntries must be an Array".into());
        }

        Ok(Self(Arc::new(js_value.try_into_cell_entry_references()?)))
    }
}

impl TryCastFromJs for CellEntryReference {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::resolve(value, || {
            if let Ok(cell_entry) = CellEntry::try_ref_from_js_value(&value) {
                Ok(Self::from(cell_entry.clone()))
            } else if let Some(object) = Object::try_from(value.as_ref()) {
                let address = object.try_cast_into::<Address>("address")?;
                let outpoint = TransactionOutpoint::try_from(object.get_value("outpoint")?.as_ref())?;
                let cell_entry = Object::from(object.get_value("cellEntry")?);

                let cell_entry = if !cell_entry.is_undefined() {
                    let amount = cell_entry.get_u64("amount").map_err(|_| {
                        Error::custom("Supplied object does not contain `cellEntry.amount` property (or it is not a numerical value)")
                    })?;
                    let block_daa_score = cell_entry.get_u64("blockDaaScore").map_err(|_| {
                        Error::custom(
                            "Supplied object does not contain `cellEntry.blockDaaScore` property (or it is not a numerical value)",
                        )
                    })?;
                    let is_coinbase = cell_entry.get_bool("isCoinbase")?;
                    let capacity = cell_entry.get_u64("capacity").ok();
                    let data_bytes = cell_entry.get_u64("dataBytes").ok();
                    let lock_hash: Option<TransactionId> =
                        cell_entry.get_value("lockHash").ok().and_then(|value| value.try_into_owned().ok());
                    let type_hash: Option<TransactionId> =
                        cell_entry.get_value("typeHash").ok().and_then(|value| value.try_into_owned().ok());
                    let data_hash: Option<TransactionId> =
                        cell_entry.get_value("dataHash").ok().and_then(|value| value.try_into_owned().ok());

                    CellEntry {
                        address,
                        outpoint,
                        amount,
                        capacity,
                        data_bytes,
                        lock_hash,
                        type_hash,
                        data_hash,
                        block_daa_score,
                        is_coinbase,
                    }
                } else {
                    let amount = object.get_u64("amount").map_err(|_| {
                        Error::custom("Supplied object does not contain `amount` property (or it is not a numerical value)")
                    })?;
                    let block_daa_score = object.get_u64("blockDaaScore").map_err(|_| {
                        Error::custom("Supplied object does not contain `blockDaaScore` property (or it is not a numerical value)")
                    })?;
                    let is_coinbase = object.try_get_bool("isCoinbase")?.unwrap_or(false);
                    let capacity = object.get_u64("capacity").ok();
                    let data_bytes = object.get_u64("dataBytes").ok();
                    let lock_hash: Option<TransactionId> =
                        object.get_value("lockHash").ok().and_then(|value| value.try_into_owned().ok());
                    let type_hash: Option<TransactionId> =
                        object.get_value("typeHash").ok().and_then(|value| value.try_into_owned().ok());
                    let data_hash: Option<TransactionId> =
                        object.get_value("dataHash").ok().and_then(|value| value.try_into_owned().ok());

                    CellEntry {
                        address,
                        outpoint,
                        amount,
                        capacity,
                        data_bytes,
                        lock_hash,
                        type_hash,
                        data_hash,
                        block_daa_score,
                        is_coinbase,
                    }
                };

                Ok(CellEntryReference::from(cell_entry))
            } else {
                Err("Data type supplied to CellEntryReference must be an object".into())
            }
        })
    }
}

impl CellEntryReference {
    pub fn simulated(amount: u64) -> Self {
        use spora_addresses::{Prefix, Version};
        let address = Address::new(Prefix::Testnet, Version::PubKey, &rand::random::<[u8; 32]>()).expect("Valid test address");
        Self::simulated_with_address(amount, &address)
    }

    pub fn simulated_with_address(amount: u64, address: &Address) -> Self {
        let outpoint = TransactionOutpoint::simulated();
        let lock_hash: TransactionId = pay_to_address_lock_script(address).hash().into();
        let block_daa_score = 0;
        let is_coinbase = false;

        let cell_entry = CellEntry {
            address: Some(address.clone()),
            outpoint,
            amount,
            capacity: Some(amount),
            data_bytes: Some(0),
            lock_hash: Some(lock_hash),
            type_hash: None,
            data_hash: Some(TransactionId::from([0; 32])),
            block_daa_score,
            is_coinbase,
        };

        CellEntryReference::from(cell_entry)
    }
}
