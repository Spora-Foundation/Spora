//!
//! [`WalletApi`] trait implementation for the [`Wallet`] struct.
//!

use crate::api::{message::*, traits::WalletApi};
use crate::events::Events;
use crate::imports::*;
use crate::result::Result;
use crate::storage::interface::TransactionRangeResult;
use crate::storage::Binding;
use crate::tx::Fees;
use spora_rpc_core::RpcFeeEstimate;
use spora_wallet_psst::bundle::Bundle;
use workflow_core::channel::Receiver;

#[async_trait]
impl WalletApi for super::Wallet {
    async fn register_notifications(self: Arc<Self>, channel: Receiver<WalletNotification>) -> Result<u64> {
        let channel_id = self.inner.next_notification_channel_id.fetch_add(1, Ordering::SeqCst);
        self.inner.notification_channels.lock().unwrap().insert(channel_id, channel);
        Ok(channel_id)
    }
    async fn unregister_notifications(self: Arc<Self>, channel_id: u64) -> Result<()> {
        self.inner
            .notification_channels
            .lock()
            .unwrap()
            .remove(&channel_id)
            .ok_or_else(|| Error::custom(format!("Unknown wallet notification channel id: {channel_id}")))?;
        Ok(())
    }

    async fn get_status_call(self: Arc<Self>, request: GetStatusRequest) -> Result<GetStatusResponse> {
        let guard = self.guard();
        let guard = guard.lock().await;

        let GetStatusRequest { name } = request;
        let context = name.and_then(|name| self.inner.retained_contexts.lock().unwrap().get(&name).cloned());

        let is_connected = self.is_connected();
        let is_synced = self.is_synced();
        let is_open = self.is_open();
        let network_id = self.network_id().ok();
        let (url, is_wrpc_client) =
            if let Some(wrpc_client) = self.try_wrpc_client() { (wrpc_client.url(), true) } else { (None, false) };

        let selected_account_id = self.inner.selected_account.lock().unwrap().as_ref().map(|account| *account.id());

        let (wallet_descriptor, account_descriptors) = if self.is_open() {
            let wallet_descriptor = self.descriptor();
            let account_descriptors = self.account_descriptors(&guard).await.ok();
            (wallet_descriptor, account_descriptors)
        } else {
            (None, None)
        };

        Ok(GetStatusResponse {
            is_connected,
            is_synced,
            is_open,
            network_id,
            url,
            is_wrpc_client,
            context,
            selected_account_id,
            wallet_descriptor,
            account_descriptors,
        })
    }

    async fn retain_context_call(self: Arc<Self>, request: RetainContextRequest) -> Result<RetainContextResponse> {
        let RetainContextRequest { name, data } = request;

        if let Some(data) = data {
            self.inner.retained_contexts.lock().unwrap().insert(name, Arc::new(data));
            Ok(RetainContextResponse {})
        } else {
            self.inner.retained_contexts.lock().unwrap().remove(&name);
            Ok(RetainContextResponse {})
        }
    }

    async fn get_context_call(self: Arc<Self>, request: GetContextRequest) -> Result<GetContextResponse> {
        let GetContextRequest { name } = request;
        let data = self.inner.retained_contexts.lock().unwrap().get(&name).map(|data| (**data).clone());
        Ok(GetContextResponse { data })
    }

    // -------------------------------------------------------------------------------------

    async fn connect_call(self: Arc<Self>, request: ConnectRequest) -> Result<ConnectResponse> {
        use workflow_rpc::client::{ConnectOptions, ConnectStrategy};

        let ConnectRequest { url, network_id, retry_on_error, block_async_connect, require_sync } = request;

        if let Some(wrpc_client) = self.try_wrpc_client().as_ref() {
            let strategy = if retry_on_error { ConnectStrategy::Retry } else { ConnectStrategy::Fallback };

            let url = url
                .map(|url| wrpc_client.parse_url_with_network_type(url, network_id.into()).map_err(|e| e.to_string()))
                .transpose()?;
            let options = ConnectOptions { block_async_connect, strategy, url, ..Default::default() };
            wrpc_client.disconnect().await?;

            self.set_network_id(&network_id)?;

            let processor = self.cell_processor().clone();
            let (sender, receiver) = oneshot();

            // set connection signaler that gets triggered
            // by cell processor when connection occurs
            processor.set_connection_signaler(sender);

            // connect rpc
            wrpc_client.connect(Some(options)).await.map_err(|e| e.to_string())?;

            // wait for connection signal, cascade if error
            receiver.recv().await?.map_err(Error::custom)?;

            if require_sync && !self.is_synced() {
                Err(Error::NotSynced)
            } else {
                Ok(ConnectResponse {})
            }
        } else {
            Err(Error::NotWrpcClient)
        }
    }

