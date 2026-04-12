use crate::cell as native;
use crate::error::Error;
use crate::events::{EventKind, Events};
use crate::imports::*;
use crate::result::Result;
use crate::wasm::notify::{CellProcessorEventTarget, CellProcessorNotificationCallback, CellProcessorNotificationTypeOrCallback};
use spora_consensus_core::network::NetworkIdT;
use spora_wallet_macros::declare_typescript_wasm_interface as declare;
use spora_wasm_core::events::{get_event_targets, Sink};
use spora_wrpc_wasm::RpcClient;
use workflow_log::log_error;

declare! {
    ICellProcessorArgs,
    r#"
    /**
     * CellProcessor constructor arguments.
     * 
     * @see {@link CellProcessor}, {@link CellContext}, {@link RpcClient}, {@link NetworkId}
     * @category Wallet SDK
     */
    export interface ICellProcessorArgs {
        /**
         * The RPC client to use for network communication.
         */
        rpc : RpcClient;
        networkId : NetworkId | string;
    }
    "#,
}

pub struct Inner {
    processor: native::CellProcessor,
    rpc: RpcClient,

    callbacks: Mutex<AHashMap<EventKind, Vec<Sink>>>,
    task_running: AtomicBool,
    task_ctl: DuplexChannel,
}

impl Inner {
    fn callbacks(&self, event: EventKind) -> Option<Vec<Sink>> {
        let callbacks = self.callbacks.lock().unwrap();
        let all = callbacks.get(&EventKind::All).cloned();
        let target = callbacks.get(&event).cloned();
        match (all, target) {
            (Some(mut vec_all), Some(vec_target)) => {
                vec_all.extend(vec_target);
                Some(vec_all)
            }
            (Some(vec_all), None) => Some(vec_all),
            (None, Some(vec_target)) => Some(vec_target),
            (None, None) => None,
        }
    }
}

cfg_if! {
    if #[cfg(any(feature = "wasm32-core", feature = "wasm32-sdk"))] {
        #[wasm_bindgen(typescript_custom_section)]
        const TS_NOTIFY: &'static str = r#"
        interface CellProcessor {
            /**
            * @param {CellProcessorNotificationCallback} callback
            */
            addEventListener(callback: CellProcessorNotificationCallback): void;
            /**
            * @param {CellProcessorEventType} event
            * @param {CellProcessorNotificationCallback} [callback]
            */
            addEventListener<E extends keyof CellProcessorEventMap>(
                event: E,
                callback: CellProcessorNotificationCallback<E>
            )
        }"#;
    }
}

///
/// CellProcessor class is the main coordinator that manages cell processing
/// between multiple CellContext instances. It acts as a bridge between the
/// Spora node RPC connection, address subscriptions and CellContext instances.
///
/// @see {@link ICellProcessorArgs},
/// {@link CellContext},
/// {@link RpcClient},
/// {@link NetworkId},
/// {@link IConnectEvent}
/// {@link IDisconnectEvent}
/// @category Wallet SDK
///
#[derive(Clone, CastFromJs)]
#[wasm_bindgen(inspectable)]
pub struct CellProcessor {
    inner: Arc<Inner>,
}

#[wasm_bindgen]
impl CellProcessor {
    /// CellProcessor constructor.
    ///
    ///
    ///
    /// @see {@link ICellProcessorArgs}
    #[wasm_bindgen(constructor)]
    pub fn ctor(js_value: ICellProcessorArgs) -> Result<CellProcessor> {
        let CellProcessorCreateArgs { rpc, network_id } = js_value.try_into()?;
        let rpc_api: Arc<DynRpcApi> = rpc.client().clone();
        let rpc_ctl = rpc.client().rpc_ctl().clone();
        let rpc_binding = Rpc::new(rpc_api, rpc_ctl);
        let processor = native::CellProcessor::new(Some(rpc_binding), Some(network_id), None, None);

        let this = CellProcessor {
            inner: Arc::new(Inner {
                processor: processor.clone(),
                rpc,
                callbacks: Mutex::new(AHashMap::new()),
                task_running: AtomicBool::new(false),
                task_ctl: DuplexChannel::oneshot(),
            }),
        };

        Ok(this)
    }

