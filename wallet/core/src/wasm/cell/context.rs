use crate::cell as native;
use crate::cell::{CellContextBinding, CellContextId};
use crate::imports::*;
use crate::result::Result;
use crate::wasm::cell::CellProcessor;
use crate::wasm::{Balance, BalanceStrings};
use spora_addresses::AddressOrStringArrayT;
use spora_consensus_client::CellEntryReferenceArrayT;
use spora_hashes::Hash;
use spora_wallet_macros::declare_typescript_wasm_interface as declare;

declare! {
    ICellContextArgs,
    r#"
    /**
     * CellContext constructor arguments.
     * 
     * @see {@link CellProcessor}, {@link CellContext}, {@link RpcClient}
     * @category Wallet SDK
     */
    export interface ICellContextArgs {
        /**
         * Associated CellProcessor.
         */
        processor: CellProcessor;
        /**
         * Optional id for the CellContext.
         * **The id must be a valid 32-byte hex string.**
         * You can use {@link blake3FromBinary} or {@link blake3FromText} to generate a valid id.
         * 
         * If not provided, a random id will be generated.
         * The IDs are deterministic, based on the order CellContexts are created.
         */
        id?: HexString;
    }
    "#,
}

///
/// CellContext is a class that provides a way to track addresses activity
/// on the Spora network.  When an address is registered with CellContext
/// it aggregates all live cells for that address and emits events when
/// any activity against these addresses occurs.
///
/// CellContext constructor accepts {@link ICellContextArgs} interface that
/// can contain an optional id parameter.  If supplied, this `id` parameter
/// will be included in all notifications emitted by the CellContext as
/// well as included as a part of {@link ITransactionRecord} emitted when
/// transactions occur. If not provided, a random id will be generated. This id
/// typically represents an account id in the context of a wallet application.
/// The integrated Wallet API uses CellContext to represent wallet accounts.
///
/// **Exchanges:** if you are building an exchange wallet, it is recommended
/// to use CellContext for each user account.  This way you can track and isolate
/// each user activity (use address set, balances, transaction records).
///
/// CellContext maintains a real-time cumulative balance of all addresses
/// registered against it and provides balance update notification events
/// when the balance changes.
///
/// The CellContext balance is comprised of 3 values:
/// - `mature`: amount of funds available for spending.
/// - `pending`: amount of funds that are being received.
/// - `outgoing`: amount of funds that are being sent but are not yet accepted by the network.
///
/// Please see {@link IBalance} interface for more details.
///
/// CellContext can be supplied as a cell source to the transaction {@link Generator}
/// allowing the {@link Generator} to create transactions using the
/// cell entries it manages.
///
/// **IMPORTANT:** CellContext is meant to represent a single account.  It is not
/// designed to be used as a global cell manager for all addresses in a very large
/// wallet (such as an exchange wallet). For such use cases, it is recommended to
/// perform manual cell management by subscribing to cell notifications using
/// {@link RpcClient.subscribeCellsChanged} and {@link RpcClient.getCellsByAddresses}.
///
/// @see {@link ICellContextArgs},
/// {@link CellProcessor},
/// {@link Generator},
/// {@link createTransactions},
/// {@link IBalance},
/// {@link IBalanceEvent},
/// {@link IPendingEvent},
/// {@link IReorgEvent},
/// {@link IStasisEvent},
/// {@link IMaturityEvent},
/// {@link IDiscoveryEvent},
/// {@link IBalanceEvent},
/// {@link ITransactionRecord}
///
/// @category Wallet SDK
///
#[derive(Clone, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct CellContext {
    inner: native::CellContext,
}

impl CellContext {
    pub fn inner(&self) -> &native::CellContext {
        &self.inner
    }

    pub fn context(&self) -> MutexGuard<'_, native::context::Context> {
        self.inner.context()
    }

    pub fn processor(&self) -> &native::CellProcessor {
        self.inner.processor()
    }
}

#[wasm_bindgen]
impl CellContext {
    #[wasm_bindgen(constructor)]
    pub fn ctor(js_value: ICellContextArgs) -> Result<CellContext> {
        let CellContextCreateArgs { processor, binding } = js_value.try_into()?;
        let inner = native::CellContext::new(processor.processor(), binding);
        Ok(CellContext { inner })
    }

    /// Performs a scan of the given addresses and registers them in the context for event notifications.
    #[wasm_bindgen(js_name = "trackAddresses")]
    pub async fn track_addresses(&self, addresses: AddressOrStringArrayT, optional_current_daa_score: Option<BigInt>) -> Result<()> {
        let current_daa_score = if let Some(big_int) = optional_current_daa_score {
            Some(big_int.try_into().map_err(|v| Error::custom(format!("Unable to convert BigInt value {v:?}")))?)
        } else {
            None
        };
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner().scan_and_register_addresses(addresses, current_daa_score).await?;
        Ok(())
    }