    async fn disconnect_call(self: Arc<Self>, _request: DisconnectRequest) -> Result<DisconnectResponse> {
        if let Some(wrpc_client) = self.try_wrpc_client() {
            wrpc_client.disconnect().await?;
            Ok(DisconnectResponse {})
        } else {
            Err(Error::NotWrpcClient)
        }
    }

    async fn change_network_id_call(self: Arc<Self>, request: ChangeNetworkIdRequest) -> Result<ChangeNetworkIdResponse> {
        let ChangeNetworkIdRequest { network_id } = &request;
        self.set_network_id(network_id)?;
        Ok(ChangeNetworkIdResponse {})
    }

    // -------------------------------------------------------------------------------------

    async fn ping_call(self: Arc<Self>, request: PingRequest) -> Result<PingResponse> {
        log_info!("Wallet received ping request '{:?}' ...", request.message);
        Ok(PingResponse { message: request.message })
    }

    async fn batch_call(self: Arc<Self>, _request: BatchRequest) -> Result<BatchResponse> {
        self.store().batch().await?;
        Ok(BatchResponse {})
    }

    async fn flush_call(self: Arc<Self>, request: FlushRequest) -> Result<FlushResponse> {
        let FlushRequest { wallet_secret } = request;
        self.store().flush(&wallet_secret).await?;
        Ok(FlushResponse {})
    }

    async fn wallet_enumerate_call(self: Arc<Self>, _request: WalletEnumerateRequest) -> Result<WalletEnumerateResponse> {
        let wallet_descriptors = self.store().wallet_list().await?;
        self.notify(Events::WalletHint { hint: None }).await.ok();
        Ok(WalletEnumerateResponse { wallet_descriptors })
    }

    async fn wallet_create_call(self: Arc<Self>, request: WalletCreateRequest) -> Result<WalletCreateResponse> {
        let WalletCreateRequest { wallet_secret, wallet_args } = request;

        let (wallet_descriptor, storage_descriptor) = self.create_wallet(&wallet_secret, wallet_args).await?;

        Ok(WalletCreateResponse { wallet_descriptor, storage_descriptor })
    }

    async fn wallet_open_call(self: Arc<Self>, request: WalletOpenRequest) -> Result<WalletOpenResponse> {
        let guard = self.guard();
        let guard = guard.lock().await;

        let WalletOpenRequest { wallet_secret, filename, account_descriptors } = request;
        let args = WalletOpenArgs { account_descriptors };
        let account_descriptors = self.open(&wallet_secret, filename, args, &guard).await?;
        Ok(WalletOpenResponse { account_descriptors })
    }

    async fn wallet_close_call(self: Arc<Self>, _request: WalletCloseRequest) -> Result<WalletCloseResponse> {
        self.close().await?;
        Ok(WalletCloseResponse {})
    }

    async fn wallet_reload_call(self: Arc<Self>, request: WalletReloadRequest) -> Result<WalletReloadResponse> {
        let WalletReloadRequest { reactivate } = request;
        if !self.is_open() {
            return Err(Error::WalletNotOpen);
        }

        let guard = self.guard();
        let guard = guard.lock().await;

        self.reload(reactivate, &guard).await?;
        Ok(WalletReloadResponse {})
    }

