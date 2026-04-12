//!
//! Implementation of the [`CellContext`] which is a runtime
//! primitive responsible for monitoring multiple addresses,
//! generation of address-related events and balance tracking.
//!

use crate::cell::{
    CellContextBinding, CellEntryId, CellEntryReference, CellEntryReferenceExtension, CellProcessor, Maturity, NetworkParams,
    OutgoingTransaction, PendingCellEntryReference,
};
use crate::encryption::blake3_hash;
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::storage::TransactionRecord;
use crate::tx::PendingTransaction;
use sorted_insert::SortedInsertBinaryByKey;
use spora_consensus_client::CellEntry;
use spora_hashes::Hash;
static CELL_CONTEXT_ID_SEQUENCER: AtomicU64 = AtomicU64::new(0);
fn next_cell_context_id() -> Hash {
    let id = CELL_CONTEXT_ID_SEQUENCER.fetch_add(1, Ordering::SeqCst);
    Hash::from_slice(blake3_hash(id.to_le_bytes().as_slice()).as_ref())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Hash, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CellContextId(pub(crate) Hash);

impl Default for CellContextId {
    fn default() -> Self {
        CellContextId(next_cell_context_id())
    }
}

impl From<AccountId> for CellContextId {
    fn from(id: AccountId) -> Self {
        CellContextId(id.0)
    }
}

impl From<&AccountId> for CellContextId {
    fn from(id: &AccountId) -> Self {
        CellContextId(id.0)
    }
}

impl From<CellContextId> for AccountId {
    fn from(id: CellContextId) -> Self {
        AccountId(id.0)
    }
}

impl CellContextId {
    pub fn new(id: Hash) -> Self {
        CellContextId(id)
    }

    pub fn short(&self) -> String {
        let hex = self.to_hex();
        format!("[{}]", &hex[0..4])
    }
}

impl ToHex for CellContextId {
    fn to_hex(&self) -> String {
        self.0.to_hex()
    }
}

impl std::fmt::Display for CellContextId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub enum CellEntryVariant {
    Mature(CellEntryReference),
    Pending(CellEntryReference),
    Stasis(CellEntryReference),
}

pub struct Context {
    /// Mature (confirmed) cells
    pub(crate) mature: Vec<CellEntryReference>,
    /// Cells that are pending confirmation
    pub(crate) pending: AHashMap<CellEntryId, CellEntryReference>,
    /// Cells that are in stasis (freshly minted coinbase transactions only)
    pub(crate) stasis: AHashMap<CellEntryId, CellEntryReference>,
    /// All cells in possession of this context instance
    pub(crate) map: AHashMap<CellEntryId, CellEntryReference>,
    /// Outgoing transactions that have not yet been confirmed.
    /// Confirmation occurs when the transaction cells are
    /// removed from the context by the cell-change notification.
    pub(crate) outgoing: AHashMap<TransactionId, OutgoingTransaction>,
    /// Total balance of all cells in this context (mature, pending)
    balance: Option<Balance>,
    /// Addresses monitored by this cell context
    addresses: Arc<DashSet<Arc<Address>>>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            mature: vec![],
            pending: AHashMap::default(),
            stasis: AHashMap::default(),
            map: AHashMap::default(),
            outgoing: AHashMap::default(),
            balance: None,
            addresses: Arc::new(DashSet::new()),
        }
    }
}

impl Context {
    fn new_with_mature(mature: Vec<CellEntryReference>) -> Self {
        Self { mature, ..Default::default() }
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.mature.clear();
        self.stasis.clear();
        self.pending.clear();
        self.outgoing.clear();
        self.addresses.clear();
        self.balance = None;
    }
}

struct Inner {
    id: CellContextId,
    binding: CellContextBinding,
    context: Mutex<Context>,
    processor: CellProcessor,
}

impl Inner {
    pub fn new(processor: &CellProcessor, binding: CellContextBinding) -> Self {
        Self { id: binding.id(), binding, context: Mutex::new(Context::default()), processor: processor.clone() }
    }

    pub fn new_with_mature_entries(processor: &CellProcessor, binding: CellContextBinding, mature: Vec<CellEntryReference>) -> Self {
        let context = Context::new_with_mature(mature);
        Self { id: binding.id(), binding, context: Mutex::new(context), processor: processor.clone() }
    }
}

