//!
//! Generic wallet [`Account`] trait implementation used
//! by different types of accounts.
//!

pub mod descriptor;
pub mod kind;
pub mod pssb;
pub mod variants;
use crate::cell::balance::{AtomicBalance, BalanceStrings};
use crate::cell::{CellContext, CellContextBinding};
use crate::derivation::build_derivate_paths;
use crate::derivation::AddressDerivationManagerTrait;
use crate::imports::*;
use crate::storage::account::AccountSettings;
use crate::storage::AccountMetadata;
use crate::storage::{PrvKeyData, PrvKeyDataId};
use crate::tx::PaymentOutput;
use crate::tx::{Fees, Generator, GeneratorSettings, GeneratorSummary, PaymentDestination, PendingTransaction, Signer};
pub use kind::*;
use pssb::{
    bundle_from_psst_generator, bundle_to_finalizer_stream, commit_reveal_batch_bundle, pssb_signer_for_address,
    psst_to_pending_transaction, PSSBSigner, PSSTGenerator,
};
use spora_bip32::PrivateKey;
use spora_bip32::{ChildNumber, ExtendedPrivateKey};
use spora_consensus_core::cell_diff::CellMeta;
use spora_wallet_psst::bundle::Bundle;
pub use variants::*;
use workflow_core::abortable::Abortable;

/// Notification callback type used by [`Account::sweep`] and [`Account::send`].
/// Allows tracking in-flight transactions during transaction generation.
pub type GenerationNotifier = Arc<dyn Fn(&PendingTransaction) + Send + Sync>;
/// Scan notification callback type used by [`DerivationCapableAccount::derivation_scan`].
/// Provides derivation discovery scan progress information.
pub type ScanNotifier = Arc<dyn Fn(usize, usize, u64, Option<TransactionId>) + Send + Sync>;

/// General-purpose wrapper around [`AccountSettings`] (managed by [`Inner`]).
pub struct Context {
    pub settings: AccountSettings,
}

impl Context {
    pub fn new(settings: AccountSettings) -> Self {
        Self { settings }
    }

    pub fn settings(&self) -> &AccountSettings {
        &self.settings
    }
}

/// Account `Inner` struct used by most account types.
pub struct Inner {
    context: Mutex<Context>,
    id: AccountId,
    storage_key: AccountStorageKey,
    wallet: Arc<Wallet>,
    cell_context: CellContext,
}

impl Inner {
    pub fn new(wallet: &Arc<Wallet>, id: AccountId, storage_key: AccountStorageKey, settings: AccountSettings) -> Self {
        let cell_context = CellContext::new(wallet.cell_processor(), CellContextBinding::AccountId(id));

        let context = Context { settings };
        Inner { context: Mutex::new(context), id, storage_key, wallet: wallet.clone(), cell_context: cell_context.clone() }
    }

    pub fn from_storage(wallet: &Arc<Wallet>, storage: &AccountStorage) -> Self {
        Self::new(wallet, storage.id, storage.storage_key, storage.settings.clone())
    }

    pub fn context(&self) -> MutexGuard<'_, Context> {
        self.context.lock().unwrap()
    }

    pub fn store(&self) -> &Arc<dyn Interface> {
        self.wallet.store()
    }
}