    async fn wallet_rename_call(self: Arc<Self>, request: WalletRenameRequest) -> Result<WalletRenameResponse> {
        let WalletRenameRequest { wallet_secret, title, filename } = request;
        self.rename(title, filename, &wallet_secret).await?;
        Ok(WalletRenameResponse {})
    }

    async fn wallet_change_secret_call(self: Arc<Self>, request: WalletChangeSecretRequest) -> Result<WalletChangeSecretResponse> {
        let WalletChangeSecretRequest { old_wallet_secret, new_wallet_secret } = request;
        self.store().change_secret(&old_wallet_secret, &new_wallet_secret).await?;
        Ok(WalletChangeSecretResponse {})
    }

    async fn wallet_export_call(self: Arc<Self>, request: WalletExportRequest) -> Result<WalletExportResponse> {
        let WalletExportRequest { wallet_secret, include_transactions } = request;

        let options = storage::WalletExportOptions { include_transactions };
        let wallet_data = self.store().wallet_export(&wallet_secret, options).await?;

        Ok(WalletExportResponse { wallet_data })
    }

    async fn wallet_import_call(self: Arc<Self>, request: WalletImportRequest) -> Result<WalletImportResponse> {
        let WalletImportRequest { wallet_secret, wallet_data } = request;

        let wallet_descriptor = self.store().wallet_import(&wallet_secret, &wallet_data).await?;

        Ok(WalletImportResponse { wallet_descriptor })
    }

    async fn prv_key_data_enumerate_call(
        self: Arc<Self>,
        _request: PrvKeyDataEnumerateRequest,
    ) -> Result<PrvKeyDataEnumerateResponse> {
        let prv_key_data_list = self.store().as_prv_key_data_store()?.iter().await?.try_collect::<Vec<_>>().await?;
        Ok(PrvKeyDataEnumerateResponse { prv_key_data_list })
    }

    async fn prv_key_data_create_call(self: Arc<Self>, request: PrvKeyDataCreateRequest) -> Result<PrvKeyDataCreateResponse> {
        let PrvKeyDataCreateRequest { wallet_secret, prv_key_data_args } = request;
        let prv_key_data_id = self.create_prv_key_data(&wallet_secret, prv_key_data_args).await?;
        Ok(PrvKeyDataCreateResponse { prv_key_data_id })
    }

    async fn prv_key_data_remove_call(self: Arc<Self>, request: PrvKeyDataRemoveRequest) -> Result<PrvKeyDataRemoveResponse> {
        let PrvKeyDataRemoveRequest { wallet_secret, prv_key_data_id } = request;

        let guard = self.guard();
        let _guard = guard.lock().await;

        let account_store = self.store().as_account_store()?;
        let mut accounts = account_store.iter(None).await?;
        let mut referencing_account = None;
        while let Some((account, _)) = accounts.try_next().await? {
            if account.prv_key_data_ids.contains(&prv_key_data_id) {
                referencing_account = Some(account.id);
                break;
            }
        }

        if let Some(account_id) = referencing_account {
            return Err(Error::custom(format!(
                "Private key data {} is still referenced by account {}",
                prv_key_data_id.to_hex(),
                account_id
            )));
        }

        self.store().as_prv_key_data_store()?.remove(&wallet_secret, &prv_key_data_id).await?;
        self.store().commit(&wallet_secret).await?;

        Ok(PrvKeyDataRemoveResponse {})
    }

    async fn prv_key_data_get_call(self: Arc<Self>, request: PrvKeyDataGetRequest) -> Result<PrvKeyDataGetResponse> {
        let PrvKeyDataGetRequest { prv_key_data_id, wallet_secret } = request;

        let prv_key_data = self.store().as_prv_key_data_store()?.load_key_data(&wallet_secret, &prv_key_data_id).await?;

        Ok(PrvKeyDataGetResponse { prv_key_data })
    }

    async fn accounts_rename_call(self: Arc<Self>, request: AccountsRenameRequest) -> Result<AccountsRenameResponse> {
        let AccountsRenameRequest { account_id, name, wallet_secret } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;
        account.rename(&wallet_secret, name.as_deref()).await?;

        Ok(AccountsRenameResponse {})
    }

