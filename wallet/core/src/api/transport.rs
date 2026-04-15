//!
//! Server and Client transport wrappers that provide automatic
//! serialization and deserialization of Wallet API method
//! arguments and their return values.
//!
//! The serialization occurs using the underlying transport
//! which can be either Borsh or Serde JSON. At compile time,
//! the transport interface macro generates a unique `u64` id
//! (hash) for each API method based on the method name.
//! This id is then use to identify the method.
//!

use super::message::*;
use super::traits::WalletApi;
use crate::error::Error;
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::wallet::Wallet;
use async_trait::async_trait;
use borsh::BorshDeserialize;
use spora_wallet_macros::{build_wallet_client_transport_interface, build_wallet_server_transport_interface};
use workflow_core::channel::{unbounded, DuplexChannel, Receiver, Sender};
use workflow_core::task::spawn;

/// Transport interface supporting Borsh serialization
#[async_trait]
pub trait BorshCodec: Send + Sync {
    async fn call(&self, op: u64, request: Vec<u8>) -> Result<Vec<u8>>;
}

/// Transport interface supporting Serde JSON serialization
#[async_trait]
pub trait SerdeCodec: Send + Sync {
    async fn call(&self, op: &str, request: &str) -> Result<String>;
}

/// Transport interface enum supporting either Borsh and Serde JSON serialization
#[derive(Clone)]
pub enum Codec {
    Borsh(Arc<dyn BorshCodec>),
    Serde(Arc<dyn SerdeCodec>),
}

/// [`WalletClient`] is a client-side transport interface declaring
/// API methods that can be invoked via WalletApi method calls.
/// [`WalletClient`] is a counter-part to [`WalletServer`].
pub struct WalletClient {
    pub codec: Codec,
    notification_channels: Mutex<HashMap<u64, Sender<WalletNotification>>>,
    next_notification_channel_id: AtomicU64,
}

impl WalletClient {
    pub fn new(codec: Codec) -> Self {
        Self { codec, notification_channels: Mutex::new(HashMap::new()), next_notification_channel_id: AtomicU64::new(1) }
    }
}

#[async_trait]
impl WalletApi for WalletClient {
    async fn register_notifications(self: Arc<Self>) -> Result<(u64, Receiver<WalletNotification>)> {
        let channel_id = self.next_notification_channel_id.fetch_add(1, Ordering::SeqCst);
        let (sender, receiver) = unbounded();
        self.notification_channels.lock().unwrap().insert(channel_id, sender);
        Ok((channel_id, receiver))
    }
    async fn unregister_notifications(self: Arc<Self>, channel_id: u64) -> Result<()> {
        self.notification_channels
            .lock()
            .unwrap()
            .remove(&channel_id)
            .ok_or_else(|| Error::custom(format!("Unknown wallet notification channel id: {channel_id}")))?;
        Ok(())
    }

    build_wallet_client_transport_interface! {[
        Ping,
        GetStatus,
        Connect,
        Disconnect,
        ChangeNetworkId,
        RetainContext,
        GetContext,
        Batch,
        Flush,
        WalletEnumerate,
        WalletCreate,
        WalletOpen,
        WalletClose,
        WalletReload,
        WalletRename,
        WalletChangeSecret,
        WalletExport,
        WalletImport,
        PrvKeyDataEnumerate,
        PrvKeyDataCreate,
        PrvKeyDataRename,
        PrvKeyDataRemove,
        PrvKeyDataGet,
        AccountsRename,
        AccountsSelect,
        AccountsEnumerate,
        AccountsDiscovery,
        AccountsCreate,
        AccountsEnsureDefault,
        AccountsImport,
        AccountsActivate,
        AccountsDeactivate,
        AccountsGet,
        AccountsCreateNewAddress,
        AccountsSend,
        AccountsPssbSign,
        AccountsPssbBroadcast,
        AccountsPssbSend,
        AccountsGetCells,
        AccountsTransfer,
        AccountsEstimate,
        TransactionsDataGet,
        TransactionsReplaceNote,
        TransactionsReplaceMetadata,
        AddressBookEnumerate,
        FeeRateEstimate,
        FeeRatePollerEnable,
        FeeRatePollerDisable,
        AccountsCommitReveal,
        AccountsCommitRevealManual,

    ]}
}

impl WalletClient {
    fn notify_registered_channels(&self, notification: WalletNotification) {
        let failed_ids = {
            let channels = self.notification_channels.lock().unwrap();
            channels
                .iter()
                .filter_map(|(channel_id, sender)| sender.try_send(notification.clone()).err().map(|_| *channel_id))
                .collect::<Vec<_>>()
        };
        if !failed_ids.is_empty() {
            let mut channels = self.notification_channels.lock().unwrap();
            for channel_id in failed_ids {
                channels.remove(&channel_id);
            }
        }
    }
}

