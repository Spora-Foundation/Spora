use super::client::ListeningClient;
use itertools::Itertools;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use secp256k1::Keypair;
use spora_addresses::Address;
use spora_consensus_client::{Transaction, TransactionInput, TransactionOutput};
use spora_consensus_core::{
    cell_diff::{CellCollection, CellDiff},
    constants::TX_VERSION,
    header::Header,
    sign::sign,
    tx::{
        pay_to_address_lock_script, CellEntry, CellOut, CellRef, CellTx, MutableTransaction, ScriptRef, SignableTransaction,
        TransactionId, TransactionOutpoint,
    },
};
use spora_core::info;
use spora_grpc_client::GrpcClient;
use spora_rpc_core::{api::rpc::RpcApi, BlockAddedNotification, Notification, RpcCellEntry, VirtualDaaScoreChangedNotification};
use std::{
    collections::{hash_map::Entry::Occupied, HashMap, HashSet},
    future::Future,
    sync::Arc,
    time::Duration,
};
use tokio::time::timeout;

pub(crate) const EXPAND_FACTOR: u64 = 1;
pub(crate) const CONTRACT_FACTOR: u64 = 1;

const fn estimated_mass(num_inputs: usize, num_outputs: u64) -> u64 {
    200 + 34 * num_outputs + 1000 * (num_inputs as u64)
}

pub const fn required_fee(num_inputs: usize, num_outputs: u64) -> u64 {
    const FEE_RATE: u64 = 10;
    FEE_RATE * estimated_mass(num_inputs, num_outputs)
}