    async fn accounts_select_call(self: Arc<Self>, request: AccountsSelectRequest) -> Result<AccountsSelectResponse> {
        let AccountsSelectRequest { account_id } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        if let Some(account_id) = account_id {
            let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;
            self.select(Some(&account)).await?;
        } else {
            self.select(None).await?;
        }
        // self.inner.selected_account.lock().unwrap().replace(account);

        Ok(AccountsSelectResponse {})
    }

    async fn accounts_enumerate_call(self: Arc<Self>, _request: AccountsEnumerateRequest) -> Result<AccountsEnumerateResponse> {
        let guard = self.guard();
        let guard = guard.lock().await;

        let account_descriptors = self.account_descriptors(&guard).await?;
        Ok(AccountsEnumerateResponse { account_descriptors })
    }

    async fn accounts_activate_call(self: Arc<Self>, request: AccountsActivateRequest) -> Result<AccountsActivateResponse> {
        let AccountsActivateRequest { account_ids } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        self.activate_accounts(account_ids.as_deref(), &guard).await?;

        Ok(AccountsActivateResponse {})
    }

    async fn accounts_deactivate_call(self: Arc<Self>, request: AccountsDeactivateRequest) -> Result<AccountsDeactivateResponse> {
        let AccountsDeactivateRequest { account_ids } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        self.deactivate_accounts(account_ids.as_deref(), &guard).await?;

        Ok(AccountsDeactivateResponse {})
    }

    async fn accounts_discovery_call(self: Arc<Self>, request: AccountsDiscoveryRequest) -> Result<AccountsDiscoveryResponse> {
        let AccountsDiscoveryRequest { discovery_kind: _, address_scan_extent, account_scan_extent, bip39_passphrase, bip39_mnemonic } =
            request;

        let last_account_index_found =
            self.scan_bip44_accounts(bip39_mnemonic, bip39_passphrase, address_scan_extent, account_scan_extent).await?;

        Ok(AccountsDiscoveryResponse { last_account_index_found })
    }

    async fn accounts_create_call(self: Arc<Self>, request: AccountsCreateRequest) -> Result<AccountsCreateResponse> {
        let AccountsCreateRequest { wallet_secret, account_create_args } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.create_account(&wallet_secret, account_create_args, true, &guard).await?;
        let account_descriptor = account.descriptor()?;

        Ok(AccountsCreateResponse { account_descriptor })
    }

    async fn accounts_ensure_default_call(
        self: Arc<Self>,
        request: AccountsEnsureDefaultRequest,
    ) -> Result<AccountsEnsureDefaultResponse> {
        let AccountsEnsureDefaultRequest { wallet_secret, payment_secret, account_kind, mnemonic_phrase } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account_descriptor = self
            .ensure_default_account_impl(&wallet_secret, payment_secret.as_ref(), account_kind, mnemonic_phrase.as_ref(), &guard)
            .await?;

        Ok(AccountsEnsureDefaultResponse { account_descriptor })
    }

    async fn accounts_import_call(self: Arc<Self>, request: AccountsImportRequest) -> Result<AccountsImportResponse> {
        let AccountsImportRequest { wallet_secret, account_create_args } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.create_account(&wallet_secret, account_create_args, true, &guard).await?;
        account.clone().scan(Some(100), Some(5000)).await?;
        let account_descriptor = account.descriptor()?;
        self.store().as_account_store()?.store_single(&account.to_storage()?, account.metadata()?.as_ref()).await?;
        self.store().commit(&wallet_secret).await?;

        Ok(AccountsImportResponse { account_descriptor })
    }

    async fn accounts_get_call(self: Arc<Self>, request: AccountsGetRequest) -> Result<AccountsGetResponse> {
        let AccountsGetRequest { account_id } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;
        let account_descriptor =
            account.descriptor().map_err(|e| Error::Custom(format!("Account descriptor not available: {}", e)))?;
        Ok(AccountsGetResponse { account_descriptor })
    }