#[async_trait]
impl EventHandler for WalletClient {
    async fn handle_event(&self, event: &Events) {
        self.notify_registered_channels(WalletNotification::from(event));
    }
}

// ----------------------------

#[async_trait]
pub trait EventHandler: Send + Sync {
    // pub trait EventHandler {
    // async fn handle_event(&self, event: &Box<Events>);
    async fn handle_event(&self, event: &Events);
}

/// [`WalletServer`] is a server-side transport interface that declares
/// API methods that can be invoked via Borsh or Serde messages containing
/// serializations created using the [`Codec`] interface. The [`WalletServer`]
/// is a counter-part to [`WalletClient`].
pub struct WalletServer {
    // pub wallet_api: Arc<dyn WalletApi>,
    pub wallet: Arc<Wallet>,
    pub event_handler: Arc<dyn EventHandler>,
    task_ctl: DuplexChannel,
}

impl WalletServer {
    // pub fn new(wallet_api: Arc<dyn WalletApi>, event_handler : Arc<dyn EventHandler>) -> Self {
    //     Self { wallet_api, event_handler }
    pub fn new(wallet: Arc<Wallet>, event_handler: Arc<dyn EventHandler>) -> Self {
        Self { wallet, event_handler, task_ctl: DuplexChannel::unbounded() }
    }

    pub fn wallet_api(&self) -> Arc<dyn WalletApi> {
        self.wallet.clone()
    }
}

impl WalletServer {
    build_wallet_server_transport_interface! {[
        Ping,
        GetStatus,
        Connect,
        Disconnect,
        ChangeNetworkId,
        RetainContext,
        GetContext,
        Batch,
        Flush,
        WalletEnumerate,
        WalletCreate,
        WalletOpen,
        WalletClose,
        WalletReload,
        WalletRename,
        WalletChangeSecret,
        WalletExport,
        WalletImport,
        PrvKeyDataEnumerate,
        PrvKeyDataCreate,
        PrvKeyDataRename,
        PrvKeyDataRemove,
        PrvKeyDataGet,
        AccountsRename,
        AccountsSelect,
        AccountsEnumerate,
        AccountsDiscovery,
        AccountsCreate,
        AccountsEnsureDefault,
        AccountsImport,
        AccountsActivate,
        AccountsDeactivate,
        AccountsGet,
        AccountsCreateNewAddress,
        AccountsSend,
        AccountsPssbSign,
        AccountsPssbBroadcast,
        AccountsPssbSend,
        AccountsGetCells,
        AccountsTransfer,
        AccountsEstimate,
        TransactionsDataGet,
        TransactionsReplaceNote,
        TransactionsReplaceMetadata,
        AddressBookEnumerate,
        FeeRateEstimate,
        FeeRatePollerEnable,
        FeeRatePollerDisable,
        AccountsCommitReveal,
        AccountsCommitRevealManual,
    ]}
}

impl WalletServer {
    pub fn start(self: &Arc<Self>) {
        let task_ctl_receiver = self.task_ctl.request.receiver.clone();
        let task_ctl_sender = self.task_ctl.response.sender.clone();
        let events = self.wallet.multiplexer().channel();

        let this = self.clone();
        spawn(async move {
            loop {
                select! {
                    _ = task_ctl_receiver.recv().fuse() => {
                        break;
                    },

                    msg = events.receiver.recv().fuse() => {
                        match msg {
                            Ok(event) => {
                                this.event_handler.handle_event(&event).await;//.unwrap_or_else(|e| log_error!("Wallet::handle_event() error: {}", e));
                            },
                            Err(err) => {
                                log_error!("Wallet: error while receiving multiplexer message: {err}");
                                log_error!("Suspending Wallet processing...");

                                break;
                            }
                        }
                    },
                }
            }

            task_ctl_sender.send(()).await.unwrap();
        });
    }