///
///  CellContext is a data structure responsible for monitoring multiple addresses
/// for transactions. It scans the address set for existing CellEntry records, then
/// monitors for transaction-related events in order to maintain a consistent view
/// on that cell set throughout its connection lifetime.
///
/// CellContext typically represents a single wallet account, but can monitor any set
/// of addresses. When receiving transaction events, CellContext detects types of these
/// events and emits corresponding notifications on the CellProcessor event multiplexer.
///
/// In addition to standard monitoring, CellContext works in conjunction with the
/// TransactionGenerator to track outgoing transactions in an effort to segregate
/// different types of CellEntry updates (regular incoming vs. change).
///
#[derive(Clone)]
pub struct CellContext {
    inner: Arc<Inner>,
}

impl CellContext {
    pub fn new(processor: &CellProcessor, binding: CellContextBinding) -> Self {
        Self { inner: Arc::new(Inner::new(processor, binding)) }
    }

    pub fn new_with_mature_entries(
        processor: &CellProcessor,
        binding: CellContextBinding,
        mature_entries: Vec<CellEntryReference>,
    ) -> Self {
        Self { inner: Arc::new(Inner::new_with_mature_entries(processor, binding, mature_entries)) }
    }

    pub fn context(&self) -> MutexGuard<'_, Context> {
        self.inner.context.lock().unwrap()
    }

    pub fn processor(&self) -> &CellProcessor {
        &self.inner.processor
    }

    pub fn binding(&self) -> CellContextBinding {
        self.inner.binding.clone()
    }

    pub fn id(&self) -> CellContextId {
        self.inner.id
    }

    pub fn id_as_ref(&self) -> &CellContextId {
        &self.inner.id
    }

    pub fn mature_cell_size(&self) -> usize {
        self.context().mature.len()
    }

    pub fn pending_cell_size(&self) -> usize {
        self.context().pending.len()
    }

    pub fn balance(&self) -> Option<Balance> {
        self.context().balance.clone()
    }

    pub fn addresses(&self) -> Arc<DashSet<Arc<Address>>> {
        self.context().addresses.clone()
    }

    pub async fn clear(&self) -> Result<()> {
        let local = self.addresses();
        let addresses = local.iter().map(|v| v.clone()).collect::<Vec<_>>();
        if !addresses.is_empty() {
            self.processor().unregister_addresses(addresses).await?;
            local.clear();
        }

        self.context().clear();

        Ok(())
    }

    pub async fn update_balance(&self) -> Result<Balance> {
        let balance = {
            let previous_balance = self.balance();
            let mut balance = self.calculate_balance().await;
            balance.delta(&previous_balance);
            let mut context = self.context();
            context.balance.replace(balance.clone());
            balance
        };
        self.processor().notify(Events::Balance { balance: Some(balance.clone()), id: self.id() }).await?;

        Ok(balance)
    }

    /// Process pending transaction. Remove mature cell entries and add them to the consumed set.
    /// Produces a notification on the even multiplexer.
    pub(crate) async fn register_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        {
            let current_daa_score =
                self.processor().current_daa_score().ok_or(Error::MissingDaaScore("register_outgoing_transaction()"))?;

            let mut context = self.context();
            let pending_cell_entries = pending_tx.cell_entries();
            context.mature.retain(|entry| !pending_cell_entries.contains_key(&entry.id()));

            let outgoing_transaction = OutgoingTransaction::new(current_daa_score, self.clone(), pending_tx.clone());
            self.processor().register_outgoing_transaction(outgoing_transaction.clone());
            context.outgoing.insert(outgoing_transaction.id(), outgoing_transaction);
        }

        Ok(())
    }

    pub(crate) async fn notify_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        let outgoing_tx = self.processor().outgoing().get(&pending_tx.id()).expect("outgoing transaction for notification");

        if pending_tx.is_batch() {
            let record = TransactionRecord::new_batch(self, &outgoing_tx, None)?;
            self.processor().notify(Events::Pending { record }).await?;
        } else {
            let record = TransactionRecord::new_outgoing(self, &outgoing_tx, None)?;
            self.processor().notify(Events::Pending { record }).await?;
        }
        self.update_balance().await?;
        Ok(())
    }

    /// Cancel outgoing transaction in case of a submission error. Removes [`OutgoingTransaction`] from the
    /// [`CellProcessor`] and returns cell entries from the outgoing transaction back to the mature pool.
    pub(crate) async fn cancel_outgoing_transaction(&self, pending_tx: &PendingTransaction) -> Result<()> {
        self.processor().cancel_outgoing_transaction(pending_tx.id());

        let mut context = self.context();

        let outgoing_transaction = context.outgoing.remove(&pending_tx.id()).expect("outgoing transaction");
        outgoing_transaction.cell_entries().iter().for_each(|(_, entry)| {
            context.mature.push(entry.clone());
        });

        Ok(())
    }

    /// Insert `cell_entry` into the tracked cell set.
    /// NOTE: The insert will be ignored if already present in the inner map.
    pub async fn insert(&self, cell_entry: CellEntryReference, current_daa_score: u64, force_maturity: bool) -> Result<()> {
        let mut context = self.context();
        if let std::collections::hash_map::Entry::Vacant(e) = context.map.entry(cell_entry.id().clone()) {
            e.insert(cell_entry.clone());
            if force_maturity {
                context.mature.sorted_insert_binary_asc_by_key(cell_entry.clone(), |entry| entry.amount_as_ref());
            } else {
                let params = NetworkParams::from(self.processor().network_id()?);
                match cell_entry.maturity(params, current_daa_score) {
                    Maturity::Stasis => {
                        context.stasis.insert(cell_entry.id().clone(), cell_entry.clone());
                        self.processor()
                            .stasis()
                            .insert(cell_entry.id().clone(), PendingCellEntryReference::new(cell_entry, self.clone()));
                    }
                    Maturity::Pending => {
                        context.pending.insert(cell_entry.id().clone(), cell_entry.clone());
                        self.processor()
                            .pending()
                            .insert(cell_entry.id().clone(), PendingCellEntryReference::new(cell_entry, self.clone()));
                    }
                    Maturity::Confirmed => {
                        context.mature.sorted_insert_binary_asc_by_key(cell_entry.clone(), |entry| entry.amount_as_ref());
                    }
                }
            }
            Ok(())
        } else {
            // log_warn!("Warning: ignoring duplicate cell entry");
            Ok(())
        }
    }

    pub async fn update(&self, cell_entry: CellEntryReference, _current_daa_score: u64, _force_maturity: bool) -> Result<bool> {
        let mut context = self.context();
        if context.map.get(&cell_entry.id()).is_some() {
            // if old_entry.block_daa_score() > cell_entry.block_daa_score() {
            //     return Ok(false);
            // }
            let id = cell_entry.id();
            let entry = PendingCellEntryReference::new(cell_entry.clone(), self.clone());

            context.stasis.entry(id.clone()).and_modify(|e| *e = cell_entry.clone());
            self.processor().stasis().entry(id.clone()).and_modify(|e| *e = entry.clone());

            context.pending.entry(id.clone()).and_modify(|e| *e = cell_entry.clone());
            self.processor().pending().entry(id.clone()).and_modify(|e| *e = entry.clone());

            if let Some(entry) = context.mature.iter_mut().find(|entry| entry.id() == id) {
                *entry = cell_entry.clone();
            }

            context.map.entry(id.clone()).and_modify(|e| *e = cell_entry);

            return Ok(true);
        }

        Ok(false)
    }

    pub async fn remove(&self, cells: Vec<CellEntryReference>) -> Result<Vec<CellEntryVariant>> {
        let mut context = self.context();
        let mut removed = vec![];
        let mut remove_mature_ids = vec![];

        for cell in cells.into_iter() {
            let id = cell.id();
            // remove from local map
            if context.map.remove(&id).is_some() {
                if let Some(pending) = context.pending.remove(&id) {
                    removed.push(CellEntryVariant::Pending(pending));
                    if self.processor().pending().remove(&id).is_none() {
                        log_error!("Error: unable to remove cell entry from global pending (with context)");
                    }
                } else if let Some(stasis) = context.stasis.remove(&id) {
                    removed.push(CellEntryVariant::Stasis(stasis));
                    if self.processor().stasis().remove(&id).is_none() {
                        log_error!("Error: unable to remove cell entry from global pending (with context)");
                    }
                } else {
                    remove_mature_ids.push(id);
                }
            } else if context.outgoing.get(&cell.transaction_id()).is_none() {
                // log_warn!("Warning: cell not found in CellContext map!");
            }
        }

        context.mature.retain(|entry| {
            if remove_mature_ids.contains(&entry.id()) {
                removed.push(CellEntryVariant::Mature(entry.clone()));
                false
            } else {
                true
            }
        });

        Ok(removed)
    }

    /// This function handles `Pending` to `Mature` transformation.
    pub async fn promote(&self, cells: Vec<CellEntryReference>) -> Result<()> {
        let transactions = HashMap::group_from(cells.iter().map(|cell| (cell.transaction_id(), cell.clone())));

        for (txid, cells) in transactions.into_iter() {
            for cell_entry in cells.iter() {
                let mut context = self.context();
                if context.pending.remove(cell_entry.id_as_ref()).is_some() {
                    context.mature.sorted_insert_binary_asc_by_key(cell_entry.clone(), |entry| entry.amount_as_ref());
                } else {
                    log_error!("Error: non-pending cell promotion!");
                }
            }

            // sanity check
            if self.context().outgoing.get(&txid).is_some() {
                unreachable!("Error: promotion of the outgoing transaction!");
            }

            let record = TransactionRecord::new_incoming(self, txid, &cells);
            self.processor().notify(Events::Maturity { record }).await?;
        }

        Ok(())
    }

    /// This function handles `Stasis` to `Pending` transformation.
    pub async fn revive(&self, cells: Vec<CellEntryReference>) -> Result<()> {
        let transactions = HashMap::group_from(cells.into_iter().map(|cell| (cell.transaction_id(), cell)));

        for (txid, cells) in transactions.into_iter() {
            for cell_entry in cells.iter() {
                let mut context = self.context();
                if context.stasis.remove(cell_entry.id_as_ref()).is_some() {
                    context.pending.insert(cell_entry.id(), cell_entry.clone());
                } else {
                    log_error!("Error: non-stasis cell revival!");
                    panic!("Error: non-stasis cell revival!");
                }
            }

            let record = TransactionRecord::new_incoming(self, txid, &cells);
            self.processor().notify(Events::Pending { record }).await?;
        }

        Ok(())
    }

    pub fn remove_outgoing_transaction(&self, txid: &TransactionId) -> Option<OutgoingTransaction> {
        let mut context = self.context();
        context.outgoing.remove(txid)
    }

    pub async fn extend_from_scan(&self, cell_entries: Vec<CellEntryReference>, current_daa_score: u64) -> Result<()> {
        let (pending, mature) = {
            let mut context = self.context();

            let mut pending = vec![];
            let mut mature = Vec::with_capacity(cell_entries.len());

            let params = NetworkParams::from(self.processor().network_id()?);

            for cell_entry in cell_entries.into_iter() {
                if let std::collections::hash_map::Entry::Vacant(e) = context.map.entry(cell_entry.id()) {
                    e.insert(cell_entry.clone());
                    match cell_entry.maturity(params, current_daa_score) {
                        Maturity::Stasis => {
                            context.stasis.insert(cell_entry.id().clone(), cell_entry.clone());
                            self.processor()
                                .stasis()
                                .insert(cell_entry.id().clone(), PendingCellEntryReference::new(cell_entry, self.clone()));
                        }
                        Maturity::Pending => {
                            pending.push(cell_entry.clone());
                            context.pending.insert(cell_entry.id().clone(), cell_entry.clone());
                            self.processor()
                                .pending()
                                .insert(cell_entry.id().clone(), PendingCellEntryReference::new(cell_entry, self.clone()));
                        }
                        Maturity::Confirmed => {
                            mature.push(cell_entry.clone());
                        }
                    }
                } else {
                    log_warn!("ignoring duplicate cell entry");
                }
            }

            context.mature.extend(mature.iter().cloned());
            context.mature.sort_by_key(|entry| entry.amount());

            (pending, mature)
        };

        // cascade discovery to the processor
        // for unixtime resolution

        let pending = HashMap::group_from(pending.into_iter().map(|cell| (cell.transaction_id(), cell)));
        for (id, cells) in pending.into_iter() {
            let record = TransactionRecord::new_external(self, id, &cells);
            self.processor().handle_discovery(record).await?;
        }

        let mature = HashMap::group_from(mature.into_iter().map(|cell| (cell.transaction_id(), cell)));
        for (id, cells) in mature.into_iter() {
            let record = TransactionRecord::new_external(self, id, &cells);
            self.processor().handle_discovery(record).await?;
        }

        Ok(())
    }

    pub async fn calculate_balance(&self) -> Balance {
        let context = self.context();
        let mature: u64 = context.mature.iter().map(|e| e.as_ref().amount).sum();
        let pending: u64 = context.pending.values().map(|e| e.as_ref().amount).sum();

        // this will aggregate only transactions containing
        // the final payments (not compound transactions)
        // and outgoing transactions that have not yet
        // been accepted
        let mut outgoing_without_batch_tx = 0;
        let mut outgoing: u64 = 0;
        let mut consumed: u64 = 0;

        let transactions = context.outgoing.values().filter(|tx| !tx.is_accepted());
        for tx in transactions {
            if let Some(payment_value) = tx.payment_value() {
                consumed += tx.aggregate_input_value();
                if tx.is_batch() {
                    outgoing += tx.fees() + tx.aggregate_output_value();
                } else {
                    // final tx
                    outgoing += tx.fees() + payment_value;
                    outgoing_without_batch_tx += payment_value;
                }
            } else {
                // compound tx has no payment value
                outgoing += tx.fees() + tx.aggregate_output_value();
                consumed += tx.aggregate_input_value();
            }
        }

        // TODO - remove this check once we are confident that
        // this condition does not occur. This is a temporary
        // log for a fixed bug, but we want to keep the check
        // just in case.
        if consumed < outgoing {
            log_error!("Error: outgoing transaction value exceeds available balance, mature: {mature}, consumed: {consumed}, outgoing: {outgoing}");
        }

        let mature = (mature + consumed).saturating_sub(outgoing);
        Balance::new(mature, pending, outgoing_without_batch_tx, context.mature.len(), context.pending.len(), context.stasis.len())
    }

    pub(crate) async fn update_cells(&self, cells: Vec<CellEntryReference>, current_daa_score: u64) -> Result<()> {
        if cells.is_empty() {
            return Ok(());
        }

        let cells = HashMap::group_from(cells.into_iter().map(|cell| (cell.transaction_id(), cell)));
        for (txid, cells) in cells.into_iter() {
            // get outgoing transaction from the processor in case the transaction
            // originates from a different [`Account`] represented by a different [`CellContext`].
            let outgoing_transaction = self.processor().outgoing().get(&txid);
            let force_maturity_if_outgoing = outgoing_transaction.is_some();
            let is_batch = outgoing_transaction.as_ref().map_or_else(|| false, |tx| tx.is_batch());
            if !is_batch {
                for cell in cells.iter() {
                    if let Err(err) = self.update(cell.clone(), current_daa_score, force_maturity_if_outgoing).await {
                        // TODO - remove `Result<>` from insert at a later date once
                        // we are confident that the insert will never result in an error.
                        log_error!("{}", err);
                    }
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn handle_cell_added(&self, cells: Vec<CellEntryReference>, current_daa_score: u64) -> Result<()> {
        // add cells to account set

        let params = NetworkParams::from(self.processor().network_id()?);

        let mut accepted_outgoing_transactions = AHashSet::new();

        let added = HashMap::group_from(cells.into_iter().map(|cell| (cell.transaction_id(), cell)));
        for (txid, cells) in added.into_iter() {
            // get outgoing transaction from the processor in case the transaction
            // originates from a different [`Account`] represented by a different [`CellContext`].
            let outgoing_transaction = self.processor().outgoing().get(&txid);

            let force_maturity_if_outgoing = outgoing_transaction.is_some();
            let is_coinbase_stasis =
                cells.first().map(|cell| matches!(cell.maturity(params, current_daa_score), Maturity::Stasis)).unwrap_or_default();
            let is_batch = outgoing_transaction.as_ref().map_or_else(|| false, |tx| tx.is_batch());
            if !is_batch {
                for cell in cells.iter() {
                    if let Err(err) = self.insert(cell.clone(), current_daa_score, force_maturity_if_outgoing).await {
                        // TODO - remove `Result<>` from insert at a later date once
                        // we are confident that the insert will never result in an error.
                        log_error!("{}", err);
                    }
                }
            }

            if let Some(outgoing_transaction) = outgoing_transaction {
                accepted_outgoing_transactions.insert((*outgoing_transaction).clone());

                if outgoing_transaction.is_batch() {
                    let record = TransactionRecord::new_batch(self, &outgoing_transaction, Some(current_daa_score))?;
                    self.processor().notify(Events::Maturity { record }).await?;
                } else if outgoing_transaction.originating_context() == self {
                    let record = TransactionRecord::new_change(self, &outgoing_transaction, Some(current_daa_score), &cells)?;
                    self.processor().notify(Events::Maturity { record }).await?;
                } else {
                    let record =
                        TransactionRecord::new_transfer_incoming(self, &outgoing_transaction, Some(current_daa_score), &cells)?;
                    self.processor().notify(Events::Maturity { record }).await?;
                }
            } else if !is_coinbase_stasis {
                // do not notify if coinbase transaction is in stasis
                let record = TransactionRecord::new_incoming(self, txid, &cells);
                self.processor().notify(Events::Pending { record }).await?;
            }
        }

        for outgoing_transaction in accepted_outgoing_transactions.into_iter() {
            outgoing_transaction.tag_as_accepted_at_daa_score(current_daa_score);
        }

        Ok(())
    }

    pub(crate) async fn handle_cell_removed(&self, cells: Vec<CellEntryReference>, current_daa_score: u64) -> Result<()> {
        // remove cells from account set

        let outgoing_transactions = self.processor().outgoing();
        #[allow(clippy::mutable_key_type)]
        let mut accepted_outgoing_transactions = HashSet::<OutgoingTransaction>::new();

        for cell in &cells {
            for outgoing_transaction in outgoing_transactions.iter() {
                if outgoing_transaction.cell_entries().contains_key(&cell.id()) {
                    accepted_outgoing_transactions.insert((*outgoing_transaction).clone());
                }
            }
        }

        for accepted_outgoing_transaction in accepted_outgoing_transactions.into_iter() {
            if accepted_outgoing_transaction.is_batch() {
                let record = TransactionRecord::new_batch(self, &accepted_outgoing_transaction, Some(current_daa_score))?;
                self.processor().notify(Events::Maturity { record }).await?;
            } else if accepted_outgoing_transaction.destination_context().is_some() {
                let record =
                    TransactionRecord::new_transfer_outgoing(self, &accepted_outgoing_transaction, Some(current_daa_score), &cells)?;
                self.processor().notify(Events::Maturity { record }).await?;
            } else {
                let record = TransactionRecord::new_outgoing(self, &accepted_outgoing_transaction, Some(current_daa_score))?;
                self.processor().notify(Events::Maturity { record }).await?;
            }
        }

        if cells.is_empty() {
            return Ok(());
        }

        let removed = self.remove(cells).await?;

        let mut mature = vec![];
        let mut pending = vec![];
        let mut stasis = vec![];

        removed.into_iter().for_each(|entry| match entry {
            CellEntryVariant::Mature(cell) => {
                mature.push(cell);
            }
            CellEntryVariant::Pending(cell) => {
                pending.push(cell);
            }
            CellEntryVariant::Stasis(cell) => {
                stasis.push(cell);
            }
        });

        let mature = HashMap::group_from(mature.into_iter().map(|cell| (cell.transaction_id(), cell)));
        let pending = HashMap::group_from(pending.into_iter().map(|cell| (cell.transaction_id(), cell)));
        let stasis = HashMap::group_from(stasis.into_iter().map(|cell| (cell.transaction_id(), cell)));

        for (txid, cells) in mature.into_iter() {
            let record = TransactionRecord::new_external(self, txid, &cells);
            self.processor().notify(Events::Maturity { record }).await?;
        }

        for (txid, cells) in pending.into_iter() {
            let record = TransactionRecord::new_reorg(self, txid, &cells);
            self.processor().notify(Events::Reorg { record }).await?;
        }

        for (txid, cells) in stasis.into_iter() {
            let record = TransactionRecord::new_stasis(self, txid, &cells);
            self.processor().notify(Events::Stasis { record }).await?;
        }

        Ok(())
    }

    pub async fn register_addresses(&self, addresses: &[Address]) -> Result<()> {
        if addresses.is_empty() {
            log_error!("cell processor: register for an empty address set");
        }

        let local = self.addresses();

        // addresses are filtered for a known address set where
        // addresses can already be registered with the processor
        // as a part of address space (Scan window) pre-caching.
        let addresses = addresses
            .iter()
            .filter_map(|address| {
                let address = Arc::new(address.clone());
                if local.insert(address.clone()) {
                    Some(address)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if addresses.is_not_empty() {
            self.processor().register_addresses(addresses, self).await?;
        }

        Ok(())
    }

    pub async fn unregister_addresses(&self, addresses: Vec<Address>) -> Result<()> {
        if !addresses.is_empty() {
            let local = self.addresses();
            let addresses = addresses.clone().into_iter().map(Arc::new).collect::<Vec<_>>();
            self.processor().unregister_addresses(addresses.clone()).await?;
            addresses.iter().for_each(|address| {
                local.remove(address);
            });
        } else {
            log_warn!("cell processor: unregister for an empty address set")
        }

        Ok(())
    }

    pub async fn scan_and_register_addresses(&self, addresses: Vec<Address>, current_daa_score: Option<u64>) -> Result<()> {
        self.register_addresses(&addresses).await?;
        let resp = self.processor().rpc_api().get_cells_by_addresses(addresses).await?;
        let refs: Vec<CellEntryReference> = resp.into_iter().map(CellEntryReference::from).collect();
        let current_daa_score = current_daa_score.or_else(|| {
                self.processor()
                    .current_daa_score()
            }).ok_or(Error::MissingDaaScore("Expecting DAA score or initialized CellProcessor when invoking scan_and_register_addresses() - You might be accessing CellProcessor APIs before it is initialized (see `cell-proc-start` event)"))?;
        self.extend_from_scan(refs, current_daa_score).await?;
        self.update_balance().await?;
        Ok(())
    }

    pub async fn get_cells(&self, addresses: Option<Vec<Address>>, min_amount_sau: Option<u64>) -> Result<Vec<CellEntry>> {
        let cells = &self.context().mature;
        let mut amount = 0;
        if let Some(addresses) = &addresses {
            if let Some(min_amount_sau) = min_amount_sau {
                let mut amount = 0;
                let filtered_cells = cells
                    .iter()
                    .filter_map(|cell| {
                        if let Some(address) = cell.address() {
                            if addresses.contains(&address) && amount < min_amount_sau {
                                amount += cell.amount();
                                return Some(cell.entry().clone());
                            }
                        }

                        None
                    })
                    .collect();
                return Ok(filtered_cells);
            } else {
                let filtered_cells = cells
                    .iter()
                    .filter_map(|cell| {
                        if let Some(address) = cell.address() {
                            if addresses.contains(&address) {
                                return Some(cell.entry().clone());
                            }
                        }
                        None
                    })
                    .collect();
                return Ok(filtered_cells);
            }
        }
        if let Some(min_amount_sau) = min_amount_sau {
            let filtered_cells = cells
                .iter()
                .filter_map(|cell| {
                    if amount < min_amount_sau {
                        amount += cell.amount();
                        return Some(cell.entry().clone());
                    }
                    None
                })
                .collect();
            return Ok(filtered_cells);
        }
        Ok(cells.iter().map(|cell| cell.entry().clone()).collect())
    }
}

impl Eq for CellContext {}

impl PartialEq for CellContext {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl std::hash::Hash for CellContext {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl Ord for CellContext {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id().cmp(other.id_as_ref())
    }
}

impl PartialOrd for CellContext {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.id().cmp(other.id_as_ref()))
    }
}