/// Generic wallet [`Account`] trait implementation used
/// by different types of accounts.
#[async_trait]
pub trait Account: AnySync + Send + Sync + 'static {
    fn inner(&self) -> &Arc<Inner>;

    fn context(&self) -> MutexGuard<'_, Context> {
        self.inner().context.lock().unwrap()
    }

    fn id(&self) -> &AccountId {
        &self.inner().id
    }

    fn storage_key(&self) -> &AccountStorageKey {
        &self.inner().storage_key
    }

    fn account_kind(&self) -> AccountKind;

    fn wallet(&self) -> &Arc<Wallet> {
        &self.inner().wallet
    }

    fn cell_context(&self) -> &CellContext {
        &self.inner().cell_context
    }

    fn balance(&self) -> Option<Balance> {
        self.cell_context().balance()
    }

    fn balance_as_strings(&self, padding: Option<usize>) -> Result<BalanceStrings> {
        Ok(BalanceStrings::from((self.balance().as_ref(), &self.wallet().network_id()?.into(), padding)))
    }

    fn name(&self) -> Option<String> {
        self.context().settings.name.clone()
    }

    fn feature(&self) -> Option<String> {
        None
    }

    fn xpub_keys(&self) -> Option<&ExtendedPublicKeys> {
        None
    }

    fn name_or_id(&self) -> String {
        if let Some(name) = self.name() {
            if name.is_empty() {
                self.id().short()
            } else {
                name
            }
        } else {
            self.id().short()
        }
    }

    fn name_with_id(&self) -> String {
        if let Some(name) = self.name() {
            if name.is_empty() {
                self.id().short()
            } else {
                format!("{name} {}", self.id().short())
            }
        } else {
            self.id().short()
        }
    }

    async fn rename(&self, wallet_secret: &Secret, name: Option<&str>) -> Result<()> {
        {
            let mut context = self.context();
            context.settings.name = name.map(String::from);
        }

        let account = self.to_storage()?;
        self.wallet().store().as_account_store()?.store_single(&account, None).await?;

        self.wallet().store().commit(wallet_secret).await?;
        Ok(())
    }

    fn get_list_string(&self) -> Result<String> {
        let name = style(self.name_with_id()).blue();
        let balance = self.balance_as_strings(None)?;
        let mature_cell_count = self.cell_context().mature_cell_size();
        let pending_cell_count = self.cell_context().pending_cell_size();
        let info = match (mature_cell_count, pending_cell_count) {
            (0, 0) => "".to_string(),
            (_, 0) => {
                format!("{} cells", mature_cell_count.separated_string())
            }
            (0, _) => {
                format!("{} cells pending", pending_cell_count.separated_string())
            }
            _ => {
                format!("{} cells, {} cells pending", mature_cell_count.separated_string(), pending_cell_count.separated_string())
            }
        };
        Ok(format!("{name}: {balance}   {}", style(info).dim()))
    }

    fn prv_key_data_id(&self) -> Result<&PrvKeyDataId> {
        // TODO - change to AssocPrvKeyDataIds
        Err(Error::ResidentAccount)
    }

    async fn prv_key_data(&self, wallet_secret: Secret) -> Result<PrvKeyData> {
        let prv_key_data_id = self.prv_key_data_id()?;

        let keydata = self
            .wallet()
            .store()
            .as_prv_key_data_store()?
            .load_key_data(&wallet_secret, prv_key_data_id)
            .await?
            .ok_or(Error::PrivateKeyNotFound(*prv_key_data_id))?;
        Ok(keydata)
    }

    fn to_storage(&self) -> Result<AccountStorage>;
    fn metadata(&self) -> Result<Option<AccountMetadata>>;
    fn descriptor(&self) -> Result<descriptor::AccountDescriptor>;

    async fn scan(self: Arc<Self>, window_size: Option<usize>, extent: Option<u32>) -> Result<()> {
        self.cell_context().clear().await?;

        let current_daa_score = self.wallet().current_daa_score().ok_or(Error::NotConnected)?;
        let balance = Arc::new(AtomicBalance::default());

        match self.clone().as_derivation_capable() {
            Ok(account) => {
                let derivation = account.derivation();

                let extent = match extent {
                    Some(depth) => ScanExtent::Depth(depth),
                    None => ScanExtent::EmptyWindow,
                };

                let scans = [
                    Scan::new_with_address_manager(
                        derivation.receive_address_manager(),
                        &balance,
                        current_daa_score,
                        window_size,
                        Some(extent),
                    ),
                    Scan::new_with_address_manager(
                        derivation.change_address_manager(),
                        &balance,
                        current_daa_score,
                        window_size,
                        Some(extent),
                    ),
                ];

                let futures = scans.iter().map(|scan| scan.scan(self.cell_context())).collect::<Vec<_>>();

                join_all(futures).await.into_iter().collect::<Result<Vec<_>>>()?;
            }
            Err(_) => {
                let mut address_set = HashSet::<Address>::new();
                address_set.insert(self.receive_address()?);
                address_set.insert(self.change_address()?);

                let scan = Scan::new_with_address_set(address_set, &balance, current_daa_score);
                scan.scan(self.cell_context()).await?;
            }
        }

        self.cell_context().update_balance().await?;

        Ok(())
    }

    fn minimum_signatures(&self) -> u16;

    // default account address (receive[0])
    fn default_address(&self) -> Result<Address> {
        self.receive_address()
    }

    // all addresses in the account (receive + change up to and including the last used index)
    fn account_addresses(&self) -> Result<Vec<Address>> {
        let receive = self.receive_address()?;
        let change = self.change_address()?;
        if receive == change {
            Ok(vec![receive])
        } else {
            Ok(vec![receive, change])
        }
    }

    fn receive_address(&self) -> Result<Address>;

    fn change_address(&self) -> Result<Address>;

    /// Start Account service task
    async fn start(self: Arc<Self>) -> Result<()> {
        self.connect().await?;
        Ok(())
    }

    /// Stop Account service task
    async fn stop(self: Arc<Self>) -> Result<()> {
        self.cell_context().clear().await?;
        self.disconnect().await?;
        Ok(())
    }

    /// handle connection event
    async fn connect(self: Arc<Self>) -> Result<()> {
        let vacated = self.wallet().active_accounts().insert(self.clone().as_dyn_arc());
        if vacated.is_none() && self.wallet().is_connected() {
            self.scan(None, None).await?;
        }
        Ok(())
    }

    /// handle disconnection event
    async fn disconnect(&self) -> Result<()> {
        self.wallet().active_accounts().remove(self.id());
        Ok(())
    }

    fn as_dyn_arc(self: Arc<Self>) -> Arc<dyn Account>;

    /// Aggregate all account cells into the change address.
    /// Also known as "compounding".
    async fn sweep(
        self: Arc<Self>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        fee_rate: Option<f64>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<spora_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));

        let settings = GeneratorSettings::try_new_with_account(
            self.clone().as_dyn_arc(),
            PaymentDestination::Change,
            fee_rate,
            Fees::None,
            None,
        )?;

        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transaction.try_sign()?;
            ids.push(transaction.try_submit(&self.wallet().rpc_api()).await?);

            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    /// Send funds to a [`PaymentDestination`] comprised of one or multiple [`PaymentOutputs`](crate::tx::PaymentOutputs)
    /// or [`PaymentDestination::Change`] variant that will forward funds to the change address.
    async fn send(
        self: Arc<Self>,
        destination: PaymentDestination,
        fee_rate: Option<f64>,
        _priority_fee_sau: Fees,
        payload: Option<Vec<u8>>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
    ) -> Result<(GeneratorSummary, Vec<spora_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));

        let settings =
            GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), destination, fee_rate, _priority_fee_sau, payload)?;

        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transaction.try_sign()?;
            ids.push(transaction.try_submit(&self.wallet().rpc_api()).await?);

            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    async fn commit_reveal_manual(
        self: Arc<Self>,
        start_destination: PaymentDestination,
        end_destination: PaymentDestination,
        script_sig: Vec<u8>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        fee_rate: Option<f64>,
        reveal_fee_sau: u64,
        payload: Option<Vec<u8>>,
        abortable: &Abortable,
    ) -> Result<Bundle, Error> {
        commit_reveal_batch_bundle(
            pssb::CommitRevealBatchKind::Manual { hop_payment: start_destination, destination_payment: end_destination },
            reveal_fee_sau,
            script_sig,
            payload,
            fee_rate,
            self.clone().as_dyn_arc(),
            wallet_secret,
            payment_secret,
            abortable,
        )
        .await
    }

    async fn commit_reveal(
        self: Arc<Self>,
        address: Address,
        script_sig: Vec<u8>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        commit_amount_sau: u64,
        fee_rate: Option<f64>,
        reveal_fee_sau: u64,
        payload: Option<Vec<u8>>,
        abortable: &Abortable,
    ) -> Result<Bundle, Error> {
        commit_reveal_batch_bundle(
            pssb::CommitRevealBatchKind::Parameterized { address, commit_amount_sau },
            reveal_fee_sau,
            script_sig,
            payload,
            fee_rate,
            self.clone().as_dyn_arc(),
            wallet_secret,
            payment_secret,
            abortable,
        )
        .await
    }

    async fn pssb_from_send_generator(
        self: Arc<Self>,
        destination: PaymentDestination,
        fee_rate: Option<f64>,
        _priority_fee_sau: Fees,
        payload: Option<Vec<u8>>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
    ) -> Result<Bundle, Error> {
        let settings =
            GeneratorSettings::try_new_with_account(self.clone().as_dyn_arc(), destination, fee_rate, _priority_fee_sau, payload)?;
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(PSSBSigner::new(self.clone().as_dyn_arc(), keydata, payment_secret));
        let generator = Generator::try_new(settings, None, Some(abortable))?;

        let bundle = bundle_from_psst_generator(PSSTGenerator::new(generator, signer, self.wallet().address_prefix()?)).await?;
        Ok(bundle)
    }

    async fn get_cells(self: Arc<Self>, addresses: Option<Vec<Address>>, min_amount_sau: Option<u64>) -> Result<Vec<CellMeta>> {
        let cells = self.cell_context().get_cells(addresses, min_amount_sau).await?;
        Ok(cells
            .into_iter()
            .map(|cell| {
                let metadata = cell.embedded_cell_metadata().expect("wallet cells must carry canonical Cell metadata");
                CellMeta::from_cell_metadata(
                    cell.capacity(),
                    metadata.data_bytes,
                    metadata.lock_hash,
                    metadata.type_hash,
                    metadata.data_hash,
                    cell.block_daa_score,
                    cell.is_coinbase,
                )
            })
            .collect())
    }

    async fn pssb_sign(
        self: Arc<Self>,
        bundle: &Bundle,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        sign_for_address: Option<&Address>,
    ) -> Result<Bundle, Error> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(PSSBSigner::new(self.clone().as_dyn_arc(), keydata.clone(), payment_secret.clone()));

        let network_id = self.wallet().clone().network_id()?;
        let (derivation_path, key_fingerprint) = if self.account_kind() == KEYPAIR_ACCOUNT_KIND {
            // let secret_key = keydata.as_secret_key(payment_secret.as_ref())?.ok_or(Error::Custom(format!("Private key not found for account")))?;
            // (None, secp256k1::PublicKey::from_secret_key_global(&secret_key).fingerprint())
            (None, None)
        } else {
            let derivation = self.as_derivation_capable()?;

            let (derivation_path, _) =
                build_derivate_paths(&derivation.account_kind(), derivation.account_index(), derivation.cosigner_index())?;

            let key_fingerprint = keydata.get_xprv(payment_secret.clone().as_ref())?.public_key().fingerprint();
            (Some(derivation_path), Some(key_fingerprint))
        };

        match pssb_signer_for_address(bundle, signer, network_id, sign_for_address, derivation_path, key_fingerprint).await {
            Ok(signer) => Ok(signer),
            Err(e) => Err(Error::from(e.to_string())),
        }
    }

    /// Execute a transfer to another wallet account.
    async fn transfer(
        self: Arc<Self>,
        destination_account_id: AccountId,
        transfer_amount_sau: u64,
        fee_rate: Option<f64>,
        _priority_fee_sau: Fees,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        abortable: &Abortable,
        notifier: Option<GenerationNotifier>,
        guard: &WalletGuard,
    ) -> Result<(GeneratorSummary, Vec<spora_hashes::Hash>)> {
        let keydata = self.prv_key_data(wallet_secret).await?;
        let signer = Arc::new(Signer::new(self.clone().as_dyn_arc(), keydata, payment_secret));

        let destination_account = self
            .wallet()
            .get_account_by_id(&destination_account_id, guard)
            .await?
            .ok_or_else(|| Error::AccountNotFound(destination_account_id))?;

        let destination_address = destination_account.receive_address()?;
        let final_transaction_destination = PaymentDestination::from(PaymentOutput::new(destination_address, transfer_amount_sau));
        let final_transaction_payload = None;

        let settings = GeneratorSettings::try_new_with_account(
            self.clone().as_dyn_arc(),
            final_transaction_destination,
            fee_rate,
            _priority_fee_sau,
            final_transaction_payload,
        )?
        .cell_context_transfer(destination_account.cell_context());

        let generator = Generator::try_new(settings, Some(signer), Some(abortable))?;

        let mut stream = generator.stream();
        let mut ids = vec![];
        while let Some(transaction) = stream.try_next().await? {
            transaction.try_sign()?;
            ids.push(transaction.try_submit(&self.wallet().rpc_api()).await?);

            if let Some(notifier) = notifier.as_ref() {
                notifier(&transaction);
            }
            yield_executor().await;
        }

        Ok((generator.summary(), ids))
    }

    async fn estimate(
        self: Arc<Self>,
        destination: PaymentDestination,
        fee_rate: Option<f64>,
        _priority_fee_sau: Fees,
        payload: Option<Vec<u8>>,
        abortable: &Abortable,
    ) -> Result<GeneratorSummary> {
        let settings = GeneratorSettings::try_new_with_account(self.as_dyn_arc(), destination, fee_rate, _priority_fee_sau, payload)?;

        let generator = Generator::try_new(settings, None, Some(abortable))?;

        let mut stream = generator.stream();
        while let Some(_transaction) = stream.try_next().await? {
            yield_executor().await;
        }

        Ok(generator.summary())
    }

    fn as_derivation_capable(self: Arc<Self>) -> Result<Arc<dyn DerivationCapableAccount>> {
        Err(Error::AccountAddressDerivationCaps)
    }

    fn create_address_private_keys<'l>(
        self: Arc<Self>,
        key_data: &PrvKeyData,
        payment_secret: &Option<Secret>,
        addresses: &[&'l Address],
    ) -> Result<Vec<(Address, secp256k1::SecretKey)>> {
        let account = self.clone().as_derivation_capable().expect("expecting derivation capable account");
        let (receive, change) = account.derivation().addresses_indexes(addresses)?;
        let private_keys = account.create_private_keys(key_data, payment_secret, &receive, &change)?;
        Ok(private_keys.into_iter().map(|(addr, key)| (addr.clone(), key)).collect())
    }

    async fn pssb_broadcast(self: Arc<Self>, bundle: &Bundle) -> Result<Vec<spora_hashes::Hash>> {
        let mut ids = Vec::new();
        let mut stream = bundle_to_finalizer_stream(bundle);

        while let Some(result) = stream.next().await {
            match result {
                Ok(psst) => {
                    let change = self.change_address()?;
                    let transaction =
                        psst_to_pending_transaction(psst, self.wallet().network_id()?, change, self.cell_context().clone().into())?;
                    ids.push(transaction.try_submit(&self.wallet().rpc_api()).await?);
                }
                Err(e) => {
                    eprintln!("Error processing a PSST from bundle: {:?}", e);
                }
            }
        }

        Ok(ids)
    }
}