    async fn accounts_create_new_address_call(
        self: Arc<Self>,
        request: AccountsCreateNewAddressRequest,
    ) -> Result<AccountsCreateNewAddressResponse> {
        let AccountsCreateNewAddressRequest { account_id, kind } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;

        let address = match kind {
            NewAddressKind::Receive => account.as_derivation_capable()?.receive_address()?,
            NewAddressKind::Change => account.as_derivation_capable()?.change_address()?,
        };

        Ok(AccountsCreateNewAddressResponse { address })
    }

    async fn accounts_send_call(self: Arc<Self>, request: AccountsSendRequest) -> Result<AccountsSendResponse> {
        let AccountsSendRequest { account_id, wallet_secret, payment_secret, destination, fee_rate, _priority_fee_sau, payload } =
            request;

        let guard = self.guard();
        let guard = guard.lock().await;
        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;

        let abortable = Abortable::new();
        let (generator_summary, transaction_ids) =
            account.send(destination, fee_rate, _priority_fee_sau, payload, wallet_secret, payment_secret, &abortable, None).await?;

        Ok(AccountsSendResponse { generator_summary, transaction_ids })
    }

    async fn accounts_pssb_sign_call(self: Arc<Self>, request: AccountsPssbSignRequest) -> Result<AccountsPssbSignResponse> {
        let AccountsPssbSignRequest { account_id, pssb, wallet_secret, payment_secret, sign_for_address } = request;
        let pssb = Bundle::deserialize(&pssb)?;
        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;
        let pssb = account.pssb_sign(&pssb, wallet_secret, payment_secret, sign_for_address.as_ref()).await?;

        Ok(AccountsPssbSignResponse { pssb: pssb.serialize()? })
    }

    async fn accounts_pssb_broadcast_call(
        self: Arc<Self>,
        request: AccountsPssbBroadcastRequest,
    ) -> Result<AccountsPssbBroadcastResponse> {
        let AccountsPssbBroadcastRequest { account_id, pssb } = request;
        let pssb = Bundle::deserialize(&pssb)?;
        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;
        // PSSB broadcast is now handled by the Account trait default implementation
        let transaction_ids = account.pssb_broadcast(&pssb).await?;
        Ok(AccountsPssbBroadcastResponse { transaction_ids })
    }

    async fn accounts_get_cells_call(self: Arc<Self>, request: AccountsGetCellsRequest) -> Result<AccountsGetCellsResponse> {
        let AccountsGetCellsRequest { account_id, addresses, min_amount_sau } = request;
        let guard = self.guard();
        let guard = guard.lock().await;
        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;
        let cells = account.get_cells(addresses, min_amount_sau).await?;
        Ok(AccountsGetCellsResponse { cells: cells.into_iter().map(|entry| entry.into()).collect::<Vec<CellEntryWrapper>>() })
    }

    async fn accounts_pssb_send_call(self: Arc<Self>, request: AccountsPssbSendRequest) -> Result<AccountsPssbSendResponse> {
        let AccountsPssbSendRequest { account_id, pssb, wallet_secret, payment_secret, sign_for_address } = request;
        let pssb = Bundle::deserialize(&pssb)?;
        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;
        let pssb = account.clone().pssb_sign(&pssb, wallet_secret, payment_secret, sign_for_address.as_ref()).await?;
        // PSSB broadcast is now handled by the Account trait default implementation
        let transaction_ids = account.pssb_broadcast(&pssb).await?;
        Ok(AccountsPssbSendResponse { transaction_ids })
    }