    /// Starts the CellProcessor and begins processing cell and other notifications.
    pub async fn start(&self) -> Result<()> {
        self.start_notification_task(self.inner.processor.multiplexer()).await?;
        self.inner.processor.start().await?;
        Ok(())
    }

    /// Stops the CellProcessor and ends processing cell and other notifications.
    pub async fn stop(&self) -> Result<()> {
        self.inner.processor.stop().await?;
        self.stop_notification_task().await?;
        Ok(())
    }

    #[wasm_bindgen(getter)]
    pub fn rpc(&self) -> RpcClient {
        self.inner.rpc.clone()
    }

    #[wasm_bindgen(getter, js_name = "networkId")]
    pub fn network_id(&self) -> Option<String> {
        self.inner.processor.network_id().ok().map(|network_id| network_id.to_string())
    }

    #[wasm_bindgen(js_name = "setNetworkId")]
    pub fn set_network_id(&self, network_id: &NetworkIdT) -> Result<()> {
        let network_id = NetworkId::try_cast_from(network_id)?;
        self.inner.processor.set_network_id(network_id.as_ref());
        Ok(())
    }

    #[wasm_bindgen(getter, js_name = "isActive")]
    pub fn is_active(&self) -> bool {
        let processor = &self.inner.processor;
        processor.try_rpc_ctl().map(|ctl| ctl.is_connected()).unwrap_or(false) && processor.is_connected() && processor.is_running()
    }

    ///
    /// Set the coinbase transaction maturity period DAA score for a given network.
    /// This controls the DAA period after which the user transactions are considered mature
    /// and the wallet subsystem emits the transaction maturity event.
    ///
    /// @see {@link TransactionRecord}
    /// @see {@link ICellProcessorEvent}
    ///
    /// @category Wallet SDK
    ///
    #[wasm_bindgen(js_name = "setCoinbaseTransactionMaturityDAA")]
    pub fn set_coinbase_transaction_maturity_period_daa_js(network_id: &NetworkIdT, value: u64) -> Result<()> {
        let network_id = NetworkId::try_cast_from(network_id)?.into_owned();
        crate::cell::set_coinbase_transaction_maturity_period_daa(&network_id, value);
        Ok(())
    }

    ///
    /// Set the user transaction maturity period DAA score for a given network.
    /// This controls the DAA period after which the user transactions are considered mature
    /// and the wallet subsystem emits the transaction maturity event.
    ///
    /// @see {@link TransactionRecord}
    /// @see {@link ICellProcessorEvent}
    ///
    /// @category Wallet SDK
    ///
    #[wasm_bindgen(js_name = "setUserTransactionMaturityDAA")]
    pub fn set_user_transaction_maturity_period_daa_js(network_id: &NetworkIdT, value: u64) -> Result<()> {
        let network_id = NetworkId::try_cast_from(network_id)?.into_owned();
        crate::cell::set_user_transaction_maturity_period_daa(&network_id, value);
        Ok(())
    }
}

impl TryCastFromJs for CellProcessor {
    type Error = workflow_wasm::error::Error;
    fn try_cast_from<'a, R>(value: &'a R) -> Result<Cast<'a, Self>, Self::Error>
    where
        R: AsRef<JsValue> + 'a,
    {
        Self::try_ref_from_js_value_as_cast(value)
    }
}

pub struct CellProcessorCreateArgs {
    rpc: RpcClient,
    network_id: NetworkId,
}

impl TryFrom<ICellProcessorArgs> for CellProcessorCreateArgs {
    type Error = Error;
    fn try_from(value: ICellProcessorArgs) -> std::result::Result<Self, Self::Error> {
        if let Some(object) = Object::try_from(&value) {
            let rpc = object.get_value("rpc")?;
            let rpc = RpcClient::try_ref_from_js_value(&rpc)?.clone();
            let network_id = object.get::<NetworkId>("networkId")?;
            Ok(CellProcessorCreateArgs { rpc, network_id })
        } else {
            Err(Error::custom("CellProcessor: supplied value must be an object"))
        }
    }
}