downcast_sync!(dyn Account);

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait DerivationCapableAccount: Account {
    fn derivation(&self) -> Arc<dyn AddressDerivationManagerTrait>;

    fn account_index(&self) -> u64;

    async fn derivation_scan(
        self: Arc<Self>,
        wallet_secret: Secret,
        payment_secret: Option<Secret>,
        start: usize,
        extent: usize,
        window: usize,
        sweep: bool,
        fee_rate: Option<f64>,
        abortable: &Abortable,
        update_address_indexes: bool,
        notifier: Option<ScanNotifier>,
    ) -> Result<()> {
        let derivation = self.derivation();

        let prv_key_data = self.prv_key_data(wallet_secret).await?;
        let payload = prv_key_data.payload.decrypt(payment_secret.as_ref())?;
        let xkey = payload.get_xprv(payment_secret.as_ref())?;

        let receive_address_manager = derivation.receive_address_manager();
        let change_address_manager = derivation.change_address_manager();

        let change_address_index = change_address_manager.index();
        let change_address_keypair =
            derivation.get_range_with_keys(true, change_address_index..change_address_index + 1, false, &xkey).await?;

        let rpc = self.wallet().rpc_api();
        let notifier = notifier.as_ref();

        let mut index: usize = start;
        let mut last_notification = 0;
        let mut aggregate_balance = 0;
        let mut aggregate_cell_count = 0;
        let mut last_change_address_index = change_address_index;
        let mut last_receive_address_index = receive_address_manager.index();

        let change_address = change_address_keypair[0].0.clone();

        while index < extent && !abortable.is_aborted() {
            let first = index as u32;
            let last = (index + window) as u32;
            index = last as usize;

            let (mut keys, addresses) = if sweep {
                let mut keypairs = derivation.get_range_with_keys(false, first..last, true, &xkey).await?;
                let change_keypairs = derivation.get_range_with_keys(true, first..last, true, &xkey).await?;
                keypairs.extend(change_keypairs);
                let mut keys = vec![];
                let addresses = keypairs
                    .iter()
                    .map(|(address, key)| {
                        keys.push(key.to_bytes());
                        address.clone()
                    })
                    .collect::<Vec<_>>();
                keys.push(change_address_keypair[0].1.to_bytes());
                (keys, addresses)
            } else {
                let mut addresses = receive_address_manager.get_range_with_args(first..last, true)?;
                let change_addresses = change_address_manager.get_range_with_args(first..last, true)?;
                addresses.extend(change_addresses);
                (vec![], addresses)
            };

            let cells = rpc.get_cells_by_addresses(addresses.clone()).await?;
            let mut balance = 0;
            let cells = cells
                .iter()
                .map(|cell| {
                    let cell_ref = CellEntryReference::from(cell);
                    if let Some(address) = cell_ref.cell.address.as_ref() {
                        if let Some(address_index) = receive_address_manager.inner().address_to_index_map.get(address) {
                            if last_receive_address_index < *address_index {
                                last_receive_address_index = *address_index;
                            }
                        } else if let Some(address_index) = change_address_manager.inner().address_to_index_map.get(address) {
                            if last_change_address_index < *address_index {
                                last_change_address_index = *address_index;
                            }
                        } else {
                            panic!("Account::derivation_scan() has received an unknown address: `{address}`");
                        }
                    }
                    balance += cell_ref.cell.amount;
                    cell_ref
                })
                .collect::<Vec<_>>();
            aggregate_cell_count += cells.len();

            if balance > 0 {
                aggregate_balance += balance;
                if sweep {
                    let settings = GeneratorSettings::try_new_with_iterator(
                        self.wallet().network_id()?,
                        Box::new(cells.into_iter()),
                        None,
                        change_address.clone(),
                        1,
                        PaymentDestination::Change,
                        fee_rate,
                        Fees::None,
                        None,
                        None,
                    )?;

                    let generator = Generator::try_new(settings, None, Some(abortable))?;

                    let mut stream = generator.stream();
                    while let Some(transaction) = stream.try_next().await? {
                        transaction.try_sign_with_keys(&keys, None)?;
                        let id = transaction.try_submit(&rpc).await?;
                        if let Some(notifier) = notifier {
                            notifier(index, aggregate_cell_count, balance, Some(id));
                        }
                        yield_executor().await;
                    }
                } else {
                    if let Some(notifier) = notifier {
                        notifier(index, aggregate_cell_count, aggregate_balance, None);
                    }
                    yield_executor().await;
                }
            }

            if index > last_notification + 1_000 {
                last_notification = index;
                if let Some(notifier) = notifier {
                    notifier(index, aggregate_cell_count, aggregate_balance, None);
                }
                yield_executor().await;
            }

            keys.zeroize();
        }

        if index > last_notification {
            if let Some(notifier) = notifier {
                notifier(index, aggregate_cell_count, aggregate_balance, None);
            }
        }

        // update address manager with the last used index
        if update_address_indexes {
            receive_address_manager.set_index(last_receive_address_index)?;
            change_address_manager.set_index(last_change_address_index)?;

            let metadata = self.metadata()?.expect("derivation accounts must provide metadata");
            let store = self.wallet().store().as_account_store()?;
            store.update_metadata(vec![metadata]).await?;
            self.clone().scan(None, None).await?;
            self.wallet().notify(Events::AccountUpdate { account_descriptor: self.descriptor()? }).await?;
        }

        Ok(())
    }

    async fn new_receive_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().receive_address_manager().new_address()?;
        self.cell_context().register_addresses(std::slice::from_ref(&address)).await?;

        let metadata = self.metadata()?.expect("derivation accounts must provide metadata");
        let store = self.wallet().store().as_account_store()?;
        store.update_metadata(vec![metadata]).await?;

        self.wallet().notify(Events::AccountUpdate { account_descriptor: self.descriptor()? }).await?;

        Ok(address)
    }

    async fn new_change_address(self: Arc<Self>) -> Result<Address> {
        let address = self.derivation().change_address_manager().new_address()?;
        self.cell_context().register_addresses(std::slice::from_ref(&address)).await?;

        let metadata = self.metadata()?.expect("derivation accounts must provide metadata");
        let store = self.wallet().store().as_account_store()?;
        store.update_metadata(vec![metadata]).await?;

        self.wallet().notify(Events::AccountUpdate { account_descriptor: self.descriptor()? }).await?;

        Ok(address)
    }

    fn cosigner_index(&self) -> u32 {
        0
    }

    fn create_private_keys<'l>(
        &self,
        key_data: &PrvKeyData,
        payment_secret: &Option<Secret>,
        receive: &[(&'l Address, u32)],
        change: &[(&'l Address, u32)],
    ) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
        let payload = key_data.payload.decrypt(payment_secret.as_ref())?;
        let xkey = payload.get_xprv(payment_secret.as_ref())?;
        create_private_keys(&self.account_kind(), self.cosigner_index(), self.account_index(), &xkey, receive, change)
    }

    // Retrieve receive address by index.
    async fn receive_address_at_index(self: Arc<Self>, index: u32) -> Result<Address> {
        let address = self.derivation().receive_address_manager().get_range(index..index + 1)?.first().unwrap().clone();
        Ok(address)
    }

    // Retrieve change address by index.
    async fn change_address_at_index(self: Arc<Self>, index: u32) -> Result<Address> {
        let address = self.derivation().change_address_manager().get_range(index..index + 1)?.first().unwrap().clone();
        Ok(address)
    }
}

downcast_sync!(dyn DerivationCapableAccount);

pub(crate) fn create_private_keys<'l>(
    account_kind: &AccountKind,
    cosigner_index: u32,
    account_index: u64,
    xkey: &ExtendedPrivateKey<secp256k1::SecretKey>,
    receive: &[(&'l Address, u32)],
    change: &[(&'l Address, u32)],
) -> Result<Vec<(&'l Address, secp256k1::SecretKey)>> {
    let paths = build_derivate_paths(account_kind, account_index, cosigner_index)?;
    let mut private_keys = vec![];
    let receive_xkey = xkey.clone().derive_path(&paths.0)?;
    let change_xkey = xkey.clone().derive_path(&paths.1)?;

    for (address, index) in receive.iter() {
        private_keys.push((*address, *receive_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
    }
    for (address, index) in change.iter() {
        private_keys.push((*address, *change_xkey.derive_child(ChildNumber::new(*index, false)?)?.private_key()));
    }

    Ok(private_keys)
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::create_private_keys;
    use super::ExtendedPrivateKey;
    use crate::imports::BIP32_ACCOUNT_KIND;
    use spora_addresses::{Address, Prefix, Version};
    use spora_bip32::secp256k1::SecretKey;
    use std::str::FromStr;

    fn dummy_address() -> Address {
        Address::new(Prefix::Testnet, Version::PubKey, &[0u8; 32]).expect("Valid dummy address")
    }

    #[tokio::test]
    async fn bip32_private_keys_are_derived() {
        let key = "xprv9s21ZrQH143K2SDYtUz6dphDH3yRLAC7Jc552GYiXai3STvqgc3JBZxH2M4KaKhriaZDSS9KL7zUi5kYpggFspkiZBYWNCxbp27CCcnsJUs";
        let xkey = ExtendedPrivateKey::<SecretKey>::from_str(key).unwrap();

        let dummy = dummy_address();

        let receive_addrs = (0u32..5).map(|i| (&dummy, i)).collect::<Vec<(&Address, u32)>>();
        let change_addrs = (0u32..5).map(|i| (&dummy, i)).collect::<Vec<(&Address, u32)>>();

        let receive_derived = create_private_keys(&BIP32_ACCOUNT_KIND.into(), 0, 0, &xkey, &receive_addrs, &[]).unwrap();
        let change_derived = create_private_keys(&BIP32_ACCOUNT_KIND.into(), 0, 0, &xkey, &[], &change_addrs).unwrap();

        assert_eq!(receive_derived.len(), 5);
        assert_eq!(change_derived.len(), 5);
        assert_ne!(receive_derived[0].1.secret_bytes(), receive_derived[1].1.secret_bytes());
        assert_ne!(change_derived[0].1.secret_bytes(), change_derived[1].1.secret_bytes());
    }
}