/// Builds a TX DAG based on the initial cell set and on constant params
pub fn generate_tx_dag(
    mut cell_set: CellCollection,
    schnorr_key: Keypair,
    lock_script: ScriptRef,
    target_levels: usize,
    target_width: usize,
) -> Vec<Arc<CellTx>> {
    /*
    Algo:
       perform level by level:
           for target txs per level:
               select random cells (distinctly)
               create and sign a tx
               append tx to level txs
               append tx to cell diff
           apply level cell diff to the cell collection
    */

    let num_inputs = CONTRACT_FACTOR as usize;
    let num_outputs = EXPAND_FACTOR;

    let mut txs = Vec::with_capacity(target_levels * target_width);

    for i in 0..target_levels {
        let mut cell_diff = CellDiff::default();
        cell_set
            .iter()
            .take(num_inputs * target_width)
            .chunks(num_inputs)
            .into_iter()
            .map(|c| {
                c.into_iter()
                    .map(|(o, e)| {
                        // Convert OutPoint to CellRef
                        let cell_ref = CellRef::new(*o, 0); // since = 0 for test
                        (cell_ref, e.clone())
                    })
                    .unzip()
            })
            .collect::<Vec<(Vec<_>, Vec<_>)>>()
            .into_par_iter()
            .map(|(inputs, entries)| {
                let total_in = entries.iter().map(|e| e.capacity()).sum::<u64>();
                let total_out = total_in - required_fee(num_inputs, num_outputs);
                let outputs: Vec<CellOut> = (0..num_outputs)
                    .map(|_| CellOut { capacity: total_out / num_outputs, lock: lock_script.clone(), type_: None })
                    .collect_vec();
                let outputs_data: Vec<Vec<u8>> = (0..num_outputs).map(|_| vec![]).collect();
                let witnesses: Vec<Vec<u8>> = inputs.iter().map(|_| vec![]).collect();
                let unsigned_tx = CellTx::new(inputs, vec![], outputs, outputs_data, witnesses).expect("valid CellTx");
                sign(SignableTransaction::with_entries(unsigned_tx, entries), schnorr_key)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .for_each(|signed_tx| {
                cell_diff.add_transaction(&signed_tx.as_verifiable(), 0).unwrap();
                txs.push(Arc::new(signed_tx.tx));
            });
        cell_set.remove_collection(&cell_diff.remove);
        cell_set.add_collection(&cell_diff.add);

        if i % (target_levels / 10).max(1) == 0 {
            info!("Generated {} txs", txs.len());
        }
    }

    txs
}

/// Sanity test verifying that the generated TX DAG is valid, topologically ordered and has no double spends
pub fn verify_tx_dag(initial_cell_set: &CellCollection, txs: &[Arc<CellTx>]) {
    let mut prev_txs: HashMap<TransactionId, Arc<CellTx>> = HashMap::new();
    let mut used_outpoints = HashSet::with_capacity(txs.len() * 2);
    for tx in txs.iter() {
        for input in tx.inputs.iter() {
            assert!(used_outpoints.insert(input.out_point));
            if let Occupied(e) = prev_txs.entry(TransactionId::from_bytes(input.out_point.tx_hash)) {
                assert!(e.get().outputs.len() > input.out_point.index as usize);
            } else {
                assert!(initial_cell_set.contains_key(&input.out_point));
            }
        }
        assert!(prev_txs.insert(tx.id(), tx.clone()).is_none());
    }
}

pub async fn wait_for<Fut>(sleep_millis: u64, max_iterations: u64, success: impl Fn() -> Fut, panic_message: &'static str)
where
    Fut: Future<Output = bool>,
{
    let mut i: u64 = 0;
    loop {
        i += 1;
        tokio::time::sleep(Duration::from_millis(sleep_millis)).await;
        if success().await {
            break;
        } else if i >= max_iterations {
            panic!("{}", panic_message);
        }
    }
}

pub fn generate_tx(
    schnorr_key: Keypair,
    cells: &[(TransactionOutpoint, CellEntry)],
    amount: u64,
    num_outputs: u64,
    address: &Address,
) -> CellTx {
    let total_in = cells.iter().map(|x| x.1.capacity()).sum::<u64>();
    assert!(amount <= total_in - required_fee(cells.len(), num_outputs));
    let lock_script = pay_to_address_lock_script(address);
    let inputs: Vec<CellRef> = cells
        .iter()
        .map(|(op, _)| CellRef::new(*op, 0)) // since = 0 for test
        .collect_vec();

    let outputs: Vec<CellOut> =
        (0..num_outputs).map(|_| CellOut { capacity: amount / num_outputs, lock: lock_script.clone(), type_: None }).collect_vec();
    let outputs_data: Vec<Vec<u8>> = (0..num_outputs).map(|_| vec![]).collect();
    let witnesses: Vec<Vec<u8>> = inputs.iter().map(|_| vec![]).collect();
    let unsigned_tx = CellTx::new(inputs, vec![], outputs, outputs_data, witnesses).expect("valid CellTx");
    let signed_tx =
        sign(MutableTransaction::with_entries(unsigned_tx, cells.iter().map(|(_, entry)| entry.clone()).collect_vec()), schnorr_key);
    signed_tx.tx
}

pub async fn fetch_spendable_cells(
    client: &GrpcClient,
    address: Address,
    coinbase_maturity: u64,
) -> Vec<(TransactionOutpoint, CellEntry)> {
    let resp = client.get_cells_by_addresses(vec![address.clone()]).await.unwrap();
    let virtual_daa_score = client.get_server_info().await.unwrap().virtual_daa_score;
    let mut cells = Vec::with_capacity(resp.len());
    for resp_entry in
        resp.into_iter().filter(|resp_entry| is_cell_spendable(&resp_entry.cell_entry, virtual_daa_score, coinbase_maturity))
    {
        assert!(resp_entry.address.is_some());
        assert_eq!(*resp_entry.address.as_ref().unwrap(), address);
        cells.push((TransactionOutpoint::from(resp_entry.outpoint), CellEntry::from(resp_entry.cell_entry)));
    }
    cells.sort_by(|a, b| b.1.capacity().cmp(&a.1.capacity()));
    cells
}

pub fn is_cell_spendable(entry: &RpcCellEntry, virtual_daa_score: u64, coinbase_maturity: u64) -> bool {
    let needed_confirmations = if !entry.is_coinbase { 10 } else { coinbase_maturity };
    entry.block_daa_score + needed_confirmations <= virtual_daa_score
}

pub async fn mine_block(pay_address: Address, submitting_client: &GrpcClient, listening_clients: &[ListeningClient]) {
    // Discard all unreceived block added notifications in each listening client
    listening_clients.iter().for_each(|x| x.block_added_listener().unwrap().drain());

    // Mine a block
    let template = submitting_client.get_block_template(pay_address.clone(), vec![]).await.unwrap();
    let header: Header = (&template.block.header).into();
    let block_hash = header.hash;
    submitting_client.submit_block(template.block, false).await.unwrap();

    // Wait for each listening client to get notified the submitted block was added to the DAG
    for client in listening_clients.iter() {
        let block_daa_score: u64 = match timeout(Duration::from_millis(500), client.block_added_listener().unwrap().receiver.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Notification::BlockAdded(BlockAddedNotification { block }) => {
                assert_eq!(block.header.hash, block_hash);
                block.header.daa_score
            }
            _ => panic!("wrong notification type"),
        };
        match timeout(Duration::from_millis(500), client.virtual_daa_score_changed_listener().unwrap().receiver.recv())
            .await
            .unwrap()
            .unwrap()
        {
            Notification::VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification { virtual_daa_score }) => {
                assert_eq!(virtual_daa_score, block_daa_score + 1);
            }
            _ => panic!("wrong notification type"),
        }
    }
}