    async fn accounts_transfer_call(self: Arc<Self>, request: AccountsTransferRequest) -> Result<AccountsTransferResponse> {
        let AccountsTransferRequest {
            source_account_id,
            destination_account_id,
            wallet_secret,
            payment_secret,
            fee_rate,
            _priority_fee_sau,
            transfer_amount_sau,
        } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let source_account =
            self.get_account_by_id(&source_account_id, &guard).await?.ok_or(Error::AccountNotFound(source_account_id))?;

        let abortable = Abortable::new();
        let (generator_summary, transaction_ids) = source_account
            .transfer(
                destination_account_id,
                transfer_amount_sau,
                fee_rate,
                _priority_fee_sau.unwrap_or(Fees::SenderPays(0)),
                wallet_secret,
                payment_secret,
                &abortable,
                None,
                &guard,
            )
            .await?;

        Ok(AccountsTransferResponse { generator_summary, transaction_ids })
    }
    async fn accounts_commit_reveal_manual_call(
        self: Arc<Self>,
        request: AccountsCommitRevealManualRequest,
    ) -> Result<AccountsCommitRevealManualResponse> {
        let AccountsCommitRevealManualRequest {
            account_id,
            script_sig,
            start_destination,
            end_destination,
            wallet_secret,
            payment_secret,
            fee_rate,
            reveal_fee_sau,
            payload,
        } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;

        let abortable = Abortable::new();

        let bundle = account
            .clone()
            .commit_reveal_manual(
                start_destination,
                end_destination,
                script_sig,
                wallet_secret,
                payment_secret,
                fee_rate,
                reveal_fee_sau,
                payload,
                &abortable,
            )
            .await?;

        let transaction_ids = account.pssb_broadcast(&bundle).await?;
        Ok(AccountsCommitRevealManualResponse { transaction_ids })
    }

    async fn accounts_commit_reveal_call(
        self: Arc<Self>,
        request: AccountsCommitRevealRequest,
    ) -> Result<AccountsCommitRevealResponse> {
        let AccountsCommitRevealRequest {
            account_id,
            address_type,
            address_index,
            script_sig,
            commit_amount_sau,
            wallet_secret,
            payment_secret,
            fee_rate,
            reveal_fee_sau,
            payload,
        } = request;

        let guard = self.guard();
        let guard = guard.lock().await;

        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;

        let address = match address_type {
            CommitRevealAddressKind::Receive => {
                if account.account_kind() == KEYPAIR_ACCOUNT_KIND {
                    account.receive_address()?
                } else {
                    account.clone().as_derivation_capable()?.receive_address_at_index(address_index).await?
                }
            }
            CommitRevealAddressKind::Change => {
                if account.account_kind() == KEYPAIR_ACCOUNT_KIND {
                    account.change_address()?
                } else {
                    account.clone().as_derivation_capable()?.change_address_at_index(address_index).await?
                }
            }
        };

        let abortable = Abortable::new();

        let bundle = account
            .clone()
            .commit_reveal(
                address,
                script_sig,
                wallet_secret,
                payment_secret,
                commit_amount_sau,
                fee_rate,
                reveal_fee_sau,
                payload,
                &abortable,
            )
            .await?;

        let transaction_ids = account.pssb_broadcast(&bundle).await?;
        Ok(AccountsCommitRevealResponse { transaction_ids })
    }

    async fn accounts_estimate_call(self: Arc<Self>, request: AccountsEstimateRequest) -> Result<AccountsEstimateResponse> {
        let AccountsEstimateRequest { account_id, destination, _priority_fee_sau, payload, fee_rate } = request;

        let guard = self.guard();
        let guard = guard.lock().await;
        let account = self.get_account_by_id(&account_id, &guard).await?.ok_or(Error::AccountNotFound(account_id))?;

        // Abort currently running async estimate for the same account if present. The estimate
        // call can be invoked continuously by the client/UI. If the estimate call is
        // invoked more than once for the same account, the previous estimate call should
        // be aborted.  The [`Abortable`] is an [`AtomicBool`] that is periodically checked by the
        // [`Generator`], resulting in the [`Generator`] halting the estimation process if it
        // detects that the [`Abortable`] is set to `true`. This effectively halts the previously
        // spawned async task that will return [`Error::Aborted`].
        if let Some(abortable) = self.inner.estimation_abortables.lock().unwrap().get(&account_id) {
            abortable.abort();
        }

        let abortable = Abortable::new();
        self.inner.estimation_abortables.lock().unwrap().insert(account_id, abortable.clone());
        let result = account.estimate(destination, fee_rate, _priority_fee_sau, payload, &abortable).await;
        self.inner.estimation_abortables.lock().unwrap().remove(&account_id);

        Ok(AccountsEstimateResponse { generator_summary: result? })
    }