impl CellProcessor {
    pub fn inner(&self) -> &Arc<Inner> {
        &self.inner
    }

    pub fn processor(&self) -> &native::CellProcessor {
        &self.inner.processor
    }

    pub async fn start_notification_task(&self, multiplexer: &Multiplexer<Box<Events>>) -> Result<()> {
        let inner = self.inner.clone();

        if inner.task_running.load(Ordering::SeqCst) {
            log_error!("You are calling `CellProcessor.start()` twice without calling `CellProcessor.stop()`!");
            panic!("CellProcessor background task is already running");
        } else {
            inner.task_running.store(true, Ordering::SeqCst);
        }

        let ctl_receiver = inner.task_ctl.request.receiver.clone();
        let ctl_sender = inner.task_ctl.response.sender.clone();
        let channel = multiplexer.channel();

        spawn(async move {
            loop {
                select! {
                    _ = ctl_receiver.recv().fuse() => {
                        break;
                    },
                    msg = channel.receiver.recv().fuse() => {
                        if let Ok(notification) = &msg {
                            let event_type = EventKind::from(notification.as_ref());
                            let callbacks = inner.callbacks(event_type);
                            if let Some(handlers) = callbacks {
                                for handler in handlers.into_iter() {
                                    let value = notification.as_ref().to_js_value();
                                    if let Err(err) = handler.call(&value) {
                                        log_error!("Error while executing notification callback: {:?}", err);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            channel.close();
            inner.task_running.store(false, Ordering::SeqCst);
            ctl_sender.send(()).await.ok();
        });

        Ok(())
    }

    pub async fn stop_notification_task(&self) -> Result<()> {
        let inner = &self.inner;
        if inner.task_running.load(Ordering::SeqCst) {
            inner.task_ctl.signal(()).await.map_err(|err| JsValue::from_str(&err.to_string()))?;
        }
        Ok(())
    }
}

#[wasm_bindgen]
impl CellProcessor {
    #[wasm_bindgen(js_name = "addEventListener", skip_typescript)]
    pub fn add_event_listener(
        &self,
        event: CellProcessorNotificationTypeOrCallback,
        callback: Option<CellProcessorNotificationCallback>,
    ) -> Result<()> {
        if let Ok(sink) = Sink::try_from(&event) {
            let event = EventKind::All;
            self.inner.callbacks.lock().unwrap().entry(event).or_default().push(sink);
            Ok(())
        } else if let Some(Ok(sink)) = callback.map(Sink::try_from) {
            let targets: Vec<EventKind> = get_event_targets(event)?;
            for event in targets {
                self.inner.callbacks.lock().unwrap().entry(event).or_default().push(sink.clone());
            }
            Ok(())
        } else {
            Err(Error::custom("Invalid event listener callback"))
        }
    }

    #[wasm_bindgen(js_name = "removeEventListener")]
    pub fn remove_event_listener(
        &self,
        event: CellProcessorEventTarget,
        callback: Option<CellProcessorNotificationCallback>,
    ) -> Result<()> {
        let mut callbacks = self.inner.callbacks.lock().unwrap();
        if let Ok(sink) = Sink::try_from(&event) {
            // remove callback from all events
            for (_, handlers) in callbacks.iter_mut() {
                handlers.retain(|handler| handler != &sink);
            }
        } else if let Some(Ok(sink)) = callback.map(Sink::try_from) {
            // remove callback from specific events
            let targets: Vec<EventKind> = get_event_targets(event)?;
            for target in targets.into_iter() {
                callbacks.entry(target).and_modify(|handlers| {
                    handlers.retain(|handler| handler != &sink);
                });
            }
        } else {
            // remove all callbacks for the event
            let targets: Vec<EventKind> = get_event_targets(event)?;
            for event in targets {
                callbacks.remove(&event);
            }
        }
        Ok(())
    }
}
