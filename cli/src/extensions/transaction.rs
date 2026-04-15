use crate::imports::*;
use spora_consensus_core::tx::TransactionId;
use spora_wallet_core::storage::Binding;
use spora_wallet_core::storage::{TransactionData, TransactionKind, TransactionRecord};
use spora_wallet_core::wallet::WalletGuard;
use workflow_log::style;

pub trait TransactionTypeExtension {
    fn style(&self, s: &str) -> String;
    fn style_with_sign(&self, s: &str, history: bool) -> String;
}

impl TransactionTypeExtension for TransactionKind {
    fn style(&self, s: &str) -> String {
        match self {
            TransactionKind::Incoming => style(s).green().to_string(),
            TransactionKind::Outgoing => style(s).red().to_string(),
            TransactionKind::External => style(s).red().to_string(),
            TransactionKind::Batch => style(s).dim().to_string(),
            TransactionKind::Reorg => style(s).dim().to_string(),
            TransactionKind::Stasis => style(s).dim().to_string(),
            TransactionKind::TransferIncoming => style(s).green().to_string(),
            TransactionKind::TransferOutgoing => style(s).red().to_string(),
            TransactionKind::Change => style(s).dim().to_string(),
        }
    }

    fn style_with_sign(&self, s: &str, history: bool) -> String {
        match self {
            TransactionKind::Incoming => style("+".to_string() + s).green().to_string(),
            TransactionKind::TransferIncoming => style("+".to_string() + s).green().to_string(),
            TransactionKind::Outgoing => style("-".to_string() + s).red().to_string(),
            TransactionKind::TransferOutgoing => style("-".to_string() + s).red().to_string(),
            TransactionKind::External => style("-".to_string() + s).red().to_string(),
            TransactionKind::Batch => style("".to_string() + s).dim().to_string(),
            TransactionKind::Reorg => {
                if history {
                    style("".to_string() + s).dim()
                } else {
                    style("-".to_string() + s).red()
                }
            }
            .to_string(),
            TransactionKind::Stasis => style("".to_string() + s).dim().to_string(),
            _ => style(s).dim().to_string(),
        }
    }
}

#[async_trait]
pub trait TransactionExtension {
    async fn format_transaction(&self, wallet: &Arc<Wallet>, include_cells: bool, guard: &WalletGuard) -> Vec<String>;
    async fn format_transaction_with_state(
        &self,
        wallet: &Arc<Wallet>,
        state: Option<&str>,
        include_cells: bool,
        guard: &WalletGuard,
    ) -> Vec<String>;
    async fn format_transaction_with_args(
        &self,
        wallet: &Arc<Wallet>,
        state: Option<&str>,
        current_daa_score: Option<u64>,
        include_cells: bool,
        history: bool,
        account: Option<Arc<dyn Account>>,
        guard: &WalletGuard,
    ) -> Vec<String>;
}

#[async_trait]
impl TransactionExtension for TransactionRecord {
    async fn format_transaction(&self, wallet: &Arc<Wallet>, include_cells: bool, guard: &WalletGuard) -> Vec<String> {
        self.format_transaction_with_args(wallet, None, None, include_cells, false, None, guard).await
    }

    async fn format_transaction_with_state(
        &self,
        wallet: &Arc<Wallet>,
        state: Option<&str>,
        include_cells: bool,
        guard: &WalletGuard,
    ) -> Vec<String> {
        self.format_transaction_with_args(wallet, state, None, include_cells, false, None, guard).await
    }