    async fn transactions_data_get_call(self: Arc<Self>, request: TransactionsDataGetRequest) -> Result<TransactionsDataGetResponse> {
        let TransactionsDataGetRequest { account_id, network_id, filter, start, end } = request;

        if start > end {
            return Err(Error::InvalidRange(start, end));
        }

        let binding = Binding::Account(account_id);
        let store = self.store().as_transaction_record_store()?;
        let TransactionRangeResult { transactions, total } =
            store.load_range(&binding, &network_id, filter, start as usize..end as usize).await?;

        Ok(TransactionsDataGetResponse { transactions, total, account_id, start })
    }

    async fn transactions_replace_note_call(
        self: Arc<Self>,
        request: TransactionsReplaceNoteRequest,
    ) -> Result<TransactionsReplaceNoteResponse> {
        let TransactionsReplaceNoteRequest { account_id, network_id, transaction_id, note } = request;

        self.store()
            .as_transaction_record_store()?
            .store_transaction_note(&Binding::Account(account_id), &network_id, transaction_id, note)
            .await?;

        Ok(TransactionsReplaceNoteResponse {})
    }

    async fn transactions_replace_metadata_call(
        self: Arc<Self>,
        request: TransactionsReplaceMetadataRequest,
    ) -> Result<TransactionsReplaceMetadataResponse> {
        let TransactionsReplaceMetadataRequest { account_id, network_id, transaction_id, metadata } = request;

        self.store()
            .as_transaction_record_store()?
            .store_transaction_metadata(&Binding::Account(account_id), &network_id, transaction_id, metadata)
            .await?;

        Ok(TransactionsReplaceMetadataResponse {})
    }

    async fn address_book_enumerate_call(
        self: Arc<Self>,
        _request: AddressBookEnumerateRequest,
    ) -> Result<AddressBookEnumerateResponse> {
        let entries = self.store().as_address_book_store()?.iter().await?.try_collect::<Vec<_>>().await?;
        Ok(AddressBookEnumerateResponse { entries })
    }

    async fn fee_rate_estimate_call(self: Arc<Self>, _request: FeeRateEstimateRequest) -> Result<FeeRateEstimateResponse> {
        let RpcFeeEstimate { priority_bucket, normal_buckets, low_buckets } = self.rpc_api().get_fee_estimate().await?;

        Ok(FeeRateEstimateResponse {
            priority: priority_bucket.into(),
            normal: normal_buckets.first().ok_or(Error::custom("missing normal feerate bucket"))?.into(),
            low: low_buckets.first().ok_or(Error::custom("missing normal feerate bucket"))?.into(),
        })
    }

    async fn fee_rate_poller_enable_call(self: Arc<Self>, request: FeeRatePollerEnableRequest) -> Result<FeeRatePollerEnableResponse> {
        let FeeRatePollerEnableRequest { interval_seconds } = request;
        self.cell_processor().start_fee_rate_poller(Duration::from_secs(interval_seconds)).await?;
        Ok(FeeRatePollerEnableResponse {})
    }

    async fn fee_rate_poller_disable_call(
        self: Arc<Self>,
        _request: FeeRatePollerDisableRequest,
    ) -> Result<FeeRatePollerDisableResponse> {
        self.cell_processor().stop_fee_rate_poller().await?;
        Ok(FeeRatePollerDisableResponse {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workflow_core::channel::unbounded;

    #[tokio::test]
    async fn register_and_unregister_notifications_tracks_channel_ids() {
        let wallet =
            Arc::new(super::super::Wallet::try_with_rpc(None, super::super::Wallet::resident_store().unwrap(), None).unwrap());
        let (_sender, receiver) = unbounded();

        let channel_id = wallet.clone().register_notifications(receiver).await.unwrap();
        assert!(wallet.inner.notification_channels.lock().unwrap().contains_key(&channel_id));

        wallet.clone().unregister_notifications(channel_id).await.unwrap();
        assert!(!wallet.inner.notification_channels.lock().unwrap().contains_key(&channel_id));
    }
}