    pub async fn stop_task(&self) -> Result<()> {
        self.task_ctl.signal(()).await.expect("Wallet::stop_task() `signal` error");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::keydata::PrvKeyDataVariantKind;
    use crate::tests::make_xpub;
    use spora_bip32::Prefix as KeyPrefix;
    use spora_consensus_core::network::{NetworkId, NetworkType};
    use std::time::Duration;
    use tokio::time::timeout;

    struct NoopCodec;

    #[async_trait]
    impl BorshCodec for NoopCodec {
        async fn call(&self, _op: u64, _request: Vec<u8>) -> Result<Vec<u8>> {
            Err(Error::custom("unused in test"))
        }
    }

    #[tokio::test]
    async fn wallet_client_event_handler_forwards_registered_notifications() {
        let client = Arc::new(WalletClient::new(Codec::Borsh(Arc::new(NoopCodec))));
        let (_channel_id, receiver) = client.clone().register_notifications().await.unwrap();

        client.handle_event(&Events::Error { message: "transport notification".to_string() }).await;

        let notification = receiver.recv().await.unwrap();
        assert!(matches!(notification, WalletNotification::Error { message } if message == "transport notification"));
    }

    #[tokio::test]
    async fn wallet_server_forwards_multiplexer_events_to_client_notification_channels() {
        let wallet = Arc::new(Wallet::try_with_rpc(None, Wallet::resident_store().unwrap(), None).unwrap());
        let client = Arc::new(WalletClient::new(Codec::Borsh(Arc::new(NoopCodec))));
        let server = Arc::new(WalletServer::new(wallet.clone(), client.clone()));
        let (_channel_id, receiver) = client.clone().register_notifications().await.unwrap();

        server.start();
        wallet.notify(Events::Error { message: "server notification".to_string() }).await.unwrap();

        let notification = receiver.recv().await.unwrap();
        assert!(matches!(notification, WalletNotification::Error { message } if message == "server notification"));

        server.stop_task().await.unwrap();
    }

    #[tokio::test]
    async fn wallet_server_forwards_watch_only_account_create_notification() {
        let wallet = Arc::new(
            Wallet::try_with_rpc(None, Wallet::resident_store().unwrap(), None)
                .unwrap()
                .with_network_id(NetworkId::new(NetworkType::Mainnet))
                .unwrap(),
        );
        let wallet_secret = Secret::from("test-wallet-secret");
        wallet
            .clone()
            .wallet_create(wallet_secret.clone(), WalletCreateArgs::new(None, None, EncryptionKind::default(), None, false))
            .await
            .unwrap();

        let client = Arc::new(WalletClient::new(Codec::Borsh(Arc::new(NoopCodec))));
        let server = Arc::new(WalletServer::new(wallet.clone(), client.clone()));
        let (_channel_id, receiver) = client.clone().register_notifications().await.unwrap();

        server.start();
        let descriptor = wallet
            .clone()
            .accounts_create(
                wallet_secret,
                AccountCreateArgs::new_watch_only(None, vec![make_xpub().to_string(Some(KeyPrefix::XPUB))], 1, false),
            )
            .await
            .unwrap();
        assert_eq!(descriptor.kind.as_ref(), WATCH_ONLY_ACCOUNT_KIND);

        let notification = receiver.recv().await.unwrap();
        assert!(matches!(
            notification,
            WalletNotification::AccountCreate { account_descriptor }
            if account_descriptor.kind.as_ref() == WATCH_ONLY_ACCOUNT_KIND
        ));

        server.stop_task().await.unwrap();
    }

    #[tokio::test]
    async fn wallet_server_forwards_multisig_account_create_notification() {
        let wallet = Arc::new(
            Wallet::try_with_rpc(None, Wallet::resident_store().unwrap(), None)
                .unwrap()
                .with_network_id(NetworkId::new(NetworkType::Mainnet))
                .unwrap(),
        );
        let wallet_secret = Secret::from("test-wallet-secret");
        wallet
            .clone()
            .wallet_create(wallet_secret.clone(), WalletCreateArgs::new(None, None, EncryptionKind::default(), None, false))
            .await
            .unwrap();
        let prv_key_data_id = wallet
            .clone()
            .prv_key_data_create(
                wallet_secret.clone(),
                PrvKeyDataCreateArgs::new(
                    Some("multisig".to_string()),
                    None,
                    Secret::from("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"),
                    PrvKeyDataVariantKind::Mnemonic,
                ),
            )
            .await
            .unwrap();

        let client = Arc::new(WalletClient::new(Codec::Borsh(Arc::new(NoopCodec))));
        let server = Arc::new(WalletServer::new(wallet.clone(), client.clone()));
        let (_channel_id, receiver) = client.clone().register_notifications().await.unwrap();

        server.start();
        let descriptor = wallet
            .clone()
            .accounts_create(
                wallet_secret,
                AccountCreateArgs::new_multisig(
                    vec![PrvKeyDataArgs::new(prv_key_data_id, None)],
                    vec![make_xpub().to_string(Some(KeyPrefix::XPUB))],
                    Some("team vault".to_string()),
                    2,
                ),
            )
            .await
            .unwrap();
        assert_eq!(descriptor.kind.as_ref(), MULTISIG_ACCOUNT_KIND);

        let notification = receiver.recv().await.unwrap();
        assert!(matches!(
            notification,
            WalletNotification::AccountCreate { account_descriptor }
            if account_descriptor.kind.as_ref() == MULTISIG_ACCOUNT_KIND
        ));

        server.stop_task().await.unwrap();
    }

    #[tokio::test]
    async fn wallet_server_forwards_watch_only_account_import_notification_offline() {
        let wallet = Arc::new(
            Wallet::try_with_rpc(None, Wallet::resident_store().unwrap(), None)
                .unwrap()
                .with_network_id(NetworkId::new(NetworkType::Mainnet))
                .unwrap(),
        );
        let wallet_secret = Secret::from("test-wallet-secret");
        wallet
            .clone()
            .wallet_create(wallet_secret.clone(), WalletCreateArgs::new(None, None, EncryptionKind::default(), None, false))
            .await
            .unwrap();

        let client = Arc::new(WalletClient::new(Codec::Borsh(Arc::new(NoopCodec))));
        let server = Arc::new(WalletServer::new(wallet.clone(), client.clone()));
        let (_channel_id, receiver) = client.clone().register_notifications().await.unwrap();

        server.start();
        let descriptor = wallet
            .clone()
            .accounts_import(
                wallet_secret,
                AccountCreateArgs::new_watch_only(None, vec![make_xpub().to_string(Some(KeyPrefix::XPUB))], 1, false),
            )
            .await
            .unwrap();
        assert_eq!(descriptor.kind.as_ref(), WATCH_ONLY_ACCOUNT_KIND);

        let notification = receiver.recv().await.unwrap();
        assert!(matches!(
            notification,
            WalletNotification::AccountCreate { account_descriptor }
            if account_descriptor.kind.as_ref() == WATCH_ONLY_ACCOUNT_KIND
        ));

        server.stop_task().await.unwrap();
    }

    #[tokio::test]
    async fn wallet_server_forwards_multisig_account_import_notification_offline() {
        let wallet = Arc::new(
            Wallet::try_with_rpc(None, Wallet::resident_store().unwrap(), None)
                .unwrap()
                .with_network_id(NetworkId::new(NetworkType::Mainnet))
                .unwrap(),
        );
        let wallet_secret = Secret::from("test-wallet-secret");
        wallet
            .clone()
            .wallet_create(wallet_secret.clone(), WalletCreateArgs::new(None, None, EncryptionKind::default(), None, false))
            .await
            .unwrap();
        let prv_key_data_id = wallet
            .clone()
            .prv_key_data_create(
                wallet_secret.clone(),
                PrvKeyDataCreateArgs::new(
                    Some("multisig".to_string()),
                    None,
                    Secret::from("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about"),
                    PrvKeyDataVariantKind::Mnemonic,
                ),
            )
            .await
            .unwrap();

        let client = Arc::new(WalletClient::new(Codec::Borsh(Arc::new(NoopCodec))));
        let server = Arc::new(WalletServer::new(wallet.clone(), client.clone()));
        let (_channel_id, receiver) = client.clone().register_notifications().await.unwrap();

        server.start();
        let descriptor = wallet
            .clone()
            .accounts_import(
                wallet_secret,
                AccountCreateArgs::new_multisig(
                    vec![PrvKeyDataArgs::new(prv_key_data_id, None)],
                    vec![make_xpub().to_string(Some(KeyPrefix::XPUB))],
                    Some("team vault".to_string()),
                    2,
                ),
            )
            .await
            .unwrap();
        assert_eq!(descriptor.kind.as_ref(), MULTISIG_ACCOUNT_KIND);

        let notification = receiver.recv().await.unwrap();
        assert!(matches!(
            notification,
            WalletNotification::AccountCreate { account_descriptor }
            if account_descriptor.kind.as_ref() == MULTISIG_ACCOUNT_KIND
        ));

        server.stop_task().await.unwrap();
    }

    #[tokio::test]
    async fn wallet_client_unregister_notifications_stops_transport_delivery() {
        let wallet = Arc::new(Wallet::try_with_rpc(None, Wallet::resident_store().unwrap(), None).unwrap());
        let client = Arc::new(WalletClient::new(Codec::Borsh(Arc::new(NoopCodec))));
        let server = Arc::new(WalletServer::new(wallet.clone(), client.clone()));
        let (channel_id, receiver) = client.clone().register_notifications().await.unwrap();

        server.start();
        client.clone().unregister_notifications(channel_id).await.unwrap();
        wallet.notify(Events::Error { message: "should-not-deliver".to_string() }).await.unwrap();

        let recv_result = timeout(Duration::from_millis(200), receiver.recv())
            .await
            .expect("receiver should close once notification channel is unregistered");
        assert!(recv_result.is_err(), "receiver should be closed after unregister");

        server.stop_task().await.unwrap();
    }
}