    async fn format_transaction_with_args(
        &self,
        wallet: &Arc<Wallet>,
        state: Option<&str>,
        current_daa_score: Option<u64>,
        include_cells: bool,
        history: bool,
        account: Option<Arc<dyn Account>>,
        guard: &WalletGuard,
    ) -> Vec<String> {
        let TransactionRecord { id, binding, block_daa_score, transaction_data, .. } = self;

        let name = match binding {
            Binding::Custom(id) => style(id.short()).cyan(),
            Binding::Account(account_id) => {
                let account = if let Some(account) = account {
                    Some(account)
                } else {
                    wallet.get_account_by_id(account_id, guard).await.ok().flatten()
                };

                if let Some(account) = account {
                    style(account.name_with_id()).cyan()
                } else {
                    style(account_id.short() + " ??").magenta()
                }
            }
        };

        let transaction_type = transaction_data.kind();
        let kind = transaction_type.style(&transaction_type.to_string());

        let maturity = current_daa_score.map(|score| self.maturity(score).to_string()).unwrap_or_default();

        let block_daa_score = block_daa_score.separated_string();
        let state = state.unwrap_or(&maturity);
        let mut lines = vec![format!("{name} {id} @{block_daa_score} DAA - {kind} {state}")];

        let suffix = spora_suffix(&self.network_id.network_type);

        match transaction_data {
            TransactionData::Reorg { cell_entries, aggregate_input_value }
            | TransactionData::Stasis { cell_entries, aggregate_input_value }
            | TransactionData::Incoming { cell_entries, aggregate_input_value }
            | TransactionData::External { cell_entries, aggregate_input_value }
            | TransactionData::Change { cell_entries, aggregate_input_value, .. } => {
                let aggregate_input_value =
                    transaction_type.style_with_sign(sau_to_spora_string(*aggregate_input_value).as_str(), history);
                lines.push(format!("{:>4}Cells: {}  Total: {}", "", cell_entries.len(), aggregate_input_value));
                if include_cells {
                    for cell_entry in cell_entries {
                        let address =
                            style(cell_entry.address.as_ref().map(|addr| addr.to_string()).unwrap_or_else(|| "n/a".to_string()))
                                .blue();
                        let index = cell_entry.index;
                        let is_coinbase = if cell_entry.is_coinbase {
                            style(format!("coinbase cell [{index}]")).dim()
                        } else {
                            style(format!("standard cell [{index}]")).dim()
                        };
                        let amount = transaction_type.style_with_sign(sau_to_spora_string(cell_entry.amount).as_str(), history);

                        lines.push(format!("{:>4}{address}", ""));
                        lines.push(format!("{:>4}{amount} {suffix} {is_coinbase}", ""));
                    }
                }
            }
            TransactionData::Outgoing { fees, aggregate_input_value, transaction, payment_value, change_value, .. }
            | TransactionData::Batch { fees, aggregate_input_value, transaction, payment_value, change_value, .. }
            | TransactionData::TransferIncoming { fees, aggregate_input_value, transaction, payment_value, change_value, .. }
            | TransactionData::TransferOutgoing { fees, aggregate_input_value, transaction, payment_value, change_value, .. } => {
                if let Some(payment_value) = payment_value {
                    lines.push(format!(
                        "{:>4}Payment: {}  Used: {}  Fees: {}  Change: {}  Cells: [{}↠{}]",
                        "",
                        style(sau_to_spora_string(*payment_value)).red(),
                        style(sau_to_spora_string(*aggregate_input_value)).blue(),
                        style(sau_to_spora_string(*fees)).red(),
                        style(sau_to_spora_string(*change_value)).green(),
                        transaction.inputs.len(),
                        transaction.outputs.len(),
                    ));
                } else {
                    lines.push(format!(
                        "{:>4}Sweep: {}  Fees: {}  Change: {}  Cells: [{}↠{}]",
                        "",
                        style(sau_to_spora_string(*aggregate_input_value)).blue(),
                        style(sau_to_spora_string(*fees)).red(),
                        style(sau_to_spora_string(*change_value)).green(),
                        transaction.inputs.len(),
                        transaction.outputs.len(),
                    ));
                }

                if include_cells {
                    for input in transaction.inputs.iter() {
                        let out_point = &input.previous_output;
                        let index = out_point.index;
                        let transaction_id = TransactionId::from_bytes(out_point.tx_hash);
                        let since = input.since;

                        lines.push(format!("{:>4}{since:>2}: {transaction_id}:{index}", ""));
                    }
                }
            }
        }

        lines
    }
}