    /// Unregister a list of addresses from the context. This will stop tracking of these addresses.
    #[wasm_bindgen(js_name = "unregisterAddresses")]
    pub async fn unregister_addresses(&self, addresses: AddressOrStringArrayT) -> Result<()> {
        let addresses: Vec<Address> = addresses.try_into()?;
        self.inner().unregister_addresses(addresses).await
    }

    /// Clear the CellContext. Unregister all addresses and clear all tracked cell entries.
    /// IMPORTANT: This function must be manually called when disconnecting or re-connecting to the node
    /// (followed by address re-registration).  
    pub async fn clear(&self) -> Result<()> {
        self.inner().clear().await
    }

    #[wasm_bindgen(getter, js_name = "isActive")]
    pub fn active(&self) -> bool {
        let processor = self.inner().processor();
        processor.try_rpc_ctl().map(|ctl| ctl.is_connected()).unwrap_or(false) && processor.is_connected() && processor.is_running()
    }

    // Returns all mature cell entries that are currently managed by the CellContext and are available for spending.
    // This function is for informational purposes only.
    // pub fn mature(&self) -> Result<ICellEntryReferenceArray> {
    //     let context = self.context();
    //     let array = Array::new();
    //     for entry in context.mature.iter() {
    //         array.push(&JsValue::from(entry.clone()));
    //     }
    //     Ok(array.unchecked_into())
    // }

    ///
    /// Returns a range of mature cell entries that are currently
    /// managed by the CellContext and are available for spending.
    ///
    /// NOTE: This function is provided for informational purposes only.
    /// **You should not manage cell entries manually if they are owned by CellContext.**
    ///
    /// The resulting range may be less than requested if cell entries
    /// have been spent asynchronously by CellContext or by other means
    /// (i.e. CellContext has received notification from the network that
    /// entries have been spent externally).
    ///
    /// Entries are kept in ascending sorted order by their amount.
    ///
    #[wasm_bindgen(js_name = "getMatureRange")]
    pub fn mature_range(&self, mut from: usize, mut to: usize) -> Result<CellEntryReferenceArrayT> {
        let context = self.context();
        if from > to {
            return Err(Error::custom("'from' must be less than or equal to 'to'"));
        }
        if from > context.mature.len() {
            from = context.mature.len();
        }
        if to > context.mature.len() {
            to = context.mature.len();
        }
        if from == to {
            return Ok(Array::new().unchecked_into());
        }
        let slice = context.mature.get(from..to).unwrap();
        let array = Array::new();
        for entry in slice.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array.unchecked_into())
    }

    /// Obtain the length of the mature cell entries that are currently
    /// managed by the CellContext.
    #[wasm_bindgen(getter, js_name = "matureLength")]
    pub fn mature_length(&self) -> usize {
        self.context().mature.len()
    }

    /// Returns pending cell entries that are currently managed by the CellContext.
    #[wasm_bindgen(js_name = "getPending")]
    pub fn pending(&self) -> Result<CellEntryReferenceArrayT> {
        let context = self.context();
        let array = Array::new();
        for (_, entry) in context.pending.iter() {
            array.push(&JsValue::from(entry.clone()));
        }
        Ok(array.unchecked_into())
    }

    /// Current {@link Balance} of the CellContext.
    #[wasm_bindgen(getter, js_name = "balance")]
    pub fn balance(&self) -> Option<Balance> {
        self.inner().balance().map(Balance::from)
    }

    /// Current {@link BalanceStrings} of the CellContext.
    #[wasm_bindgen(getter, js_name = "balanceStrings")]
    pub fn balance_strings(&self) -> Result<Option<BalanceStrings>> {
        let network_id = self.inner.processor().network_id().ok();
        if let (Some(network_id), Some(balance)) = (network_id, self.inner().balance()) {
            let balance_strings = balance.to_balance_strings(&network_id.into(), None);
            Ok(Some(BalanceStrings::from(balance_strings)))
        } else {
            Ok(None)
        }
    }
}

impl From<native::CellContext> for CellContext {
    fn from(inner: native::CellContext) -> Self {
        Self { inner }
    }
}

impl From<CellContext> for native::CellContext {
    fn from(cell_context: CellContext) -> Self {
        cell_context.inner
    }
}

impl TryCastFromJs for CellContext {
    type Error = Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Ok(Self::try_ref_from_js_value_as_cast(value)?)
    }
}

pub struct CellContextCreateArgs {
    processor: CellProcessor,
    binding: CellContextBinding,
}

impl TryFrom<ICellContextArgs> for CellContextCreateArgs {
    type Error = Error;
    fn try_from(value: ICellContextArgs) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let processor = object.cast_into::<CellProcessor>("processor")?;

            let binding = if let Some(id) = object.try_cast_into::<Hash>("id")? {
                CellContextBinding::Id(CellContextId::new(id))
            } else {
                CellContextBinding::default()
            };

            Ok(CellContextCreateArgs { binding, processor })
        } else {
            Err(Error::custom("CellProcessor: supplied value must be an object"))
        }
    }
}
