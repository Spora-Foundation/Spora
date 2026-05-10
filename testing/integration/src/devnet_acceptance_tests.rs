#![cfg(test)]
#![cfg(all(feature = "integration-tests", feature = "devnet-prealloc"))]

use crate::common::{
    args::ArgsBuilder,
    cellscript_contracts::{
        compile_all_spora_example_contracts, compile_fixed_output_spora_contract, compile_noop_spora_lock_contract,
        compile_parameterized_amount_spora_contract, CompiledCellScriptActionArtifact,
    },
    daemon::Daemon,
    devnet_bootstrap::{generate_devnet_bootstrap, DEFAULT_PREALLOC_AMOUNT_SAU},
    utils::{fetch_spendable_cells, generate_signed_cell_tx, generate_tx, generate_tx_to_outputs, required_fee, wait_for},
};
use serde::Serialize;
use spora_addresses::Address;
use spora_alloc::init_allocator_with_default_settings;
use spora_bip32::WordCount;
use spora_consensus::params::DEVNET_PARAMS;
use spora_consensus_core::mass::{calc_storage_mass, CellMass};
use spora_consensus_core::network::{NetworkId, NetworkType};
use spora_consensus_core::tx::{CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script, TransactionOutpoint};
use spora_exec::celltx::{
    decode_cellscript_scheduler_witness, CellScriptSchedulerWitness, CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP,
    CELLSCRIPT_SCHEDULER_SOURCE_INPUT, CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT,
};
use spora_exec::scripts::{always_success_code_hash, ALWAYS_SUCCESS_SCRIPT};
use spora_rpc_core::api::rpc::RpcApi;
use std::collections::{HashSet, VecDeque};

const DEVNET_ACCEPTANCE_BLOCK_MAX_MASS: u64 = 100_000_000;
const SPORA_STANDARD_RELAY_MAX_TX_MASS: u64 = 500_000;
const BASE_REPORT_ENV: &str = "DEVNET_ACCEPTANCE_BASE_REPORT_JSON";
const STANDARD_MASS_POLICY_ENV: &str = "DEVNET_ACCEPTANCE_STANDARD_MASS_POLICY";

/// Real devnet acceptance base:
/// bootstrap wallet -> prealloc cells -> mine confirmations -> signed transfer -> mempool -> template -> block -> cellindex
/// -> deploy script code cell -> spend VM-locked cell with cell dep -> deploy/spend a compiled CellScript ELF contract
/// through a real code cell dependency -> deploy every bundled CellScript example. The default mode
/// uses the explicit non-standard relay profile for broad development coverage; production mode sets
/// `DEVNET_ACCEPTANCE_STANDARD_MASS_POLICY=1` and uses the default standard mass policy.
///
/// `cargo test -p spora-testing-integration --lib --features "integration-tests devnet-prealloc vm" -- devnet_acceptance_tests::devnet_acceptance_base --exact --nocapture --test-threads=1`
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn devnet_acceptance_base() {
    init_allocator_with_default_settings();
    spora_core::log::try_init_logger("INFO");

    let standard_mass_policy = std::env::var(STANDARD_MASS_POLICY_ENV).ok().as_deref() == Some("1");
    let prealloc_cells = if standard_mass_policy { 64 } else { 28 };
    let prealloc_amount_spora = if standard_mass_policy { 10_000 } else { 500 };
    let bootstrap = generate_devnet_bootstrap(
        NetworkType::Devnet,
        "acceptance-base",
        WordCount::Words12,
        prealloc_cells,
        prealloc_amount_spora * spora_consensus_core::constants::SAU_PER_SPORA,
    )
    .expect("devnet bootstrap wallet must be generated");
    assert_eq!(bootstrap.manifest.node.prealloc_amount_sau, prealloc_amount_spora * spora_consensus_core::constants::SAU_PER_SPORA);
    assert!(bootstrap.manifest.node.prealloc_amount_sau >= DEFAULT_PREALLOC_AMOUNT_SAU);

    let prealloc_address = bootstrap.address.clone();
    let prealloc_schnorr_key = bootstrap.schnorr_keypair();
    let recipient_address = Address::new_std_single(NetworkType::Devnet.into(), &[7; 32]).expect("recipient address must be valid");
    let miner_address = Address::new_std_single(NetworkType::Devnet.into(), &[9; 32]).expect("miner address must be valid");
    let block_max_mass = if standard_mass_policy { DEVNET_PARAMS.max_block_mass } else { DEVNET_ACCEPTANCE_BLOCK_MAX_MASS };

    let mut args_builder = ArgsBuilder::devnet(prealloc_cells, prealloc_amount_spora)
        .prealloc_address(prealloc_address.clone())
        .cellindex(true)
        .resumable_virtual_state_step_cycles(10_000);
    if !standard_mass_policy {
        args_builder = args_builder.relay_non_standard(true).block_max_mass(DEVNET_ACCEPTANCE_BLOCK_MAX_MASS);
    }
    let args = args_builder.build();

    let mut sporad = Daemon::new_random_with_args(args, 10);
    let rpc_client = sporad.start().await;

    let info = rpc_client.get_info().await.expect("get_info must succeed");
    assert!(info.is_cell_indexed, "devnet acceptance requires cellindex");
    assert_eq!(info.mempool_size, 0);

    let server_info = rpc_client.get_server_info().await.expect("get_server_info must succeed");
    assert!(server_info.has_cell_index, "server must advertise cellindex");
    assert_eq!(server_info.network_id, NetworkId::new(NetworkType::Devnet));
    let mut base_report = BaseReport {
        profile: "base",
        network_id: "devnet",
        relaxed_mass_policy: RelaxedMassPolicyReport {
            mode: if standard_mass_policy { "standard" } else { "relaxed" },
            relay_non_standard: !standard_mass_policy,
            block_max_mass,
            applies_to_all_networks_when_explicitly_enabled: true,
            standard_policy_preserved_by_default: true,
        },
        prealloc_cells,
        prealloc_amount_sau: bootstrap.manifest.node.prealloc_amount_sau,
        signed_transfer_confirmed: false,
        multi_input_multi_output_confirmed: false,
        parent_child_mempool_confirmed: false,
        scheduler_tamper_rejected: false,
        always_success_vm_spend_confirmed: false,
        noop_cellscript_spend_confirmed: false,
        cellscript_schema_output_spend_confirmed: false,
        cellscript_parameterized_amount_spend_confirmed: false,
        bundled_examples: Vec::new(),
        production_gate: SporaProductionGateReport::default(),
    };

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move { client.get_cells_by_addresses(vec![address]).await.unwrap().len() == prealloc_cells as usize }
            }
        },
        "preallocated cells were not indexed",
    )
    .await;

    for expected_daa in 1..=10 {
        let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client.submit_block(template.block, false).await.unwrap();
        wait_for(
            50,
            40,
            {
                let client = rpc_client.clone();
                move || {
                    let client = client.clone();
                    async move { client.get_server_info().await.unwrap().virtual_daa_score >= expected_daa }
                }
            },
            "virtual DAA score did not advance while preparing spendable prealloc cells",
        )
        .await;
    }

    let spendable_cells = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity()).await;
    assert_eq!(spendable_cells.len(), prealloc_cells as usize, "all preallocated cells should be spendable after 10 blocks");

    let tx_amount = spendable_cells[0].1.capacity() / 2;
    let transaction = generate_tx(prealloc_schnorr_key, &spendable_cells[..1], tx_amount, 1, &recipient_address);
    let transaction_id = spora_hashes::Hash::from_bytes(transaction.id());
    rpc_client.submit_transaction((&transaction).into(), false).await.unwrap();

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            move || {
                let client = client.clone();
                async move { client.get_mempool_entry(transaction_id, false, false).await.is_ok() }
            }
        },
        "signed prealloc transaction was not added to the mempool",
    )
    .await;

    let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| spora_consensus_core::tx::CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == transaction_id),
        "expected block template to include the submitted prealloc transaction"
    );
    rpc_client.submit_block(template.block, false).await.unwrap();

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = recipient_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .any(|cell| cell.outpoint.transaction_id == transaction_id)
                }
            }
        },
        "recipient cell from the prealloc transaction was not indexed after block acceptance",
    )
    .await;

    let recipient_cells = rpc_client.get_cells_by_addresses(vec![recipient_address]).await.unwrap();
    assert!(recipient_cells.iter().any(|cell| cell.outpoint.transaction_id == transaction_id));
    base_report.signed_transfer_confirmed = true;

    let recipient_a = Address::new_std_single(NetworkType::Devnet.into(), &[11; 32]).expect("recipient A address must be valid");
    let recipient_b = Address::new_std_single(NetworkType::Devnet.into(), &[12; 32]).expect("recipient B address must be valid");
    let conflict_recipient =
        Address::new_std_single(NetworkType::Devnet.into(), &[13; 32]).expect("conflict recipient address must be valid");
    let remaining_prealloc = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity()).await;
    assert!(remaining_prealloc.len() >= 2, "multi-input acceptance needs at least two remaining prealloc cells");

    let multi_inputs = &remaining_prealloc[..2];
    let multi_input_capacity = multi_inputs.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let recipient_a_amount = multi_input_capacity / 5;
    let recipient_b_amount = multi_input_capacity / 5;
    let multi_fee = required_fee(multi_inputs.len(), 3);
    let change_amount = multi_input_capacity
        .checked_sub(recipient_a_amount)
        .and_then(|value| value.checked_sub(recipient_b_amount))
        .and_then(|value| value.checked_sub(multi_fee))
        .expect("multi-input transaction must leave change");
    let multi_tx = generate_tx_to_outputs(
        prealloc_schnorr_key,
        multi_inputs,
        vec![
            (recipient_a.clone(), recipient_a_amount),
            (recipient_b.clone(), recipient_b_amount),
            (prealloc_address.clone(), change_amount),
        ],
    );
    let multi_tx_id = spora_hashes::Hash::from_bytes(multi_tx.id());
    rpc_client.submit_transaction((&multi_tx).into(), false).await.unwrap();

    let conflict_amount = multi_input_capacity / 10;
    let conflict_fee = required_fee(multi_inputs.len(), 2);
    let conflict_change = multi_input_capacity
        .checked_sub(conflict_amount)
        .and_then(|value| value.checked_sub(conflict_fee))
        .expect("conflict transaction must leave change");
    let conflict_tx = generate_tx_to_outputs(
        prealloc_schnorr_key,
        multi_inputs,
        vec![(conflict_recipient, conflict_amount), (prealloc_address.clone(), conflict_change)],
    );
    assert!(
        rpc_client.submit_transaction((&conflict_tx).into(), false).await.is_err(),
        "mempool must reject a conflicting transaction spending the same prealloc inputs"
    );

    let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| spora_consensus_core::tx::CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == multi_tx_id),
        "expected block template to include the submitted multi-input transaction"
    );
    rpc_client.submit_block(template.block, false).await.unwrap();

    for address in [recipient_a, recipient_b] {
        wait_for(
            50,
            40,
            {
                let client = rpc_client.clone();
                let address = address.clone();
                move || {
                    let client = client.clone();
                    let address = address.clone();
                    async move {
                        client
                            .get_cells_by_addresses(vec![address])
                            .await
                            .unwrap()
                            .iter()
                            .any(|cell| cell.outpoint.transaction_id == multi_tx_id)
                    }
                }
            },
            "recipient cell from the multi-input transaction was not indexed after block acceptance",
        )
        .await;
    }
    base_report.multi_input_multi_output_confirmed = true;

    let remaining_prealloc = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity()).await;
    assert!(!remaining_prealloc.is_empty(), "script deployment acceptance needs one remaining spendable prealloc cell");

    let deploy_input = &remaining_prealloc[..1];
    let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let code_cell_capacity = deploy_input_capacity / 4;
    let deploy_fee = required_fee(deploy_input.len(), 2).saturating_add(100_000);
    let vm_locked_capacity = deploy_input_capacity
        .checked_sub(code_cell_capacity)
        .and_then(|value| value.checked_sub(deploy_fee))
        .expect("script deployment transaction must leave capacity for the VM-locked cell");
    let always_success_lock = Script::new(always_success_code_hash(), 0, vec![]);
    let deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &deploy_input,
        vec![],
        vec![
            CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(&prealloc_address), type_: None },
            CellOutput { capacity: vm_locked_capacity, lock: always_success_lock.clone(), type_: None },
        ],
        vec![ALWAYS_SUCCESS_SCRIPT.to_vec(), vec![]],
    );
    let deploy_tx_id = spora_hashes::Hash::from_bytes(deploy_tx.id());
    let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);
    let vm_locked_outpoint = TransactionOutpoint::new(deploy_tx.id(), 1);

    rpc_client.submit_transaction((&deploy_tx).into(), false).await.unwrap();
    let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == deploy_tx_id),
        "expected block template to include the script deployment transaction"
    );
    rpc_client.submit_block(template.block, false).await.unwrap();

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .filter(|cell| cell.outpoint.transaction_id == deploy_tx_id)
                        .any(|cell| cell.outpoint.index == 0)
                }
            }
        },
        "always-success script code cell was not indexed after deployment block acceptance",
    )
    .await;

    let remaining_prealloc = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity()).await;
    assert!(!remaining_prealloc.is_empty(), "parent/child mempool acceptance needs one remaining prealloc cell");

    let parent_input = &remaining_prealloc[..1];
    let parent_input_capacity = parent_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let parent_child_seed_amount = parent_input_capacity / 3;
    let parent_fee = required_fee(parent_input.len(), 2);
    let parent_change = parent_input_capacity
        .checked_sub(parent_child_seed_amount)
        .and_then(|value| value.checked_sub(parent_fee))
        .expect("parent transaction must leave change");
    let parent_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &parent_input,
        vec![],
        vec![
            CellOutput { capacity: parent_child_seed_amount, lock: always_success_lock.clone(), type_: None },
            CellOutput { capacity: parent_change, lock: pay_to_acceptance_owner(&prealloc_address), type_: None },
        ],
        vec![vec![], vec![]],
    );
    let parent_tx_id = spora_hashes::Hash::from_bytes(parent_tx.id());
    let parent_child_outpoint = TransactionOutpoint::new(parent_tx.id(), 0);
    let child_recipient =
        Address::new_std_single(NetworkType::Devnet.into(), &[16; 32]).expect("parent/child recipient address must be valid");
    let child_output_capacity = parent_child_seed_amount.checked_sub(required_fee(1, 1)).expect("child transaction must leave fee");
    let child_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(parent_child_outpoint.tx_hash, parent_child_outpoint.index), 0)],
        vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
        vec![CellOutput { capacity: child_output_capacity, lock: pay_to_acceptance_owner(&child_recipient), type_: None }],
        vec![vec![]],
        vec![vec![]],
    )
    .expect("valid child transaction spending a mempool parent output");
    let child_tx_id = spora_hashes::Hash::from_bytes(child_tx.id());

    rpc_client.submit_transaction((&parent_tx).into(), false).await.unwrap();
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            move || {
                let client = client.clone();
                async move { client.get_mempool_entry(parent_tx_id, false, false).await.is_ok() }
            }
        },
        "parent transaction was not added to the mempool",
    )
    .await;
    rpc_client.submit_transaction((&child_tx).into(), false).await.unwrap();
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            move || {
                let client = client.clone();
                async move { client.get_mempool_entry(child_tx_id, false, false).await.is_ok() }
            }
        },
        "child transaction spending a mempool parent output was not added to the mempool",
    )
    .await;
    let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    let template_txs =
        template.block.transactions.iter().skip(1).filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok()).collect::<Vec<_>>();
    let parent_pos = template_txs
        .iter()
        .position(|tx| spora_hashes::Hash::from_bytes(tx.id()) == parent_tx_id)
        .expect("block template must include parent transaction");
    let child_pos = template_txs.iter().position(|tx| spora_hashes::Hash::from_bytes(tx.id()) == child_tx_id);
    if let Some(child_pos) = child_pos {
        assert!(parent_pos < child_pos, "block template must keep parent before child");
    }
    rpc_client.submit_block(template.block, false).await.unwrap();

    if child_pos.is_none() {
        wait_for(
            50,
            40,
            {
                let client = rpc_client.clone();
                move || {
                    let client = client.clone();
                    async move { client.get_mempool_entry(child_tx_id, false, false).await.is_ok() }
                }
            },
            "child transaction should remain in mempool after parent acceptance",
        )
        .await;
        let child_template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        assert!(
            child_template
                .block
                .transactions
                .iter()
                .skip(1)
                .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
                .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == child_tx_id),
            "next block template must promote the child transaction after parent acceptance"
        );
        rpc_client.submit_block(child_template.block, false).await.unwrap();
    }

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = child_recipient.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .any(|cell| cell.outpoint.transaction_id == child_tx_id)
                }
            }
        },
        "child output from the parent/child mempool chain was not indexed after block acceptance",
    )
    .await;
    base_report.parent_child_mempool_confirmed = true;

    let remaining_prealloc = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity()).await;
    let required_probe_cells = if standard_mass_policy { 3 } else { 4 };
    assert!(
        remaining_prealloc.len() >= required_probe_cells,
        "CellScript deployment acceptance needs {required_probe_cells} remaining spendable prealloc cells"
    );

    let cellscript_contract = compile_noop_spora_lock_contract();
    let cellscript_code_deploy_input = &remaining_prealloc[..1];
    let cellscript_code_deploy_input_capacity = cellscript_code_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let cellscript_code_cell_capacity = cellscript_code_deploy_input_capacity
        .checked_sub(required_fee(cellscript_code_deploy_input.len(), 1).saturating_add(100_000))
        .expect("CellScript code deployment transaction must leave capacity for the code cell");
    let cellscript_lock = Script::new(cellscript_contract.code_hash, 0, vec![]);
    let cellscript_code_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &cellscript_code_deploy_input,
        vec![],
        vec![CellOutput { capacity: cellscript_code_cell_capacity, lock: pay_to_acceptance_owner(&prealloc_address), type_: None }],
        vec![cellscript_contract.artifact_bytes.clone()],
    );
    let cellscript_code_deploy_tx_id = spora_hashes::Hash::from_bytes(cellscript_code_deploy_tx.id());
    let cellscript_code_outpoint = TransactionOutpoint::new(cellscript_code_deploy_tx.id(), 0);

    let cellscript_lock_create_input = &remaining_prealloc[1..2];
    let cellscript_lock_create_input_capacity = cellscript_lock_create_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let cellscript_locked_capacity = cellscript_lock_create_input_capacity
        .checked_sub(required_fee(cellscript_lock_create_input.len(), 1).saturating_add(100_000))
        .expect("CellScript lock creation transaction must leave capacity for the VM-locked cell");
    let cellscript_lock_create_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &cellscript_lock_create_input,
        vec![],
        vec![CellOutput { capacity: cellscript_locked_capacity, lock: cellscript_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let cellscript_lock_create_tx_id = spora_hashes::Hash::from_bytes(cellscript_lock_create_tx.id());
    let cellscript_locked_outpoint = TransactionOutpoint::new(cellscript_lock_create_tx.id(), 0);

    let fixed_output_contract = compile_fixed_output_spora_contract();
    let fixed_output_deploy_input = &remaining_prealloc[2..3];
    let fixed_output_deploy_input_capacity = fixed_output_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let fixed_output_code_cell_capacity = fixed_output_deploy_input_capacity / 2;
    let fixed_output_locked_capacity = fixed_output_deploy_input_capacity
        .checked_sub(fixed_output_code_cell_capacity)
        .and_then(|value| value.checked_sub(required_fee(fixed_output_deploy_input.len(), 2).saturating_add(100_000)))
        .expect("CellScript fixed-output deployment transaction must leave capacity for the locked probe cell");
    let fixed_output_lock = Script::new(fixed_output_contract.code_hash, 0, vec![]);
    let fixed_output_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &fixed_output_deploy_input,
        vec![],
        vec![
            CellOutput { capacity: fixed_output_code_cell_capacity, lock: pay_to_acceptance_owner(&prealloc_address), type_: None },
            CellOutput { capacity: fixed_output_locked_capacity, lock: fixed_output_lock, type_: None },
        ],
        vec![fixed_output_contract.artifact_bytes.clone(), vec![]],
    );
    let fixed_output_deploy_tx_id = spora_hashes::Hash::from_bytes(fixed_output_deploy_tx.id());
    let fixed_output_code_outpoint = TransactionOutpoint::new(fixed_output_deploy_tx.id(), 0);
    let fixed_output_locked_outpoint = TransactionOutpoint::new(fixed_output_deploy_tx.id(), 1);

    let parameterized_amount_probe = if standard_mass_policy {
        None
    } else {
        let parameterized_amount_contract = compile_parameterized_amount_spora_contract();
        let parameterized_amount_deploy_input = &remaining_prealloc[3..4];
        let parameterized_amount_deploy_input_capacity =
            parameterized_amount_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
        let parameterized_amount_code_cell_capacity = parameterized_amount_deploy_input_capacity / 2;
        let parameterized_amount_locked_capacity = parameterized_amount_deploy_input_capacity
            .checked_sub(parameterized_amount_code_cell_capacity)
            .and_then(|value| value.checked_sub(required_fee(parameterized_amount_deploy_input.len(), 2).saturating_add(100_000)))
            .expect("CellScript parameterized amount deployment transaction must leave capacity for the locked probe cell");
        let parameterized_amount_lock = Script::new(parameterized_amount_contract.code_hash, 0, vec![]);
        let parameterized_amount_deploy_tx = generate_signed_cell_tx(
            prealloc_schnorr_key,
            &parameterized_amount_deploy_input,
            vec![],
            vec![
                CellOutput {
                    capacity: parameterized_amount_code_cell_capacity,
                    lock: pay_to_acceptance_owner(&prealloc_address),
                    type_: None,
                },
                CellOutput { capacity: parameterized_amount_locked_capacity, lock: parameterized_amount_lock, type_: None },
            ],
            vec![parameterized_amount_contract.artifact_bytes.clone(), vec![]],
        );
        Some((
            parameterized_amount_contract,
            parameterized_amount_locked_capacity,
            spora_hashes::Hash::from_bytes(parameterized_amount_deploy_tx.id()),
            TransactionOutpoint::new(parameterized_amount_deploy_tx.id(), 0),
            TransactionOutpoint::new(parameterized_amount_deploy_tx.id(), 1),
            parameterized_amount_deploy_tx,
        ))
    };

    rpc_client.submit_transaction((&cellscript_code_deploy_tx).into(), false).await.unwrap();
    rpc_client.submit_transaction((&cellscript_lock_create_tx).into(), false).await.unwrap();
    rpc_client.submit_transaction((&fixed_output_deploy_tx).into(), false).await.unwrap();
    if let Some((_, _, _, _, _, parameterized_amount_deploy_tx)) = &parameterized_amount_probe {
        rpc_client.submit_transaction((parameterized_amount_deploy_tx).into(), false).await.unwrap();
    }
    let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == cellscript_code_deploy_tx_id),
        "expected block template to include the CellScript code deployment transaction"
    );
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == cellscript_lock_create_tx_id),
        "expected block template to include the CellScript lock creation transaction"
    );
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == fixed_output_deploy_tx_id),
        "expected block template to include the CellScript fixed-output deployment transaction"
    );
    if let Some((_, _, parameterized_amount_deploy_tx_id, _, _, _)) = &parameterized_amount_probe {
        assert!(
            template
                .block
                .transactions
                .iter()
                .skip(1)
                .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
                .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == *parameterized_amount_deploy_tx_id),
            "expected block template to include the CellScript parameterized amount deployment transaction"
        );
    }
    rpc_client.submit_block(template.block, false).await.unwrap();

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .filter(|cell| cell.outpoint.transaction_id == cellscript_code_deploy_tx_id)
                        .any(|cell| cell.outpoint.index == 0)
                }
            }
        },
        "CellScript script code cell was not indexed after deployment block acceptance",
    )
    .await;

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .filter(|cell| cell.outpoint.transaction_id == fixed_output_deploy_tx_id)
                        .any(|cell| cell.outpoint.index == 0)
                }
            }
        },
        "CellScript fixed-output script code cell was not indexed after deployment block acceptance",
    )
    .await;

    if let Some((_, _, parameterized_amount_deploy_tx_id, _, _, _)) = &parameterized_amount_probe {
        wait_for(
            50,
            40,
            {
                let client = rpc_client.clone();
                let address = prealloc_address.clone();
                let tx_id = *parameterized_amount_deploy_tx_id;
                move || {
                    let client = client.clone();
                    let address = address.clone();
                    async move {
                        client
                            .get_cells_by_addresses(vec![address])
                            .await
                            .unwrap()
                            .iter()
                            .filter(|cell| cell.outpoint.transaction_id == tx_id)
                            .any(|cell| cell.outpoint.index == 0)
                    }
                }
            },
            "CellScript parameterized amount script code cell was not indexed after deployment block acceptance",
        )
        .await;
    }

    let example_contracts = compile_all_spora_example_contracts();
    let remaining_prealloc = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity()).await;
    assert!(
        remaining_prealloc.len() >= example_contracts.len(),
        "bundled CellScript example deployment needs one matured prealloc cell per example"
    );

    let mut example_deployments = Vec::with_capacity(example_contracts.len());
    for (index, example) in example_contracts.into_iter().enumerate() {
        let example_deploy_input = &remaining_prealloc[index..index + 1];
        let example_input_capacity = example_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
        let example_code_cell_capacity = example_input_capacity / 2;
        let base_deployment_fee = required_fee(example_deploy_input.len(), 2).saturating_add(100_000);
        let preliminary_locked_capacity = example_input_capacity
            .checked_sub(example_code_cell_capacity)
            .and_then(|value| value.checked_sub(base_deployment_fee))
            .unwrap_or_else(|| panic!("{} deployment transaction must leave capacity for its locked probe cell", example.name));
        let example_lock = Script::new(example.code_hash, 0, vec![]);
        let example_code_output =
            CellOutput { capacity: example_code_cell_capacity, lock: pay_to_acceptance_owner(&prealloc_address), type_: None };
        let preliminary_locked_output = CellOutput { capacity: preliminary_locked_capacity, lock: example_lock.clone(), type_: None };
        let preliminary_standard_deployment_storage_mass = deployment_storage_mass(
            example_deploy_input,
            &[(example_code_output.clone(), example.artifact_bytes.len()), (preliminary_locked_output, 0)],
        );
        let example_deployment_fee = base_deployment_fee.max(preliminary_standard_deployment_storage_mass.saturating_add(100_000));
        let example_locked_capacity = example_input_capacity
            .checked_sub(example_code_cell_capacity)
            .and_then(|value| value.checked_sub(example_deployment_fee))
            .unwrap_or_else(|| panic!("{} deployment transaction must leave capacity for its locked probe cell", example.name));
        let example_locked_output = CellOutput { capacity: example_locked_capacity, lock: example_lock.clone(), type_: None };
        let example_standard_deployment_storage_mass = deployment_storage_mass(
            example_deploy_input,
            &[(example_code_output.clone(), example.artifact_bytes.len()), (example_locked_output.clone(), 0)],
        );
        let example_deploy_tx = generate_signed_cell_tx(
            prealloc_schnorr_key,
            &example_deploy_input,
            vec![],
            vec![example_code_output, example_locked_output],
            vec![example.artifact_bytes.clone(), vec![]],
        );
        let example_deploy_tx_id = spora_hashes::Hash::from_bytes(example_deploy_tx.id());
        let example_code_outpoint = TransactionOutpoint::new(example_deploy_tx.id(), 0);
        let example_locked_outpoint = TransactionOutpoint::new(example_deploy_tx.id(), 1);

        let mut deployment_probe_succeeded = false;
        let mut deployment_error = None;
        let deployment_probe_status;
        let mut code_cell_indexed = false;
        match rpc_client.submit_transaction((&example_deploy_tx).into(), false).await {
            Ok(_) => {
                let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
                assert!(
                    template
                        .block
                        .transactions
                        .iter()
                        .skip(1)
                        .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
                        .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == example_deploy_tx_id),
                    "expected block template to include {} deployment transaction",
                    example.name
                );
                rpc_client.submit_block(template.block, false).await.unwrap();

                wait_for(
                    50,
                    40,
                    {
                        let client = rpc_client.clone();
                        let address = prealloc_address.clone();
                        move || {
                            let client = client.clone();
                            let address = address.clone();
                            async move {
                                client
                                    .get_cells_by_addresses(vec![address])
                                    .await
                                    .unwrap()
                                    .iter()
                                    .filter(|cell| cell.outpoint.transaction_id == example_deploy_tx_id)
                                    .any(|cell| cell.outpoint.index == 0)
                            }
                        }
                    },
                    "bundled CellScript example code cell was not indexed after deployment block acceptance",
                )
                .await;
                deployment_probe_succeeded = true;
                deployment_probe_status = "accepted-indexed";
                code_cell_indexed = true;
            }
            Err(err) if standard_mass_policy => {
                deployment_error = Some(err.to_string());
                deployment_probe_status = "standard-policy-rejected";
            }
            Err(err) => {
                panic!("{} deployment must pass explicit relaxed relay policy: {err}", example.name);
            }
        }

        example_deployments.push(CellScriptExampleDeployment {
            name: example.name,
            artifact_size_bytes: example.artifact_bytes.len(),
            standard_deployment_storage_mass: example_standard_deployment_storage_mass,
            deployment_tx_id: example_deploy_tx_id,
            code_outpoint: example_code_outpoint,
            locked_outpoint: example_locked_outpoint,
            locked_capacity: example_locked_capacity,
            deployment_probe_succeeded,
            deployment_error,
            deployment_probe_status,
            code_cell_indexed,
            malformed_spend_rejected: false,
            malformed_spend_probe_status: "not-run",
            malformed_spend_reject_reason: String::new(),
            malformed_spend_rejected_by_standard_policy: false,
            ckb_runtime_required: example.ckb_runtime_required,
            action_artifacts: example.action_artifacts,
            action_names: example.action_names,
            estimated_compute_mass: example.estimated_compute_mass,
            estimated_storage_mass: example.estimated_storage_mass,
            estimated_transient_mass: example.estimated_transient_mass,
            estimated_code_deployment_mass: example.estimated_code_deployment_mass,
            requires_relaxed_mass_policy: example.requires_relaxed_mass_policy,
        });
    }

    for _ in 0..10 {
        let target_daa = rpc_client.get_server_info().await.unwrap().virtual_daa_score + 1;
        let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client.submit_block(template.block, false).await.unwrap();
        wait_for(
            50,
            40,
            {
                let client = rpc_client.clone();
                move || {
                    let client = client.clone();
                    async move { client.get_server_info().await.unwrap().virtual_daa_score >= target_daa }
                }
            },
            "virtual DAA score did not advance while maturing bundled CellScript example locked cells",
        )
        .await;
    }

    for (index, deployment) in example_deployments.iter_mut().enumerate() {
        if !deployment.deployment_probe_succeeded {
            deployment.malformed_spend_reject_reason = deployment
                .deployment_error
                .clone()
                .unwrap_or_else(|| "deployment probe skipped under standard mass policy".to_string());
            deployment.malformed_spend_probe_status = "skipped-deployment-not-indexed";
            deployment.malformed_spend_rejected_by_standard_policy = deployment
                .deployment_error
                .as_deref()
                .map(|reason| {
                    let lowered = reason.to_ascii_lowercase();
                    lowered.contains("not standard")
                        || lowered.contains("storage mass")
                        || lowered.contains("compute mass")
                        || lowered.contains("transient")
                        || lowered.contains("cycles exceeded")
                        || lowered.contains("cycles limit")
                })
                .unwrap_or(false);
            continue;
        }
        let malformed_output_capacity = deployment
            .locked_capacity
            .checked_sub(required_fee(1, 1).saturating_add(100_000))
            .unwrap_or_else(|| panic!("{} malformed spend must leave fee", deployment.name));
        let malformed_recipient = Address::new_std_single(NetworkType::Devnet.into(), &[40 + index as u8; 32])
            .unwrap_or_else(|err| panic!("{} malformed spend recipient address must be valid: {err}", deployment.name));
        let malformed_output =
            CellOutput { capacity: malformed_output_capacity, lock: pay_to_acceptance_owner(&malformed_recipient), type_: None };
        let malformed_spend = CellTx::new(
            vec![CellInput::new(OutPoint::new(deployment.locked_outpoint.tx_hash, deployment.locked_outpoint.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(deployment.code_outpoint.tx_hash, deployment.code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![malformed_output],
            vec![vec![]],
            vec![vec![]],
        )
        .unwrap_or_else(|err| panic!("{} malformed spend transaction must be structurally valid: {err}", deployment.name));
        let reject_reason = rpc_client
            .submit_transaction((&malformed_spend).into(), false)
            .await
            .expect_err("bundled example malformed spend should be rejected after loading the deployed script")
            .to_string();
        assert!(
            !reject_reason.contains("not standard")
                && !reject_reason.contains("storage mass")
                && !reject_reason.contains("compute mass")
                && !reject_reason.contains("transient")
                && !reject_reason.contains("cycles exceeded")
                && !reject_reason.contains("cycles limit"),
            "{} malformed spend must fail fast in script/business validation, not standard policy or VM cycles limit: {}",
            deployment.name,
            reject_reason
        );
        deployment.malformed_spend_rejected = true;
        deployment.malformed_spend_probe_status = "script-rejected";
        deployment.malformed_spend_reject_reason = reject_reason;
        deployment.malformed_spend_rejected_by_standard_policy = false;
    }

    for deployment in &example_deployments {
        base_report.bundled_examples.push(BundledExampleReport {
            name: deployment.name,
            artifact_size_bytes: deployment.artifact_size_bytes,
            ckb_runtime_required: deployment.ckb_runtime_required,
            action_names: deployment.action_names.clone(),
            action_count: deployment.action_names.len(),
            estimated_compute_mass: deployment.estimated_compute_mass,
            estimated_storage_mass: deployment.estimated_storage_mass,
            estimated_transient_mass: deployment.estimated_transient_mass,
            estimated_code_deployment_mass: deployment.estimated_code_deployment_mass,
            requires_relaxed_mass_policy: deployment.requires_relaxed_mass_policy,
            estimated_standard_deployment_storage_mass: deployment.standard_deployment_storage_mass,
            fits_standard_relay_transaction_mass: deployment.standard_deployment_storage_mass <= SPORA_STANDARD_RELAY_MAX_TX_MASS,
            deployment_tx_id: hash_hex(&deployment.deployment_tx_id.as_bytes()),
            code_cell_outpoint: outpoint_report(&deployment.code_outpoint),
            locked_probe_outpoint: outpoint_report(&deployment.locked_outpoint),
            deployment_probe_status: deployment.deployment_probe_status,
            code_cell_indexed: deployment.code_cell_indexed,
            malformed_spend_rejected: deployment.malformed_spend_rejected,
            malformed_spend_probe_status: deployment.malformed_spend_probe_status,
            malformed_spend_reject_reason: deployment.malformed_spend_reject_reason.clone(),
            malformed_spend_rejected_by_standard_policy: deployment.malformed_spend_rejected_by_standard_policy,
        });
    }
    prepare_action_builder_funding_cells(&rpc_client, &miner_address, prealloc_schnorr_key, &prealloc_address, standard_mass_policy)
        .await;

    let mut action_builder_matrix = run_nft_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    let token_action_builder_matrix = run_token_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    action_builder_matrix.valid.extend(token_action_builder_matrix.valid);
    action_builder_matrix.malformed.extend(token_action_builder_matrix.malformed);
    let amm_action_builder_matrix = run_amm_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    action_builder_matrix.valid.extend(amm_action_builder_matrix.valid);
    action_builder_matrix.malformed.extend(amm_action_builder_matrix.malformed);
    let launch_action_builder_matrix = run_launch_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    action_builder_matrix.valid.extend(launch_action_builder_matrix.valid);
    action_builder_matrix.malformed.extend(launch_action_builder_matrix.malformed);
    let vesting_action_builder_matrix = run_vesting_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    action_builder_matrix.valid.extend(vesting_action_builder_matrix.valid);
    action_builder_matrix.malformed.extend(vesting_action_builder_matrix.malformed);
    let multisig_action_builder_matrix = run_multisig_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    action_builder_matrix.valid.extend(multisig_action_builder_matrix.valid);
    action_builder_matrix.malformed.extend(multisig_action_builder_matrix.malformed);
    let timelock_action_builder_matrix = run_timelock_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    action_builder_matrix.valid.extend(timelock_action_builder_matrix.valid);
    action_builder_matrix.malformed.extend(timelock_action_builder_matrix.malformed);
    let invoice_action_builder_matrix = run_invoice_financing_action_builder_matrix(
        &rpc_client,
        &miner_address,
        prealloc_schnorr_key,
        &prealloc_address,
        &code_outpoint,
        &example_deployments,
    )
    .await;
    action_builder_matrix.valid.extend(invoice_action_builder_matrix.valid);
    action_builder_matrix.malformed.extend(invoice_action_builder_matrix.malformed);
    base_report.production_gate = build_spora_production_gate(&example_deployments, standard_mass_policy, &action_builder_matrix);

    let vm_spend_output_capacity =
        vm_locked_capacity.checked_sub(required_fee(1, 1).saturating_add(100_000)).expect("VM spend must leave fee");
    let vm_recipient = Address::new_std_single(NetworkType::Devnet.into(), &[14; 32]).expect("VM recipient address must be valid");
    let vm_spend_output = CellOutput { capacity: vm_spend_output_capacity, lock: pay_to_acceptance_owner(&vm_recipient), type_: None };
    let vm_spend_with_tampered_scheduler_witness = CellTx::new(
        vec![CellInput::new(OutPoint::new(vm_locked_outpoint.tx_hash, vm_locked_outpoint.index), 0)],
        vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
        vec![vm_spend_output.clone()],
        vec![vec![]],
        vec![vec![0x11, 0xCE, 0x01]],
    )
    .expect("valid VM spend transaction with malformed CellScript scheduler witness bytes");
    let scheduler_reject_reason = rpc_client
        .submit_transaction((&vm_spend_with_tampered_scheduler_witness).into(), false)
        .await
        .expect_err("mempool must reject malformed CellScript scheduler witness bytes")
        .to_string();
    assert!(
        scheduler_reject_reason.contains("CellScript scheduler metadata policy")
            || scheduler_reject_reason.contains("invalid CellScript scheduler witness"),
        "scheduler tamper rejection should be attributed to CellScript scheduler policy: {scheduler_reject_reason}"
    );
    base_report.scheduler_tamper_rejected = true;

    let vm_spend_without_dep = CellTx::new(
        vec![CellInput::new(OutPoint::new(vm_locked_outpoint.tx_hash, vm_locked_outpoint.index), 0)],
        vec![],
        vec![vm_spend_output.clone()],
        vec![vec![]],
        vec![vec![]],
    )
    .expect("valid VM spend transaction without deps");
    assert!(
        rpc_client.submit_transaction((&vm_spend_without_dep).into(), false).await.is_err(),
        "VM-locked spend must be rejected when the script code cell dep is missing"
    );

    let vm_spend_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(vm_locked_outpoint.tx_hash, vm_locked_outpoint.index), 0)],
        vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
        vec![vm_spend_output],
        vec![vec![]],
        vec![vec![]],
    )
    .expect("valid VM spend transaction with code dep");
    let vm_spend_tx_id = spora_hashes::Hash::from_bytes(vm_spend_tx.id());
    rpc_client.submit_transaction((&vm_spend_tx).into(), false).await.unwrap();

    let cellscript_spend_output_capacity = cellscript_locked_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("CellScript VM spend must leave fee");
    let cellscript_recipient =
        Address::new_std_single(NetworkType::Devnet.into(), &[15; 32]).expect("CellScript recipient address must be valid");
    let cellscript_spend_output =
        CellOutput { capacity: cellscript_spend_output_capacity, lock: pay_to_acceptance_owner(&cellscript_recipient), type_: None };
    let cellscript_spend_without_dep = CellTx::new(
        vec![CellInput::new(OutPoint::new(cellscript_locked_outpoint.tx_hash, cellscript_locked_outpoint.index), 0)],
        vec![],
        vec![cellscript_spend_output.clone()],
        vec![vec![]],
        vec![vec![]],
    )
    .expect("valid CellScript VM spend transaction without deps");
    assert!(
        rpc_client.submit_transaction((&cellscript_spend_without_dep).into(), false).await.is_err(),
        "CellScript VM-locked spend must be rejected when the compiled script code cell dep is missing"
    );

    let cellscript_spend_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(cellscript_locked_outpoint.tx_hash, cellscript_locked_outpoint.index), 0)],
        vec![CellDep {
            out_point: OutPoint::new(cellscript_code_outpoint.tx_hash, cellscript_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![cellscript_spend_output],
        vec![vec![]],
        vec![vec![]],
    )
    .expect("valid CellScript VM spend transaction with code dep");
    let cellscript_spend_tx_id = spora_hashes::Hash::from_bytes(cellscript_spend_tx.id());
    rpc_client.submit_transaction((&cellscript_spend_tx).into(), false).await.unwrap();

    let fixed_output_spend_capacity = fixed_output_locked_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("CellScript fixed-output spend must leave fee");
    let fixed_output_recipient = Address::new_std_single(NetworkType::Devnet.into(), &[17; 32])
        .expect("CellScript fixed-output recipient address must be valid");
    let fixed_output_spend_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(fixed_output_locked_outpoint.tx_hash, fixed_output_locked_outpoint.index), 0)],
        vec![CellDep {
            out_point: OutPoint::new(fixed_output_code_outpoint.tx_hash, fixed_output_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![CellOutput {
            capacity: fixed_output_spend_capacity,
            lock: pay_to_acceptance_owner(&fixed_output_recipient),
            type_: None,
        }],
        vec![42u64.to_le_bytes().to_vec()],
        vec![vec![]],
    )
    .expect("valid CellScript fixed-output spend transaction with code dep");
    let fixed_output_spend_tx_id = spora_hashes::Hash::from_bytes(fixed_output_spend_tx.id());
    let fixed_output_data_hash = spora_cell_data_hash(&42u64.to_le_bytes());
    rpc_client
        .submit_transaction((&fixed_output_spend_tx).into(), false)
        .await
        .expect("CellScript fixed-output spend should verify output cell data via LOAD_CELL_DATA");

    let parameterized_amount_spend = if let Some((
        parameterized_amount_contract,
        parameterized_amount_locked_capacity,
        _parameterized_amount_deploy_tx_id,
        parameterized_amount_code_outpoint,
        parameterized_amount_locked_outpoint,
        _parameterized_amount_deploy_tx,
    )) = &parameterized_amount_probe
    {
        let parameterized_amount = 77u64;
        let parameterized_amount_spend_capacity = parameterized_amount_locked_capacity
            .checked_sub(required_fee(1, 1).saturating_add(100_000))
            .expect("CellScript parameterized amount spend must leave fee");
        let parameterized_amount_recipient = Address::new_std_single(NetworkType::Devnet.into(), &[18; 32])
            .expect("CellScript parameterized amount recipient address must be valid");
        let parameterized_amount_spend_tx = CellTx::new(
            vec![CellInput::new(
                OutPoint::new(parameterized_amount_locked_outpoint.tx_hash, parameterized_amount_locked_outpoint.index),
                0,
            )],
            vec![CellDep {
                out_point: OutPoint::new(parameterized_amount_code_outpoint.tx_hash, parameterized_amount_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput {
                capacity: parameterized_amount_spend_capacity,
                lock: pay_to_acceptance_owner(&parameterized_amount_recipient),
                type_: None,
            }],
            vec![vec![]],
            vec![parameterized_amount_contract.entry_witness_for_amount(parameterized_amount)],
        )
        .expect("valid CellScript parameterized amount spend transaction with code dep and witness args");
        let parameterized_amount_spend_tx_id = spora_hashes::Hash::from_bytes(parameterized_amount_spend_tx.id());
        rpc_client
            .submit_transaction((&parameterized_amount_spend_tx).into(), false)
            .await
            .expect("CellScript parameterized amount spend should verify witness-bound scalar entry arguments");
        Some((parameterized_amount_recipient, parameterized_amount_spend_tx_id))
    } else {
        None
    };

    let template = rpc_client.get_block_template(miner_address, vec![]).await.unwrap();
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == vm_spend_tx_id),
        "expected block template to include the VM-locked spend transaction"
    );
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == cellscript_spend_tx_id),
        "expected block template to include the CellScript VM-locked spend transaction"
    );
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == fixed_output_spend_tx_id),
        "expected block template to include the CellScript fixed-output spend transaction"
    );
    if let Some((_, parameterized_amount_spend_tx_id)) = &parameterized_amount_spend {
        assert!(
            template
                .block
                .transactions
                .iter()
                .skip(1)
                .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
                .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == *parameterized_amount_spend_tx_id),
            "expected block template to include the CellScript parameterized amount spend transaction"
        );
    }
    rpc_client.submit_block(template.block, false).await.unwrap();

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = vm_recipient.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .any(|cell| cell.outpoint.transaction_id == vm_spend_tx_id)
                }
            }
        },
        "recipient cell from the VM-locked spend was not indexed after block acceptance",
    )
    .await;
    base_report.always_success_vm_spend_confirmed = true;

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = cellscript_recipient.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .any(|cell| cell.outpoint.transaction_id == cellscript_spend_tx_id)
                }
            }
        },
        "recipient cell from the CellScript VM-locked spend was not indexed after block acceptance",
    )
    .await;
    base_report.noop_cellscript_spend_confirmed = true;

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = fixed_output_recipient.clone();
            let data_hash = fixed_output_data_hash;
            move || {
                let client = client.clone();
                let address = address.clone();
                let data_hash = data_hash;
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == fixed_output_spend_tx_id
                            && cell.cell_entry.data_bytes == 8
                            && cell.cell_entry.data_hash == data_hash
                    })
                }
            }
        },
        "recipient cell from the CellScript fixed-output spend was not indexed with verified schema output data",
    )
    .await;
    base_report.cellscript_schema_output_spend_confirmed = true;

    if let Some((parameterized_amount_recipient, parameterized_amount_spend_tx_id)) = parameterized_amount_spend {
        wait_for(
            50,
            40,
            {
                let client = rpc_client.clone();
                let address = parameterized_amount_recipient.clone();
                move || {
                    let client = client.clone();
                    let address = address.clone();
                    async move {
                        client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                            cell.outpoint.transaction_id == parameterized_amount_spend_tx_id && cell.cell_entry.data_bytes == 0
                        })
                    }
                }
            },
            "recipient cell from the CellScript parameterized amount spend was not indexed after witness-bound scalar verification",
        )
        .await;
        base_report.cellscript_parameterized_amount_spend_confirmed = true;
    }
    write_base_report(&base_report);
}

fn pay_to_acceptance_owner(address: &Address) -> Script {
    spora_consensus_core::tx::pay_to_address_lock_script(address)
}

fn spora_cell_data_hash(data: &[u8]) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"spora-cell/data");
    hasher.update(data);
    *hasher.finalize().as_bytes()
}

fn standard_mass_policy_enabled() -> bool {
    std::env::var(STANDARD_MASS_POLICY_ENV).ok().as_deref() == Some("1")
}

fn pop_plain_cells(
    spendable_cells: &mut VecDeque<(TransactionOutpoint, spora_consensus_core::cell_diff::CellMeta)>,
    count: usize,
    context: &str,
) -> Vec<(TransactionOutpoint, spora_consensus_core::cell_diff::CellMeta)> {
    let mut selected = Vec::with_capacity(count);
    for _ in 0..count {
        selected.push(spendable_cells.pop_front().unwrap_or_else(|| panic!("{context} needs {count} matured plain prealloc cells")));
    }
    selected
}

fn code_deploy_storage_mass(
    inputs: &[(TransactionOutpoint, spora_consensus_core::cell_diff::CellMeta)],
    prealloc_address: &Address,
    artifact_size_bytes: usize,
) -> u64 {
    let input_capacity = inputs.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let output_capacity = input_capacity
        .checked_sub(required_fee(inputs.len(), 1).saturating_add(100_000))
        .expect("scoped action deployment must leave capacity for code cell");
    let output = CellOutput { capacity: output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None };
    deployment_storage_mass(inputs, &[(output, artifact_size_bytes)])
}

fn deployment_storage_mass(
    inputs: &[(TransactionOutpoint, spora_consensus_core::cell_diff::CellMeta)],
    outputs: &[(CellOutput, usize)],
) -> u64 {
    calc_storage_mass(
        false,
        inputs.iter().map(|(_, meta)| CellMass::from(meta)),
        outputs.iter().map(|(output, data_len)| CellMass::from((output, *data_len))),
        DEVNET_PARAMS.storage_mass_parameter,
    )
    .expect("deployment storage mass must be computable")
}

fn pop_code_deploy_cells(
    spendable_cells: &mut VecDeque<(TransactionOutpoint, spora_consensus_core::cell_diff::CellMeta)>,
    prealloc_address: &Address,
    artifact_size_bytes: usize,
    context: &str,
) -> Vec<(TransactionOutpoint, spora_consensus_core::cell_diff::CellMeta)> {
    let standard_mass_policy = standard_mass_policy_enabled();
    let mut selected = Vec::new();
    loop {
        selected.push(spendable_cells.pop_front().unwrap_or_else(|| panic!("{context} ran out of matured plain prealloc cells")));
        if !standard_mass_policy
            || code_deploy_storage_mass(&selected, prealloc_address, artifact_size_bytes) <= SPORA_STANDARD_RELAY_MAX_TX_MASS
        {
            return selected;
        }
    }
}

async fn deploy_cellscript_action_artifact(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    spendable_cells: &mut VecDeque<(TransactionOutpoint, spora_consensus_core::cell_diff::CellMeta)>,
    artifact: &CompiledCellScriptActionArtifact,
    context: &str,
) -> TransactionOutpoint {
    let deploy_input = pop_code_deploy_cells(spendable_cells, prealloc_address, artifact.artifact_bytes.len(), context);
    let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let code_cell_capacity = deploy_input_capacity
        .checked_sub(required_fee(deploy_input.len(), 1).saturating_add(100_000))
        .unwrap_or_else(|| panic!("{context} scoped action deployment must leave capacity for code cell"));
    let deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &deploy_input,
        vec![],
        vec![CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![artifact.artifact_bytes.clone()],
    );
    let deploy_tx_id = spora_hashes::Hash::from_bytes(deploy_tx.id());
    let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&deploy_tx).into(), false)
        .await
        .unwrap_or_else(|error| panic!("{context} scoped action deployment must be accepted: {error}"));
    submit_next_template_containing(rpc_client, miner_address, deploy_tx_id, &format!("{context} scoped action deployment")).await;
    code_outpoint
}

async fn prepare_action_builder_funding_cells(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    standard_mass_policy: bool,
) {
    const ACTION_BUILDER_FUNDING_OUTPUTS: u64 = 112;
    const STANDARD_ACTION_BUILDER_FUNDING_OUTPUTS_PER_TX: u64 = 16;
    let spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    let outputs_per_tx =
        if standard_mass_policy { STANDARD_ACTION_BUILDER_FUNDING_OUTPUTS_PER_TX } else { ACTION_BUILDER_FUNDING_OUTPUTS };
    let required_batches = ACTION_BUILDER_FUNDING_OUTPUTS.div_ceil(outputs_per_tx) as usize;
    assert!(
        spendable_cells.len() >= required_batches,
        "action-builder funding split needs {required_batches} matured plain prealloc cells"
    );

    let mut remaining_outputs = ACTION_BUILDER_FUNDING_OUTPUTS;
    for (batch_index, (input_outpoint, input_meta)) in spendable_cells.iter().take(required_batches).enumerate() {
        let batch_outputs = remaining_outputs.min(outputs_per_tx);
        let input = vec![(input_outpoint.clone(), input_meta.clone())];
        let input_capacity = input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
        let funding_capacity = input_capacity
            .checked_sub(required_fee(input.len(), batch_outputs).saturating_add(100_000))
            .and_then(|value| value.checked_div(batch_outputs))
            .expect("action-builder funding split must leave capacity for funding cells");
        let funding_tx = generate_signed_cell_tx(
            prealloc_schnorr_key,
            &input,
            vec![],
            (0..batch_outputs)
                .map(|_| CellOutput { capacity: funding_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None })
                .collect(),
            (0..batch_outputs).map(|_| vec![]).collect(),
        );
        let funding_tx_id = spora_hashes::Hash::from_bytes(funding_tx.id());
        rpc_client
            .submit_transaction((&funding_tx).into(), false)
            .await
            .unwrap_or_else(|err| panic!("action-builder funding split batch {} must be accepted: {err}", batch_index + 1));
        submit_next_template_containing(
            rpc_client,
            miner_address,
            funding_tx_id,
            &format!("action-builder funding split batch {}", batch_index + 1),
        )
        .await;
        remaining_outputs = remaining_outputs.saturating_sub(batch_outputs);
    }
    submit_empty_blocks(rpc_client, miner_address, 10).await;
}

async fn run_token_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(token_deployment) = deployments.iter().find(|deployment| deployment.name == "token.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(mint_artifact) = token_deployment.action_artifacts.iter().find(|artifact| artifact.name == "mint") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(transfer_artifact) = token_deployment.action_artifacts.iter().find(|artifact| artifact.name == "transfer_token") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(merge_artifact) = token_deployment.action_artifacts.iter().find(|artifact| artifact.name == "merge") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(burn_artifact) = token_deployment.action_artifacts.iter().find(|artifact| artifact.name == "burn") else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 6,
        "token action builder matrix needs six matured prealloc cells for scoped deploys and fixtures"
    );

    let deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, transfer_artifact.artifact_bytes.len(), "token transfer");
    let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let code_cell_capacity = deploy_input_capacity
        .checked_sub(required_fee(deploy_input.len(), 1).saturating_add(100_000))
        .expect("token transfer scoped action deployment must leave capacity for code cell");
    let deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &deploy_input,
        vec![],
        vec![CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![transfer_artifact.artifact_bytes.clone()],
    );
    let deploy_tx_id = spora_hashes::Hash::from_bytes(deploy_tx.id());
    let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);
    rpc_client.submit_transaction((&deploy_tx).into(), false).await.expect("token transfer scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, deploy_tx_id, "token transfer scoped action deployment").await;

    let token_data = token_cell_data(100, *b"SPORATKN");
    let fixture_input = pop_plain_cells(&mut spendable_cells, 1, "token transfer fixture");
    let fixture_input_capacity = fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let token_cell_capacity = fixture_input_capacity / 4;
    let fixture_change_capacity = fixture_input_capacity
        .checked_sub(token_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(fixture_input.len(), 3).saturating_add(100_000)))
        .expect("token transfer fixture transaction must leave change");
    let token_lock = Script::new(transfer_artifact.code_hash, 0, vec![]);
    let fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &fixture_input,
        vec![],
        vec![
            CellOutput { capacity: token_cell_capacity, lock: token_lock.clone(), type_: None },
            CellOutput { capacity: token_cell_capacity, lock: token_lock, type_: None },
            CellOutput { capacity: fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![token_data.clone(), token_data.clone(), vec![]],
    );
    let fixture_tx_id = spora_hashes::Hash::from_bytes(fixture_tx.id());
    let malformed_input = TransactionOutpoint::new(fixture_tx.id(), 0);
    let valid_input = TransactionOutpoint::new(fixture_tx.id(), 1);
    rpc_client.submit_transaction((&fixture_tx).into(), false).await.expect("token transfer fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, fixture_tx_id, "token transfer fixture cells").await;

    let recipient =
        Address::new_std_single(NetworkType::Devnet.into(), &[61; 32]).expect("token transfer recipient address must be valid");
    let recipient_lock = pay_to_acceptance_owner(&recipient);
    let recipient_lock_hash = recipient_lock.hash();
    let witness = transfer_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(recipient_lock_hash)])
        .expect("token transfer action witness must encode recipient lock hash");
    let spend_capacity = token_cell_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("token transfer action spend must leave fee");

    let malformed_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_input.tx_hash, malformed_input.index), 0)],
        vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
        vec![CellOutput { capacity: spend_capacity, lock: recipient_lock.clone(), type_: None }],
        vec![token_cell_data(101, *b"SPORATKN")],
        vec![witness.clone()],
    )
    .expect("malformed token transfer transaction must be structurally valid");
    let malformed_reject_reason = rpc_client
        .submit_transaction((&malformed_tx).into(), false)
        .await
        .expect_err("malformed token transfer must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_reject_reason.contains("not standard")
            && !malformed_reject_reason.contains("storage mass")
            && !malformed_reject_reason.contains("compute mass")
            && !malformed_reject_reason.contains("transient")
            && !malformed_reject_reason.contains("cycles exceeded")
            && !malformed_reject_reason.contains("cycles limit"),
        "malformed token transfer must fail in script validation, not policy or mass: {malformed_reject_reason}"
    );

    let valid_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_input.tx_hash, valid_input.index), 0)],
        vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
        vec![CellOutput { capacity: spend_capacity, lock: recipient_lock, type_: None }],
        vec![token_data.clone()],
        vec![witness],
    )
    .expect("valid token transfer transaction must be structurally valid");
    let valid_tx_id = spora_hashes::Hash::from_bytes(valid_tx.id());
    rpc_client
        .submit_transaction((&valid_tx).into(), false)
        .await
        .expect("valid token transfer must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_tx_id, "valid token transfer action").await;

    let token_data_hash = spora_cell_data_hash(&token_data);
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = recipient.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_tx_id
                            && cell.cell_entry.data_bytes == 16
                            && cell.cell_entry.data_hash == token_data_hash
                    })
                }
            }
        },
        "recipient token cell from the token transfer action builder was not indexed",
    )
    .await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    coverage.valid.insert(("token.cell".to_string(), "transfer_token".to_string()));
    coverage.malformed.insert(("token.cell".to_string(), "transfer_token".to_string()));

    let merge_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, merge_artifact.artifact_bytes.len(), "token merge");
    let merge_deploy_input_capacity = merge_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let merge_code_cell_capacity = merge_deploy_input_capacity
        .checked_sub(required_fee(merge_deploy_input.len(), 1).saturating_add(100_000))
        .expect("token merge scoped action deployment must leave capacity for code cell");
    let merge_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &merge_deploy_input,
        vec![],
        vec![CellOutput { capacity: merge_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![merge_artifact.artifact_bytes.clone()],
    );
    let merge_deploy_tx_id = spora_hashes::Hash::from_bytes(merge_deploy_tx.id());
    let merge_code_outpoint = TransactionOutpoint::new(merge_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&merge_deploy_tx).into(), false)
        .await
        .expect("token merge scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, merge_deploy_tx_id, "token merge scoped action deployment").await;

    let merge_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "token merge fixture");
    let merge_fixture_input_capacity = merge_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let merge_token_cell_capacity = merge_fixture_input_capacity / 8;
    let merge_fixture_change_capacity = merge_fixture_input_capacity
        .checked_sub(merge_token_cell_capacity.saturating_mul(4))
        .and_then(|value| value.checked_sub(required_fee(merge_fixture_input.len(), 5).saturating_add(100_000)))
        .expect("token merge fixture transaction must leave change");
    let merge_lock = Script::new(merge_artifact.code_hash, 0, vec![]);
    let merge_symbol = *b"SPORATKN";
    let other_symbol = *b"OTHER123";
    let merge_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &merge_fixture_input,
        vec![],
        vec![
            CellOutput { capacity: merge_token_cell_capacity, lock: merge_lock.clone(), type_: None },
            CellOutput { capacity: merge_token_cell_capacity, lock: merge_lock.clone(), type_: None },
            CellOutput { capacity: merge_token_cell_capacity, lock: merge_lock.clone(), type_: None },
            CellOutput { capacity: merge_token_cell_capacity, lock: merge_lock, type_: None },
            CellOutput { capacity: merge_fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            token_cell_data(10, merge_symbol),
            token_cell_data(20, other_symbol),
            token_cell_data(30, merge_symbol),
            token_cell_data(40, merge_symbol),
            vec![],
        ],
    );
    let merge_fixture_tx_id = spora_hashes::Hash::from_bytes(merge_fixture_tx.id());
    let malformed_left = TransactionOutpoint::new(merge_fixture_tx.id(), 0);
    let malformed_right = TransactionOutpoint::new(merge_fixture_tx.id(), 1);
    let valid_left = TransactionOutpoint::new(merge_fixture_tx.id(), 2);
    let valid_right = TransactionOutpoint::new(merge_fixture_tx.id(), 3);
    rpc_client.submit_transaction((&merge_fixture_tx).into(), false).await.expect("token merge fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, merge_fixture_tx_id, "token merge fixture cells").await;

    let merge_recipient =
        Address::new_std_single(NetworkType::Devnet.into(), &[62; 32]).expect("token merge recipient address must be valid");
    let merge_recipient_lock = pay_to_acceptance_owner(&merge_recipient);
    let merge_witness = merge_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(merge_recipient_lock.hash())])
        .expect("token merge action witness must encode recipient lock hash");
    let merge_spend_capacity = merge_token_cell_capacity
        .checked_mul(2)
        .and_then(|value| value.checked_sub(required_fee(2, 1).saturating_add(100_000)))
        .expect("token merge action spend must leave fee");

    let malformed_merge_tx = CellTx::new(
        vec![
            CellInput::new(OutPoint::new(malformed_left.tx_hash, malformed_left.index), 0),
            CellInput::new(OutPoint::new(malformed_right.tx_hash, malformed_right.index), 0),
        ],
        vec![CellDep { out_point: OutPoint::new(merge_code_outpoint.tx_hash, merge_code_outpoint.index), dep_type: DepType::Code }],
        vec![CellOutput { capacity: merge_spend_capacity, lock: merge_recipient_lock.clone(), type_: None }],
        vec![token_cell_data(30, merge_symbol)],
        vec![merge_witness.clone(), vec![]],
    )
    .expect("malformed token merge transaction must be structurally valid");
    let malformed_merge_reason = rpc_client
        .submit_transaction((&malformed_merge_tx).into(), false)
        .await
        .expect_err("malformed token merge must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_merge_reason.contains("not standard")
            && !malformed_merge_reason.contains("storage mass")
            && !malformed_merge_reason.contains("compute mass")
            && !malformed_merge_reason.contains("transient")
            && !malformed_merge_reason.contains("cycles exceeded")
            && !malformed_merge_reason.contains("cycles limit"),
        "malformed token merge must fail in script validation, not policy or mass: {malformed_merge_reason}"
    );

    let valid_merge_data = token_cell_data(70, merge_symbol);
    let valid_merge_tx = CellTx::new(
        vec![
            CellInput::new(OutPoint::new(valid_left.tx_hash, valid_left.index), 0),
            CellInput::new(OutPoint::new(valid_right.tx_hash, valid_right.index), 0),
        ],
        vec![CellDep { out_point: OutPoint::new(merge_code_outpoint.tx_hash, merge_code_outpoint.index), dep_type: DepType::Code }],
        vec![CellOutput { capacity: merge_spend_capacity, lock: merge_recipient_lock, type_: None }],
        vec![valid_merge_data.clone()],
        vec![merge_witness, vec![]],
    )
    .expect("valid token merge transaction must be structurally valid");
    let valid_merge_tx_id = spora_hashes::Hash::from_bytes(valid_merge_tx.id());
    rpc_client
        .submit_transaction((&valid_merge_tx).into(), false)
        .await
        .expect("valid token merge must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_merge_tx_id, "valid token merge action").await;

    let valid_merge_data_hash = spora_cell_data_hash(&valid_merge_data);
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = merge_recipient.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_merge_tx_id
                            && cell.cell_entry.data_bytes == 16
                            && cell.cell_entry.data_hash == valid_merge_data_hash
                    })
                }
            }
        },
        "recipient token cell from the token merge action builder was not indexed",
    )
    .await;

    coverage.valid.insert(("token.cell".to_string(), "merge".to_string()));
    coverage.malformed.insert(("token.cell".to_string(), "merge".to_string()));

    let burn_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, burn_artifact.artifact_bytes.len(), "token burn");
    let burn_deploy_input_capacity = burn_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let burn_code_cell_capacity = burn_deploy_input_capacity
        .checked_sub(required_fee(burn_deploy_input.len(), 1).saturating_add(100_000))
        .expect("token burn scoped action deployment must leave capacity for code cell");
    let burn_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &burn_deploy_input,
        vec![],
        vec![CellOutput { capacity: burn_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![burn_artifact.artifact_bytes.clone()],
    );
    let burn_deploy_tx_id = spora_hashes::Hash::from_bytes(burn_deploy_tx.id());
    let burn_code_outpoint = TransactionOutpoint::new(burn_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&burn_deploy_tx).into(), false)
        .await
        .expect("token burn scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, burn_deploy_tx_id, "token burn scoped action deployment").await;

    let burn_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "token burn fixture");
    let burn_fixture_input_capacity = burn_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let burn_token_cell_capacity = burn_fixture_input_capacity / 4;
    let burn_fixture_change_capacity = burn_fixture_input_capacity
        .checked_sub(burn_token_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(burn_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("token burn fixture transaction must leave change");
    let burn_lock = Script::new(burn_artifact.code_hash, 0, vec![]);
    let burn_type = Script::new(always_success_code_hash(), 0, b"token-burn-type".to_vec());
    let burn_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &burn_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: burn_token_cell_capacity, lock: burn_lock.clone(), type_: Some(burn_type.clone()) },
            CellOutput { capacity: burn_token_cell_capacity, lock: burn_lock, type_: Some(burn_type) },
            CellOutput { capacity: burn_fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![token_cell_data(0, *b"SPORATKN"), token_cell_data(100, *b"SPORATKN"), vec![]],
    );
    let burn_fixture_tx_id = spora_hashes::Hash::from_bytes(burn_fixture_tx.id());
    let malformed_burn_input = TransactionOutpoint::new(burn_fixture_tx.id(), 0);
    let valid_burn_input = TransactionOutpoint::new(burn_fixture_tx.id(), 1);
    rpc_client.submit_transaction((&burn_fixture_tx).into(), false).await.expect("token burn fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, burn_fixture_tx_id, "token burn fixture cells").await;

    let burn_witness = burn_artifact.action.entry_witness_args(&[]).expect("token burn action witness must encode");
    let burn_change_capacity =
        burn_token_cell_capacity.checked_sub(required_fee(1, 1).saturating_add(100_000)).expect("token burn action must leave change");

    let malformed_burn_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_burn_input.tx_hash, malformed_burn_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(burn_code_outpoint.tx_hash, burn_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: burn_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![vec![]],
        vec![burn_witness.clone()],
    )
    .expect("malformed token burn transaction must be structurally valid");
    let malformed_burn_reason = rpc_client
        .submit_transaction((&malformed_burn_tx).into(), false)
        .await
        .expect_err("malformed token burn must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_burn_reason.contains("not standard")
            && !malformed_burn_reason.contains("storage mass")
            && !malformed_burn_reason.contains("compute mass")
            && !malformed_burn_reason.contains("transient")
            && !malformed_burn_reason.contains("cycles exceeded")
            && !malformed_burn_reason.contains("cycles limit"),
        "malformed token burn must fail in script validation, not policy or mass: {malformed_burn_reason}"
    );

    let valid_burn_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_burn_input.tx_hash, valid_burn_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(burn_code_outpoint.tx_hash, burn_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: burn_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![vec![]],
        vec![burn_witness],
    )
    .expect("valid token burn transaction must be structurally valid");
    let valid_burn_tx_id = spora_hashes::Hash::from_bytes(valid_burn_tx.id());
    rpc_client
        .submit_transaction((&valid_burn_tx).into(), false)
        .await
        .expect("valid token burn must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_burn_tx_id, "valid token burn action").await;

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .any(|cell| cell.outpoint.transaction_id == valid_burn_tx_id && cell.cell_entry.data_bytes == 0)
                }
            }
        },
        "change cell from the token burn action builder was not indexed",
    )
    .await;

    coverage.valid.insert(("token.cell".to_string(), "burn".to_string()));
    coverage.malformed.insert(("token.cell".to_string(), "burn".to_string()));

    submit_empty_blocks(rpc_client, miner_address, 10).await;

    let mut mint_spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        mint_spendable_cells.len() >= 2,
        "token mint action builder needs two matured plain prealloc cells for scoped deploy and fixture"
    );

    let mint_deploy_input =
        pop_code_deploy_cells(&mut mint_spendable_cells, prealloc_address, mint_artifact.artifact_bytes.len(), "token mint");
    let mint_deploy_input_capacity = mint_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let mint_code_cell_capacity = mint_deploy_input_capacity
        .checked_sub(required_fee(mint_deploy_input.len(), 1).saturating_add(100_000))
        .expect("token mint scoped action deployment must leave capacity for code cell");
    let mint_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &mint_deploy_input,
        vec![],
        vec![CellOutput { capacity: mint_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![mint_artifact.artifact_bytes.clone()],
    );
    let mint_deploy_tx_id = spora_hashes::Hash::from_bytes(mint_deploy_tx.id());
    let mint_code_outpoint = TransactionOutpoint::new(mint_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&mint_deploy_tx).into(), false)
        .await
        .expect("token mint scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, mint_deploy_tx_id, "token mint scoped action deployment").await;

    let mint_fixture_input = pop_plain_cells(&mut mint_spendable_cells, 1, "token mint fixture");
    let mint_fixture_input_capacity = mint_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let mint_authority_cell_capacity = mint_fixture_input_capacity / 4;
    let mint_fixture_change_capacity = mint_fixture_input_capacity
        .checked_sub(mint_authority_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(mint_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("token mint fixture transaction must leave change");
    let mint_lock = Script::new(mint_artifact.code_hash, 0, vec![]);
    let mint_type = Script::new(always_success_code_hash(), 0, b"token-mint-type".to_vec());
    let mint_symbol = *b"SPORATKN";
    let mint_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &mint_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: mint_authority_cell_capacity, lock: mint_lock.clone(), type_: Some(mint_type.clone()) },
            CellOutput { capacity: mint_authority_cell_capacity, lock: mint_lock.clone(), type_: Some(mint_type.clone()) },
            CellOutput { capacity: mint_fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![mint_authority_cell_data(mint_symbol, 1_000, 10), mint_authority_cell_data(mint_symbol, 1_000, 10), vec![]],
    );
    let mint_fixture_tx_id = spora_hashes::Hash::from_bytes(mint_fixture_tx.id());
    let malformed_mint_input = TransactionOutpoint::new(mint_fixture_tx.id(), 0);
    let valid_mint_input = TransactionOutpoint::new(mint_fixture_tx.id(), 1);
    rpc_client.submit_transaction((&mint_fixture_tx).into(), false).await.expect("token mint fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, mint_fixture_tx_id, "token mint fixture cells").await;

    let mint_recipient =
        Address::new_std_single(NetworkType::Devnet.into(), &[64; 32]).expect("token mint recipient address must be valid");
    let mint_recipient_lock = pay_to_acceptance_owner(&mint_recipient);
    let mint_amount = 25;
    let mint_witness = mint_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(mint_recipient_lock.hash()),
            cellscript::EntryWitnessArg::U64(mint_amount),
        ])
        .expect("token mint action witness must encode recipient lock hash and amount");
    let minted_token_capacity = mint_authority_cell_capacity / 4;
    let replacement_capacity = mint_authority_cell_capacity
        .checked_sub(minted_token_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("token mint action must leave capacity for token output and authority replacement");
    let valid_minted_token_data = token_cell_data(mint_amount, mint_symbol);
    let valid_replacement_data = mint_authority_cell_data(mint_symbol, 1_000, 35);

    let malformed_mint_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_mint_input.tx_hash, malformed_mint_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(mint_code_outpoint.tx_hash, mint_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: replacement_capacity, lock: mint_lock.clone(), type_: Some(mint_type.clone()) },
            CellOutput { capacity: minted_token_capacity, lock: mint_recipient_lock.clone(), type_: None },
        ],
        vec![mint_authority_cell_data(mint_symbol, 1_000, 36), valid_minted_token_data.clone()],
        vec![mint_witness.clone()],
    )
    .expect("malformed token mint transaction must be structurally valid");
    let malformed_mint_reason = rpc_client
        .submit_transaction((&malformed_mint_tx).into(), false)
        .await
        .expect_err("malformed token mint must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_mint_reason.contains("not standard")
            && !malformed_mint_reason.contains("storage mass")
            && !malformed_mint_reason.contains("compute mass")
            && !malformed_mint_reason.contains("transient")
            && !malformed_mint_reason.contains("cycles exceeded")
            && !malformed_mint_reason.contains("cycles limit"),
        "malformed token mint must fail in script validation, not policy or mass: {malformed_mint_reason}"
    );

    let valid_mint_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_mint_input.tx_hash, valid_mint_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(mint_code_outpoint.tx_hash, mint_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: replacement_capacity, lock: mint_lock, type_: Some(mint_type) },
            CellOutput { capacity: minted_token_capacity, lock: mint_recipient_lock, type_: None },
        ],
        vec![valid_replacement_data, valid_minted_token_data.clone()],
        vec![mint_witness],
    )
    .expect("valid token mint transaction must be structurally valid");
    let valid_mint_tx_id = spora_hashes::Hash::from_bytes(valid_mint_tx.id());
    rpc_client
        .submit_transaction((&valid_mint_tx).into(), false)
        .await
        .expect("valid token mint must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_mint_tx_id, "valid token mint action").await;

    let minted_token_data_hash = spora_cell_data_hash(&valid_minted_token_data);
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = mint_recipient.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_mint_tx_id
                            && cell.cell_entry.data_bytes == 16
                            && cell.cell_entry.data_hash == minted_token_data_hash
                    })
                }
            }
        },
        "recipient token cell from the token mint action builder was not indexed",
    )
    .await;

    coverage.valid.insert(("token.cell".to_string(), "mint".to_string()));
    coverage.malformed.insert(("token.cell".to_string(), "mint".to_string()));
    coverage
}

async fn run_amm_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(amm_deployment) = deployments.iter().find(|deployment| deployment.name == "amm_pool.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(seed_pool_artifact) = amm_deployment.action_artifacts.iter().find(|artifact| artifact.name == "seed_pool") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(swap_artifact) = amm_deployment.action_artifacts.iter().find(|artifact| artifact.name == "swap_a_for_b") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(add_liquidity_artifact) = amm_deployment.action_artifacts.iter().find(|artifact| artifact.name == "add_liquidity") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(remove_liquidity_artifact) = amm_deployment.action_artifacts.iter().find(|artifact| artifact.name == "remove_liquidity")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(isqrt_artifact) = amm_deployment.action_artifacts.iter().find(|artifact| artifact.name == "isqrt") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(min_artifact) = amm_deployment.action_artifacts.iter().find(|artifact| artifact.name == "min") else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 12,
        "AMM action builder matrix needs twelve matured plain prealloc cells for scoped deploys and executable fixtures"
    );

    let deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, seed_pool_artifact.artifact_bytes.len(), "AMM seed_pool");
    let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let code_cell_capacity = deploy_input_capacity
        .checked_sub(required_fee(deploy_input.len(), 1).saturating_add(100_000))
        .expect("AMM seed_pool scoped action deployment must leave capacity for code cell");
    let deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &deploy_input,
        vec![],
        vec![CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![seed_pool_artifact.artifact_bytes.clone()],
    );
    let deploy_tx_id = spora_hashes::Hash::from_bytes(deploy_tx.id());
    let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);
    rpc_client.submit_transaction((&deploy_tx).into(), false).await.expect("AMM seed_pool scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, deploy_tx_id, "AMM seed_pool scoped action deployment").await;

    let fixture_input = pop_plain_cells(&mut spendable_cells, 1, "AMM seed_pool fixture");
    let fixture_input_capacity = fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let token_cell_capacity = fixture_input_capacity / 4;
    let fixture_change_capacity = fixture_input_capacity
        .checked_sub(token_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(fixture_input.len(), 3).saturating_add(100_000)))
        .expect("AMM seed_pool fixture transaction must leave change");
    let action_lock = Script::new(seed_pool_artifact.code_hash, 0, vec![]);
    let token_a_type = Script::new(always_success_code_hash(), 0, b"amm-token-a-type".to_vec());
    let token_b_type = Script::new(always_success_code_hash(), 0, b"amm-token-b-type".to_vec());
    let symbol_a = *b"AMMA0001";
    let symbol_b = *b"AMMB0001";
    let amount_a = 100_u64;
    let amount_b = 400_u64;
    let fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: token_cell_capacity, lock: action_lock.clone(), type_: Some(token_a_type) },
            CellOutput { capacity: token_cell_capacity, lock: action_lock.clone(), type_: Some(token_b_type) },
            CellOutput { capacity: fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![token_cell_data(amount_a, symbol_a), token_cell_data(amount_b, symbol_b), vec![]],
    );
    let fixture_tx_id = spora_hashes::Hash::from_bytes(fixture_tx.id());
    let token_a_input = TransactionOutpoint::new(fixture_tx.id(), 0);
    let token_b_input = TransactionOutpoint::new(fixture_tx.id(), 1);
    rpc_client.submit_transaction((&fixture_tx).into(), false).await.expect("AMM seed_pool fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, fixture_tx_id, "AMM seed_pool fixture cells").await;

    let fee_rate_bps = 30_u16;
    let provider_address =
        Address::new_std_single(NetworkType::Devnet.into(), &[190; 32]).expect("AMM provider address must be valid");
    let provider_lock = pay_to_acceptance_owner(&provider_address);
    let provider = provider_lock.hash();
    let pool_type = Script::new(always_success_code_hash(), 0, b"amm-pool-type".to_vec());
    let receipt_type = Script::new(always_success_code_hash(), 0, b"amm-lp-receipt-type".to_vec());
    let pool_id = pool_type.hash();
    let initial_lp = 200_u64;
    let witness = seed_pool_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::U16(fee_rate_bps), cellscript::EntryWitnessArg::Address(provider)])
        .expect("AMM seed_pool witness must encode fee_rate_bps and provider");
    let output_capacity =
        token_cell_capacity.checked_sub(required_fee(2, 2).saturating_add(100_000)).expect("AMM seed_pool action must leave capacity")
            / 2;
    let outputs = vec![
        CellOutput { capacity: output_capacity, lock: action_lock.clone(), type_: Some(pool_type.clone()) },
        CellOutput { capacity: output_capacity, lock: provider_lock, type_: Some(receipt_type) },
    ];
    let valid_output_data = vec![
        pool_cell_data(symbol_a, symbol_b, amount_a, amount_b, initial_lp, fee_rate_bps),
        lp_receipt_cell_data(pool_id, initial_lp, provider),
    ];
    let malformed_output_data = vec![
        pool_cell_data(symbol_a, symbol_b, amount_a, amount_b + 1, initial_lp, fee_rate_bps),
        lp_receipt_cell_data(pool_id, initial_lp, provider),
    ];
    let cell_deps = vec![
        CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code },
        CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        },
    ];
    let malformed_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(token_a_input.tx_hash, token_a_input.index), 0),
                CellInput::new(OutPoint::new(token_b_input.tx_hash, token_b_input.index), 0),
            ],
            cell_deps.clone(),
            outputs.clone(),
            malformed_output_data,
            vec![witness.clone(), vec![]],
        )
        .expect("malformed AMM seed_pool transaction must be structurally valid"),
        &seed_pool_artifact.action,
        "malformed AMM seed_pool transaction",
    );
    let malformed_reason = rpc_client
        .submit_transaction((&malformed_tx).into(), false)
        .await
        .expect_err("malformed AMM seed_pool must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_reason, "AMM seed_pool");

    let valid_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(token_a_input.tx_hash, token_a_input.index), 0),
                CellInput::new(OutPoint::new(token_b_input.tx_hash, token_b_input.index), 0),
            ],
            cell_deps,
            outputs,
            valid_output_data,
            vec![witness, vec![]],
        )
        .expect("valid AMM seed_pool transaction must be structurally valid"),
        &seed_pool_artifact.action,
        "valid AMM seed_pool transaction",
    );
    let valid_tx_id = spora_hashes::Hash::from_bytes(valid_tx.id());
    rpc_client
        .submit_transaction((&valid_tx).into(), false)
        .await
        .expect("valid AMM seed_pool must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_tx_id, "valid AMM seed_pool action").await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    coverage.valid.insert(("amm_pool.cell".to_string(), "seed_pool".to_string()));
    coverage.malformed.insert(("amm_pool.cell".to_string(), "seed_pool".to_string()));

    let swap_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, swap_artifact.artifact_bytes.len(), "AMM swap_a_for_b");
    let swap_deploy_input_capacity = swap_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let swap_code_cell_capacity = swap_deploy_input_capacity
        .checked_sub(required_fee(swap_deploy_input.len(), 1).saturating_add(100_000))
        .expect("AMM swap_a_for_b scoped action deployment must leave capacity for code cell");
    let swap_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &swap_deploy_input,
        vec![],
        vec![CellOutput { capacity: swap_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![swap_artifact.artifact_bytes.clone()],
    );
    let swap_deploy_tx_id = spora_hashes::Hash::from_bytes(swap_deploy_tx.id());
    let swap_code_outpoint = TransactionOutpoint::new(swap_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&swap_deploy_tx).into(), false)
        .await
        .expect("AMM swap_a_for_b scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, swap_deploy_tx_id, "AMM swap_a_for_b scoped action deployment").await;

    let swap_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "AMM swap_a_for_b fixture");
    let swap_fixture_input_capacity = swap_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let swap_cell_capacity = swap_fixture_input_capacity / 4;
    let swap_change_capacity = swap_fixture_input_capacity
        .checked_sub(swap_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(swap_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("AMM swap_a_for_b fixture transaction must leave change");
    let swap_lock = Script::new(swap_artifact.code_hash, 0, vec![]);
    let swap_token_a_type = Script::new(always_success_code_hash(), 0, b"amm-swap-token-a-type".to_vec());
    let swap_pool_type = Script::new(always_success_code_hash(), 0, b"amm-swap-pool-type".to_vec());
    let swap_reserve_a = 1_000_u64;
    let swap_reserve_b = 2_000_u64;
    let swap_total_lp = 1_000_u64;
    let swap_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &swap_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: swap_cell_capacity, lock: swap_lock.clone(), type_: Some(swap_token_a_type) },
            CellOutput { capacity: swap_cell_capacity, lock: swap_lock.clone(), type_: Some(swap_pool_type.clone()) },
            CellOutput { capacity: swap_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            token_cell_data(amount_a, symbol_a),
            pool_cell_data(symbol_a, symbol_b, swap_reserve_a, swap_reserve_b, swap_total_lp, fee_rate_bps),
            vec![],
        ],
    );
    let swap_fixture_tx_id = spora_hashes::Hash::from_bytes(swap_fixture_tx.id());
    let swap_token_input = TransactionOutpoint::new(swap_fixture_tx.id(), 0);
    let swap_pool_input = TransactionOutpoint::new(swap_fixture_tx.id(), 1);
    rpc_client.submit_transaction((&swap_fixture_tx).into(), false).await.expect("AMM swap_a_for_b fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, swap_fixture_tx_id, "AMM swap_a_for_b fixture cells").await;

    let swap_fee = amount_a * fee_rate_bps as u64 / 10_000;
    let swap_net_input = amount_a - swap_fee;
    let swap_output = swap_reserve_b * swap_net_input / (swap_reserve_a + swap_net_input);
    let swap_output_reserve_a = swap_reserve_a + amount_a;
    let swap_output_reserve_b = swap_reserve_b - swap_output;
    let swap_recipient =
        Address::new_std_single(NetworkType::Devnet.into(), &[191; 32]).expect("AMM swap recipient address must be valid");
    let swap_recipient_lock = pay_to_acceptance_owner(&swap_recipient);
    let swap_recipient_hash = swap_recipient_lock.hash();
    let swap_witness = swap_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::U64(swap_output),
            cellscript::EntryWitnessArg::Address(swap_recipient_hash),
        ])
        .expect("AMM swap_a_for_b witness must encode min_output and recipient");
    let swap_output_capacity = swap_cell_capacity
        .checked_sub(required_fee(2, 2).saturating_add(100_000))
        .expect("AMM swap_a_for_b action must leave capacity")
        / 2;
    let swap_outputs = vec![
        CellOutput { capacity: swap_output_capacity, lock: swap_lock, type_: Some(swap_pool_type) },
        CellOutput { capacity: swap_output_capacity, lock: swap_recipient_lock, type_: None },
    ];
    let swap_valid_output_data = vec![
        pool_cell_data(symbol_a, symbol_b, swap_output_reserve_a, swap_output_reserve_b, swap_total_lp, fee_rate_bps),
        token_cell_data(swap_output, symbol_b),
    ];
    let swap_malformed_output_data = vec![
        pool_cell_data(symbol_a, symbol_b, swap_output_reserve_a, swap_output_reserve_b - 1, swap_total_lp, fee_rate_bps),
        token_cell_data(swap_output + 1, symbol_b),
    ];
    let swap_cell_deps = vec![
        CellDep { out_point: OutPoint::new(swap_code_outpoint.tx_hash, swap_code_outpoint.index), dep_type: DepType::Code },
        CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        },
    ];
    let malformed_swap_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(swap_pool_input.tx_hash, swap_pool_input.index), 0),
                CellInput::new(OutPoint::new(swap_token_input.tx_hash, swap_token_input.index), 0),
            ],
            swap_cell_deps.clone(),
            swap_outputs.clone(),
            swap_malformed_output_data,
            vec![swap_witness.clone(), vec![]],
        )
        .expect("malformed AMM swap_a_for_b transaction must be structurally valid"),
        &swap_artifact.action,
        "malformed AMM swap_a_for_b transaction",
    );
    let malformed_swap_reason = rpc_client
        .submit_transaction((&malformed_swap_tx).into(), false)
        .await
        .expect_err("malformed AMM swap_a_for_b must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_swap_reason, "AMM swap_a_for_b");

    let valid_swap_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(swap_pool_input.tx_hash, swap_pool_input.index), 0),
                CellInput::new(OutPoint::new(swap_token_input.tx_hash, swap_token_input.index), 0),
            ],
            swap_cell_deps,
            swap_outputs,
            swap_valid_output_data,
            vec![swap_witness, vec![]],
        )
        .expect("valid AMM swap_a_for_b transaction must be structurally valid"),
        &swap_artifact.action,
        "valid AMM swap_a_for_b transaction",
    );
    let valid_swap_tx_id = spora_hashes::Hash::from_bytes(valid_swap_tx.id());
    rpc_client
        .submit_transaction((&valid_swap_tx).into(), false)
        .await
        .expect("valid AMM swap_a_for_b must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_swap_tx_id, "valid AMM swap_a_for_b action").await;

    coverage.valid.insert(("amm_pool.cell".to_string(), "swap_a_for_b".to_string()));
    coverage.malformed.insert(("amm_pool.cell".to_string(), "swap_a_for_b".to_string()));

    let add_liquidity_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        add_liquidity_artifact.artifact_bytes.len(),
        "AMM add_liquidity",
    );
    let add_liquidity_deploy_input_capacity = add_liquidity_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let add_liquidity_code_cell_capacity = add_liquidity_deploy_input_capacity
        .checked_sub(required_fee(add_liquidity_deploy_input.len(), 1).saturating_add(100_000))
        .expect("AMM add_liquidity scoped action deployment must leave capacity for code cell");
    let add_liquidity_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &add_liquidity_deploy_input,
        vec![],
        vec![CellOutput { capacity: add_liquidity_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![add_liquidity_artifact.artifact_bytes.clone()],
    );
    let add_liquidity_deploy_tx_id = spora_hashes::Hash::from_bytes(add_liquidity_deploy_tx.id());
    let add_liquidity_code_outpoint = TransactionOutpoint::new(add_liquidity_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&add_liquidity_deploy_tx).into(), false)
        .await
        .expect("AMM add_liquidity scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        add_liquidity_deploy_tx_id,
        "AMM add_liquidity scoped action deployment",
    )
    .await;

    let add_liquidity_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "AMM add_liquidity fixture");
    let add_liquidity_fixture_input_capacity = add_liquidity_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let add_liquidity_cell_capacity = add_liquidity_fixture_input_capacity / 5;
    let add_liquidity_change_capacity = add_liquidity_fixture_input_capacity
        .checked_sub(add_liquidity_cell_capacity.saturating_mul(3))
        .and_then(|value| value.checked_sub(required_fee(add_liquidity_fixture_input.len(), 4).saturating_add(100_000)))
        .expect("AMM add_liquidity fixture transaction must leave change");
    let add_liquidity_lock = Script::new(add_liquidity_artifact.code_hash, 0, vec![]);
    let add_token_a_type = Script::new(always_success_code_hash(), 0, b"amm-add-token-a-type".to_vec());
    let add_token_b_type = Script::new(always_success_code_hash(), 0, b"amm-add-token-b-type".to_vec());
    let add_pool_type = Script::new(always_success_code_hash(), 0, b"amm-add-pool-type".to_vec());
    let add_reserve_a = 1_000_u64;
    let add_reserve_b = 2_000_u64;
    let add_total_lp = 1_000_u64;
    let add_amount_a = 100_u64;
    let add_amount_b = 200_u64;
    let add_liquidity_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &add_liquidity_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: add_liquidity_cell_capacity, lock: add_liquidity_lock.clone(), type_: Some(add_token_a_type) },
            CellOutput { capacity: add_liquidity_cell_capacity, lock: add_liquidity_lock.clone(), type_: Some(add_token_b_type) },
            CellOutput { capacity: add_liquidity_cell_capacity, lock: add_liquidity_lock.clone(), type_: Some(add_pool_type.clone()) },
            CellOutput { capacity: add_liquidity_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            token_cell_data(add_amount_a, symbol_a),
            token_cell_data(add_amount_b, symbol_b),
            pool_cell_data(symbol_a, symbol_b, add_reserve_a, add_reserve_b, add_total_lp, fee_rate_bps),
            vec![],
        ],
    );
    let add_liquidity_fixture_tx_id = spora_hashes::Hash::from_bytes(add_liquidity_fixture_tx.id());
    let add_token_a_input = TransactionOutpoint::new(add_liquidity_fixture_tx.id(), 0);
    let add_token_b_input = TransactionOutpoint::new(add_liquidity_fixture_tx.id(), 1);
    let add_pool_input = TransactionOutpoint::new(add_liquidity_fixture_tx.id(), 2);
    rpc_client
        .submit_transaction((&add_liquidity_fixture_tx).into(), false)
        .await
        .expect("AMM add_liquidity fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, add_liquidity_fixture_tx_id, "AMM add_liquidity fixture cells").await;

    let add_lp_from_a = add_amount_a * add_total_lp / add_reserve_a;
    let add_lp_from_b = add_amount_b * add_total_lp / add_reserve_b;
    let add_lp_amount = add_lp_from_a.min(add_lp_from_b);
    let add_provider =
        Address::new_std_single(NetworkType::Devnet.into(), &[192; 32]).expect("AMM add liquidity provider address must be valid");
    let add_provider_lock = pay_to_acceptance_owner(&add_provider);
    let add_provider_hash = add_provider_lock.hash();
    let add_pool_id = add_pool_type.hash();
    let add_liquidity_witness = add_liquidity_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(add_provider_hash)])
        .expect("AMM add_liquidity witness must encode provider");
    let add_liquidity_output_capacity = add_liquidity_cell_capacity
        .checked_sub(required_fee(3, 2).saturating_add(100_000))
        .expect("AMM add_liquidity action must leave capacity")
        / 2;
    let add_liquidity_outputs = vec![
        CellOutput { capacity: add_liquidity_output_capacity, lock: add_liquidity_lock, type_: Some(add_pool_type) },
        CellOutput { capacity: add_liquidity_output_capacity, lock: add_provider_lock, type_: None },
    ];
    let add_liquidity_valid_output_data = vec![
        pool_cell_data(
            symbol_a,
            symbol_b,
            add_reserve_a + add_amount_a,
            add_reserve_b + add_amount_b,
            add_total_lp + add_lp_amount,
            fee_rate_bps,
        ),
        lp_receipt_cell_data(add_pool_id, add_lp_amount, add_provider_hash),
    ];
    let add_liquidity_malformed_output_data = vec![
        pool_cell_data(
            symbol_a,
            symbol_b,
            add_reserve_a + add_amount_a,
            add_reserve_b + add_amount_b,
            add_total_lp + add_lp_amount,
            fee_rate_bps,
        ),
        lp_receipt_cell_data(add_pool_id, add_lp_amount + 1, add_provider_hash),
    ];
    let add_liquidity_cell_deps = vec![
        CellDep {
            out_point: OutPoint::new(add_liquidity_code_outpoint.tx_hash, add_liquidity_code_outpoint.index),
            dep_type: DepType::Code,
        },
        CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        },
    ];
    let malformed_add_liquidity_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(add_pool_input.tx_hash, add_pool_input.index), 0),
                CellInput::new(OutPoint::new(add_token_a_input.tx_hash, add_token_a_input.index), 0),
                CellInput::new(OutPoint::new(add_token_b_input.tx_hash, add_token_b_input.index), 0),
            ],
            add_liquidity_cell_deps.clone(),
            add_liquidity_outputs.clone(),
            add_liquidity_malformed_output_data,
            vec![add_liquidity_witness.clone(), vec![]],
        )
        .expect("malformed AMM add_liquidity transaction must be structurally valid"),
        &add_liquidity_artifact.action,
        "malformed AMM add_liquidity transaction",
    );
    let malformed_add_liquidity_reason = rpc_client
        .submit_transaction((&malformed_add_liquidity_tx).into(), false)
        .await
        .expect_err("malformed AMM add_liquidity must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_add_liquidity_reason, "AMM add_liquidity");

    let valid_add_liquidity_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(add_pool_input.tx_hash, add_pool_input.index), 0),
                CellInput::new(OutPoint::new(add_token_a_input.tx_hash, add_token_a_input.index), 0),
                CellInput::new(OutPoint::new(add_token_b_input.tx_hash, add_token_b_input.index), 0),
            ],
            add_liquidity_cell_deps,
            add_liquidity_outputs,
            add_liquidity_valid_output_data,
            vec![add_liquidity_witness, vec![]],
        )
        .expect("valid AMM add_liquidity transaction must be structurally valid"),
        &add_liquidity_artifact.action,
        "valid AMM add_liquidity transaction",
    );
    let valid_add_liquidity_tx_id = spora_hashes::Hash::from_bytes(valid_add_liquidity_tx.id());
    rpc_client
        .submit_transaction((&valid_add_liquidity_tx).into(), false)
        .await
        .expect("valid AMM add_liquidity must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_add_liquidity_tx_id, "valid AMM add_liquidity action").await;

    coverage.valid.insert(("amm_pool.cell".to_string(), "add_liquidity".to_string()));
    coverage.malformed.insert(("amm_pool.cell".to_string(), "add_liquidity".to_string()));

    let remove_liquidity_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        remove_liquidity_artifact.artifact_bytes.len(),
        "AMM remove_liquidity",
    );
    let remove_liquidity_deploy_input_capacity = remove_liquidity_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let remove_liquidity_code_cell_capacity = remove_liquidity_deploy_input_capacity
        .checked_sub(required_fee(remove_liquidity_deploy_input.len(), 1).saturating_add(100_000))
        .expect("AMM remove_liquidity scoped action deployment must leave capacity for code cell");
    let remove_liquidity_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &remove_liquidity_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: remove_liquidity_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![remove_liquidity_artifact.artifact_bytes.clone()],
    );
    let remove_liquidity_deploy_tx_id = spora_hashes::Hash::from_bytes(remove_liquidity_deploy_tx.id());
    let remove_liquidity_code_outpoint = TransactionOutpoint::new(remove_liquidity_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&remove_liquidity_deploy_tx).into(), false)
        .await
        .expect("AMM remove_liquidity scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        remove_liquidity_deploy_tx_id,
        "AMM remove_liquidity scoped action deployment",
    )
    .await;

    let remove_liquidity_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "AMM remove_liquidity fixture");
    let remove_liquidity_fixture_input_capacity = remove_liquidity_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let remove_liquidity_cell_capacity = remove_liquidity_fixture_input_capacity / 5;
    let remove_liquidity_change_capacity = remove_liquidity_fixture_input_capacity
        .checked_sub(remove_liquidity_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(remove_liquidity_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("AMM remove_liquidity fixture transaction must leave change");
    let remove_liquidity_lock = Script::new(remove_liquidity_artifact.code_hash, 0, vec![]);
    let remove_pool_type = Script::new(always_success_code_hash(), 0, b"amm-remove-pool-type".to_vec());
    let remove_receipt_type = Script::new(always_success_code_hash(), 0, b"amm-remove-lp-receipt-type".to_vec());
    let remove_pool_id = remove_pool_type.hash();
    let remove_reserve_a = 1_000_u64;
    let remove_reserve_b = 2_000_u64;
    let remove_total_lp = 1_000_u64;
    let remove_lp_amount = 100_u64;
    let remove_provider =
        Address::new_std_single(NetworkType::Devnet.into(), &[193; 32]).expect("AMM remove liquidity provider address must be valid");
    let remove_provider_lock = pay_to_acceptance_owner(&remove_provider);
    let remove_provider_hash = remove_provider_lock.hash();
    let remove_liquidity_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &remove_liquidity_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: remove_liquidity_cell_capacity,
                lock: remove_liquidity_lock.clone(),
                type_: Some(remove_receipt_type),
            },
            CellOutput {
                capacity: remove_liquidity_cell_capacity,
                lock: remove_liquidity_lock.clone(),
                type_: Some(remove_pool_type.clone()),
            },
            CellOutput { capacity: remove_liquidity_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            lp_receipt_cell_data(remove_pool_id, remove_lp_amount, remove_provider_hash),
            pool_cell_data(symbol_a, symbol_b, remove_reserve_a, remove_reserve_b, remove_total_lp, fee_rate_bps),
            vec![],
        ],
    );
    let remove_liquidity_fixture_tx_id = spora_hashes::Hash::from_bytes(remove_liquidity_fixture_tx.id());
    let remove_receipt_input = TransactionOutpoint::new(remove_liquidity_fixture_tx.id(), 0);
    let remove_pool_input = TransactionOutpoint::new(remove_liquidity_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&remove_liquidity_fixture_tx).into(), false)
        .await
        .expect("AMM remove_liquidity fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, remove_liquidity_fixture_tx_id, "AMM remove_liquidity fixture cells")
        .await;

    let remove_amount_a = remove_lp_amount * remove_reserve_a / remove_total_lp;
    let remove_amount_b = remove_lp_amount * remove_reserve_b / remove_total_lp;
    let remove_liquidity_witness = remove_liquidity_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(remove_provider_hash)])
        .expect("AMM remove_liquidity witness must encode provider");
    let remove_liquidity_output_capacity = remove_liquidity_cell_capacity
        .checked_sub(required_fee(2, 3).saturating_add(100_000))
        .expect("AMM remove_liquidity action must leave capacity")
        / 3;
    let remove_liquidity_outputs = vec![
        CellOutput { capacity: remove_liquidity_output_capacity, lock: remove_liquidity_lock, type_: Some(remove_pool_type) },
        CellOutput { capacity: remove_liquidity_output_capacity, lock: remove_provider_lock.clone(), type_: None },
        CellOutput { capacity: remove_liquidity_output_capacity, lock: remove_provider_lock, type_: None },
    ];
    let remove_liquidity_valid_output_data = vec![
        pool_cell_data(
            symbol_a,
            symbol_b,
            remove_reserve_a - remove_amount_a,
            remove_reserve_b - remove_amount_b,
            remove_total_lp - remove_lp_amount,
            fee_rate_bps,
        ),
        token_cell_data(remove_amount_a, symbol_a),
        token_cell_data(remove_amount_b, symbol_b),
    ];
    let remove_liquidity_malformed_output_data = vec![
        pool_cell_data(
            symbol_a,
            symbol_b,
            remove_reserve_a - remove_amount_a,
            remove_reserve_b - remove_amount_b,
            remove_total_lp - remove_lp_amount,
            fee_rate_bps,
        ),
        token_cell_data(remove_amount_a + 1, symbol_a),
        token_cell_data(remove_amount_b, symbol_b),
    ];
    let remove_liquidity_cell_deps = vec![
        CellDep {
            out_point: OutPoint::new(remove_liquidity_code_outpoint.tx_hash, remove_liquidity_code_outpoint.index),
            dep_type: DepType::Code,
        },
        CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        },
    ];
    let malformed_remove_liquidity_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(remove_pool_input.tx_hash, remove_pool_input.index), 0),
                CellInput::new(OutPoint::new(remove_receipt_input.tx_hash, remove_receipt_input.index), 0),
            ],
            remove_liquidity_cell_deps.clone(),
            remove_liquidity_outputs.clone(),
            remove_liquidity_malformed_output_data,
            vec![remove_liquidity_witness.clone(), vec![]],
        )
        .expect("malformed AMM remove_liquidity transaction must be structurally valid"),
        &remove_liquidity_artifact.action,
        "malformed AMM remove_liquidity transaction",
    );
    let malformed_remove_liquidity_reason = rpc_client
        .submit_transaction((&malformed_remove_liquidity_tx).into(), false)
        .await
        .expect_err("malformed AMM remove_liquidity must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_remove_liquidity_reason, "AMM remove_liquidity");

    let valid_remove_liquidity_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(remove_pool_input.tx_hash, remove_pool_input.index), 0),
                CellInput::new(OutPoint::new(remove_receipt_input.tx_hash, remove_receipt_input.index), 0),
            ],
            remove_liquidity_cell_deps,
            remove_liquidity_outputs,
            remove_liquidity_valid_output_data,
            vec![remove_liquidity_witness, vec![]],
        )
        .expect("valid AMM remove_liquidity transaction must be structurally valid"),
        &remove_liquidity_artifact.action,
        "valid AMM remove_liquidity transaction",
    );
    let valid_remove_liquidity_tx_id = spora_hashes::Hash::from_bytes(valid_remove_liquidity_tx.id());
    rpc_client
        .submit_transaction((&valid_remove_liquidity_tx).into(), false)
        .await
        .expect("valid AMM remove_liquidity must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_remove_liquidity_tx_id, "valid AMM remove_liquidity action")
        .await;

    coverage.valid.insert(("amm_pool.cell".to_string(), "remove_liquidity".to_string()));
    coverage.malformed.insert(("amm_pool.cell".to_string(), "remove_liquidity".to_string()));

    let isqrt_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, isqrt_artifact.artifact_bytes.len(), "AMM isqrt");
    let isqrt_deploy_input_capacity = isqrt_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let isqrt_code_cell_capacity = isqrt_deploy_input_capacity
        .checked_sub(required_fee(isqrt_deploy_input.len(), 1).saturating_add(100_000))
        .expect("AMM isqrt scoped action deployment must leave capacity for code cell");
    let isqrt_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &isqrt_deploy_input,
        vec![],
        vec![CellOutput { capacity: isqrt_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![isqrt_artifact.artifact_bytes.clone()],
    );
    let isqrt_deploy_tx_id = spora_hashes::Hash::from_bytes(isqrt_deploy_tx.id());
    let isqrt_code_outpoint = TransactionOutpoint::new(isqrt_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&isqrt_deploy_tx).into(), false)
        .await
        .expect("AMM isqrt scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, isqrt_deploy_tx_id, "AMM isqrt scoped action deployment").await;

    let isqrt_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "AMM isqrt fixture");
    let isqrt_fixture_input_capacity = isqrt_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let isqrt_locked_capacity = isqrt_fixture_input_capacity
        .checked_sub(required_fee(isqrt_fixture_input.len(), 1).saturating_add(100_000))
        .expect("AMM isqrt fixture transaction must leave locked capacity");
    let isqrt_lock = Script::new(isqrt_artifact.code_hash, 0, vec![]);
    let isqrt_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &isqrt_fixture_input,
        vec![],
        vec![CellOutput { capacity: isqrt_locked_capacity, lock: isqrt_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let isqrt_fixture_tx_id = spora_hashes::Hash::from_bytes(isqrt_fixture_tx.id());
    let isqrt_input = TransactionOutpoint::new(isqrt_fixture_tx.id(), 0);
    rpc_client.submit_transaction((&isqrt_fixture_tx).into(), false).await.expect("AMM isqrt fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, isqrt_fixture_tx_id, "AMM isqrt fixture cell").await;

    let isqrt_valid_witness =
        isqrt_artifact.action.entry_witness_args(&[cellscript::EntryWitnessArg::U64(0)]).expect("AMM isqrt witness must encode n");
    let isqrt_malformed_witness = vec![0_u8];
    let isqrt_output_capacity = isqrt_locked_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("AMM isqrt action must leave output capacity");
    let isqrt_outputs =
        vec![CellOutput { capacity: isqrt_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }];
    let isqrt_cell_deps =
        vec![CellDep { out_point: OutPoint::new(isqrt_code_outpoint.tx_hash, isqrt_code_outpoint.index), dep_type: DepType::Code }];
    let malformed_isqrt_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(isqrt_input.tx_hash, isqrt_input.index), 0)],
            isqrt_cell_deps.clone(),
            isqrt_outputs.clone(),
            vec![vec![]],
            vec![isqrt_malformed_witness],
        )
        .expect("malformed AMM isqrt transaction must be structurally valid"),
        &isqrt_artifact.action,
        "malformed AMM isqrt transaction",
    );
    let malformed_isqrt_reason = rpc_client
        .submit_transaction((&malformed_isqrt_tx).into(), false)
        .await
        .expect_err("malformed AMM isqrt must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_isqrt_reason, "AMM isqrt");

    let valid_isqrt_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(isqrt_input.tx_hash, isqrt_input.index), 0)],
            isqrt_cell_deps,
            isqrt_outputs,
            vec![vec![]],
            vec![isqrt_valid_witness],
        )
        .expect("valid AMM isqrt transaction must be structurally valid"),
        &isqrt_artifact.action,
        "valid AMM isqrt transaction",
    );
    let valid_isqrt_tx_id = spora_hashes::Hash::from_bytes(valid_isqrt_tx.id());
    rpc_client
        .submit_transaction((&valid_isqrt_tx).into(), false)
        .await
        .expect("valid AMM isqrt must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_isqrt_tx_id, "valid AMM isqrt action").await;
    coverage.valid.insert(("amm_pool.cell".to_string(), "isqrt".to_string()));
    coverage.malformed.insert(("amm_pool.cell".to_string(), "isqrt".to_string()));

    let min_deploy_input = pop_code_deploy_cells(&mut spendable_cells, prealloc_address, min_artifact.artifact_bytes.len(), "AMM min");
    let min_deploy_input_capacity = min_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let min_code_cell_capacity = min_deploy_input_capacity
        .checked_sub(required_fee(min_deploy_input.len(), 1).saturating_add(100_000))
        .expect("AMM min scoped action deployment must leave capacity for code cell");
    let min_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &min_deploy_input,
        vec![],
        vec![CellOutput { capacity: min_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![min_artifact.artifact_bytes.clone()],
    );
    let min_deploy_tx_id = spora_hashes::Hash::from_bytes(min_deploy_tx.id());
    let min_code_outpoint = TransactionOutpoint::new(min_deploy_tx.id(), 0);
    rpc_client.submit_transaction((&min_deploy_tx).into(), false).await.expect("AMM min scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, min_deploy_tx_id, "AMM min scoped action deployment").await;

    let min_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "AMM min fixture");
    let min_fixture_input_capacity = min_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let min_locked_capacity = min_fixture_input_capacity
        .checked_sub(required_fee(min_fixture_input.len(), 1).saturating_add(100_000))
        .expect("AMM min fixture transaction must leave locked capacity");
    let min_lock = Script::new(min_artifact.code_hash, 0, vec![]);
    let min_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &min_fixture_input,
        vec![],
        vec![CellOutput { capacity: min_locked_capacity, lock: min_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let min_fixture_tx_id = spora_hashes::Hash::from_bytes(min_fixture_tx.id());
    let min_input = TransactionOutpoint::new(min_fixture_tx.id(), 0);
    rpc_client.submit_transaction((&min_fixture_tx).into(), false).await.expect("AMM min fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, min_fixture_tx_id, "AMM min fixture cell").await;

    let min_valid_witness = min_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::U64(7), cellscript::EntryWitnessArg::U64(0)])
        .expect("AMM min witness must encode a and b");
    let min_malformed_witness = vec![0_u8];
    let min_output_capacity = min_locked_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("AMM min action must leave output capacity");
    let min_outputs = vec![CellOutput { capacity: min_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }];
    let min_cell_deps =
        vec![CellDep { out_point: OutPoint::new(min_code_outpoint.tx_hash, min_code_outpoint.index), dep_type: DepType::Code }];
    let malformed_min_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(min_input.tx_hash, min_input.index), 0)],
            min_cell_deps.clone(),
            min_outputs.clone(),
            vec![vec![]],
            vec![min_malformed_witness],
        )
        .expect("malformed AMM min transaction must be structurally valid"),
        &min_artifact.action,
        "malformed AMM min transaction",
    );
    let malformed_min_reason = rpc_client
        .submit_transaction((&malformed_min_tx).into(), false)
        .await
        .expect_err("malformed AMM min must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_min_reason, "AMM min");

    let valid_min_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(min_input.tx_hash, min_input.index), 0)],
            min_cell_deps,
            min_outputs,
            vec![vec![]],
            vec![min_valid_witness],
        )
        .expect("valid AMM min transaction must be structurally valid"),
        &min_artifact.action,
        "valid AMM min transaction",
    );
    let valid_min_tx_id = spora_hashes::Hash::from_bytes(valid_min_tx.id());
    rpc_client
        .submit_transaction((&valid_min_tx).into(), false)
        .await
        .expect("valid AMM min must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_min_tx_id, "valid AMM min action").await;
    coverage.valid.insert(("amm_pool.cell".to_string(), "min".to_string()));
    coverage.malformed.insert(("amm_pool.cell".to_string(), "min".to_string()));
    coverage
}

async fn run_nft_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(nft_deployment) = deployments.iter().find(|deployment| deployment.name == "nft.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(mint_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "mint") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(batch_mint_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "batch_mint") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    assert!(
        batch_mint_artifact.action.estimated_cycles >= 30_000,
        "NFT batch_mint scoped action artifact must carry the production cycle estimate"
    );
    let Some(transfer_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "transfer") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(create_listing_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "create_listing")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(cancel_listing_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "cancel_listing")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(buy_from_listing_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "buy_from_listing")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(create_offer_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "create_offer") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(accept_offer_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "accept_offer") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(burn_artifact) = nft_deployment.action_artifacts.iter().find(|artifact| artifact.name == "burn") else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 18,
        "NFT action builder matrix needs eighteen matured plain prealloc cells for scoped deploys and fixtures"
    );

    let transfer_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, transfer_artifact.artifact_bytes.len(), "NFT transfer");
    let transfer_deploy_input_capacity = transfer_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let transfer_code_cell_capacity = transfer_deploy_input_capacity
        .checked_sub(required_fee(transfer_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT transfer scoped action deployment must leave capacity for code cell");
    let transfer_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &transfer_deploy_input,
        vec![],
        vec![CellOutput { capacity: transfer_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![transfer_artifact.artifact_bytes.clone()],
    );
    let transfer_deploy_tx_id = spora_hashes::Hash::from_bytes(transfer_deploy_tx.id());
    let transfer_code_outpoint = TransactionOutpoint::new(transfer_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&transfer_deploy_tx).into(), false)
        .await
        .expect("NFT transfer scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, transfer_deploy_tx_id, "NFT transfer scoped action deployment").await;

    let transfer_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT transfer fixture");
    let transfer_fixture_input_capacity = transfer_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let nft_cell_capacity = transfer_fixture_input_capacity / 4;
    let transfer_fixture_change_capacity = transfer_fixture_input_capacity
        .checked_sub(nft_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(transfer_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("NFT transfer fixture transaction must leave change");
    let transfer_lock = Script::new(transfer_artifact.code_hash, 0, vec![]);
    let transfer_type = Script::new(always_success_code_hash(), 0, b"nft-transfer-type".to_vec());
    let original_owner = [71; 32];
    let metadata_hash = [72; 32];
    let royalty_recipient = [73; 32];
    let transfer_fixture_data = nft_cell_data(1, original_owner, metadata_hash, royalty_recipient, 250);
    let transfer_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &transfer_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: nft_cell_capacity, lock: transfer_lock.clone(), type_: Some(transfer_type.clone()) },
            CellOutput { capacity: nft_cell_capacity, lock: transfer_lock.clone(), type_: Some(transfer_type.clone()) },
            CellOutput { capacity: transfer_fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![transfer_fixture_data.clone(), transfer_fixture_data, vec![]],
    );
    let transfer_fixture_tx_id = spora_hashes::Hash::from_bytes(transfer_fixture_tx.id());
    let malformed_transfer_input = TransactionOutpoint::new(transfer_fixture_tx.id(), 0);
    let valid_transfer_input = TransactionOutpoint::new(transfer_fixture_tx.id(), 1);
    rpc_client.submit_transaction((&transfer_fixture_tx).into(), false).await.expect("NFT transfer fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, transfer_fixture_tx_id, "NFT transfer fixture cells").await;

    let new_owner = [74; 32];
    let wrong_owner = [75; 32];
    let transfer_witness = transfer_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(new_owner)])
        .expect("NFT transfer action witness must encode new owner");
    let transfer_spend_capacity =
        nft_cell_capacity.checked_sub(required_fee(1, 1).saturating_add(100_000)).expect("NFT transfer action must leave fee");
    let malformed_transfer_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_transfer_input.tx_hash, malformed_transfer_input.index), 0)],
        vec![
            CellDep {
                out_point: OutPoint::new(transfer_code_outpoint.tx_hash, transfer_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: transfer_spend_capacity, lock: transfer_lock.clone(), type_: Some(transfer_type.clone()) }],
        vec![nft_cell_data(1, wrong_owner, metadata_hash, royalty_recipient, 250)],
        vec![transfer_witness.clone()],
    )
    .expect("malformed NFT transfer transaction must be structurally valid");
    let malformed_transfer_reason = rpc_client
        .submit_transaction((&malformed_transfer_tx).into(), false)
        .await
        .expect_err("malformed NFT transfer must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_transfer_reason.contains("not standard")
            && !malformed_transfer_reason.contains("storage mass")
            && !malformed_transfer_reason.contains("compute mass")
            && !malformed_transfer_reason.contains("transient")
            && !malformed_transfer_reason.contains("cycles exceeded")
            && !malformed_transfer_reason.contains("cycles limit"),
        "malformed NFT transfer must fail in script validation, not policy or mass: {malformed_transfer_reason}"
    );

    let valid_transfer_data = nft_cell_data(1, new_owner, metadata_hash, royalty_recipient, 250);
    let valid_transfer_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_transfer_input.tx_hash, valid_transfer_input.index), 0)],
        vec![
            CellDep {
                out_point: OutPoint::new(transfer_code_outpoint.tx_hash, transfer_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: transfer_spend_capacity, lock: transfer_lock, type_: Some(transfer_type) }],
        vec![valid_transfer_data],
        vec![transfer_witness],
    )
    .expect("valid NFT transfer transaction must be structurally valid");
    let valid_transfer_tx_id = spora_hashes::Hash::from_bytes(valid_transfer_tx.id());
    rpc_client
        .submit_transaction((&valid_transfer_tx).into(), false)
        .await
        .expect("valid NFT transfer must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_transfer_tx_id, "valid NFT transfer action").await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    coverage.valid.insert(("nft.cell".to_string(), "transfer".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "transfer".to_string()));

    let burn_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, burn_artifact.artifact_bytes.len(), "NFT burn");
    let burn_deploy_input_capacity = burn_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let burn_code_cell_capacity = burn_deploy_input_capacity
        .checked_sub(required_fee(burn_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT burn scoped action deployment must leave capacity for code cell");
    let burn_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &burn_deploy_input,
        vec![],
        vec![CellOutput { capacity: burn_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![burn_artifact.artifact_bytes.clone()],
    );
    let burn_deploy_tx_id = spora_hashes::Hash::from_bytes(burn_deploy_tx.id());
    let burn_code_outpoint = TransactionOutpoint::new(burn_deploy_tx.id(), 0);
    rpc_client.submit_transaction((&burn_deploy_tx).into(), false).await.expect("NFT burn scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, burn_deploy_tx_id, "NFT burn scoped action deployment").await;

    let burn_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT burn fixture");
    let burn_fixture_input_capacity = burn_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let burn_nft_cell_capacity = burn_fixture_input_capacity / 4;
    let burn_fixture_change_capacity = burn_fixture_input_capacity
        .checked_sub(burn_nft_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(burn_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("NFT burn fixture transaction must leave change");
    let burn_lock = Script::new(burn_artifact.code_hash, 0, vec![]);
    let burn_type = Script::new(always_success_code_hash(), 0, b"nft-burn-type".to_vec());
    let burn_fixture_data = nft_cell_data(2, [81; 32], [82; 32], [83; 32], 250);
    let burn_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &burn_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: burn_nft_cell_capacity, lock: burn_lock.clone(), type_: Some(burn_type.clone()) },
            CellOutput { capacity: burn_nft_cell_capacity, lock: burn_lock.clone(), type_: Some(burn_type.clone()) },
            CellOutput { capacity: burn_fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![burn_fixture_data.clone(), burn_fixture_data.clone(), vec![]],
    );
    let burn_fixture_tx_id = spora_hashes::Hash::from_bytes(burn_fixture_tx.id());
    let malformed_burn_input = TransactionOutpoint::new(burn_fixture_tx.id(), 0);
    let valid_burn_input = TransactionOutpoint::new(burn_fixture_tx.id(), 1);
    rpc_client.submit_transaction((&burn_fixture_tx).into(), false).await.expect("NFT burn fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, burn_fixture_tx_id, "NFT burn fixture cells").await;

    let burn_witness = burn_artifact.action.entry_witness_args(&[]).expect("NFT burn action witness must encode");
    let burn_change_capacity =
        burn_nft_cell_capacity.checked_sub(required_fee(1, 1).saturating_add(100_000)).expect("NFT burn action must leave change");
    let malformed_burn_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_burn_input.tx_hash, malformed_burn_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(burn_code_outpoint.tx_hash, burn_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: burn_change_capacity, lock: burn_lock.clone(), type_: Some(burn_type.clone()) }],
        vec![burn_fixture_data],
        vec![burn_witness.clone()],
    )
    .expect("malformed NFT burn transaction must be structurally valid");
    let malformed_burn_reason = rpc_client
        .submit_transaction((&malformed_burn_tx).into(), false)
        .await
        .expect_err("malformed NFT burn must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_burn_reason.contains("not standard")
            && !malformed_burn_reason.contains("storage mass")
            && !malformed_burn_reason.contains("compute mass")
            && !malformed_burn_reason.contains("transient")
            && !malformed_burn_reason.contains("cycles exceeded")
            && !malformed_burn_reason.contains("cycles limit"),
        "malformed NFT burn must fail in script validation, not policy or mass: {malformed_burn_reason}"
    );

    let valid_burn_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_burn_input.tx_hash, valid_burn_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(burn_code_outpoint.tx_hash, burn_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: burn_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![vec![]],
        vec![burn_witness],
    )
    .expect("valid NFT burn transaction must be structurally valid");
    let valid_burn_tx_id = spora_hashes::Hash::from_bytes(valid_burn_tx.id());
    rpc_client
        .submit_transaction((&valid_burn_tx).into(), false)
        .await
        .expect("valid NFT burn must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_burn_tx_id, "valid NFT burn action").await;

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client
                        .get_cells_by_addresses(vec![address])
                        .await
                        .unwrap()
                        .iter()
                        .any(|cell| cell.outpoint.transaction_id == valid_burn_tx_id && cell.cell_entry.data_bytes == 0)
                }
            }
        },
        "change cell from the NFT burn action builder was not indexed",
    )
    .await;

    coverage.valid.insert(("nft.cell".to_string(), "burn".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "burn".to_string()));

    let create_listing_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        create_listing_artifact.artifact_bytes.len(),
        "NFT create_listing",
    );
    let create_listing_deploy_input_capacity = create_listing_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let create_listing_code_cell_capacity = create_listing_deploy_input_capacity
        .checked_sub(required_fee(create_listing_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT create_listing scoped action deployment must leave capacity for code cell");
    let create_listing_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &create_listing_deploy_input,
        vec![],
        vec![CellOutput { capacity: create_listing_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![create_listing_artifact.artifact_bytes.clone()],
    );
    let create_listing_deploy_tx_id = spora_hashes::Hash::from_bytes(create_listing_deploy_tx.id());
    let create_listing_code_outpoint = TransactionOutpoint::new(create_listing_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&create_listing_deploy_tx).into(), false)
        .await
        .expect("NFT create_listing scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        create_listing_deploy_tx_id,
        "NFT create_listing scoped action deployment",
    )
    .await;

    let create_listing_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT create_listing fixture");
    let create_listing_fixture_input_capacity = create_listing_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let create_listing_nft_cell_capacity = create_listing_fixture_input_capacity / 4;
    let create_listing_fixture_change_capacity = create_listing_fixture_input_capacity
        .checked_sub(create_listing_nft_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(create_listing_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("NFT create_listing fixture transaction must leave change");
    let create_listing_lock = Script::new(create_listing_artifact.code_hash, 0, vec![]);
    let create_listing_type = Script::new(always_success_code_hash(), 0, b"nft-create-listing-type".to_vec());
    let listing_owner = [91; 32];
    let listing_nft_data = nft_cell_data(9, listing_owner, [92; 32], [93; 32], 250);
    let create_listing_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &create_listing_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: create_listing_nft_cell_capacity,
                lock: create_listing_lock.clone(),
                type_: Some(create_listing_type.clone()),
            },
            CellOutput {
                capacity: create_listing_nft_cell_capacity,
                lock: create_listing_lock.clone(),
                type_: Some(create_listing_type.clone()),
            },
            CellOutput {
                capacity: create_listing_fixture_change_capacity,
                lock: pay_to_acceptance_owner(prealloc_address),
                type_: None,
            },
        ],
        vec![listing_nft_data.clone(), listing_nft_data, vec![]],
    );
    let create_listing_fixture_tx_id = spora_hashes::Hash::from_bytes(create_listing_fixture_tx.id());
    let malformed_create_listing_input = TransactionOutpoint::new(create_listing_fixture_tx.id(), 0);
    let valid_create_listing_input = TransactionOutpoint::new(create_listing_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&create_listing_fixture_tx).into(), false)
        .await
        .expect("NFT create_listing fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, create_listing_fixture_tx_id, "NFT create_listing fixture cells").await;

    let listing_price = 700;
    let listing_created_at = 123_456;
    let create_listing_witness = create_listing_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::U64(listing_price), cellscript::EntryWitnessArg::U64(listing_created_at)])
        .expect("NFT create_listing action witness must encode price and current time");
    let create_listing_output_capacity = create_listing_nft_cell_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("NFT create_listing action must leave fee");
    let malformed_listing_data = listing_cell_data(9, listing_owner, listing_price + 1, listing_created_at);
    let malformed_create_listing_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_create_listing_input.tx_hash, malformed_create_listing_input.index), 0)],
        vec![
            CellDep {
                out_point: OutPoint::new(malformed_create_listing_input.tx_hash, malformed_create_listing_input.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(create_listing_code_outpoint.tx_hash, create_listing_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: create_listing_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![malformed_listing_data],
        vec![create_listing_witness.clone()],
    )
    .expect("malformed NFT create_listing transaction must be structurally valid");
    let malformed_create_listing_reason = rpc_client
        .submit_transaction((&malformed_create_listing_tx).into(), false)
        .await
        .expect_err("malformed NFT create_listing must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_create_listing_reason.contains("not standard")
            && !malformed_create_listing_reason.contains("storage mass")
            && !malformed_create_listing_reason.contains("compute mass")
            && !malformed_create_listing_reason.contains("transient")
            && !malformed_create_listing_reason.contains("cycles exceeded")
            && !malformed_create_listing_reason.contains("cycles limit"),
        "malformed NFT create_listing must fail in script validation, not policy or mass: {malformed_create_listing_reason}"
    );

    let valid_listing_data = listing_cell_data(9, listing_owner, listing_price, listing_created_at);
    let valid_create_listing_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_create_listing_input.tx_hash, valid_create_listing_input.index), 0)],
        vec![
            CellDep {
                out_point: OutPoint::new(valid_create_listing_input.tx_hash, valid_create_listing_input.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(create_listing_code_outpoint.tx_hash, create_listing_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: create_listing_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![valid_listing_data.clone()],
        vec![create_listing_witness],
    )
    .expect("valid NFT create_listing transaction must be structurally valid");
    let valid_create_listing_tx_id = spora_hashes::Hash::from_bytes(valid_create_listing_tx.id());
    rpc_client
        .submit_transaction((&valid_create_listing_tx).into(), false)
        .await
        .expect("valid NFT create_listing must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_create_listing_tx_id, "valid NFT create_listing action").await;

    let valid_listing_data_hash = spora_cell_data_hash(&valid_listing_data);
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_create_listing_tx_id
                            && cell.cell_entry.data_bytes == 57
                            && cell.cell_entry.data_hash == valid_listing_data_hash
                    })
                }
            }
        },
        "listing cell from the NFT create_listing action builder was not indexed",
    )
    .await;

    coverage.valid.insert(("nft.cell".to_string(), "create_listing".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "create_listing".to_string()));

    let cancel_listing_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        cancel_listing_artifact.artifact_bytes.len(),
        "NFT cancel_listing",
    );
    let cancel_listing_deploy_input_capacity = cancel_listing_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let cancel_listing_code_cell_capacity = cancel_listing_deploy_input_capacity
        .checked_sub(required_fee(cancel_listing_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT cancel_listing scoped action deployment must leave capacity for code cell");
    let cancel_listing_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &cancel_listing_deploy_input,
        vec![],
        vec![CellOutput { capacity: cancel_listing_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![cancel_listing_artifact.artifact_bytes.clone()],
    );
    let cancel_listing_deploy_tx_id = spora_hashes::Hash::from_bytes(cancel_listing_deploy_tx.id());
    let cancel_listing_code_outpoint = TransactionOutpoint::new(cancel_listing_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&cancel_listing_deploy_tx).into(), false)
        .await
        .expect("NFT cancel_listing scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        cancel_listing_deploy_tx_id,
        "NFT cancel_listing scoped action deployment",
    )
    .await;

    let cancel_listing_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT cancel_listing fixture");
    let cancel_listing_fixture_input_capacity = cancel_listing_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let listing_cell_capacity = cancel_listing_fixture_input_capacity / 4;
    let cancel_listing_fixture_change_capacity = cancel_listing_fixture_input_capacity
        .checked_sub(listing_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(cancel_listing_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("NFT cancel_listing fixture transaction must leave change");
    let cancel_listing_lock = Script::new(cancel_listing_artifact.code_hash, 0, vec![]);
    let cancel_listing_type = Script::new(always_success_code_hash(), 0, b"nft-cancel-listing-type".to_vec());
    let cancel_listing_data = listing_cell_data(10, [94; 32], 900, 123_999);
    let cancel_listing_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &cancel_listing_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: listing_cell_capacity,
                lock: cancel_listing_lock.clone(),
                type_: Some(cancel_listing_type.clone()),
            },
            CellOutput {
                capacity: listing_cell_capacity,
                lock: cancel_listing_lock.clone(),
                type_: Some(cancel_listing_type.clone()),
            },
            CellOutput {
                capacity: cancel_listing_fixture_change_capacity,
                lock: pay_to_acceptance_owner(prealloc_address),
                type_: None,
            },
        ],
        vec![cancel_listing_data.clone(), cancel_listing_data.clone(), vec![]],
    );
    let cancel_listing_fixture_tx_id = spora_hashes::Hash::from_bytes(cancel_listing_fixture_tx.id());
    let malformed_cancel_listing_input = TransactionOutpoint::new(cancel_listing_fixture_tx.id(), 0);
    let valid_cancel_listing_input = TransactionOutpoint::new(cancel_listing_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&cancel_listing_fixture_tx).into(), false)
        .await
        .expect("NFT cancel_listing fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, cancel_listing_fixture_tx_id, "NFT cancel_listing fixture cells").await;

    let cancel_listing_witness =
        cancel_listing_artifact.action.entry_witness_args(&[]).expect("NFT cancel_listing witness must encode");
    let cancel_listing_change_capacity = listing_cell_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("NFT cancel_listing action must leave fee");
    let malformed_cancel_listing_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_cancel_listing_input.tx_hash, malformed_cancel_listing_input.index), 0)],
        vec![
            CellDep {
                out_point: OutPoint::new(cancel_listing_code_outpoint.tx_hash, cancel_listing_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput {
            capacity: cancel_listing_change_capacity,
            lock: cancel_listing_lock.clone(),
            type_: Some(cancel_listing_type.clone()),
        }],
        vec![cancel_listing_data],
        vec![cancel_listing_witness.clone()],
    )
    .expect("malformed NFT cancel_listing transaction must be structurally valid");
    let malformed_cancel_listing_reason = rpc_client
        .submit_transaction((&malformed_cancel_listing_tx).into(), false)
        .await
        .expect_err("malformed NFT cancel_listing must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_cancel_listing_reason.contains("not standard")
            && !malformed_cancel_listing_reason.contains("storage mass")
            && !malformed_cancel_listing_reason.contains("compute mass")
            && !malformed_cancel_listing_reason.contains("transient")
            && !malformed_cancel_listing_reason.contains("cycles exceeded")
            && !malformed_cancel_listing_reason.contains("cycles limit"),
        "malformed NFT cancel_listing must fail in script validation, not policy or mass: {malformed_cancel_listing_reason}"
    );

    let valid_cancel_listing_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_cancel_listing_input.tx_hash, valid_cancel_listing_input.index), 0)],
        vec![
            CellDep {
                out_point: OutPoint::new(cancel_listing_code_outpoint.tx_hash, cancel_listing_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![CellOutput { capacity: cancel_listing_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![vec![]],
        vec![cancel_listing_witness],
    )
    .expect("valid NFT cancel_listing transaction must be structurally valid");
    let valid_cancel_listing_tx_id = spora_hashes::Hash::from_bytes(valid_cancel_listing_tx.id());
    rpc_client
        .submit_transaction((&valid_cancel_listing_tx).into(), false)
        .await
        .expect("valid NFT cancel_listing must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_cancel_listing_tx_id, "valid NFT cancel_listing action").await;

    coverage.valid.insert(("nft.cell".to_string(), "cancel_listing".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "cancel_listing".to_string()));

    let create_offer_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, create_offer_artifact.artifact_bytes.len(), "NFT create_offer");
    let create_offer_deploy_input_capacity = create_offer_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let create_offer_code_cell_capacity = create_offer_deploy_input_capacity
        .checked_sub(required_fee(create_offer_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT create_offer scoped action deployment must leave capacity for code cell");
    let create_offer_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &create_offer_deploy_input,
        vec![],
        vec![CellOutput { capacity: create_offer_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![create_offer_artifact.artifact_bytes.clone()],
    );
    let create_offer_deploy_tx_id = spora_hashes::Hash::from_bytes(create_offer_deploy_tx.id());
    let create_offer_code_outpoint = TransactionOutpoint::new(create_offer_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&create_offer_deploy_tx).into(), false)
        .await
        .expect("NFT create_offer scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, create_offer_deploy_tx_id, "NFT create_offer scoped action deployment")
        .await;

    let create_offer_seed_input = pop_plain_cells(&mut spendable_cells, 1, "NFT create_offer seed");
    let create_offer_seed_input_capacity = create_offer_seed_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let offer_seed_capacity = create_offer_seed_input_capacity / 4;
    let create_offer_seed_change_capacity = create_offer_seed_input_capacity
        .checked_sub(offer_seed_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(create_offer_seed_input.len(), 3).saturating_add(100_000)))
        .expect("NFT create_offer seed transaction must leave change");
    let create_offer_lock = Script::new(create_offer_artifact.code_hash, 0, vec![]);
    let create_offer_seed_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &create_offer_seed_input,
        vec![],
        vec![
            CellOutput { capacity: offer_seed_capacity, lock: create_offer_lock.clone(), type_: None },
            CellOutput { capacity: offer_seed_capacity, lock: create_offer_lock.clone(), type_: None },
            CellOutput { capacity: create_offer_seed_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![vec![], vec![], vec![]],
    );
    let create_offer_seed_tx_id = spora_hashes::Hash::from_bytes(create_offer_seed_tx.id());
    let malformed_create_offer_input = TransactionOutpoint::new(create_offer_seed_tx.id(), 0);
    let valid_create_offer_input = TransactionOutpoint::new(create_offer_seed_tx.id(), 1);
    rpc_client.submit_transaction((&create_offer_seed_tx).into(), false).await.expect("NFT create_offer seed cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, create_offer_seed_tx_id, "NFT create_offer seed cells").await;

    let offer_token_id = 11;
    let offer_buyer = [101; 32];
    let offer_price = 1_250;
    let offer_expires_at = 222_222;
    let create_offer_witness = create_offer_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::U64(offer_token_id),
            cellscript::EntryWitnessArg::Address(offer_buyer),
            cellscript::EntryWitnessArg::U64(offer_price),
            cellscript::EntryWitnessArg::U64(offer_expires_at),
        ])
        .expect("NFT create_offer action witness must encode token, buyer, price, and expiration");
    let create_offer_output_capacity =
        offer_seed_capacity.checked_sub(required_fee(1, 1).saturating_add(100_000)).expect("NFT create_offer action must leave fee");
    let malformed_offer_data = offer_cell_data(offer_token_id, offer_buyer, offer_price + 1, offer_expires_at);
    let malformed_create_offer_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_create_offer_input.tx_hash, malformed_create_offer_input.index), 0)],
        vec![CellDep {
            out_point: OutPoint::new(create_offer_code_outpoint.tx_hash, create_offer_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![CellOutput { capacity: create_offer_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![malformed_offer_data],
        vec![create_offer_witness.clone()],
    )
    .expect("malformed NFT create_offer transaction must be structurally valid");
    let malformed_create_offer_reason = rpc_client
        .submit_transaction((&malformed_create_offer_tx).into(), false)
        .await
        .expect_err("malformed NFT create_offer must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_create_offer_reason.contains("not standard")
            && !malformed_create_offer_reason.contains("storage mass")
            && !malformed_create_offer_reason.contains("compute mass")
            && !malformed_create_offer_reason.contains("transient")
            && !malformed_create_offer_reason.contains("cycles exceeded")
            && !malformed_create_offer_reason.contains("cycles limit"),
        "malformed NFT create_offer must fail in script validation, not policy or mass: {malformed_create_offer_reason}"
    );

    let valid_offer_data = offer_cell_data(offer_token_id, offer_buyer, offer_price, offer_expires_at);
    let valid_create_offer_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_create_offer_input.tx_hash, valid_create_offer_input.index), 0)],
        vec![CellDep {
            out_point: OutPoint::new(create_offer_code_outpoint.tx_hash, create_offer_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![CellOutput { capacity: create_offer_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![valid_offer_data.clone()],
        vec![create_offer_witness],
    )
    .expect("valid NFT create_offer transaction must be structurally valid");
    let valid_create_offer_tx_id = spora_hashes::Hash::from_bytes(valid_create_offer_tx.id());
    rpc_client
        .submit_transaction((&valid_create_offer_tx).into(), false)
        .await
        .expect("valid NFT create_offer must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_create_offer_tx_id, "valid NFT create_offer action").await;

    let valid_offer_data_hash = spora_cell_data_hash(&valid_offer_data);
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_create_offer_tx_id
                            && cell.cell_entry.data_bytes == 57
                            && cell.cell_entry.data_hash == valid_offer_data_hash
                    })
                }
            }
        },
        "offer cell from the NFT create_offer action builder was not indexed",
    )
    .await;

    coverage.valid.insert(("nft.cell".to_string(), "create_offer".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "create_offer".to_string()));

    let buy_from_listing_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        buy_from_listing_artifact.artifact_bytes.len(),
        "NFT buy_from_listing",
    );
    let buy_from_listing_deploy_input_capacity = buy_from_listing_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let buy_from_listing_code_cell_capacity = buy_from_listing_deploy_input_capacity
        .checked_sub(required_fee(buy_from_listing_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT buy_from_listing scoped action deployment must leave capacity for code cell");
    let buy_from_listing_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &buy_from_listing_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: buy_from_listing_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![buy_from_listing_artifact.artifact_bytes.clone()],
    );
    let buy_from_listing_deploy_tx_id = spora_hashes::Hash::from_bytes(buy_from_listing_deploy_tx.id());
    let buy_from_listing_code_outpoint = TransactionOutpoint::new(buy_from_listing_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&buy_from_listing_deploy_tx).into(), false)
        .await
        .expect("NFT buy_from_listing scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        buy_from_listing_deploy_tx_id,
        "NFT buy_from_listing scoped action deployment",
    )
    .await;

    let buy_from_listing_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT buy_from_listing fixture");
    let buy_from_listing_fixture_input_capacity = buy_from_listing_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let buy_state_cell_capacity = buy_from_listing_fixture_input_capacity / 6;
    let buy_from_listing_fixture_change_capacity = buy_from_listing_fixture_input_capacity
        .checked_sub(buy_state_cell_capacity.saturating_mul(4))
        .and_then(|value| value.checked_sub(required_fee(buy_from_listing_fixture_input.len(), 5).saturating_add(100_000)))
        .expect("NFT buy_from_listing fixture transaction must leave change");
    let buy_from_listing_lock = Script::new(buy_from_listing_artifact.code_hash, 0, vec![]);
    let buy_listing_type = Script::new(always_success_code_hash(), 0, b"nft-buy-listing-type".to_vec());
    let buy_nft_type = Script::new(always_success_code_hash(), 0, b"nft-buy-nft-type".to_vec());
    let buy_listing_seller = [111; 32];
    let buy_buyer = [112; 32];
    let buy_royalty_recipient = [113; 32];
    let buy_listing_price = 20_000;
    let buy_listing_created_at = 333_333;
    let buy_payment = 20_000;
    let buy_royalty_amount = buy_payment * 250 / 10_000;
    let buy_seller_amount = buy_payment - buy_royalty_amount;
    let buy_listing_data = listing_cell_data(12, buy_listing_seller, buy_listing_price, buy_listing_created_at);
    let buy_nft_data = nft_cell_data(12, buy_listing_seller, [114; 32], buy_royalty_recipient, 250);
    let buy_from_listing_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &buy_from_listing_fixture_input,
        vec![
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(buy_from_listing_code_outpoint.tx_hash, buy_from_listing_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput {
                capacity: buy_state_cell_capacity,
                lock: buy_from_listing_lock.clone(),
                type_: Some(buy_listing_type.clone()),
            },
            CellOutput { capacity: buy_state_cell_capacity, lock: buy_from_listing_lock.clone(), type_: Some(buy_nft_type.clone()) },
            CellOutput {
                capacity: buy_state_cell_capacity,
                lock: buy_from_listing_lock.clone(),
                type_: Some(buy_listing_type.clone()),
            },
            CellOutput { capacity: buy_state_cell_capacity, lock: buy_from_listing_lock.clone(), type_: Some(buy_nft_type.clone()) },
            CellOutput {
                capacity: buy_from_listing_fixture_change_capacity,
                lock: pay_to_acceptance_owner(prealloc_address),
                type_: None,
            },
        ],
        vec![buy_listing_data.clone(), buy_nft_data.clone(), buy_listing_data.clone(), buy_nft_data.clone(), vec![]],
    );
    let buy_from_listing_fixture_tx_id = spora_hashes::Hash::from_bytes(buy_from_listing_fixture_tx.id());
    let malformed_buy_listing_input = TransactionOutpoint::new(buy_from_listing_fixture_tx.id(), 0);
    let malformed_buy_nft_input = TransactionOutpoint::new(buy_from_listing_fixture_tx.id(), 1);
    let valid_buy_listing_input = TransactionOutpoint::new(buy_from_listing_fixture_tx.id(), 2);
    let valid_buy_nft_input = TransactionOutpoint::new(buy_from_listing_fixture_tx.id(), 3);
    rpc_client
        .submit_transaction((&buy_from_listing_fixture_tx).into(), false)
        .await
        .expect("NFT buy_from_listing fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, buy_from_listing_fixture_tx_id, "NFT buy_from_listing fixture cells")
        .await;

    let buy_from_listing_witness = buy_from_listing_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(buy_buyer),
            cellscript::EntryWitnessArg::Address(buy_listing_seller),
            cellscript::EntryWitnessArg::U64(buy_payment),
        ])
        .expect("NFT buy_from_listing action witness must encode buyer, seller, and payment");
    let buy_output_capacity = buy_state_cell_capacity
        .checked_sub(required_fee(2, 3).saturating_add(100_000))
        .expect("NFT buy_from_listing action must leave fee")
        / 3;
    let malformed_buy_tx = CellTx::new(
        vec![
            CellInput::new(OutPoint::new(malformed_buy_nft_input.tx_hash, malformed_buy_nft_input.index), 0),
            CellInput::new(OutPoint::new(malformed_buy_listing_input.tx_hash, malformed_buy_listing_input.index), 0),
        ],
        vec![
            CellDep {
                out_point: OutPoint::new(buy_from_listing_code_outpoint.tx_hash, buy_from_listing_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: buy_output_capacity, lock: buy_from_listing_lock.clone(), type_: Some(buy_nft_type.clone()) },
            CellOutput { capacity: buy_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
            CellOutput { capacity: buy_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            nft_cell_data(12, buy_buyer, [114; 32], buy_royalty_recipient, 250),
            royalty_payment_cell_data(12, buy_royalty_recipient, buy_royalty_amount),
            royalty_payment_cell_data(12, buy_listing_seller, buy_seller_amount + 1),
        ],
        vec![buy_from_listing_witness.clone()],
    )
    .expect("malformed NFT buy_from_listing transaction must be structurally valid");
    let malformed_buy_reason = rpc_client
        .submit_transaction((&malformed_buy_tx).into(), false)
        .await
        .expect_err("malformed NFT buy_from_listing must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_buy_reason.contains("not standard")
            && !malformed_buy_reason.contains("storage mass")
            && !malformed_buy_reason.contains("compute mass")
            && !malformed_buy_reason.contains("transient")
            && !malformed_buy_reason.contains("cycles exceeded")
            && !malformed_buy_reason.contains("cycles limit"),
        "malformed NFT buy_from_listing must fail in script validation, not policy or mass: {malformed_buy_reason}"
    );

    let valid_bought_nft_data = nft_cell_data(12, buy_buyer, [114; 32], buy_royalty_recipient, 250);
    let valid_buy_tx = CellTx::new(
        vec![
            CellInput::new(OutPoint::new(valid_buy_nft_input.tx_hash, valid_buy_nft_input.index), 0),
            CellInput::new(OutPoint::new(valid_buy_listing_input.tx_hash, valid_buy_listing_input.index), 0),
        ],
        vec![
            CellDep {
                out_point: OutPoint::new(buy_from_listing_code_outpoint.tx_hash, buy_from_listing_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: buy_output_capacity, lock: buy_from_listing_lock, type_: Some(buy_nft_type) },
            CellOutput { capacity: buy_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
            CellOutput { capacity: buy_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            valid_bought_nft_data.clone(),
            royalty_payment_cell_data(12, buy_royalty_recipient, buy_royalty_amount),
            royalty_payment_cell_data(12, buy_listing_seller, buy_seller_amount),
        ],
        vec![buy_from_listing_witness],
    )
    .expect("valid NFT buy_from_listing transaction must be structurally valid");
    let valid_buy_tx_id = spora_hashes::Hash::from_bytes(valid_buy_tx.id());
    rpc_client
        .submit_transaction((&valid_buy_tx).into(), false)
        .await
        .expect("valid NFT buy_from_listing must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_buy_tx_id, "valid NFT buy_from_listing action").await;

    let valid_royalty_payment_hash = spora_cell_data_hash(&royalty_payment_cell_data(12, buy_royalty_recipient, buy_royalty_amount));
    let valid_seller_payment_hash = spora_cell_data_hash(&royalty_payment_cell_data(12, buy_listing_seller, buy_seller_amount));
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    let cells = client.get_cells_by_addresses(vec![address]).await.unwrap();
                    cells.iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_buy_tx_id
                            && cell.cell_entry.data_bytes == 48
                            && cell.cell_entry.data_hash == valid_royalty_payment_hash
                    }) && cells.iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_buy_tx_id
                            && cell.cell_entry.data_bytes == 48
                            && cell.cell_entry.data_hash == valid_seller_payment_hash
                    })
                }
            }
        },
        "royalty payment outputs from the buy_from_listing action builder were not indexed",
    )
    .await;

    coverage.valid.insert(("nft.cell".to_string(), "buy_from_listing".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "buy_from_listing".to_string()));

    let accept_offer_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, accept_offer_artifact.artifact_bytes.len(), "NFT accept_offer");
    let accept_offer_deploy_input_capacity = accept_offer_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let accept_offer_code_cell_capacity = accept_offer_deploy_input_capacity
        .checked_sub(required_fee(accept_offer_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT accept_offer scoped action deployment must leave capacity for code cell");
    let accept_offer_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &accept_offer_deploy_input,
        vec![],
        vec![CellOutput { capacity: accept_offer_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![accept_offer_artifact.artifact_bytes.clone()],
    );
    let accept_offer_deploy_tx_id = spora_hashes::Hash::from_bytes(accept_offer_deploy_tx.id());
    let accept_offer_code_outpoint = TransactionOutpoint::new(accept_offer_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&accept_offer_deploy_tx).into(), false)
        .await
        .expect("NFT accept_offer scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, accept_offer_deploy_tx_id, "NFT accept_offer scoped action deployment")
        .await;

    let accept_offer_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT accept_offer fixture");
    let accept_offer_fixture_input_capacity = accept_offer_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let accept_state_cell_capacity = accept_offer_fixture_input_capacity / 6;
    let accept_offer_fixture_change_capacity = accept_offer_fixture_input_capacity
        .checked_sub(accept_state_cell_capacity.saturating_mul(4))
        .and_then(|value| value.checked_sub(required_fee(accept_offer_fixture_input.len(), 5).saturating_add(100_000)))
        .expect("NFT accept_offer fixture transaction must leave change");
    let accept_offer_lock = Script::new(accept_offer_artifact.code_hash, 0, vec![]);
    let accept_offer_type = Script::new(always_success_code_hash(), 0, b"nft-accept-offer-type".to_vec());
    let accept_nft_type = Script::new(always_success_code_hash(), 0, b"nft-accept-nft-type".to_vec());
    let accept_seller = [121; 32];
    let accept_buyer = [122; 32];
    let accept_royalty_recipient = [123; 32];
    let accept_price = 24_000;
    let accept_current_time = 444_000;
    let accept_expires_at = 444_100;
    let accept_royalty_amount = accept_price * 250 / 10_000;
    let accept_seller_amount = accept_price - accept_royalty_amount;
    let accept_offer_data = offer_cell_data(13, accept_buyer, accept_price, accept_expires_at);
    let accept_nft_data = nft_cell_data(13, accept_seller, [124; 32], accept_royalty_recipient, 250);
    let accept_offer_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &accept_offer_fixture_input,
        vec![
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(accept_offer_code_outpoint.tx_hash, accept_offer_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput {
                capacity: accept_state_cell_capacity,
                lock: accept_offer_lock.clone(),
                type_: Some(accept_offer_type.clone()),
            },
            CellOutput { capacity: accept_state_cell_capacity, lock: accept_offer_lock.clone(), type_: Some(accept_nft_type.clone()) },
            CellOutput {
                capacity: accept_state_cell_capacity,
                lock: accept_offer_lock.clone(),
                type_: Some(accept_offer_type.clone()),
            },
            CellOutput { capacity: accept_state_cell_capacity, lock: accept_offer_lock.clone(), type_: Some(accept_nft_type.clone()) },
            CellOutput {
                capacity: accept_offer_fixture_change_capacity,
                lock: pay_to_acceptance_owner(prealloc_address),
                type_: None,
            },
        ],
        vec![accept_offer_data.clone(), accept_nft_data.clone(), accept_offer_data.clone(), accept_nft_data.clone(), vec![]],
    );
    let accept_offer_fixture_tx_id = spora_hashes::Hash::from_bytes(accept_offer_fixture_tx.id());
    let malformed_accept_offer_input = TransactionOutpoint::new(accept_offer_fixture_tx.id(), 0);
    let malformed_accept_nft_input = TransactionOutpoint::new(accept_offer_fixture_tx.id(), 1);
    let valid_accept_offer_input = TransactionOutpoint::new(accept_offer_fixture_tx.id(), 2);
    let valid_accept_nft_input = TransactionOutpoint::new(accept_offer_fixture_tx.id(), 3);
    rpc_client
        .submit_transaction((&accept_offer_fixture_tx).into(), false)
        .await
        .expect("NFT accept_offer fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, accept_offer_fixture_tx_id, "NFT accept_offer fixture cells").await;

    let expired_accept_offer_witness = accept_offer_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(accept_buyer),
            cellscript::EntryWitnessArg::Address(accept_seller),
            cellscript::EntryWitnessArg::U64(accept_price),
            cellscript::EntryWitnessArg::U64(accept_expires_at),
        ])
        .expect("NFT accept_offer expired action witness must encode buyer, seller, price, and current time");
    let accept_output_capacity = accept_state_cell_capacity
        .checked_sub(required_fee(2, 3).saturating_add(100_000))
        .expect("NFT accept_offer action must leave fee")
        / 3;
    let malformed_accept_tx = CellTx::new(
        vec![
            CellInput::new(OutPoint::new(malformed_accept_nft_input.tx_hash, malformed_accept_nft_input.index), 0),
            CellInput::new(OutPoint::new(malformed_accept_offer_input.tx_hash, malformed_accept_offer_input.index), 0),
        ],
        vec![
            CellDep {
                out_point: OutPoint::new(accept_offer_code_outpoint.tx_hash, accept_offer_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: accept_output_capacity, lock: accept_offer_lock.clone(), type_: Some(accept_nft_type.clone()) },
            CellOutput { capacity: accept_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
            CellOutput { capacity: accept_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            nft_cell_data(13, accept_buyer, [124; 32], accept_royalty_recipient, 250),
            royalty_payment_cell_data(13, accept_royalty_recipient, accept_royalty_amount),
            royalty_payment_cell_data(13, accept_seller, accept_seller_amount),
        ],
        vec![expired_accept_offer_witness],
    )
    .expect("malformed NFT accept_offer transaction must be structurally valid");
    let malformed_accept_reason = rpc_client
        .submit_transaction((&malformed_accept_tx).into(), false)
        .await
        .expect_err("malformed NFT accept_offer must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_accept_reason.contains("not standard")
            && !malformed_accept_reason.contains("storage mass")
            && !malformed_accept_reason.contains("compute mass")
            && !malformed_accept_reason.contains("transient")
            && !malformed_accept_reason.contains("cycles exceeded")
            && !malformed_accept_reason.contains("cycles limit"),
        "malformed NFT accept_offer must fail in script validation, not policy or mass: {malformed_accept_reason}"
    );

    let accept_offer_witness = accept_offer_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(accept_buyer),
            cellscript::EntryWitnessArg::Address(accept_seller),
            cellscript::EntryWitnessArg::U64(accept_price),
            cellscript::EntryWitnessArg::U64(accept_current_time),
        ])
        .expect("NFT accept_offer action witness must encode buyer, seller, price, and current time");
    let valid_accept_nft_data = nft_cell_data(13, accept_buyer, [124; 32], accept_royalty_recipient, 250);
    let valid_accept_tx = CellTx::new(
        vec![
            CellInput::new(OutPoint::new(valid_accept_nft_input.tx_hash, valid_accept_nft_input.index), 0),
            CellInput::new(OutPoint::new(valid_accept_offer_input.tx_hash, valid_accept_offer_input.index), 0),
        ],
        vec![
            CellDep {
                out_point: OutPoint::new(accept_offer_code_outpoint.tx_hash, accept_offer_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: accept_output_capacity, lock: accept_offer_lock, type_: Some(accept_nft_type) },
            CellOutput { capacity: accept_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
            CellOutput { capacity: accept_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            valid_accept_nft_data.clone(),
            royalty_payment_cell_data(13, accept_royalty_recipient, accept_royalty_amount),
            royalty_payment_cell_data(13, accept_seller, accept_seller_amount),
        ],
        vec![accept_offer_witness],
    )
    .expect("valid NFT accept_offer transaction must be structurally valid");
    let valid_accept_tx_id = spora_hashes::Hash::from_bytes(valid_accept_tx.id());
    rpc_client
        .submit_transaction((&valid_accept_tx).into(), false)
        .await
        .expect("valid NFT accept_offer must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_accept_tx_id, "valid NFT accept_offer action").await;

    let valid_accept_royalty_payment_hash =
        spora_cell_data_hash(&royalty_payment_cell_data(13, accept_royalty_recipient, accept_royalty_amount));
    let valid_accept_seller_payment_hash = spora_cell_data_hash(&royalty_payment_cell_data(13, accept_seller, accept_seller_amount));
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    let cells = client.get_cells_by_addresses(vec![address]).await.unwrap();
                    cells.iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_accept_tx_id
                            && cell.cell_entry.data_bytes == 48
                            && cell.cell_entry.data_hash == valid_accept_royalty_payment_hash
                    }) && cells.iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_accept_tx_id
                            && cell.cell_entry.data_bytes == 48
                            && cell.cell_entry.data_hash == valid_accept_seller_payment_hash
                    })
                }
            }
        },
        "royalty payment outputs from the accept_offer action builder were not indexed",
    )
    .await;

    coverage.valid.insert(("nft.cell".to_string(), "accept_offer".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "accept_offer".to_string()));

    let mint_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, mint_artifact.artifact_bytes.len(), "NFT mint");
    let mint_deploy_input_capacity = mint_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let mint_code_cell_capacity = mint_deploy_input_capacity
        .checked_sub(required_fee(mint_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT mint scoped action deployment must leave capacity for code cell");
    let mint_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &mint_deploy_input,
        vec![],
        vec![CellOutput { capacity: mint_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![mint_artifact.artifact_bytes.clone()],
    );
    let mint_deploy_tx_id = spora_hashes::Hash::from_bytes(mint_deploy_tx.id());
    let mint_code_outpoint = TransactionOutpoint::new(mint_deploy_tx.id(), 0);
    rpc_client.submit_transaction((&mint_deploy_tx).into(), false).await.expect("NFT mint scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, mint_deploy_tx_id, "NFT mint scoped action deployment").await;

    let mint_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT mint fixture");
    let mint_fixture_input_capacity = mint_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let mint_collection_capacity = mint_fixture_input_capacity / 4;
    let mint_fixture_change_capacity = mint_fixture_input_capacity
        .checked_sub(mint_collection_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(mint_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("NFT mint fixture transaction must leave change");
    let mint_lock = Script::new(mint_artifact.code_hash, 0, vec![]);
    let mint_collection_type = Script::new(always_success_code_hash(), 0, b"nft-mint-collection-type".to_vec());
    let mint_creator = [131; 32];
    let mint_collection_data = collection_cell_data(b"Acceptance NFT", b"ANFT", mint_creator, 1, 100, b"spora://acceptance/nft/");
    let mint_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &mint_fixture_input,
        vec![
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep { out_point: OutPoint::new(mint_code_outpoint.tx_hash, mint_code_outpoint.index), dep_type: DepType::Code },
        ],
        vec![
            CellOutput { capacity: mint_collection_capacity, lock: mint_lock.clone(), type_: Some(mint_collection_type.clone()) },
            CellOutput { capacity: mint_collection_capacity, lock: mint_lock.clone(), type_: Some(mint_collection_type.clone()) },
            CellOutput { capacity: mint_fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![mint_collection_data.clone(), mint_collection_data.clone(), vec![]],
    );
    let mint_fixture_tx_id = spora_hashes::Hash::from_bytes(mint_fixture_tx.id());
    let malformed_mint_collection_input = TransactionOutpoint::new(mint_fixture_tx.id(), 0);
    let valid_mint_collection_input = TransactionOutpoint::new(mint_fixture_tx.id(), 1);
    rpc_client.submit_transaction((&mint_fixture_tx).into(), false).await.expect("NFT mint fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, mint_fixture_tx_id, "NFT mint fixture cells").await;

    let mint_recipient = [132; 32];
    let mint_metadata_hash = [133; 32];
    let mint_witness = mint_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(mint_recipient),
            cellscript::EntryWitnessArg::Hash(mint_metadata_hash),
        ])
        .expect("NFT mint action witness must encode recipient and metadata hash");
    let mint_output_capacity =
        mint_collection_capacity.checked_sub(required_fee(1, 2).saturating_add(100_000)).expect("NFT mint action must leave fee") / 2;
    let malformed_mint_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(malformed_mint_collection_input.tx_hash, malformed_mint_collection_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(mint_code_outpoint.tx_hash, mint_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: mint_output_capacity, lock: mint_lock.clone(), type_: Some(mint_collection_type.clone()) },
            CellOutput { capacity: mint_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            collection_cell_data(b"Acceptance NFT", b"ANFT", mint_creator, 3, 100, b"spora://acceptance/nft/"),
            nft_cell_data(2, mint_recipient, mint_metadata_hash, mint_creator, 250),
        ],
        vec![mint_witness.clone()],
    )
    .expect("malformed NFT mint transaction must be structurally valid");
    let malformed_mint_reason = rpc_client
        .submit_transaction((&malformed_mint_tx).into(), false)
        .await
        .expect_err("malformed NFT mint must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_mint_reason.contains("not standard")
            && !malformed_mint_reason.contains("storage mass")
            && !malformed_mint_reason.contains("compute mass")
            && !malformed_mint_reason.contains("transient")
            && !malformed_mint_reason.contains("cycles exceeded")
            && !malformed_mint_reason.contains("cycles limit"),
        "malformed NFT mint must fail in script validation, not policy or mass: {malformed_mint_reason}"
    );

    let valid_minted_nft_data = nft_cell_data(2, mint_recipient, mint_metadata_hash, mint_creator, 250);
    let valid_mint_tx = CellTx::new(
        vec![CellInput::new(OutPoint::new(valid_mint_collection_input.tx_hash, valid_mint_collection_input.index), 0)],
        vec![
            CellDep { out_point: OutPoint::new(mint_code_outpoint.tx_hash, mint_code_outpoint.index), dep_type: DepType::Code },
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput { capacity: mint_output_capacity, lock: mint_lock, type_: Some(mint_collection_type) },
            CellOutput { capacity: mint_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            collection_cell_data(b"Acceptance NFT", b"ANFT", mint_creator, 2, 100, b"spora://acceptance/nft/"),
            valid_minted_nft_data.clone(),
        ],
        vec![mint_witness],
    )
    .expect("valid NFT mint transaction must be structurally valid");
    let valid_mint_tx_id = spora_hashes::Hash::from_bytes(valid_mint_tx.id());
    rpc_client
        .submit_transaction((&valid_mint_tx).into(), false)
        .await
        .expect("valid NFT mint must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_mint_tx_id, "valid NFT mint action").await;

    let valid_minted_nft_data_hash = spora_cell_data_hash(&valid_minted_nft_data);
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == valid_mint_tx_id
                            && cell.cell_entry.data_bytes == 106
                            && cell.cell_entry.data_hash == valid_minted_nft_data_hash
                    })
                }
            }
        },
        "minted NFT output from the mint action builder was not indexed",
    )
    .await;

    coverage.valid.insert(("nft.cell".to_string(), "mint".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "mint".to_string()));

    let batch_mint_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, batch_mint_artifact.artifact_bytes.len(), "NFT batch_mint");
    let batch_mint_deploy_input_capacity = batch_mint_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let batch_mint_code_cell_capacity = batch_mint_deploy_input_capacity
        .checked_sub(required_fee(batch_mint_deploy_input.len(), 1).saturating_add(100_000))
        .expect("NFT batch_mint scoped action deployment must leave capacity for code cell");
    let batch_mint_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &batch_mint_deploy_input,
        vec![],
        vec![CellOutput { capacity: batch_mint_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![batch_mint_artifact.artifact_bytes.clone()],
    );
    let batch_mint_deploy_tx_id = spora_hashes::Hash::from_bytes(batch_mint_deploy_tx.id());
    let batch_mint_code_outpoint = TransactionOutpoint::new(batch_mint_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&batch_mint_deploy_tx).into(), false)
        .await
        .expect("NFT batch_mint scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, batch_mint_deploy_tx_id, "NFT batch_mint scoped action deployment")
        .await;

    let batch_mint_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "NFT batch_mint fixture");
    let batch_mint_fixture_input_capacity = batch_mint_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let batch_collection_capacity = batch_mint_fixture_input_capacity / 4;
    let batch_mint_fixture_change_capacity = batch_mint_fixture_input_capacity
        .checked_sub(batch_collection_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(batch_mint_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("NFT batch_mint fixture transaction must leave change");
    let batch_mint_lock = Script::new(batch_mint_artifact.code_hash, 0, vec![]);
    let batch_collection_type = Script::new(always_success_code_hash(), 0, b"nft-batch-collection-type".to_vec());
    let batch_creator = [141; 32];
    let batch_collection_data = collection_cell_data(b"Batch NFT", b"BNFT", batch_creator, 10, 100, b"spora://acceptance/batch/");
    let batch_mint_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &batch_mint_fixture_input,
        vec![
            CellDep {
                out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                dep_type: DepType::Code,
            },
            CellDep {
                out_point: OutPoint::new(batch_mint_code_outpoint.tx_hash, batch_mint_code_outpoint.index),
                dep_type: DepType::Code,
            },
        ],
        vec![
            CellOutput {
                capacity: batch_collection_capacity,
                lock: batch_mint_lock.clone(),
                type_: Some(batch_collection_type.clone()),
            },
            CellOutput {
                capacity: batch_collection_capacity,
                lock: batch_mint_lock.clone(),
                type_: Some(batch_collection_type.clone()),
            },
            CellOutput { capacity: batch_mint_fixture_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![batch_collection_data.clone(), batch_collection_data, vec![]],
    );
    let batch_mint_fixture_tx_id = spora_hashes::Hash::from_bytes(batch_mint_fixture_tx.id());
    let malformed_batch_collection_input = TransactionOutpoint::new(batch_mint_fixture_tx.id(), 0);
    let valid_batch_collection_input = TransactionOutpoint::new(batch_mint_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&batch_mint_fixture_tx).into(), false)
        .await
        .expect("NFT batch_mint fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, batch_mint_fixture_tx_id, "NFT batch_mint fixture cells").await;

    let batch_recipients = [[142; 32], [143; 32], [144; 32], [145; 32]];
    let batch_metadata_hashes = [[146; 32], [147; 32], [148; 32], [149; 32]];
    let batch_recipients_arg = fixed_32_array_arg(batch_recipients);
    let batch_metadata_hashes_arg = fixed_32_array_arg(batch_metadata_hashes);
    let batch_mint_witness = batch_mint_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Bytes(batch_recipients_arg),
            cellscript::EntryWitnessArg::Bytes(batch_metadata_hashes_arg),
        ])
        .expect("NFT batch_mint action witness must encode recipients and metadata hashes");
    let batch_output_capacity = batch_collection_capacity
        .checked_sub(required_fee(1, 5).saturating_add(100_000))
        .expect("NFT batch_mint action must leave fee")
        / 5;
    let malformed_batch_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(malformed_batch_collection_input.tx_hash, malformed_batch_collection_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(batch_mint_code_outpoint.tx_hash, batch_mint_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: batch_output_capacity,
                    lock: batch_mint_lock.clone(),
                    type_: Some(batch_collection_type.clone()),
                },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
            ],
            vec![
                collection_cell_data(b"Batch NFT", b"BNFT", batch_creator, 14, 100, b"spora://acceptance/batch/"),
                nft_cell_data(11, batch_recipients[0], batch_metadata_hashes[0], batch_creator, 250),
                nft_cell_data(12, batch_recipients[1], batch_metadata_hashes[1], batch_creator, 250),
                nft_cell_data(13, batch_recipients[2], batch_metadata_hashes[2], batch_creator, 250),
                nft_cell_data(14, batch_recipients[3], [150; 32], batch_creator, 250),
            ],
            vec![batch_mint_witness.clone()],
        )
        .expect("malformed NFT batch_mint transaction must be structurally valid"),
        &batch_mint_artifact.action,
        "malformed NFT batch_mint transaction",
    );
    let malformed_batch_reason = rpc_client
        .submit_transaction((&malformed_batch_tx).into(), false)
        .await
        .expect_err("malformed NFT batch_mint must be rejected by the scoped action verifier")
        .to_string();
    assert!(
        !malformed_batch_reason.contains("not standard")
            && !malformed_batch_reason.contains("storage mass")
            && !malformed_batch_reason.contains("compute mass")
            && !malformed_batch_reason.contains("transient")
            && !malformed_batch_reason.contains("cycles exceeded")
            && !malformed_batch_reason.contains("cycles limit"),
        "malformed NFT batch_mint must fail in script validation, not policy or mass: {malformed_batch_reason}"
    );

    let valid_batch_nft_data = [
        nft_cell_data(11, batch_recipients[0], batch_metadata_hashes[0], batch_creator, 250),
        nft_cell_data(12, batch_recipients[1], batch_metadata_hashes[1], batch_creator, 250),
        nft_cell_data(13, batch_recipients[2], batch_metadata_hashes[2], batch_creator, 250),
        nft_cell_data(14, batch_recipients[3], batch_metadata_hashes[3], batch_creator, 250),
    ];
    let valid_batch_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(valid_batch_collection_input.tx_hash, valid_batch_collection_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(batch_mint_code_outpoint.tx_hash, batch_mint_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: batch_output_capacity, lock: batch_mint_lock, type_: Some(batch_collection_type) },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
                CellOutput { capacity: batch_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
            ],
            vec![
                collection_cell_data(b"Batch NFT", b"BNFT", batch_creator, 14, 100, b"spora://acceptance/batch/"),
                valid_batch_nft_data[0].clone(),
                valid_batch_nft_data[1].clone(),
                valid_batch_nft_data[2].clone(),
                valid_batch_nft_data[3].clone(),
            ],
            vec![batch_mint_witness],
        )
        .expect("valid NFT batch_mint transaction must be structurally valid"),
        &batch_mint_artifact.action,
        "valid NFT batch_mint transaction",
    );
    let valid_batch_tx_id = spora_hashes::Hash::from_bytes(valid_batch_tx.id());
    rpc_client
        .submit_transaction((&valid_batch_tx).into(), false)
        .await
        .expect("valid NFT batch_mint must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_batch_tx_id, "valid NFT batch_mint action").await;

    let valid_batch_hashes = valid_batch_nft_data.iter().map(|data| spora_cell_data_hash(data)).collect::<Vec<_>>();
    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = prealloc_address.clone();
            move || {
                let client = client.clone();
                let address = address.clone();
                let hashes = valid_batch_hashes.clone();
                async move {
                    let cells = client.get_cells_by_addresses(vec![address]).await.unwrap();
                    hashes.iter().all(|hash| {
                        cells.iter().any(|cell| {
                            cell.outpoint.transaction_id == valid_batch_tx_id
                                && cell.cell_entry.data_bytes == 106
                                && cell.cell_entry.data_hash == *hash
                        })
                    })
                }
            }
        },
        "minted NFT outputs from the batch_mint action builder were not indexed",
    )
    .await;

    coverage.valid.insert(("nft.cell".to_string(), "batch_mint".to_string()));
    coverage.malformed.insert(("nft.cell".to_string(), "batch_mint".to_string()));
    coverage
}

async fn run_launch_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(launch_deployment) = deployments.iter().find(|deployment| deployment.name == "launch.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(simple_launch_artifact) = launch_deployment.action_artifacts.iter().find(|artifact| artifact.name == "simple_launch")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(launch_token_artifact) = launch_deployment.action_artifacts.iter().find(|artifact| artifact.name == "launch_token")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 4,
        "launch action builder matrix needs four matured plain prealloc cells for scoped deploys and executable fixtures"
    );

    let deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        simple_launch_artifact.artifact_bytes.len(),
        "launch simple_launch",
    );
    let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let code_cell_capacity = deploy_input_capacity
        .checked_sub(required_fee(deploy_input.len(), 1).saturating_add(100_000))
        .expect("launch simple_launch scoped action deployment must leave capacity for code cell");
    let deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &deploy_input,
        vec![],
        vec![CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![simple_launch_artifact.artifact_bytes.clone()],
    );
    let deploy_tx_id = spora_hashes::Hash::from_bytes(deploy_tx.id());
    let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&deploy_tx).into(), false)
        .await
        .expect("launch simple_launch scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, deploy_tx_id, "launch simple_launch scoped action deployment").await;

    let fixture_input = pop_plain_cells(&mut spendable_cells, 1, "launch simple_launch fixture");
    let fixture_input_capacity = fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let action_lock = Script::new(simple_launch_artifact.code_hash, 0, vec![]);
    let fixture_capacity = fixture_input_capacity
        .checked_sub(required_fee(fixture_input.len(), 1).saturating_add(100_000))
        .expect("launch simple_launch fixture transaction must leave capacity");
    let fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &fixture_input,
        vec![],
        vec![CellOutput { capacity: fixture_capacity, lock: action_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let fixture_tx_id = spora_hashes::Hash::from_bytes(fixture_tx.id());
    let action_input = TransactionOutpoint::new(fixture_tx.id(), 0);
    rpc_client.submit_transaction((&fixture_tx).into(), false).await.expect("launch simple_launch fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, fixture_tx_id, "launch simple_launch fixture cell").await;

    let symbol = *b"LAUNCH01";
    let max_supply = 10_000;
    let initial_mint = 1_000;
    let creator_address =
        Address::new_std_single(NetworkType::Devnet.into(), &[170; 32]).expect("launch creator address must be valid");
    let creator_lock = pay_to_acceptance_owner(&creator_address);
    let creator = creator_lock.hash();
    let recipient_addresses = [
        Address::new_std_single(NetworkType::Devnet.into(), &[171; 32]).expect("launch recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[172; 32]).expect("launch recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[173; 32]).expect("launch recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[174; 32]).expect("launch recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[175; 32]).expect("launch recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[176; 32]).expect("launch recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[177; 32]).expect("launch recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[178; 32]).expect("launch recipient address must be valid"),
    ];
    let recipient_locks = recipient_addresses.iter().map(pay_to_acceptance_owner).collect::<Vec<_>>();
    let recipient_amounts = [10_u64, 20, 30, 40, 50, 60, 70, 80];
    let recipient_hashes = recipient_locks.iter().map(Script::hash).collect::<Vec<_>>();
    let total_distributed = recipient_amounts.iter().sum::<u64>();
    let remaining = initial_mint - total_distributed;
    let recipients_arg = fixed_recipient_tuple_array8_arg(&recipient_hashes, recipient_amounts);
    let witness = simple_launch_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Bytes(symbol.to_vec()),
            cellscript::EntryWitnessArg::U64(max_supply),
            cellscript::EntryWitnessArg::U64(initial_mint),
            cellscript::EntryWitnessArg::Address(creator),
            cellscript::EntryWitnessArg::Bytes(recipients_arg),
        ])
        .expect("launch simple_launch witness must encode symbol, supply, creator, and recipients");

    let output_capacity = fixture_capacity
        .checked_sub(required_fee(1, 10).saturating_add(100_000))
        .and_then(|value| value.checked_div(10))
        .expect("launch simple_launch action must leave capacity for ten outputs");
    let auth_type = Script::new(always_success_code_hash(), 0, b"launch-auth-type".to_vec());
    let token_type = Script::new(always_success_code_hash(), 0, b"launch-token-type".to_vec());
    let mut outputs = vec![CellOutput { capacity: output_capacity, lock: creator_lock.clone(), type_: Some(auth_type.clone()) }];
    outputs.extend(recipient_locks.iter().map(|lock| CellOutput {
        capacity: output_capacity,
        lock: lock.clone(),
        type_: Some(token_type.clone()),
    }));
    outputs.push(CellOutput { capacity: output_capacity, lock: creator_lock, type_: Some(token_type) });
    let mut output_data = vec![mint_authority_cell_data(symbol, max_supply, initial_mint)];
    output_data.extend(recipient_amounts.iter().map(|amount| token_cell_data(*amount, symbol)));
    output_data.push(token_cell_data(remaining, symbol));
    let mut malformed_output_data = output_data.clone();
    *malformed_output_data.last_mut().expect("simple_launch malformed output data must include remaining token") =
        token_cell_data(remaining - 1, symbol);

    let cell_deps = vec![
        CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code },
        CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        },
    ];
    let malformed_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(action_input.tx_hash, action_input.index), 0)],
            cell_deps.clone(),
            outputs.clone(),
            malformed_output_data,
            vec![witness.clone()],
        )
        .expect("malformed launch simple_launch transaction must be structurally valid"),
        &simple_launch_artifact.action,
        "malformed launch simple_launch transaction",
    );
    let malformed_reason = rpc_client
        .submit_transaction((&malformed_tx).into(), false)
        .await
        .expect_err("malformed launch simple_launch must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_reason, "launch simple_launch");

    let valid_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(action_input.tx_hash, action_input.index), 0)],
            cell_deps,
            outputs,
            output_data,
            vec![witness],
        )
        .expect("valid launch simple_launch transaction must be structurally valid"),
        &simple_launch_artifact.action,
        "valid launch simple_launch transaction",
    );
    let valid_tx_id = spora_hashes::Hash::from_bytes(valid_tx.id());
    rpc_client
        .submit_transaction((&valid_tx).into(), false)
        .await
        .expect("valid launch simple_launch must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_tx_id, "valid launch simple_launch action").await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    coverage.valid.insert(("launch.cell".to_string(), "simple_launch".to_string()));
    coverage.malformed.insert(("launch.cell".to_string(), "simple_launch".to_string()));

    let launch_token_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        launch_token_artifact.artifact_bytes.len(),
        "launch launch_token",
    );
    let launch_token_deploy_input_capacity = launch_token_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let launch_token_code_cell_capacity = launch_token_deploy_input_capacity
        .checked_sub(required_fee(launch_token_deploy_input.len(), 1).saturating_add(100_000))
        .expect("launch launch_token scoped action deployment must leave capacity for code cell");
    let launch_token_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &launch_token_deploy_input,
        vec![],
        vec![CellOutput { capacity: launch_token_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![launch_token_artifact.artifact_bytes.clone()],
    );
    let launch_token_deploy_tx_id = spora_hashes::Hash::from_bytes(launch_token_deploy_tx.id());
    let launch_token_code_outpoint = TransactionOutpoint::new(launch_token_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&launch_token_deploy_tx).into(), false)
        .await
        .expect("launch launch_token scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        launch_token_deploy_tx_id,
        "launch launch_token scoped action deployment",
    )
    .await;

    let launch_token_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "launch launch_token fixture");
    let launch_token_fixture_input_capacity = launch_token_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let launch_token_fixture_capacity = launch_token_fixture_input_capacity
        .checked_sub(required_fee(launch_token_fixture_input.len(), 2).saturating_add(100_000))
        .and_then(|value| value.checked_div(2))
        .expect("launch launch_token fixture transaction must leave capacity");
    let launch_token_change_capacity = launch_token_fixture_input_capacity
        .checked_sub(launch_token_fixture_capacity)
        .and_then(|value| value.checked_sub(required_fee(launch_token_fixture_input.len(), 2).saturating_add(100_000)))
        .expect("launch launch_token fixture transaction must leave change");
    let launch_token_lock = Script::new(launch_token_artifact.code_hash, 0, vec![]);
    let paired_token_type = Script::new(always_success_code_hash(), 0, b"launch-paired-token-type".to_vec());
    let launch_token_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &launch_token_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: launch_token_fixture_capacity,
                lock: launch_token_lock.clone(),
                type_: Some(paired_token_type.clone()),
            },
            CellOutput { capacity: launch_token_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![token_cell_data(400, *b"PAIR0001"), vec![]],
    );
    let launch_token_fixture_tx_id = spora_hashes::Hash::from_bytes(launch_token_fixture_tx.id());
    let paired_token_input = TransactionOutpoint::new(launch_token_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&launch_token_fixture_tx).into(), false)
        .await
        .expect("launch launch_token fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, launch_token_fixture_tx_id, "launch launch_token fixture cell").await;

    let launch_symbol = *b"LNCHTKN1";
    let launch_max_supply = 10_000_u64;
    let launch_initial_mint = 1_000_u64;
    let pool_seed_amount = 100_u64;
    let fee_rate_bps = 30_u16;
    let launch_creator_address =
        Address::new_std_single(NetworkType::Devnet.into(), &[179; 32]).expect("launch_token creator address must be valid");
    let launch_creator_lock = pay_to_acceptance_owner(&launch_creator_address);
    let launch_creator = launch_creator_lock.hash();
    let dist_addresses = [
        Address::new_std_single(NetworkType::Devnet.into(), &[180; 32]).expect("launch_token recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[181; 32]).expect("launch_token recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[182; 32]).expect("launch_token recipient address must be valid"),
        Address::new_std_single(NetworkType::Devnet.into(), &[183; 32]).expect("launch_token recipient address must be valid"),
    ];
    let dist_locks = dist_addresses.iter().map(pay_to_acceptance_owner).collect::<Vec<_>>();
    let dist_amounts = [50_u64, 60, 70, 80];
    let dist_hashes = dist_locks.iter().map(Script::hash).collect::<Vec<_>>();
    let distribution_arg = fixed_recipient_tuple_array4_arg(&dist_hashes, dist_amounts);
    let launch_token_witness = launch_token_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Bytes(launch_symbol.to_vec()),
            cellscript::EntryWitnessArg::U64(launch_max_supply),
            cellscript::EntryWitnessArg::U64(launch_initial_mint),
            cellscript::EntryWitnessArg::U64(pool_seed_amount),
            cellscript::EntryWitnessArg::U16(fee_rate_bps),
            cellscript::EntryWitnessArg::Address(launch_creator),
            cellscript::EntryWitnessArg::Bytes(distribution_arg),
        ])
        .expect("launch launch_token witness must encode launch parameters");
    let launch_token_output_capacity = launch_token_fixture_capacity
        .checked_sub(required_fee(1, 8).saturating_add(100_000))
        .and_then(|value| value.checked_div(8))
        .expect("launch launch_token action must leave capacity for eight outputs");
    let launch_auth_type = Script::new(always_success_code_hash(), 0, b"launch-token-auth-type".to_vec());
    let launch_pool_type = Script::new(always_success_code_hash(), 0, b"launch-pool-type".to_vec());
    let launch_lp_receipt_type = Script::new(always_success_code_hash(), 0, b"launch-lp-receipt-type".to_vec());
    let launch_dist_token_type = Script::new(always_success_code_hash(), 0, b"launch-distribution-token-type".to_vec());
    let mut launch_outputs =
        vec![CellOutput { capacity: launch_token_output_capacity, lock: launch_creator_lock.clone(), type_: Some(launch_auth_type) }];
    launch_outputs.push(CellOutput {
        capacity: launch_token_output_capacity,
        lock: launch_token_lock,
        type_: Some(launch_pool_type.clone()),
    });
    launch_outputs.push(CellOutput {
        capacity: launch_token_output_capacity,
        lock: launch_creator_lock.clone(),
        type_: Some(launch_lp_receipt_type),
    });
    launch_outputs.extend(dist_locks.iter().map(|lock| CellOutput {
        capacity: launch_token_output_capacity,
        lock: lock.clone(),
        type_: Some(launch_dist_token_type.clone()),
    }));
    launch_outputs.push(CellOutput {
        capacity: launch_token_output_capacity,
        lock: launch_creator_lock,
        type_: Some(launch_dist_token_type),
    });
    let launch_remaining = launch_initial_mint - dist_amounts.iter().sum::<u64>() - pool_seed_amount;
    let mut launch_output_data = vec![
        mint_authority_cell_data(launch_symbol, launch_max_supply, launch_initial_mint),
        pool_cell_data(launch_symbol, *b"PAIR0001", pool_seed_amount, 400, pool_seed_amount, fee_rate_bps),
        lp_receipt_cell_data(launch_pool_type.hash(), pool_seed_amount, launch_creator),
    ];
    launch_output_data.extend(dist_amounts.iter().map(|amount| token_cell_data(*amount, launch_symbol)));
    launch_output_data.push(token_cell_data(launch_remaining, launch_symbol));
    let mut malformed_launch_output_data = launch_output_data.clone();
    malformed_launch_output_data[7] = token_cell_data(launch_remaining + 1, launch_symbol);
    let launch_token_cell_deps = vec![
        CellDep {
            out_point: OutPoint::new(launch_token_code_outpoint.tx_hash, launch_token_code_outpoint.index),
            dep_type: DepType::Code,
        },
        CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        },
    ];
    let malformed_launch_token_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(paired_token_input.tx_hash, paired_token_input.index), 0)],
            launch_token_cell_deps.clone(),
            launch_outputs.clone(),
            malformed_launch_output_data,
            vec![launch_token_witness.clone()],
        )
        .expect("malformed launch launch_token transaction must be structurally valid"),
        &launch_token_artifact.action,
        "malformed launch launch_token transaction",
    );
    let malformed_launch_token_reason = rpc_client
        .submit_transaction((&malformed_launch_token_tx).into(), false)
        .await
        .expect_err("malformed launch launch_token must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_launch_token_reason, "launch launch_token");

    let valid_launch_token_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(paired_token_input.tx_hash, paired_token_input.index), 0)],
            launch_token_cell_deps,
            launch_outputs,
            launch_output_data,
            vec![launch_token_witness],
        )
        .expect("valid launch launch_token transaction must be structurally valid"),
        &launch_token_artifact.action,
        "valid launch launch_token transaction",
    );
    let valid_launch_token_tx_id = spora_hashes::Hash::from_bytes(valid_launch_token_tx.id());
    rpc_client
        .submit_transaction((&valid_launch_token_tx).into(), false)
        .await
        .expect("valid launch launch_token must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_launch_token_tx_id, "valid launch launch_token action").await;

    coverage.valid.insert(("launch.cell".to_string(), "launch_token".to_string()));
    coverage.malformed.insert(("launch.cell".to_string(), "launch_token".to_string()));
    coverage
}

async fn run_vesting_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    _always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(vesting_deployment) = deployments.iter().find(|deployment| deployment.name == "vesting.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(create_config_artifact) =
        vesting_deployment.action_artifacts.iter().find(|artifact| artifact.name == "create_vesting_config")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(grant_vesting_artifact) = vesting_deployment.action_artifacts.iter().find(|artifact| artifact.name == "grant_vesting")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(claim_vested_artifact) = vesting_deployment.action_artifacts.iter().find(|artifact| artifact.name == "claim_vested")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(revoke_grant_artifact) = vesting_deployment.action_artifacts.iter().find(|artifact| artifact.name == "revoke_grant")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 9,
        "vesting action builder matrix needs nine matured plain prealloc cells for scoped deploys and executable fixtures"
    );

    let deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        create_config_artifact.artifact_bytes.len(),
        "vesting create_vesting_config",
    );
    let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let code_cell_capacity = deploy_input_capacity
        .checked_sub(required_fee(deploy_input.len(), 1).saturating_add(100_000))
        .expect("vesting create_vesting_config scoped action deployment must leave capacity for code cell");
    let deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &deploy_input,
        vec![],
        vec![CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![create_config_artifact.artifact_bytes.clone()],
    );
    let deploy_tx_id = spora_hashes::Hash::from_bytes(deploy_tx.id());
    let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&deploy_tx).into(), false)
        .await
        .expect("vesting create_vesting_config scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, deploy_tx_id, "vesting create_vesting_config scoped action deployment")
        .await;

    let fixture_input = pop_plain_cells(&mut spendable_cells, 1, "vesting create_vesting_config fixture");
    let fixture_input_capacity = fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let action_lock = Script::new(create_config_artifact.code_hash, 0, vec![]);
    let fixture_capacity = fixture_input_capacity
        .checked_sub(required_fee(fixture_input.len(), 1).saturating_add(100_000))
        .expect("vesting create_vesting_config fixture transaction must leave capacity");
    let fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &fixture_input,
        vec![],
        vec![CellOutput { capacity: fixture_capacity, lock: action_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let fixture_tx_id = spora_hashes::Hash::from_bytes(fixture_tx.id());
    let action_input = TransactionOutpoint::new(fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&fixture_tx).into(), false)
        .await
        .expect("vesting create_vesting_config fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, fixture_tx_id, "vesting create_vesting_config fixture cell").await;

    let admin_address = Address::new_std_single(NetworkType::Devnet.into(), &[181; 32]).expect("vesting admin address must be valid");
    let admin_lock = pay_to_acceptance_owner(&admin_address);
    let admin = admin_lock.hash();
    let symbol = *b"VEST0001";
    let cliff_period = 10_u64;
    let total_period = 100_u64;
    let revocable = true;
    let witness = create_config_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(admin),
            cellscript::EntryWitnessArg::Bytes(symbol.to_vec()),
            cellscript::EntryWitnessArg::U64(cliff_period),
            cellscript::EntryWitnessArg::U64(total_period),
            cellscript::EntryWitnessArg::Bool(revocable),
        ])
        .expect("vesting create_vesting_config witness must encode config fields");
    let output_capacity = fixture_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("vesting create_vesting_config action must leave capacity");
    let malformed_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(action_input.tx_hash, action_input.index), 0)],
            vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
            vec![CellOutput { capacity: output_capacity, lock: admin_lock.clone(), type_: None }],
            vec![vesting_config_cell_data(admin, symbol, cliff_period, total_period + 1, revocable)],
            vec![witness.clone()],
        )
        .expect("malformed vesting create_vesting_config transaction must be structurally valid"),
        &create_config_artifact.action,
        "malformed vesting create_vesting_config transaction",
    );
    let malformed_reason = rpc_client
        .submit_transaction((&malformed_tx).into(), false)
        .await
        .expect_err("malformed vesting create_vesting_config must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_reason, "vesting create_vesting_config");

    let valid_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(action_input.tx_hash, action_input.index), 0)],
            vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
            vec![CellOutput { capacity: output_capacity, lock: admin_lock.clone(), type_: None }],
            vec![vesting_config_cell_data(admin, symbol, cliff_period, total_period, revocable)],
            vec![witness],
        )
        .expect("valid vesting create_vesting_config transaction must be structurally valid"),
        &create_config_artifact.action,
        "valid vesting create_vesting_config transaction",
    );
    let valid_tx_id = spora_hashes::Hash::from_bytes(valid_tx.id());
    let config_outpoint = TransactionOutpoint::new(valid_tx.id(), 0);
    rpc_client
        .submit_transaction((&valid_tx).into(), false)
        .await
        .expect("valid vesting create_vesting_config must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_tx_id, "valid vesting create_vesting_config action").await;

    let beneficiary_address =
        Address::new_std_single(NetworkType::Devnet.into(), &[182; 32]).expect("vesting beneficiary address must be valid");
    let beneficiary_lock = pay_to_acceptance_owner(&beneficiary_address);
    let beneficiary = beneficiary_lock.hash();

    let grant_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        grant_vesting_artifact.artifact_bytes.len(),
        "vesting grant_vesting",
    );
    let grant_deploy_input_capacity = grant_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let grant_code_cell_capacity = grant_deploy_input_capacity
        .checked_sub(required_fee(grant_deploy_input.len(), 1).saturating_add(100_000))
        .expect("vesting grant_vesting scoped action deployment must leave capacity for code cell");
    let grant_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &grant_deploy_input,
        vec![],
        vec![CellOutput { capacity: grant_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![grant_vesting_artifact.artifact_bytes.clone()],
    );
    let grant_deploy_tx_id = spora_hashes::Hash::from_bytes(grant_deploy_tx.id());
    let grant_code_outpoint = TransactionOutpoint::new(grant_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&grant_deploy_tx).into(), false)
        .await
        .expect("vesting grant_vesting scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, grant_deploy_tx_id, "vesting grant_vesting scoped action deployment")
        .await;

    let grant_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "vesting grant_vesting fixture");
    let grant_fixture_input_capacity = grant_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let grant_action_lock = Script::new(grant_vesting_artifact.code_hash, 0, vec![]);
    let grant_token_capacity = grant_fixture_input_capacity
        .checked_sub(required_fee(grant_fixture_input.len(), 1).saturating_add(100_000))
        .expect("vesting grant_vesting fixture transaction must leave token capacity");
    let grant_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &grant_fixture_input,
        vec![],
        vec![CellOutput { capacity: grant_token_capacity, lock: grant_action_lock.clone(), type_: None }],
        vec![token_cell_data(100, symbol)],
    );
    let grant_fixture_tx_id = spora_hashes::Hash::from_bytes(grant_fixture_tx.id());
    let grant_token_input = TransactionOutpoint::new(grant_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&grant_fixture_tx).into(), false)
        .await
        .expect("vesting grant_vesting fixture token cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, grant_fixture_tx_id, "vesting grant_vesting fixture token cell").await;

    let (grant_header_dep, grant_now) = current_header_dep_hash_and_daa(rpc_client, miner_address).await;
    let expected_grant_payload =
        vesting_grant_cell_data(0, beneficiary, 100, 0, grant_now, grant_now + cliff_period, grant_now + total_period, symbol);
    let malformed_grant_payload =
        vesting_grant_cell_data(0, beneficiary, 101, 0, grant_now, grant_now + cliff_period, grant_now + total_period, symbol);
    let grant_witness = grant_vesting_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(beneficiary)])
        .expect("vesting grant_vesting witness must encode beneficiary");
    let grant_output_capacity = grant_token_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("vesting grant_vesting action must leave output capacity");
    let malformed_grant_tx = with_compiled_action_scheduler_witness(
        CellTx::new_with_header_deps(
            vec![CellInput::new(OutPoint::new(grant_token_input.tx_hash, grant_token_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(config_outpoint.tx_hash, config_outpoint.index), dep_type: DepType::Code },
                CellDep { out_point: OutPoint::new(grant_code_outpoint.tx_hash, grant_code_outpoint.index), dep_type: DepType::Code },
            ],
            vec![grant_header_dep],
            vec![CellOutput { capacity: grant_output_capacity, lock: beneficiary_lock.clone(), type_: None }],
            vec![malformed_grant_payload],
            vec![grant_witness.clone()],
        )
        .expect("malformed vesting grant_vesting transaction must be structurally valid"),
        &grant_vesting_artifact.action,
        "malformed vesting grant_vesting transaction",
    );
    let malformed_grant_reason = rpc_client
        .submit_transaction((&malformed_grant_tx).into(), false)
        .await
        .expect_err("malformed vesting grant_vesting must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_grant_reason, "vesting grant_vesting");

    let valid_grant_tx = with_compiled_action_scheduler_witness(
        CellTx::new_with_header_deps(
            vec![CellInput::new(OutPoint::new(grant_token_input.tx_hash, grant_token_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(config_outpoint.tx_hash, config_outpoint.index), dep_type: DepType::Code },
                CellDep { out_point: OutPoint::new(grant_code_outpoint.tx_hash, grant_code_outpoint.index), dep_type: DepType::Code },
            ],
            vec![grant_header_dep],
            vec![CellOutput { capacity: grant_output_capacity, lock: beneficiary_lock.clone(), type_: None }],
            vec![expected_grant_payload],
            vec![grant_witness],
        )
        .expect("valid vesting grant_vesting transaction must be structurally valid"),
        &grant_vesting_artifact.action,
        "valid vesting grant_vesting transaction",
    );
    let valid_grant_tx_id = spora_hashes::Hash::from_bytes(valid_grant_tx.id());
    rpc_client
        .submit_transaction((&valid_grant_tx).into(), false)
        .await
        .expect("valid vesting grant_vesting must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_grant_tx_id, "valid vesting grant_vesting action").await;

    let claim_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        claim_vested_artifact.artifact_bytes.len(),
        "vesting claim_vested",
    );
    let claim_deploy_input_capacity = claim_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let claim_code_cell_capacity = claim_deploy_input_capacity
        .checked_sub(required_fee(claim_deploy_input.len(), 1).saturating_add(100_000))
        .expect("vesting claim_vested scoped action deployment must leave capacity for code cell");
    let claim_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &claim_deploy_input,
        vec![],
        vec![CellOutput { capacity: claim_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![claim_vested_artifact.artifact_bytes.clone()],
    );
    let claim_deploy_tx_id = spora_hashes::Hash::from_bytes(claim_deploy_tx.id());
    let claim_code_outpoint = TransactionOutpoint::new(claim_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&claim_deploy_tx).into(), false)
        .await
        .expect("vesting claim_vested scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, claim_deploy_tx_id, "vesting claim_vested scoped action deployment")
        .await;

    let claim_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "vesting claim_vested fixture");
    let claim_fixture_input_capacity = claim_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let claim_beneficiary_lock = Script::new(claim_vested_artifact.code_hash, 0, vec![]);
    let claim_beneficiary = claim_beneficiary_lock.hash();
    let claim_grant_capacity = claim_fixture_input_capacity
        .checked_sub(required_fee(claim_fixture_input.len(), 1).saturating_add(100_000))
        .expect("vesting claim_vested fixture transaction must leave grant capacity");
    let (_, claim_grant_daa) = current_header_dep_hash_and_daa(rpc_client, miner_address).await;
    let claim_grant_payload = vesting_grant_cell_data(
        1,
        claim_beneficiary,
        100,
        99,
        claim_grant_daa.saturating_sub(10),
        claim_grant_daa.saturating_sub(5),
        claim_grant_daa.saturating_sub(1),
        symbol,
    );
    let claim_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &claim_fixture_input,
        vec![],
        vec![CellOutput { capacity: claim_grant_capacity, lock: claim_beneficiary_lock.clone(), type_: None }],
        vec![claim_grant_payload],
    );
    let claim_fixture_tx_id = spora_hashes::Hash::from_bytes(claim_fixture_tx.id());
    let claim_grant_input = TransactionOutpoint::new(claim_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&claim_fixture_tx).into(), false)
        .await
        .expect("vesting claim_vested fixture grant cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, claim_fixture_tx_id, "vesting claim_vested fixture grant cell").await;
    submit_empty_blocks(rpc_client, miner_address, 1).await;
    let (claim_header_dep, _claim_now) = current_header_dep_hash_and_daa(rpc_client, miner_address).await;
    let claim_vested_total = 1;
    let claim_token_payload = token_cell_data(claim_vested_total, symbol);
    let claim_updated_grant_payload = vesting_grant_cell_data(
        2,
        claim_beneficiary,
        100,
        100,
        claim_grant_daa.saturating_sub(10),
        claim_grant_daa.saturating_sub(5),
        claim_grant_daa.saturating_sub(1),
        symbol,
    );
    let malformed_claim_token_payload = token_cell_data(claim_vested_total.saturating_add(1), symbol);
    let claim_witness =
        claim_vested_artifact.action.entry_witness_args(&[]).expect("vesting claim_vested witness must encode empty args");
    let claim_token_output_capacity = claim_grant_capacity / 2;
    let claim_grant_output_capacity = claim_grant_capacity
        .checked_sub(claim_token_output_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("vesting claim_vested action must leave grant output capacity");
    let malformed_claim_tx = with_compiled_action_scheduler_witness(
        CellTx::new_with_header_deps(
            vec![CellInput::new(OutPoint::new(claim_grant_input.tx_hash, claim_grant_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(claim_code_outpoint.tx_hash, claim_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![claim_header_dep],
            vec![
                CellOutput { capacity: claim_token_output_capacity, lock: claim_beneficiary_lock.clone(), type_: None },
                CellOutput { capacity: claim_grant_output_capacity, lock: claim_beneficiary_lock.clone(), type_: None },
            ],
            vec![malformed_claim_token_payload, claim_updated_grant_payload.clone()],
            vec![claim_witness.clone()],
        )
        .expect("malformed vesting claim_vested transaction must be structurally valid"),
        &claim_vested_artifact.action,
        "malformed vesting claim_vested transaction",
    );
    let malformed_claim_reason = rpc_client
        .submit_transaction((&malformed_claim_tx).into(), false)
        .await
        .expect_err("malformed vesting claim_vested must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_claim_reason, "vesting claim_vested");

    let valid_claim_tx = with_compiled_action_scheduler_witness(
        CellTx::new_with_header_deps(
            vec![CellInput::new(OutPoint::new(claim_grant_input.tx_hash, claim_grant_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(claim_code_outpoint.tx_hash, claim_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![claim_header_dep],
            vec![
                CellOutput { capacity: claim_token_output_capacity, lock: claim_beneficiary_lock.clone(), type_: None },
                CellOutput { capacity: claim_grant_output_capacity, lock: claim_beneficiary_lock.clone(), type_: None },
            ],
            vec![claim_token_payload, claim_updated_grant_payload],
            vec![claim_witness],
        )
        .expect("valid vesting claim_vested transaction must be structurally valid"),
        &claim_vested_artifact.action,
        "valid vesting claim_vested transaction",
    );
    let valid_claim_tx_id = spora_hashes::Hash::from_bytes(valid_claim_tx.id());
    rpc_client
        .submit_transaction((&valid_claim_tx).into(), false)
        .await
        .expect("valid vesting claim_vested must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_claim_tx_id, "valid vesting claim_vested action").await;

    let revoke_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        revoke_grant_artifact.artifact_bytes.len(),
        "vesting revoke_grant",
    );
    let revoke_deploy_input_capacity = revoke_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let revoke_code_cell_capacity = revoke_deploy_input_capacity
        .checked_sub(required_fee(revoke_deploy_input.len(), 1).saturating_add(100_000))
        .expect("vesting revoke_grant scoped action deployment must leave capacity for code cell");
    let revoke_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &revoke_deploy_input,
        vec![],
        vec![CellOutput { capacity: revoke_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![revoke_grant_artifact.artifact_bytes.clone()],
    );
    let revoke_deploy_tx_id = spora_hashes::Hash::from_bytes(revoke_deploy_tx.id());
    let revoke_code_outpoint = TransactionOutpoint::new(revoke_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&revoke_deploy_tx).into(), false)
        .await
        .expect("vesting revoke_grant scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, revoke_deploy_tx_id, "vesting revoke_grant scoped action deployment")
        .await;

    let revoke_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "vesting revoke_grant fixture");
    let revoke_fixture_input_capacity = revoke_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let revoke_action_lock = Script::new(revoke_grant_artifact.code_hash, 0, vec![]);
    let revoke_grant_capacity = revoke_fixture_input_capacity
        .checked_sub(required_fee(revoke_fixture_input.len(), 1).saturating_add(100_000))
        .expect("vesting revoke_grant fixture transaction must leave grant capacity");
    let (_, revoke_grant_daa) = current_header_dep_hash_and_daa(rpc_client, miner_address).await;
    let revoke_grant_payload =
        vesting_grant_cell_data(0, beneficiary, 100, 0, revoke_grant_daa, revoke_grant_daa + 1_000, revoke_grant_daa + 2_000, symbol);
    let revoke_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &revoke_fixture_input,
        vec![],
        vec![CellOutput { capacity: revoke_grant_capacity, lock: revoke_action_lock.clone(), type_: None }],
        vec![revoke_grant_payload],
    );
    let revoke_fixture_tx_id = spora_hashes::Hash::from_bytes(revoke_fixture_tx.id());
    let revoke_grant_input = TransactionOutpoint::new(revoke_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&revoke_fixture_tx).into(), false)
        .await
        .expect("vesting revoke_grant fixture grant cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, revoke_fixture_tx_id, "vesting revoke_grant fixture grant cell").await;
    submit_empty_blocks(rpc_client, miner_address, 1).await;
    let (revoke_header_dep, _revoke_now) = current_header_dep_hash_and_daa(rpc_client, miner_address).await;
    let employee_amount = 0;
    let admin_amount = 100;
    let employee_token_payload = token_cell_data(employee_amount, symbol);
    let admin_token_payload = token_cell_data(admin_amount, symbol);
    let malformed_admin_token_payload = token_cell_data(admin_amount + 1, symbol);
    let revoke_employee_output_capacity = revoke_grant_capacity / 3;
    let revoke_admin_output_capacity = revoke_grant_capacity
        .checked_sub(revoke_employee_output_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("vesting revoke_grant action must leave admin output capacity");
    let revoke_witness = revoke_grant_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(admin)])
        .expect("vesting revoke_grant witness must encode admin");
    let malformed_revoke_tx = with_compiled_action_scheduler_witness(
        CellTx::new_with_header_deps(
            vec![CellInput::new(OutPoint::new(revoke_grant_input.tx_hash, revoke_grant_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(config_outpoint.tx_hash, config_outpoint.index), dep_type: DepType::Code },
                CellDep {
                    out_point: OutPoint::new(revoke_code_outpoint.tx_hash, revoke_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![revoke_header_dep],
            vec![
                CellOutput { capacity: revoke_employee_output_capacity, lock: beneficiary_lock.clone(), type_: None },
                CellOutput { capacity: revoke_admin_output_capacity, lock: admin_lock.clone(), type_: None },
            ],
            vec![employee_token_payload.clone(), malformed_admin_token_payload],
            vec![revoke_witness.clone()],
        )
        .expect("malformed vesting revoke_grant transaction must be structurally valid"),
        &revoke_grant_artifact.action,
        "malformed vesting revoke_grant transaction",
    );
    let malformed_revoke_reason = rpc_client
        .submit_transaction((&malformed_revoke_tx).into(), false)
        .await
        .expect_err("malformed vesting revoke_grant must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_revoke_reason, "vesting revoke_grant");

    let valid_revoke_tx = with_compiled_action_scheduler_witness(
        CellTx::new_with_header_deps(
            vec![CellInput::new(OutPoint::new(revoke_grant_input.tx_hash, revoke_grant_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(config_outpoint.tx_hash, config_outpoint.index), dep_type: DepType::Code },
                CellDep {
                    out_point: OutPoint::new(revoke_code_outpoint.tx_hash, revoke_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![revoke_header_dep],
            vec![
                CellOutput { capacity: revoke_employee_output_capacity, lock: beneficiary_lock, type_: None },
                CellOutput { capacity: revoke_admin_output_capacity, lock: admin_lock, type_: None },
            ],
            vec![employee_token_payload, admin_token_payload],
            vec![revoke_witness],
        )
        .expect("valid vesting revoke_grant transaction must be structurally valid"),
        &revoke_grant_artifact.action,
        "valid vesting revoke_grant transaction",
    );
    let valid_revoke_tx_id = spora_hashes::Hash::from_bytes(valid_revoke_tx.id());
    rpc_client
        .submit_transaction((&valid_revoke_tx).into(), false)
        .await
        .expect("valid vesting revoke_grant must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_revoke_tx_id, "valid vesting revoke_grant action").await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    coverage.valid.insert(("vesting.cell".to_string(), "create_vesting_config".to_string()));
    coverage.malformed.insert(("vesting.cell".to_string(), "create_vesting_config".to_string()));
    coverage.valid.insert(("vesting.cell".to_string(), "grant_vesting".to_string()));
    coverage.malformed.insert(("vesting.cell".to_string(), "grant_vesting".to_string()));
    coverage.valid.insert(("vesting.cell".to_string(), "claim_vested".to_string()));
    coverage.malformed.insert(("vesting.cell".to_string(), "claim_vested".to_string()));
    coverage.valid.insert(("vesting.cell".to_string(), "revoke_grant".to_string()));
    coverage.malformed.insert(("vesting.cell".to_string(), "revoke_grant".to_string()));
    coverage
}

async fn run_multisig_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(multisig_deployment) = deployments.iter().find(|deployment| deployment.name == "multisig.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(create_wallet_artifact) = multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "create_wallet")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(propose_transfer_artifact) =
        multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "propose_transfer")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(add_signature_artifact) = multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "add_signature")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(execute_proposal_artifact) =
        multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "execute_proposal")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(cancel_proposal_artifact) =
        multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "cancel_proposal")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(propose_add_signer_artifact) =
        multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "propose_add_signer")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(propose_remove_signer_artifact) =
        multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "propose_remove_signer")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(propose_change_threshold_artifact) =
        multisig_deployment.action_artifacts.iter().find(|artifact| artifact.name == "propose_change_threshold")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 16,
        "multisig action builder matrix needs sixteen matured plain prealloc cells for scoped deploys and executable fixtures"
    );

    let deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        create_wallet_artifact.artifact_bytes.len(),
        "multisig create_wallet",
    );
    let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let code_cell_capacity = deploy_input_capacity
        .checked_sub(required_fee(deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig create_wallet scoped action deployment must leave capacity for code cell");
    let deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &deploy_input,
        vec![],
        vec![CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![create_wallet_artifact.artifact_bytes.clone()],
    );
    let deploy_tx_id = spora_hashes::Hash::from_bytes(deploy_tx.id());
    let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&deploy_tx).into(), false)
        .await
        .expect("multisig create_wallet scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, deploy_tx_id, "multisig create_wallet scoped action deployment").await;

    let fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig create_wallet fixture");
    let fixture_input_capacity = fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let action_lock = Script::new(create_wallet_artifact.code_hash, 0, vec![]);
    let fixture_capacity = fixture_input_capacity
        .checked_sub(required_fee(fixture_input.len(), 1).saturating_add(100_000))
        .expect("multisig create_wallet fixture transaction must leave capacity");
    let fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &fixture_input,
        vec![],
        vec![CellOutput { capacity: fixture_capacity, lock: action_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let fixture_tx_id = spora_hashes::Hash::from_bytes(fixture_tx.id());
    let action_input = TransactionOutpoint::new(fixture_tx.id(), 0);
    rpc_client.submit_transaction((&fixture_tx).into(), false).await.expect("multisig create_wallet fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, fixture_tx_id, "multisig create_wallet fixture cell").await;

    let signer_a = pay_to_acceptance_owner(
        &Address::new_std_single(NetworkType::Devnet.into(), &[191; 32]).expect("multisig signer_a address must be valid"),
    )
    .hash();
    let signer_b = pay_to_acceptance_owner(
        &Address::new_std_single(NetworkType::Devnet.into(), &[192; 32]).expect("multisig signer_b address must be valid"),
    )
    .hash();
    let signers = vec![signer_a.to_vec(), signer_b.to_vec()];
    let signers_payload = molecule_fixvec_cell_data(&signers);
    let threshold = 2u8;
    let current_time = 10u64;
    let wallet_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 0, current_time);
    let malformed_wallet_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], 1, 0, current_time);
    let witness = create_wallet_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Hash([0; 32]),
            cellscript::EntryWitnessArg::Bytes(signers_payload),
            cellscript::EntryWitnessArg::U8(threshold),
            cellscript::EntryWitnessArg::U64(current_time),
        ])
        .expect("multisig create_wallet witness must encode signers, threshold, and current_time");
    let output_capacity = fixture_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("multisig create_wallet action must leave capacity");

    let malformed_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(action_input.tx_hash, action_input.index), 0)],
            vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
            vec![CellOutput { capacity: output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
            vec![malformed_wallet_payload],
            vec![witness.clone()],
        )
        .expect("malformed multisig create_wallet transaction must be structurally valid"),
        &create_wallet_artifact.action,
        "malformed multisig create_wallet transaction",
    );
    let malformed_reason = rpc_client
        .submit_transaction((&malformed_tx).into(), false)
        .await
        .expect_err("malformed multisig create_wallet must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_reason, "multisig create_wallet");

    let valid_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(action_input.tx_hash, action_input.index), 0)],
            vec![CellDep { out_point: OutPoint::new(code_outpoint.tx_hash, code_outpoint.index), dep_type: DepType::Code }],
            vec![CellOutput { capacity: output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
            vec![wallet_payload.clone()],
            vec![witness],
        )
        .expect("valid multisig create_wallet transaction must be structurally valid"),
        &create_wallet_artifact.action,
        "valid multisig create_wallet transaction",
    );
    let valid_tx_id = spora_hashes::Hash::from_bytes(valid_tx.id());
    rpc_client
        .submit_transaction((&valid_tx).into(), false)
        .await
        .expect("valid multisig create_wallet must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_tx_id, "valid multisig create_wallet action").await;

    let propose_transfer_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        propose_transfer_artifact.artifact_bytes.len(),
        "multisig propose_transfer",
    );
    let propose_transfer_deploy_input_capacity = propose_transfer_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_transfer_code_cell_capacity = propose_transfer_deploy_input_capacity
        .checked_sub(required_fee(propose_transfer_deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig propose_transfer scoped action deployment must leave capacity for code cell");
    let propose_transfer_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_transfer_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: propose_transfer_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![propose_transfer_artifact.artifact_bytes.clone()],
    );
    let propose_transfer_deploy_tx_id = spora_hashes::Hash::from_bytes(propose_transfer_deploy_tx.id());
    let propose_transfer_code_outpoint = TransactionOutpoint::new(propose_transfer_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&propose_transfer_deploy_tx).into(), false)
        .await
        .expect("multisig propose_transfer scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        propose_transfer_deploy_tx_id,
        "multisig propose_transfer scoped action deployment",
    )
    .await;

    let propose_transfer_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig propose_transfer fixture");
    let propose_transfer_fixture_input_capacity = propose_transfer_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_transfer_lock = Script::new(propose_transfer_artifact.code_hash, 0, vec![]);
    let propose_transfer_type = Script::new(always_success_code_hash(), 0, b"multisig-propose-transfer-type".to_vec());
    let propose_transfer_wallet_capacity = propose_transfer_fixture_input_capacity / 3;
    let propose_transfer_change_capacity = propose_transfer_fixture_input_capacity
        .checked_sub(propose_transfer_wallet_capacity)
        .and_then(|value| value.checked_sub(required_fee(propose_transfer_fixture_input.len(), 2).saturating_add(100_000)))
        .expect("multisig propose_transfer fixture transaction must leave change");
    let wallet_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_transfer_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: propose_transfer_wallet_capacity,
                lock: propose_transfer_lock.clone(),
                type_: Some(propose_transfer_type.clone()),
            },
            CellOutput { capacity: propose_transfer_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![wallet_payload.clone(), vec![]],
    );
    let wallet_fixture_tx_id = spora_hashes::Hash::from_bytes(wallet_fixture_tx.id());
    let wallet_input = TransactionOutpoint::new(wallet_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&wallet_fixture_tx).into(), false)
        .await
        .expect("multisig propose_transfer fixture wallet cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, wallet_fixture_tx_id, "multisig propose_transfer fixture wallet cell")
        .await;

    let proposer = signer_a;
    let target = [201; 32];
    let transfer_amount = 42u64;
    let transfer_current_time = current_time + 5;
    let proposal_id = 1u64;
    let required_signatures = threshold;
    let expires_at = transfer_current_time + 1440;
    let proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        proposal_id,
        proposer,
        0,
        target,
        transfer_amount,
        &[],
        required_signatures,
        &[],
        transfer_current_time,
        expires_at,
    );
    let malformed_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        proposal_id,
        proposer,
        0,
        target,
        transfer_amount + 1,
        &[],
        required_signatures,
        &[],
        transfer_current_time,
        expires_at,
    );
    let mutated_wallet_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 1, current_time);
    let malformed_wallet_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 0, current_time);
    let propose_transfer_witness = propose_transfer_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(proposer),
            cellscript::EntryWitnessArg::Address(target),
            cellscript::EntryWitnessArg::U64(transfer_amount),
            cellscript::EntryWitnessArg::U64(transfer_current_time),
        ])
        .expect("multisig propose_transfer witness must encode proposer, target, amount, and current_time");
    let propose_transfer_proposal_capacity = propose_transfer_wallet_capacity / 2;
    let propose_transfer_wallet_output_capacity = propose_transfer_wallet_capacity
        .checked_sub(propose_transfer_proposal_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("multisig propose_transfer action must leave wallet output capacity");
    let malformed_propose_transfer_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(wallet_input.tx_hash, wallet_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(propose_transfer_code_outpoint.tx_hash, propose_transfer_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_transfer_wallet_output_capacity,
                    lock: propose_transfer_lock.clone(),
                    type_: Some(propose_transfer_type.clone()),
                },
                CellOutput { capacity: propose_transfer_proposal_capacity, lock: propose_transfer_lock.clone(), type_: None },
            ],
            vec![malformed_wallet_payload, malformed_proposal_payload],
            vec![propose_transfer_witness.clone()],
        )
        .expect("malformed multisig propose_transfer transaction must be structurally valid"),
        &propose_transfer_artifact.action,
        "malformed multisig propose_transfer transaction",
    );
    let malformed_propose_transfer_reason = rpc_client
        .submit_transaction((&malformed_propose_transfer_tx).into(), false)
        .await
        .expect_err("malformed multisig propose_transfer must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_propose_transfer_reason, "multisig propose_transfer");

    let valid_propose_transfer_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(wallet_input.tx_hash, wallet_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(propose_transfer_code_outpoint.tx_hash, propose_transfer_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_transfer_wallet_output_capacity,
                    lock: propose_transfer_lock.clone(),
                    type_: Some(propose_transfer_type),
                },
                CellOutput { capacity: propose_transfer_proposal_capacity, lock: propose_transfer_lock, type_: None },
            ],
            vec![mutated_wallet_payload, proposal_payload],
            vec![propose_transfer_witness],
        )
        .expect("valid multisig propose_transfer transaction must be structurally valid"),
        &propose_transfer_artifact.action,
        "valid multisig propose_transfer transaction",
    );
    let valid_propose_transfer_tx_id = spora_hashes::Hash::from_bytes(valid_propose_transfer_tx.id());
    rpc_client
        .submit_transaction((&valid_propose_transfer_tx).into(), false)
        .await
        .expect("valid multisig propose_transfer must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_propose_transfer_tx_id, "valid multisig propose_transfer action")
        .await;

    let add_signature_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        add_signature_artifact.artifact_bytes.len(),
        "multisig add_signature",
    );
    let add_signature_deploy_input_capacity = add_signature_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let add_signature_code_cell_capacity = add_signature_deploy_input_capacity
        .checked_sub(required_fee(add_signature_deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig add_signature scoped action deployment must leave capacity for code cell");
    let add_signature_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &add_signature_deploy_input,
        vec![],
        vec![CellOutput { capacity: add_signature_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![add_signature_artifact.artifact_bytes.clone()],
    );
    let add_signature_deploy_tx_id = spora_hashes::Hash::from_bytes(add_signature_deploy_tx.id());
    let add_signature_code_outpoint = TransactionOutpoint::new(add_signature_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&add_signature_deploy_tx).into(), false)
        .await
        .expect("multisig add_signature scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        add_signature_deploy_tx_id,
        "multisig add_signature scoped action deployment",
    )
    .await;

    let add_signature_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig add_signature fixture");
    let add_signature_fixture_input_capacity = add_signature_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let add_signature_lock = Script::new(add_signature_artifact.code_hash, 0, vec![]);
    let add_signature_proposal_type = Script::new(always_success_code_hash(), 0, b"multisig-add-signature-proposal-type".to_vec());
    let add_signature_wallet_type = Script::new(always_success_code_hash(), 0, b"multisig-add-signature-wallet-type".to_vec());
    let add_signature_proposal_capacity = add_signature_fixture_input_capacity / 3;
    let add_signature_wallet_capacity = add_signature_fixture_input_capacity / 3;
    let add_signature_change_capacity = add_signature_fixture_input_capacity
        .checked_sub(add_signature_proposal_capacity)
        .and_then(|value| value.checked_sub(add_signature_wallet_capacity))
        .and_then(|value| value.checked_sub(required_fee(add_signature_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("multisig add_signature fixture transaction must leave change");
    let add_signature_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        proposal_id,
        proposer,
        0,
        target,
        transfer_amount,
        &[],
        required_signatures,
        &[],
        transfer_current_time,
        expires_at,
    );
    let add_signature_wallet_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 1, current_time);
    let add_signature_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &add_signature_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: add_signature_proposal_capacity,
                lock: add_signature_lock.clone(),
                type_: Some(add_signature_proposal_type.clone()),
            },
            CellOutput {
                capacity: add_signature_wallet_capacity,
                lock: add_signature_lock.clone(),
                type_: Some(add_signature_wallet_type.clone()),
            },
            CellOutput { capacity: add_signature_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![add_signature_proposal_payload.clone(), add_signature_wallet_payload.clone(), vec![]],
    );
    let add_signature_fixture_tx_id = spora_hashes::Hash::from_bytes(add_signature_fixture_tx.id());
    let add_signature_proposal_input = TransactionOutpoint::new(add_signature_fixture_tx.id(), 0);
    let add_signature_wallet_input = TransactionOutpoint::new(add_signature_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&add_signature_fixture_tx).into(), false)
        .await
        .expect("multisig add_signature fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, add_signature_fixture_tx_id, "multisig add_signature fixture cells")
        .await;

    let signature_signer = signer_b;
    let signature_bytes = [0xabu8; 64];
    let signature_current_time = transfer_current_time + 1;
    let signed_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        proposal_id,
        proposer,
        0,
        target,
        transfer_amount,
        &[],
        required_signatures,
        &[multisig_signature_struct(signature_signer, signature_bytes)],
        transfer_current_time,
        expires_at,
    );
    let malformed_signed_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        proposal_id,
        proposer,
        0,
        target,
        transfer_amount,
        &[],
        required_signatures,
        &[],
        transfer_current_time,
        expires_at,
    );
    let signature_confirmation_payload =
        multisig_signature_confirmation_cell_data(proposal_id, signature_signer, signature_current_time);
    let add_signature_witness = add_signature_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(signature_signer),
            cellscript::EntryWitnessArg::Bytes(signature_bytes.to_vec()),
            cellscript::EntryWitnessArg::U64(signature_current_time),
        ])
        .expect("multisig add_signature witness must encode signer, signature, and current_time");
    let add_signature_confirmation_capacity = add_signature_proposal_capacity / 4;
    let add_signature_output_proposal_capacity = add_signature_proposal_capacity
        .checked_sub(add_signature_confirmation_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("multisig add_signature action must leave proposal output capacity");
    let malformed_add_signature_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(add_signature_proposal_input.tx_hash, add_signature_proposal_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(add_signature_wallet_input.tx_hash, add_signature_wallet_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(add_signature_code_outpoint.tx_hash, add_signature_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: add_signature_output_proposal_capacity,
                    lock: add_signature_lock.clone(),
                    type_: Some(add_signature_proposal_type.clone()),
                },
                CellOutput { capacity: add_signature_confirmation_capacity, lock: add_signature_lock.clone(), type_: None },
            ],
            vec![malformed_signed_proposal_payload, signature_confirmation_payload.clone()],
            vec![add_signature_witness.clone()],
        )
        .expect("malformed multisig add_signature transaction must be structurally valid"),
        &add_signature_artifact.action,
        "malformed multisig add_signature transaction",
    );
    let malformed_add_signature_reason = rpc_client
        .submit_transaction((&malformed_add_signature_tx).into(), false)
        .await
        .expect_err("malformed multisig add_signature must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_add_signature_reason, "multisig add_signature");

    let valid_add_signature_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(add_signature_proposal_input.tx_hash, add_signature_proposal_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(add_signature_wallet_input.tx_hash, add_signature_wallet_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(add_signature_code_outpoint.tx_hash, add_signature_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: add_signature_output_proposal_capacity,
                    lock: add_signature_lock.clone(),
                    type_: Some(add_signature_proposal_type),
                },
                CellOutput { capacity: add_signature_confirmation_capacity, lock: add_signature_lock.clone(), type_: None },
            ],
            vec![signed_proposal_payload, signature_confirmation_payload],
            vec![add_signature_witness],
        )
        .expect("valid multisig add_signature transaction must be structurally valid"),
        &add_signature_artifact.action,
        "valid multisig add_signature transaction",
    );
    let valid_add_signature_tx_id = spora_hashes::Hash::from_bytes(valid_add_signature_tx.id());
    rpc_client
        .submit_transaction((&valid_add_signature_tx).into(), false)
        .await
        .expect("valid multisig add_signature must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_add_signature_tx_id, "valid multisig add_signature action").await;

    let execute_proposal_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        execute_proposal_artifact.artifact_bytes.len(),
        "multisig execute_proposal",
    );
    let execute_proposal_deploy_input_capacity = execute_proposal_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let execute_proposal_code_cell_capacity = execute_proposal_deploy_input_capacity
        .checked_sub(required_fee(execute_proposal_deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig execute_proposal scoped action deployment must leave capacity for code cell");
    let execute_proposal_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &execute_proposal_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: execute_proposal_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![execute_proposal_artifact.artifact_bytes.clone()],
    );
    let execute_proposal_deploy_tx_id = spora_hashes::Hash::from_bytes(execute_proposal_deploy_tx.id());
    let execute_proposal_code_outpoint = TransactionOutpoint::new(execute_proposal_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&execute_proposal_deploy_tx).into(), false)
        .await
        .expect("multisig execute_proposal scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        execute_proposal_deploy_tx_id,
        "multisig execute_proposal scoped action deployment",
    )
    .await;

    let execute_proposal_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig execute_proposal fixture");
    let execute_proposal_fixture_input_capacity = execute_proposal_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let execute_proposal_lock = Script::new(execute_proposal_artifact.code_hash, 0, vec![]);
    let execute_proposal_proposal_type = Script::new(always_success_code_hash(), 0, b"multisig-execute-proposal-type".to_vec());
    let execute_proposal_wallet_type = Script::new(always_success_code_hash(), 0, b"multisig-execute-wallet-type".to_vec());
    let execute_proposal_proposal_capacity = execute_proposal_fixture_input_capacity / 3;
    let execute_proposal_wallet_capacity = execute_proposal_fixture_input_capacity / 3;
    let execute_proposal_change_capacity = execute_proposal_fixture_input_capacity
        .checked_sub(execute_proposal_proposal_capacity)
        .and_then(|value| value.checked_sub(execute_proposal_wallet_capacity))
        .and_then(|value| value.checked_sub(required_fee(execute_proposal_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("multisig execute_proposal fixture transaction must leave change");
    let signer_a_signature = multisig_signature_struct(signer_a, [0xa1; 64]);
    let signer_b_signature = multisig_signature_struct(signer_b, [0xb2; 64]);
    let executable_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        proposal_id,
        proposer,
        0,
        target,
        transfer_amount,
        &[],
        required_signatures,
        &[signer_a_signature, signer_b_signature],
        transfer_current_time,
        expires_at,
    );
    let execute_wallet_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 1, current_time);
    let execute_proposal_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &execute_proposal_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: execute_proposal_proposal_capacity,
                lock: execute_proposal_lock.clone(),
                type_: Some(execute_proposal_proposal_type.clone()),
            },
            CellOutput {
                capacity: execute_proposal_wallet_capacity,
                lock: execute_proposal_lock.clone(),
                type_: Some(execute_proposal_wallet_type.clone()),
            },
            CellOutput { capacity: execute_proposal_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![executable_proposal_payload.clone(), execute_wallet_payload.clone(), vec![]],
    );
    let execute_proposal_fixture_tx_id = spora_hashes::Hash::from_bytes(execute_proposal_fixture_tx.id());
    let execute_proposal_input = TransactionOutpoint::new(execute_proposal_fixture_tx.id(), 0);
    let execute_wallet_input = TransactionOutpoint::new(execute_proposal_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&execute_proposal_fixture_tx).into(), false)
        .await
        .expect("multisig execute_proposal fixture cells must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        execute_proposal_fixture_tx_id,
        "multisig execute_proposal fixture cells",
    )
    .await;

    let executor = signer_b;
    let execute_current_time = transfer_current_time + 2;
    let execution_record_payload = multisig_execution_record_cell_data(proposal_id, executor, execute_current_time, true);
    let malformed_execution_record_payload = multisig_execution_record_cell_data(proposal_id, executor, execute_current_time, false);
    let execute_proposal_witness = execute_proposal_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(executor), cellscript::EntryWitnessArg::U64(execute_current_time)])
        .expect("multisig execute_proposal witness must encode executor and current_time");
    let execute_record_output_capacity = execute_proposal_proposal_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("multisig execute_proposal action must leave execution record output capacity");
    let malformed_execute_proposal_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(execute_proposal_input.tx_hash, execute_proposal_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(execute_wallet_input.tx_hash, execute_wallet_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(execute_proposal_code_outpoint.tx_hash, execute_proposal_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: execute_record_output_capacity, lock: execute_proposal_lock.clone(), type_: None }],
            vec![malformed_execution_record_payload],
            vec![execute_proposal_witness.clone()],
        )
        .expect("malformed multisig execute_proposal transaction must be structurally valid"),
        &execute_proposal_artifact.action,
        "malformed multisig execute_proposal transaction",
    );
    let malformed_execute_proposal_reason = rpc_client
        .submit_transaction((&malformed_execute_proposal_tx).into(), false)
        .await
        .expect_err("malformed multisig execute_proposal must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_execute_proposal_reason, "multisig execute_proposal");

    let valid_execute_proposal_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(execute_proposal_input.tx_hash, execute_proposal_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(execute_wallet_input.tx_hash, execute_wallet_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(execute_proposal_code_outpoint.tx_hash, execute_proposal_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: execute_record_output_capacity, lock: execute_proposal_lock, type_: None }],
            vec![execution_record_payload],
            vec![execute_proposal_witness],
        )
        .expect("valid multisig execute_proposal transaction must be structurally valid"),
        &execute_proposal_artifact.action,
        "valid multisig execute_proposal transaction",
    );
    let valid_execute_proposal_tx_id = spora_hashes::Hash::from_bytes(valid_execute_proposal_tx.id());
    rpc_client
        .submit_transaction((&valid_execute_proposal_tx).into(), false)
        .await
        .expect("valid multisig execute_proposal must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_execute_proposal_tx_id, "valid multisig execute_proposal action")
        .await;

    let cancel_proposal_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        cancel_proposal_artifact.artifact_bytes.len(),
        "multisig cancel_proposal",
    );
    let cancel_proposal_deploy_input_capacity = cancel_proposal_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let cancel_proposal_code_cell_capacity = cancel_proposal_deploy_input_capacity
        .checked_sub(required_fee(cancel_proposal_deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig cancel_proposal scoped action deployment must leave capacity for code cell");
    let cancel_proposal_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &cancel_proposal_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: cancel_proposal_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![cancel_proposal_artifact.artifact_bytes.clone()],
    );
    let cancel_proposal_deploy_tx_id = spora_hashes::Hash::from_bytes(cancel_proposal_deploy_tx.id());
    let cancel_proposal_code_outpoint = TransactionOutpoint::new(cancel_proposal_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&cancel_proposal_deploy_tx).into(), false)
        .await
        .expect("multisig cancel_proposal scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        cancel_proposal_deploy_tx_id,
        "multisig cancel_proposal scoped action deployment",
    )
    .await;

    let cancel_proposal_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig cancel_proposal fixture");
    let cancel_proposal_fixture_input_capacity = cancel_proposal_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let cancel_proposal_lock = Script::new(cancel_proposal_artifact.code_hash, 0, vec![]);
    let cancel_proposal_proposal_type = Script::new(always_success_code_hash(), 0, b"multisig-cancel-proposal-type".to_vec());
    let cancel_proposal_wallet_type = Script::new(always_success_code_hash(), 0, b"multisig-cancel-wallet-type".to_vec());
    let cancel_proposal_proposal_capacity = cancel_proposal_fixture_input_capacity / 3;
    let cancel_proposal_wallet_capacity = cancel_proposal_fixture_input_capacity / 3;
    let cancel_proposal_change_capacity = cancel_proposal_fixture_input_capacity
        .checked_sub(cancel_proposal_proposal_capacity)
        .and_then(|value| value.checked_sub(cancel_proposal_wallet_capacity))
        .and_then(|value| value.checked_sub(required_fee(cancel_proposal_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("multisig cancel_proposal fixture transaction must leave change");
    let cancellable_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        proposal_id,
        proposer,
        0,
        target,
        transfer_amount,
        &[],
        required_signatures,
        &[],
        transfer_current_time,
        expires_at,
    );
    let cancel_wallet_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 1, current_time);
    let cancel_proposal_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &cancel_proposal_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: cancel_proposal_proposal_capacity,
                lock: cancel_proposal_lock.clone(),
                type_: Some(cancel_proposal_proposal_type.clone()),
            },
            CellOutput {
                capacity: cancel_proposal_wallet_capacity,
                lock: cancel_proposal_lock.clone(),
                type_: Some(cancel_proposal_wallet_type.clone()),
            },
            CellOutput { capacity: cancel_proposal_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![cancellable_proposal_payload.clone(), cancel_wallet_payload.clone(), vec![]],
    );
    let cancel_proposal_fixture_tx_id = spora_hashes::Hash::from_bytes(cancel_proposal_fixture_tx.id());
    let cancel_proposal_input = TransactionOutpoint::new(cancel_proposal_fixture_tx.id(), 0);
    let cancel_wallet_input = TransactionOutpoint::new(cancel_proposal_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&cancel_proposal_fixture_tx).into(), false)
        .await
        .expect("multisig cancel_proposal fixture cells must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        cancel_proposal_fixture_tx_id,
        "multisig cancel_proposal fixture cells",
    )
    .await;

    let malformed_cancel_witness = cancel_proposal_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(signer_b)])
        .expect("multisig cancel_proposal malformed witness must encode canceller");
    let cancel_change_capacity = cancel_proposal_proposal_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("multisig cancel_proposal action must leave change output capacity");
    let malformed_cancel_proposal_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(cancel_proposal_input.tx_hash, cancel_proposal_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(cancel_wallet_input.tx_hash, cancel_wallet_input.index), dep_type: DepType::Code },
                CellDep {
                    out_point: OutPoint::new(cancel_proposal_code_outpoint.tx_hash, cancel_proposal_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: cancel_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
            vec![vec![]],
            vec![malformed_cancel_witness],
        )
        .expect("malformed multisig cancel_proposal transaction must be structurally valid"),
        &cancel_proposal_artifact.action,
        "malformed multisig cancel_proposal transaction",
    );
    let malformed_cancel_proposal_reason = rpc_client
        .submit_transaction((&malformed_cancel_proposal_tx).into(), false)
        .await
        .expect_err("malformed multisig cancel_proposal must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_cancel_proposal_reason, "multisig cancel_proposal");

    let valid_cancel_witness = cancel_proposal_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(proposer)])
        .expect("multisig cancel_proposal valid witness must encode canceller");
    let valid_cancel_proposal_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(cancel_proposal_input.tx_hash, cancel_proposal_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(cancel_wallet_input.tx_hash, cancel_wallet_input.index), dep_type: DepType::Code },
                CellDep {
                    out_point: OutPoint::new(cancel_proposal_code_outpoint.tx_hash, cancel_proposal_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: cancel_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
            vec![vec![]],
            vec![valid_cancel_witness],
        )
        .expect("valid multisig cancel_proposal transaction must be structurally valid"),
        &cancel_proposal_artifact.action,
        "valid multisig cancel_proposal transaction",
    );
    let valid_cancel_proposal_tx_id = spora_hashes::Hash::from_bytes(valid_cancel_proposal_tx.id());
    rpc_client
        .submit_transaction((&valid_cancel_proposal_tx).into(), false)
        .await
        .expect("valid multisig cancel_proposal must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_cancel_proposal_tx_id, "valid multisig cancel_proposal action")
        .await;

    let new_signer = pay_to_acceptance_owner(
        &Address::new_std_single(NetworkType::Devnet.into(), &[193; 32]).expect("multisig new_signer address must be valid"),
    )
    .hash();
    let remove_signer = signer_b;
    let change_threshold = 2u8;

    let propose_wallet_input_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 0, current_time);
    let propose_wallet_output_payload = multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 1, current_time);
    let malformed_propose_wallet_output_payload =
        multisig_wallet_molecule_cell_data(&[signer_a, signer_b], threshold, 0, current_time);

    let propose_add_signer_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        propose_add_signer_artifact.artifact_bytes.len(),
        "multisig propose_add_signer",
    );
    let propose_add_signer_deploy_input_capacity =
        propose_add_signer_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_add_signer_code_cell_capacity = propose_add_signer_deploy_input_capacity
        .checked_sub(required_fee(propose_add_signer_deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig propose_add_signer scoped action deployment must leave capacity for code cell");
    let propose_add_signer_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_add_signer_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: propose_add_signer_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![propose_add_signer_artifact.artifact_bytes.clone()],
    );
    let propose_add_signer_deploy_tx_id = spora_hashes::Hash::from_bytes(propose_add_signer_deploy_tx.id());
    let propose_add_signer_code_outpoint = TransactionOutpoint::new(propose_add_signer_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&propose_add_signer_deploy_tx).into(), false)
        .await
        .expect("multisig propose_add_signer scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        propose_add_signer_deploy_tx_id,
        "multisig propose_add_signer scoped action deployment",
    )
    .await;

    let propose_add_signer_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig propose_add_signer fixture");
    let propose_add_signer_fixture_input_capacity =
        propose_add_signer_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_add_signer_lock = Script::new(propose_add_signer_artifact.code_hash, 0, vec![]);
    let propose_add_signer_wallet_type =
        Script::new(always_success_code_hash(), 0, b"multisig-propose-add-signer-wallet-type".to_vec());
    let propose_add_signer_wallet_capacity = propose_add_signer_fixture_input_capacity / 3;
    let propose_add_signer_change_capacity = propose_add_signer_fixture_input_capacity
        .checked_sub(propose_add_signer_wallet_capacity)
        .and_then(|value| value.checked_sub(required_fee(propose_add_signer_fixture_input.len(), 2).saturating_add(100_000)))
        .expect("multisig propose_add_signer fixture transaction must leave change");
    let propose_add_signer_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_add_signer_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: propose_add_signer_wallet_capacity,
                lock: propose_add_signer_lock.clone(),
                type_: Some(propose_add_signer_wallet_type.clone()),
            },
            CellOutput { capacity: propose_add_signer_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![propose_wallet_input_payload.clone(), vec![]],
    );
    let propose_add_signer_fixture_tx_id = spora_hashes::Hash::from_bytes(propose_add_signer_fixture_tx.id());
    let propose_add_signer_wallet_input = TransactionOutpoint::new(propose_add_signer_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&propose_add_signer_fixture_tx).into(), false)
        .await
        .expect("multisig propose_add_signer fixture wallet cell must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        propose_add_signer_fixture_tx_id,
        "multisig propose_add_signer fixture wallet cell",
    )
    .await;

    let propose_add_signer_current_time = current_time + 6;
    let propose_add_signer_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        1,
        proposer,
        1,
        new_signer,
        0,
        &new_signer,
        threshold,
        &[],
        propose_add_signer_current_time,
        propose_add_signer_current_time + 1440,
    );
    let malformed_propose_add_signer_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        1,
        proposer,
        1,
        new_signer,
        1,
        &new_signer,
        threshold,
        &[],
        propose_add_signer_current_time,
        propose_add_signer_current_time + 1440,
    );
    let propose_add_signer_witness = propose_add_signer_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(proposer),
            cellscript::EntryWitnessArg::Address(new_signer),
            cellscript::EntryWitnessArg::U64(propose_add_signer_current_time),
        ])
        .expect("multisig propose_add_signer witness must encode proposer, new_signer, and current_time");
    let propose_add_signer_proposal_capacity = propose_add_signer_wallet_capacity / 2;
    let propose_add_signer_wallet_output_capacity = propose_add_signer_wallet_capacity
        .checked_sub(propose_add_signer_proposal_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("multisig propose_add_signer action must leave wallet output capacity");
    let malformed_propose_add_signer_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(propose_add_signer_wallet_input.tx_hash, propose_add_signer_wallet_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(propose_add_signer_code_outpoint.tx_hash, propose_add_signer_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_add_signer_wallet_output_capacity,
                    lock: propose_add_signer_lock.clone(),
                    type_: Some(propose_add_signer_wallet_type.clone()),
                },
                CellOutput { capacity: propose_add_signer_proposal_capacity, lock: propose_add_signer_lock.clone(), type_: None },
            ],
            vec![malformed_propose_wallet_output_payload.clone(), malformed_propose_add_signer_proposal_payload],
            vec![propose_add_signer_witness.clone()],
        )
        .expect("malformed multisig propose_add_signer transaction must be structurally valid"),
        &propose_add_signer_artifact.action,
        "malformed multisig propose_add_signer transaction",
    );
    let malformed_propose_add_signer_reason = rpc_client
        .submit_transaction((&malformed_propose_add_signer_tx).into(), false)
        .await
        .expect_err("malformed multisig propose_add_signer must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_propose_add_signer_reason, "multisig propose_add_signer");

    let valid_propose_add_signer_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(propose_add_signer_wallet_input.tx_hash, propose_add_signer_wallet_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(propose_add_signer_code_outpoint.tx_hash, propose_add_signer_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_add_signer_wallet_output_capacity,
                    lock: propose_add_signer_lock.clone(),
                    type_: Some(propose_add_signer_wallet_type.clone()),
                },
                CellOutput { capacity: propose_add_signer_proposal_capacity, lock: propose_add_signer_lock.clone(), type_: None },
            ],
            vec![propose_wallet_output_payload.clone(), propose_add_signer_proposal_payload],
            vec![propose_add_signer_witness],
        )
        .expect("valid multisig propose_add_signer transaction must be structurally valid"),
        &propose_add_signer_artifact.action,
        "valid multisig propose_add_signer transaction",
    );
    let valid_propose_add_signer_tx_id = spora_hashes::Hash::from_bytes(valid_propose_add_signer_tx.id());
    rpc_client
        .submit_transaction((&valid_propose_add_signer_tx).into(), false)
        .await
        .expect("valid multisig propose_add_signer must be accepted by the scoped action verifier");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        valid_propose_add_signer_tx_id,
        "valid multisig propose_add_signer action",
    )
    .await;

    let propose_remove_signer_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        propose_remove_signer_artifact.artifact_bytes.len(),
        "multisig propose_remove_signer",
    );
    let propose_remove_signer_deploy_input_capacity =
        propose_remove_signer_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_remove_signer_code_cell_capacity = propose_remove_signer_deploy_input_capacity
        .checked_sub(required_fee(propose_remove_signer_deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig propose_remove_signer scoped action deployment must leave capacity for code cell");
    let propose_remove_signer_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_remove_signer_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: propose_remove_signer_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![propose_remove_signer_artifact.artifact_bytes.clone()],
    );
    let propose_remove_signer_deploy_tx_id = spora_hashes::Hash::from_bytes(propose_remove_signer_deploy_tx.id());
    let propose_remove_signer_code_outpoint = TransactionOutpoint::new(propose_remove_signer_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&propose_remove_signer_deploy_tx).into(), false)
        .await
        .expect("multisig propose_remove_signer scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        propose_remove_signer_deploy_tx_id,
        "multisig propose_remove_signer scoped action deployment",
    )
    .await;

    let propose_remove_signer_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig propose_remove_signer fixture");
    let propose_remove_signer_fixture_input_capacity =
        propose_remove_signer_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_remove_signer_lock = Script::new(propose_remove_signer_artifact.code_hash, 0, vec![]);
    let propose_remove_signer_wallet_type =
        Script::new(always_success_code_hash(), 0, b"multisig-propose-remove-signer-wallet-type".to_vec());
    let propose_remove_signer_wallet_capacity = propose_remove_signer_fixture_input_capacity / 3;
    let propose_remove_signer_change_capacity = propose_remove_signer_fixture_input_capacity
        .checked_sub(propose_remove_signer_wallet_capacity)
        .and_then(|value| value.checked_sub(required_fee(propose_remove_signer_fixture_input.len(), 2).saturating_add(100_000)))
        .expect("multisig propose_remove_signer fixture transaction must leave change");
    let propose_remove_signer_wallet_input_payload =
        multisig_wallet_molecule_cell_data(&[signer_a, signer_b, new_signer], threshold, 0, current_time);
    let propose_remove_signer_wallet_output_payload =
        multisig_wallet_molecule_cell_data(&[signer_a, signer_b, new_signer], threshold, 1, current_time);
    let malformed_propose_remove_signer_wallet_output_payload =
        multisig_wallet_molecule_cell_data(&[signer_a, signer_b, new_signer], threshold, 0, current_time);
    let propose_remove_signer_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_remove_signer_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: propose_remove_signer_wallet_capacity,
                lock: propose_remove_signer_lock.clone(),
                type_: Some(propose_remove_signer_wallet_type.clone()),
            },
            CellOutput {
                capacity: propose_remove_signer_change_capacity,
                lock: pay_to_acceptance_owner(prealloc_address),
                type_: None,
            },
        ],
        vec![propose_remove_signer_wallet_input_payload, vec![]],
    );
    let propose_remove_signer_fixture_tx_id = spora_hashes::Hash::from_bytes(propose_remove_signer_fixture_tx.id());
    let propose_remove_signer_wallet_input = TransactionOutpoint::new(propose_remove_signer_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&propose_remove_signer_fixture_tx).into(), false)
        .await
        .expect("multisig propose_remove_signer fixture wallet cell must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        propose_remove_signer_fixture_tx_id,
        "multisig propose_remove_signer fixture wallet cell",
    )
    .await;

    let propose_remove_signer_current_time = current_time + 7;
    let propose_remove_signer_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        1,
        proposer,
        2,
        remove_signer,
        0,
        &[],
        threshold,
        &[],
        propose_remove_signer_current_time,
        propose_remove_signer_current_time + 1440,
    );
    let malformed_propose_remove_signer_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        1,
        proposer,
        2,
        remove_signer,
        1,
        &[],
        threshold,
        &[],
        propose_remove_signer_current_time,
        propose_remove_signer_current_time + 1440,
    );
    let propose_remove_signer_witness = propose_remove_signer_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(proposer),
            cellscript::EntryWitnessArg::Address(remove_signer),
            cellscript::EntryWitnessArg::U64(propose_remove_signer_current_time),
        ])
        .expect("multisig propose_remove_signer witness must encode proposer, signer_to_remove, and current_time");
    let propose_remove_signer_proposal_capacity = propose_remove_signer_wallet_capacity / 2;
    let propose_remove_signer_wallet_output_capacity = propose_remove_signer_wallet_capacity
        .checked_sub(propose_remove_signer_proposal_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("multisig propose_remove_signer action must leave wallet output capacity");
    let malformed_propose_remove_signer_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(
                OutPoint::new(propose_remove_signer_wallet_input.tx_hash, propose_remove_signer_wallet_input.index),
                0,
            )],
            vec![
                CellDep {
                    out_point: OutPoint::new(propose_remove_signer_code_outpoint.tx_hash, propose_remove_signer_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_remove_signer_wallet_output_capacity,
                    lock: propose_remove_signer_lock.clone(),
                    type_: Some(propose_remove_signer_wallet_type.clone()),
                },
                CellOutput {
                    capacity: propose_remove_signer_proposal_capacity,
                    lock: propose_remove_signer_lock.clone(),
                    type_: None,
                },
            ],
            vec![malformed_propose_remove_signer_wallet_output_payload, malformed_propose_remove_signer_proposal_payload],
            vec![propose_remove_signer_witness.clone()],
        )
        .expect("malformed multisig propose_remove_signer transaction must be structurally valid"),
        &propose_remove_signer_artifact.action,
        "malformed multisig propose_remove_signer transaction",
    );
    let malformed_propose_remove_signer_reason = rpc_client
        .submit_transaction((&malformed_propose_remove_signer_tx).into(), false)
        .await
        .expect_err("malformed multisig propose_remove_signer must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_propose_remove_signer_reason, "multisig propose_remove_signer");

    let valid_propose_remove_signer_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(
                OutPoint::new(propose_remove_signer_wallet_input.tx_hash, propose_remove_signer_wallet_input.index),
                0,
            )],
            vec![
                CellDep {
                    out_point: OutPoint::new(propose_remove_signer_code_outpoint.tx_hash, propose_remove_signer_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_remove_signer_wallet_output_capacity,
                    lock: propose_remove_signer_lock.clone(),
                    type_: Some(propose_remove_signer_wallet_type.clone()),
                },
                CellOutput {
                    capacity: propose_remove_signer_proposal_capacity,
                    lock: propose_remove_signer_lock.clone(),
                    type_: None,
                },
            ],
            vec![propose_remove_signer_wallet_output_payload, propose_remove_signer_proposal_payload],
            vec![propose_remove_signer_witness],
        )
        .expect("valid multisig propose_remove_signer transaction must be structurally valid"),
        &propose_remove_signer_artifact.action,
        "valid multisig propose_remove_signer transaction",
    );
    let valid_propose_remove_signer_tx_id = spora_hashes::Hash::from_bytes(valid_propose_remove_signer_tx.id());
    rpc_client
        .submit_transaction((&valid_propose_remove_signer_tx).into(), false)
        .await
        .expect("valid multisig propose_remove_signer must be accepted by the scoped action verifier");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        valid_propose_remove_signer_tx_id,
        "valid multisig propose_remove_signer action",
    )
    .await;

    let propose_change_threshold_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        propose_change_threshold_artifact.artifact_bytes.len(),
        "multisig propose_change_threshold",
    );
    let propose_change_threshold_deploy_input_capacity =
        propose_change_threshold_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_change_threshold_code_cell_capacity = propose_change_threshold_deploy_input_capacity
        .checked_sub(required_fee(propose_change_threshold_deploy_input.len(), 1).saturating_add(100_000))
        .expect("multisig propose_change_threshold scoped action deployment must leave capacity for code cell");
    let propose_change_threshold_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_change_threshold_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: propose_change_threshold_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![propose_change_threshold_artifact.artifact_bytes.clone()],
    );
    let propose_change_threshold_deploy_tx_id = spora_hashes::Hash::from_bytes(propose_change_threshold_deploy_tx.id());
    let propose_change_threshold_code_outpoint = TransactionOutpoint::new(propose_change_threshold_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&propose_change_threshold_deploy_tx).into(), false)
        .await
        .expect("multisig propose_change_threshold scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        propose_change_threshold_deploy_tx_id,
        "multisig propose_change_threshold scoped action deployment",
    )
    .await;

    let propose_change_threshold_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "multisig propose_change_threshold fixture");
    let propose_change_threshold_fixture_input_capacity =
        propose_change_threshold_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let propose_change_threshold_lock = Script::new(propose_change_threshold_artifact.code_hash, 0, vec![]);
    let propose_change_threshold_wallet_type =
        Script::new(always_success_code_hash(), 0, b"multisig-propose-change-threshold-wallet-type".to_vec());
    let propose_change_threshold_wallet_capacity = propose_change_threshold_fixture_input_capacity / 3;
    let propose_change_threshold_change_capacity = propose_change_threshold_fixture_input_capacity
        .checked_sub(propose_change_threshold_wallet_capacity)
        .and_then(|value| value.checked_sub(required_fee(propose_change_threshold_fixture_input.len(), 2).saturating_add(100_000)))
        .expect("multisig propose_change_threshold fixture transaction must leave change");
    let propose_change_threshold_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &propose_change_threshold_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: propose_change_threshold_wallet_capacity,
                lock: propose_change_threshold_lock.clone(),
                type_: Some(propose_change_threshold_wallet_type.clone()),
            },
            CellOutput {
                capacity: propose_change_threshold_change_capacity,
                lock: pay_to_acceptance_owner(prealloc_address),
                type_: None,
            },
        ],
        vec![propose_wallet_input_payload, vec![]],
    );
    let propose_change_threshold_fixture_tx_id = spora_hashes::Hash::from_bytes(propose_change_threshold_fixture_tx.id());
    let propose_change_threshold_wallet_input = TransactionOutpoint::new(propose_change_threshold_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&propose_change_threshold_fixture_tx).into(), false)
        .await
        .expect("multisig propose_change_threshold fixture wallet cell must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        propose_change_threshold_fixture_tx_id,
        "multisig propose_change_threshold fixture wallet cell",
    )
    .await;

    let propose_change_threshold_current_time = current_time + 8;
    let propose_change_threshold_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        1,
        proposer,
        3,
        [0; 32],
        change_threshold as u64,
        &[change_threshold],
        threshold,
        &[],
        propose_change_threshold_current_time,
        propose_change_threshold_current_time + 1440,
    );
    let malformed_propose_change_threshold_proposal_payload = multisig_proposal_molecule_cell_data(
        [0; 32],
        1,
        proposer,
        3,
        [0; 32],
        (change_threshold as u64) + 1,
        &[change_threshold],
        threshold,
        &[],
        propose_change_threshold_current_time,
        propose_change_threshold_current_time + 1440,
    );
    let propose_change_threshold_witness = propose_change_threshold_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(proposer),
            cellscript::EntryWitnessArg::U8(change_threshold),
            cellscript::EntryWitnessArg::U64(propose_change_threshold_current_time),
        ])
        .expect("multisig propose_change_threshold witness must encode proposer, new_threshold, and current_time");
    let propose_change_threshold_proposal_capacity = propose_change_threshold_wallet_capacity / 2;
    let propose_change_threshold_wallet_output_capacity = propose_change_threshold_wallet_capacity
        .checked_sub(propose_change_threshold_proposal_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("multisig propose_change_threshold action must leave wallet output capacity");
    let malformed_propose_change_threshold_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(
                OutPoint::new(propose_change_threshold_wallet_input.tx_hash, propose_change_threshold_wallet_input.index),
                0,
            )],
            vec![
                CellDep {
                    out_point: OutPoint::new(
                        propose_change_threshold_code_outpoint.tx_hash,
                        propose_change_threshold_code_outpoint.index,
                    ),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_change_threshold_wallet_output_capacity,
                    lock: propose_change_threshold_lock.clone(),
                    type_: Some(propose_change_threshold_wallet_type.clone()),
                },
                CellOutput {
                    capacity: propose_change_threshold_proposal_capacity,
                    lock: propose_change_threshold_lock.clone(),
                    type_: None,
                },
            ],
            vec![malformed_propose_wallet_output_payload, malformed_propose_change_threshold_proposal_payload],
            vec![propose_change_threshold_witness.clone()],
        )
        .expect("malformed multisig propose_change_threshold transaction must be structurally valid"),
        &propose_change_threshold_artifact.action,
        "malformed multisig propose_change_threshold transaction",
    );
    let malformed_propose_change_threshold_reason = rpc_client
        .submit_transaction((&malformed_propose_change_threshold_tx).into(), false)
        .await
        .expect_err("malformed multisig propose_change_threshold must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_propose_change_threshold_reason, "multisig propose_change_threshold");

    let valid_propose_change_threshold_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(
                OutPoint::new(propose_change_threshold_wallet_input.tx_hash, propose_change_threshold_wallet_input.index),
                0,
            )],
            vec![
                CellDep {
                    out_point: OutPoint::new(
                        propose_change_threshold_code_outpoint.tx_hash,
                        propose_change_threshold_code_outpoint.index,
                    ),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput {
                    capacity: propose_change_threshold_wallet_output_capacity,
                    lock: propose_change_threshold_lock.clone(),
                    type_: Some(propose_change_threshold_wallet_type.clone()),
                },
                CellOutput {
                    capacity: propose_change_threshold_proposal_capacity,
                    lock: propose_change_threshold_lock.clone(),
                    type_: None,
                },
            ],
            vec![propose_wallet_output_payload, propose_change_threshold_proposal_payload],
            vec![propose_change_threshold_witness],
        )
        .expect("valid multisig propose_change_threshold transaction must be structurally valid"),
        &propose_change_threshold_artifact.action,
        "valid multisig propose_change_threshold transaction",
    );
    let valid_propose_change_threshold_tx_id = spora_hashes::Hash::from_bytes(valid_propose_change_threshold_tx.id());
    rpc_client
        .submit_transaction((&valid_propose_change_threshold_tx).into(), false)
        .await
        .expect("valid multisig propose_change_threshold must be accepted by the scoped action verifier");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        valid_propose_change_threshold_tx_id,
        "valid multisig propose_change_threshold action",
    )
    .await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    coverage.valid.insert(("multisig.cell".to_string(), "create_wallet".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "create_wallet".to_string()));
    coverage.valid.insert(("multisig.cell".to_string(), "propose_transfer".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "propose_transfer".to_string()));
    coverage.valid.insert(("multisig.cell".to_string(), "add_signature".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "add_signature".to_string()));
    coverage.valid.insert(("multisig.cell".to_string(), "execute_proposal".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "execute_proposal".to_string()));
    coverage.valid.insert(("multisig.cell".to_string(), "cancel_proposal".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "cancel_proposal".to_string()));
    coverage.valid.insert(("multisig.cell".to_string(), "propose_add_signer".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "propose_add_signer".to_string()));
    coverage.valid.insert(("multisig.cell".to_string(), "propose_remove_signer".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "propose_remove_signer".to_string()));
    coverage.valid.insert(("multisig.cell".to_string(), "propose_change_threshold".to_string()));
    coverage.malformed.insert(("multisig.cell".to_string(), "propose_change_threshold".to_string()));
    coverage
}

async fn run_timelock_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(timelock_deployment) = deployments.iter().find(|deployment| deployment.name == "timelock.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(create_absolute_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "create_absolute_lock")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(create_relative_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "create_relative_lock")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(lock_asset_artifact) = timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "lock_asset") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(request_release_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "request_release")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(request_emergency_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "request_emergency_release")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(approve_emergency_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "approve_emergency_release")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(execute_emergency_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "execute_emergency_release")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(execute_release_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "execute_release")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(extend_lock_artifact) = timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "extend_lock")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(batch_create_artifact) =
        timelock_deployment.action_artifacts.iter().find(|artifact| artifact.name == "batch_create_locks")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    submit_empty_blocks(rpc_client, miner_address, 10).await;
    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 20,
        "timelock action builder matrix needs twenty matured plain prealloc cells for scoped deploys and executable fixtures"
    );

    let absolute_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        create_absolute_artifact.artifact_bytes.len(),
        "timelock create_absolute_lock",
    );
    let absolute_deploy_input_capacity = absolute_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let absolute_code_cell_capacity = absolute_deploy_input_capacity
        .checked_sub(required_fee(absolute_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock create_absolute_lock scoped action deployment must leave capacity for code cell");
    let absolute_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &absolute_deploy_input,
        vec![],
        vec![CellOutput { capacity: absolute_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![create_absolute_artifact.artifact_bytes.clone()],
    );
    let absolute_deploy_tx_id = spora_hashes::Hash::from_bytes(absolute_deploy_tx.id());
    let absolute_code_outpoint = TransactionOutpoint::new(absolute_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&absolute_deploy_tx).into(), false)
        .await
        .expect("timelock create_absolute_lock scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        absolute_deploy_tx_id,
        "timelock create_absolute_lock scoped action deployment",
    )
    .await;

    let absolute_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock create_absolute_lock fixture");
    let absolute_fixture_input_capacity = absolute_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let absolute_lock = Script::new(create_absolute_artifact.code_hash, 0, vec![]);
    let absolute_fixture_capacity = absolute_fixture_input_capacity
        .checked_sub(required_fee(absolute_fixture_input.len(), 1).saturating_add(100_000))
        .expect("timelock create_absolute_lock fixture transaction must leave capacity");
    let absolute_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &absolute_fixture_input,
        vec![],
        vec![CellOutput { capacity: absolute_fixture_capacity, lock: absolute_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let absolute_fixture_tx_id = spora_hashes::Hash::from_bytes(absolute_fixture_tx.id());
    let absolute_input = TransactionOutpoint::new(absolute_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&absolute_fixture_tx).into(), false)
        .await
        .expect("timelock create_absolute_lock fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, absolute_fixture_tx_id, "timelock create_absolute_lock fixture cell")
        .await;

    let absolute_owner = [151; 32];
    let absolute_current_height = 50;
    let absolute_unlock_height = 100;
    let absolute_witness = create_absolute_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Hash([0; 32]),
            cellscript::EntryWitnessArg::Address(absolute_owner),
            cellscript::EntryWitnessArg::U64(absolute_unlock_height),
            cellscript::EntryWitnessArg::U64(absolute_current_height),
        ])
        .expect("timelock create_absolute_lock action witness must encode owner, unlock_height, and current_height");
    let absolute_output_capacity = absolute_fixture_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("timelock create_absolute_lock action must leave fee");
    let malformed_absolute_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(absolute_input.tx_hash, absolute_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(absolute_code_outpoint.tx_hash, absolute_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: absolute_output_capacity, lock: absolute_lock.clone(), type_: None }],
            vec![timelock_cell_data(absolute_owner, 0, absolute_unlock_height + 1, absolute_current_height)],
            vec![absolute_witness.clone()],
        )
        .expect("malformed timelock create_absolute_lock transaction must be structurally valid"),
        &create_absolute_artifact.action,
        "malformed timelock create_absolute_lock transaction",
    );
    let malformed_absolute_reason = rpc_client
        .submit_transaction((&malformed_absolute_tx).into(), false)
        .await
        .expect_err("malformed timelock create_absolute_lock must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_absolute_reason, "timelock create_absolute_lock");

    let valid_absolute_data = timelock_cell_data(absolute_owner, 0, absolute_unlock_height, absolute_current_height);
    let valid_absolute_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(absolute_input.tx_hash, absolute_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(absolute_code_outpoint.tx_hash, absolute_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: absolute_output_capacity, lock: absolute_lock, type_: None }],
            vec![valid_absolute_data],
            vec![absolute_witness],
        )
        .expect("valid timelock create_absolute_lock transaction must be structurally valid"),
        &create_absolute_artifact.action,
        "valid timelock create_absolute_lock transaction",
    );
    let valid_absolute_tx_id = spora_hashes::Hash::from_bytes(valid_absolute_tx.id());
    rpc_client
        .submit_transaction((&valid_absolute_tx).into(), false)
        .await
        .expect("valid timelock create_absolute_lock must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_absolute_tx_id, "valid timelock create_absolute_lock action")
        .await;

    let relative_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        create_relative_artifact.artifact_bytes.len(),
        "timelock create_relative_lock",
    );
    let relative_deploy_input_capacity = relative_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let relative_code_cell_capacity = relative_deploy_input_capacity
        .checked_sub(required_fee(relative_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock create_relative_lock scoped action deployment must leave capacity for code cell");
    let relative_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &relative_deploy_input,
        vec![],
        vec![CellOutput { capacity: relative_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![create_relative_artifact.artifact_bytes.clone()],
    );
    let relative_deploy_tx_id = spora_hashes::Hash::from_bytes(relative_deploy_tx.id());
    let relative_code_outpoint = TransactionOutpoint::new(relative_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&relative_deploy_tx).into(), false)
        .await
        .expect("timelock create_relative_lock scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        relative_deploy_tx_id,
        "timelock create_relative_lock scoped action deployment",
    )
    .await;

    let relative_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock create_relative_lock fixture");
    let relative_fixture_input_capacity = relative_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let relative_lock = Script::new(create_relative_artifact.code_hash, 0, vec![]);
    let relative_fixture_capacity = relative_fixture_input_capacity
        .checked_sub(required_fee(relative_fixture_input.len(), 1).saturating_add(100_000))
        .expect("timelock create_relative_lock fixture transaction must leave capacity");
    let relative_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &relative_fixture_input,
        vec![],
        vec![CellOutput { capacity: relative_fixture_capacity, lock: relative_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let relative_fixture_tx_id = spora_hashes::Hash::from_bytes(relative_fixture_tx.id());
    let relative_input = TransactionOutpoint::new(relative_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&relative_fixture_tx).into(), false)
        .await
        .expect("timelock create_relative_lock fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, relative_fixture_tx_id, "timelock create_relative_lock fixture cell")
        .await;

    let relative_owner = [152; 32];
    let relative_current_height = 70;
    let relative_period = 25;
    let relative_unlock_height = relative_current_height + relative_period;
    let relative_witness = create_relative_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Hash([0; 32]),
            cellscript::EntryWitnessArg::Address(relative_owner),
            cellscript::EntryWitnessArg::U64(relative_period),
            cellscript::EntryWitnessArg::U64(relative_current_height),
        ])
        .expect("timelock create_relative_lock action witness must encode owner, lock_period, and current_height");
    let relative_output_capacity = relative_fixture_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("timelock create_relative_lock action must leave fee");
    let malformed_relative_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(relative_input.tx_hash, relative_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(relative_code_outpoint.tx_hash, relative_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: relative_output_capacity, lock: relative_lock.clone(), type_: None }],
            vec![timelock_cell_data(relative_owner, 1, relative_unlock_height + 1, relative_current_height)],
            vec![relative_witness.clone()],
        )
        .expect("malformed timelock create_relative_lock transaction must be structurally valid"),
        &create_relative_artifact.action,
        "malformed timelock create_relative_lock transaction",
    );
    let malformed_relative_reason = rpc_client
        .submit_transaction((&malformed_relative_tx).into(), false)
        .await
        .expect_err("malformed timelock create_relative_lock must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_relative_reason, "timelock create_relative_lock");

    let valid_relative_data = timelock_cell_data(relative_owner, 1, relative_unlock_height, relative_current_height);
    let valid_relative_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(relative_input.tx_hash, relative_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(relative_code_outpoint.tx_hash, relative_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: relative_output_capacity, lock: relative_lock, type_: None }],
            vec![valid_relative_data],
            vec![relative_witness],
        )
        .expect("valid timelock create_relative_lock transaction must be structurally valid"),
        &create_relative_artifact.action,
        "valid timelock create_relative_lock transaction",
    );
    let valid_relative_tx_id = spora_hashes::Hash::from_bytes(valid_relative_tx.id());
    rpc_client
        .submit_transaction((&valid_relative_tx).into(), false)
        .await
        .expect("valid timelock create_relative_lock must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_relative_tx_id, "valid timelock create_relative_lock action")
        .await;

    let lock_asset_deploy_input =
        pop_code_deploy_cells(&mut spendable_cells, prealloc_address, lock_asset_artifact.artifact_bytes.len(), "timelock lock_asset");
    let lock_asset_deploy_input_capacity = lock_asset_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let lock_asset_code_cell_capacity = lock_asset_deploy_input_capacity
        .checked_sub(required_fee(lock_asset_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock lock_asset scoped action deployment must leave capacity for code cell");
    let lock_asset_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &lock_asset_deploy_input,
        vec![],
        vec![CellOutput { capacity: lock_asset_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![lock_asset_artifact.artifact_bytes.clone()],
    );
    let lock_asset_deploy_tx_id = spora_hashes::Hash::from_bytes(lock_asset_deploy_tx.id());
    let lock_asset_code_outpoint = TransactionOutpoint::new(lock_asset_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&lock_asset_deploy_tx).into(), false)
        .await
        .expect("timelock lock_asset scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        lock_asset_deploy_tx_id,
        "timelock lock_asset scoped action deployment",
    )
    .await;

    let lock_asset_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock lock_asset fixture");
    let lock_asset_fixture_input_capacity = lock_asset_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let lock_asset_lock = Script::new(lock_asset_artifact.code_hash, 0, vec![]);
    let lock_asset_cell_capacity = lock_asset_fixture_input_capacity / 4;
    let lock_asset_change_capacity = lock_asset_fixture_input_capacity
        .checked_sub(lock_asset_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(lock_asset_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("timelock lock_asset fixture transaction must leave change");
    let lock_asset_owner = [159; 32];
    let lock_asset_time_lock_data = timelock_cell_data(lock_asset_owner, 0, 500, 1);
    let lock_asset_type_payload = vec![0];
    let lock_asset_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &lock_asset_fixture_input,
        vec![],
        vec![
            CellOutput { capacity: lock_asset_cell_capacity, lock: lock_asset_lock.clone(), type_: None },
            CellOutput { capacity: lock_asset_cell_capacity, lock: lock_asset_lock.clone(), type_: None },
            CellOutput { capacity: lock_asset_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![lock_asset_time_lock_data.clone(), lock_asset_type_payload.clone(), vec![]],
    );
    let lock_asset_fixture_tx_id = spora_hashes::Hash::from_bytes(lock_asset_fixture_tx.id());
    let lock_asset_time_lock_input = TransactionOutpoint::new(lock_asset_fixture_tx.id(), 0);
    let lock_asset_type_input = TransactionOutpoint::new(lock_asset_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&lock_asset_fixture_tx).into(), false)
        .await
        .expect("timelock lock_asset fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, lock_asset_fixture_tx_id, "timelock lock_asset fixture cells").await;

    let lock_asset_amount = 42;
    let lock_asset_witness = lock_asset_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Bytes(lock_asset_type_payload.clone()),
            cellscript::EntryWitnessArg::U64(lock_asset_amount),
        ])
        .expect("timelock lock_asset action witness must encode asset_type and amount");
    let lock_asset_output_capacity = lock_asset_cell_capacity
        .checked_sub(required_fee(2, 2).saturating_add(100_000))
        .expect("timelock lock_asset action must leave fee")
        / 2;
    let malformed_lock_asset_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(lock_asset_time_lock_input.tx_hash, lock_asset_time_lock_input.index), 0),
                CellInput::new(OutPoint::new(lock_asset_type_input.tx_hash, lock_asset_type_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(lock_asset_time_lock_input.tx_hash, lock_asset_time_lock_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(lock_asset_code_outpoint.tx_hash, lock_asset_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: lock_asset_output_capacity, lock: lock_asset_lock.clone(), type_: None },
                CellOutput { capacity: lock_asset_output_capacity, lock: lock_asset_lock.clone(), type_: None },
            ],
            vec![
                locked_asset_molecule_cell_data(&lock_asset_type_payload, lock_asset_amount + 1, [0; 32]),
                lock_asset_time_lock_data.clone(),
            ],
            vec![lock_asset_witness.clone(), vec![]],
        )
        .expect("malformed timelock lock_asset transaction must be structurally valid"),
        &lock_asset_artifact.action,
        "malformed timelock lock_asset transaction",
    );
    let malformed_lock_asset_reason = rpc_client
        .submit_transaction((&malformed_lock_asset_tx).into(), false)
        .await
        .expect_err("malformed timelock lock_asset must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_lock_asset_reason, "timelock lock_asset");

    let valid_lock_asset_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(lock_asset_time_lock_input.tx_hash, lock_asset_time_lock_input.index), 0),
                CellInput::new(OutPoint::new(lock_asset_type_input.tx_hash, lock_asset_type_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(lock_asset_time_lock_input.tx_hash, lock_asset_time_lock_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(lock_asset_code_outpoint.tx_hash, lock_asset_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: lock_asset_output_capacity, lock: lock_asset_lock.clone(), type_: None },
                CellOutput { capacity: lock_asset_output_capacity, lock: lock_asset_lock, type_: None },
            ],
            vec![locked_asset_molecule_cell_data(&lock_asset_type_payload, lock_asset_amount, [0; 32]), lock_asset_time_lock_data],
            vec![lock_asset_witness, vec![]],
        )
        .expect("valid timelock lock_asset transaction must be structurally valid"),
        &lock_asset_artifact.action,
        "valid timelock lock_asset transaction",
    );
    let valid_lock_asset_tx_id = spora_hashes::Hash::from_bytes(valid_lock_asset_tx.id());
    rpc_client
        .submit_transaction((&valid_lock_asset_tx).into(), false)
        .await
        .expect("valid timelock lock_asset must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_lock_asset_tx_id, "valid timelock lock_asset action").await;

    let request_release_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        request_release_artifact.artifact_bytes.len(),
        "timelock request_release",
    );
    let request_release_deploy_input_capacity = request_release_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let request_release_code_cell_capacity = request_release_deploy_input_capacity
        .checked_sub(required_fee(request_release_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock request_release scoped action deployment must leave capacity for code cell");
    let request_release_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &request_release_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: request_release_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![request_release_artifact.artifact_bytes.clone()],
    );
    let request_release_deploy_tx_id = spora_hashes::Hash::from_bytes(request_release_deploy_tx.id());
    let request_release_code_outpoint = TransactionOutpoint::new(request_release_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&request_release_deploy_tx).into(), false)
        .await
        .expect("timelock request_release scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        request_release_deploy_tx_id,
        "timelock request_release scoped action deployment",
    )
    .await;

    let request_release_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock request_release fixture");
    let request_release_fixture_input_capacity = request_release_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let request_release_lock = Script::new(request_release_artifact.code_hash, 0, vec![]);
    let request_release_fixture_capacity = request_release_fixture_input_capacity
        .checked_sub(required_fee(request_release_fixture_input.len(), 1).saturating_add(100_000))
        .expect("timelock request_release fixture transaction must leave capacity");
    let request_owner = [157; 32];
    let request_unlock_height = 100;
    let request_created_at = 1;
    let request_time_lock_data = timelock_cell_data(request_owner, 0, request_unlock_height, request_created_at);
    let request_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &request_release_fixture_input,
        vec![],
        vec![CellOutput { capacity: request_release_fixture_capacity, lock: request_release_lock.clone(), type_: None }],
        vec![request_time_lock_data.clone()],
    );
    let request_fixture_tx_id = spora_hashes::Hash::from_bytes(request_fixture_tx.id());
    let request_input = TransactionOutpoint::new(request_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&request_fixture_tx).into(), false)
        .await
        .expect("timelock request_release fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, request_fixture_tx_id, "timelock request_release fixture cell").await;

    let request_current_height = 125;
    let request_witness = request_release_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(request_owner),
            cellscript::EntryWitnessArg::U64(request_current_height),
        ])
        .expect("timelock request_release action witness must encode requester and current_height");
    let request_output_capacity = request_release_fixture_capacity
        .checked_sub(required_fee(1, 2).saturating_add(100_000))
        .expect("timelock request_release action must leave fee")
        / 2;
    let malformed_request_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(request_input.tx_hash, request_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(request_input.tx_hash, request_input.index), dep_type: DepType::Code },
                CellDep {
                    out_point: OutPoint::new(request_release_code_outpoint.tx_hash, request_release_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: request_output_capacity, lock: request_release_lock.clone(), type_: None },
                CellOutput { capacity: request_output_capacity, lock: request_release_lock.clone(), type_: None },
            ],
            vec![release_request_cell_data([0; 32], request_owner, request_current_height + 1), request_time_lock_data.clone()],
            vec![request_witness.clone()],
        )
        .expect("malformed timelock request_release transaction must be structurally valid"),
        &request_release_artifact.action,
        "malformed timelock request_release transaction",
    );
    let malformed_request_reason = rpc_client
        .submit_transaction((&malformed_request_tx).into(), false)
        .await
        .expect_err("malformed timelock request_release must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_request_reason, "timelock request_release");

    let valid_request_data = release_request_cell_data([0; 32], request_owner, request_current_height);
    let valid_request_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(request_input.tx_hash, request_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(request_input.tx_hash, request_input.index), dep_type: DepType::Code },
                CellDep {
                    out_point: OutPoint::new(request_release_code_outpoint.tx_hash, request_release_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: request_output_capacity, lock: request_release_lock.clone(), type_: None },
                CellOutput { capacity: request_output_capacity, lock: request_release_lock, type_: None },
            ],
            vec![valid_request_data, request_time_lock_data],
            vec![request_witness],
        )
        .expect("valid timelock request_release transaction must be structurally valid"),
        &request_release_artifact.action,
        "valid timelock request_release transaction",
    );
    let valid_request_tx_id = spora_hashes::Hash::from_bytes(valid_request_tx.id());
    rpc_client
        .submit_transaction((&valid_request_tx).into(), false)
        .await
        .expect("valid timelock request_release must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_request_tx_id, "valid timelock request_release action").await;

    let execute_release_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        execute_release_artifact.artifact_bytes.len(),
        "timelock execute_release",
    );
    let execute_release_deploy_input_capacity = execute_release_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let execute_release_code_cell_capacity = execute_release_deploy_input_capacity
        .checked_sub(required_fee(execute_release_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock execute_release scoped action deployment must leave capacity for code cell");
    let execute_release_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &execute_release_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: execute_release_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![execute_release_artifact.artifact_bytes.clone()],
    );
    let execute_release_deploy_tx_id = spora_hashes::Hash::from_bytes(execute_release_deploy_tx.id());
    let execute_release_code_outpoint = TransactionOutpoint::new(execute_release_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&execute_release_deploy_tx).into(), false)
        .await
        .expect("timelock execute_release scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        execute_release_deploy_tx_id,
        "timelock execute_release scoped action deployment",
    )
    .await;

    let execute_release_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock execute_release fixture");
    let execute_release_fixture_input_capacity = execute_release_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let execute_release_lock = Script::new(execute_release_artifact.code_hash, 0, vec![]);
    let execute_release_type = Script::new(always_success_code_hash(), 0, b"timelock-execute-release-type".to_vec());
    let execute_release_cell_capacity = execute_release_fixture_input_capacity / 4;
    let execute_release_change_capacity = execute_release_fixture_input_capacity
        .checked_sub(execute_release_cell_capacity.saturating_mul(3))
        .and_then(|value| value.checked_sub(required_fee(execute_release_fixture_input.len(), 4).saturating_add(100_000)))
        .expect("timelock execute_release fixture transaction must leave change");
    let execute_release_owner = [161; 32];
    let execute_release_executor = execute_release_owner;
    let execute_release_current_height = 160;
    let execute_release_time_lock_data = timelock_cell_data(execute_release_owner, 0, 100, 1);
    let execute_release_locked_asset_data = locked_asset_molecule_cell_data(&[0], 42, [0; 32]);
    let execute_release_request_data = release_request_cell_data([0; 32], execute_release_owner, execute_release_current_height - 1);
    let execute_release_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &execute_release_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: execute_release_cell_capacity,
                lock: execute_release_lock.clone(),
                type_: Some(execute_release_type.clone()),
            },
            CellOutput {
                capacity: execute_release_cell_capacity,
                lock: execute_release_lock.clone(),
                type_: Some(execute_release_type.clone()),
            },
            CellOutput {
                capacity: execute_release_cell_capacity,
                lock: execute_release_lock.clone(),
                type_: Some(execute_release_type.clone()),
            },
            CellOutput { capacity: execute_release_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            execute_release_time_lock_data.clone(),
            execute_release_locked_asset_data.clone(),
            execute_release_request_data.clone(),
            vec![],
        ],
    );
    let execute_release_fixture_tx_id = spora_hashes::Hash::from_bytes(execute_release_fixture_tx.id());
    let execute_release_time_lock_input = TransactionOutpoint::new(execute_release_fixture_tx.id(), 0);
    let execute_release_locked_asset_input = TransactionOutpoint::new(execute_release_fixture_tx.id(), 1);
    let execute_release_request_input = TransactionOutpoint::new(execute_release_fixture_tx.id(), 2);
    rpc_client
        .submit_transaction((&execute_release_fixture_tx).into(), false)
        .await
        .expect("timelock execute_release fixture cells must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        execute_release_fixture_tx_id,
        "timelock execute_release fixture cells",
    )
    .await;

    let execute_release_witness = execute_release_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(execute_release_executor),
            cellscript::EntryWitnessArg::U64(execute_release_current_height),
        ])
        .expect("timelock execute_release action witness must encode executor and current_height");
    let execute_release_output_capacity = execute_release_cell_capacity
        .checked_sub(required_fee(3, 1).saturating_add(100_000))
        .expect("timelock execute_release action must leave fee");
    let malformed_execute_release_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(execute_release_time_lock_input.tx_hash, execute_release_time_lock_input.index), 0),
                CellInput::new(OutPoint::new(execute_release_locked_asset_input.tx_hash, execute_release_locked_asset_input.index), 0),
                CellInput::new(OutPoint::new(execute_release_request_input.tx_hash, execute_release_request_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(execute_release_code_outpoint.tx_hash, execute_release_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: execute_release_output_capacity, lock: execute_release_lock.clone(), type_: None }],
            vec![release_record_cell_data([0; 32], execute_release_current_height + 1, execute_release_executor)],
            vec![execute_release_witness.clone()],
        )
        .expect("malformed timelock execute_release transaction must be structurally valid"),
        &execute_release_artifact.action,
        "malformed timelock execute_release transaction",
    );
    let malformed_execute_release_reason = rpc_client
        .submit_transaction((&malformed_execute_release_tx).into(), false)
        .await
        .expect_err("malformed timelock execute_release must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_execute_release_reason, "timelock execute_release");

    let valid_execute_release_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(execute_release_time_lock_input.tx_hash, execute_release_time_lock_input.index), 0),
                CellInput::new(OutPoint::new(execute_release_locked_asset_input.tx_hash, execute_release_locked_asset_input.index), 0),
                CellInput::new(OutPoint::new(execute_release_request_input.tx_hash, execute_release_request_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(execute_release_code_outpoint.tx_hash, execute_release_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: execute_release_output_capacity, lock: execute_release_lock, type_: None }],
            vec![release_record_cell_data([0; 32], execute_release_current_height, execute_release_executor)],
            vec![execute_release_witness],
        )
        .expect("valid timelock execute_release transaction must be structurally valid"),
        &execute_release_artifact.action,
        "valid timelock execute_release transaction",
    );
    let valid_execute_release_tx_id = spora_hashes::Hash::from_bytes(valid_execute_release_tx.id());
    rpc_client
        .submit_transaction((&valid_execute_release_tx).into(), false)
        .await
        .expect("valid timelock execute_release must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_execute_release_tx_id, "valid timelock execute_release action")
        .await;

    let request_emergency_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        request_emergency_artifact.artifact_bytes.len(),
        "timelock request_emergency_release",
    );
    let request_emergency_deploy_input_capacity = request_emergency_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let request_emergency_code_cell_capacity = request_emergency_deploy_input_capacity
        .checked_sub(required_fee(request_emergency_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock request_emergency_release scoped action deployment must leave capacity for code cell");
    let request_emergency_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &request_emergency_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: request_emergency_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![request_emergency_artifact.artifact_bytes.clone()],
    );
    let request_emergency_deploy_tx_id = spora_hashes::Hash::from_bytes(request_emergency_deploy_tx.id());
    let request_emergency_code_outpoint = TransactionOutpoint::new(request_emergency_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&request_emergency_deploy_tx).into(), false)
        .await
        .expect("timelock request_emergency_release scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        request_emergency_deploy_tx_id,
        "timelock request_emergency_release scoped action deployment",
    )
    .await;

    let request_emergency_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock request_emergency_release fixture");
    let request_emergency_fixture_input_capacity =
        request_emergency_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let request_emergency_lock = Script::new(request_emergency_artifact.code_hash, 0, vec![]);
    let request_emergency_cell_capacity = request_emergency_fixture_input_capacity / 4;
    let request_emergency_change_capacity = request_emergency_fixture_input_capacity
        .checked_sub(request_emergency_cell_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(request_emergency_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("timelock request_emergency_release fixture transaction must leave change");
    let emergency_owner = [160; 32];
    let emergency_current_height = 125;
    let emergency_time_lock_data = timelock_cell_data(emergency_owner, 0, 500, 1);
    let emergency_reason = molecule_bytes_cell_data(b"emergency release");
    let request_emergency_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &request_emergency_fixture_input,
        vec![],
        vec![
            CellOutput { capacity: request_emergency_cell_capacity, lock: request_emergency_lock.clone(), type_: None },
            CellOutput { capacity: request_emergency_cell_capacity, lock: request_emergency_lock.clone(), type_: None },
            CellOutput { capacity: request_emergency_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![emergency_time_lock_data.clone(), emergency_reason.clone(), vec![]],
    );
    let request_emergency_fixture_tx_id = spora_hashes::Hash::from_bytes(request_emergency_fixture_tx.id());
    let emergency_time_lock_input = TransactionOutpoint::new(request_emergency_fixture_tx.id(), 0);
    let emergency_reason_input = TransactionOutpoint::new(request_emergency_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&request_emergency_fixture_tx).into(), false)
        .await
        .expect("timelock request_emergency_release fixture cells must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        request_emergency_fixture_tx_id,
        "timelock request_emergency_release fixture cells",
    )
    .await;

    let request_emergency_witness = request_emergency_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(emergency_owner),
            cellscript::EntryWitnessArg::Bytes(emergency_reason.clone()),
            cellscript::EntryWitnessArg::U64(emergency_current_height),
        ])
        .expect("timelock request_emergency_release action witness must encode requester, reason, and current_height");
    let request_emergency_output_capacity = request_emergency_cell_capacity
        .checked_sub(required_fee(2, 2).saturating_add(100_000))
        .expect("timelock request_emergency_release action must leave fee")
        / 2;
    let malformed_emergency_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(emergency_time_lock_input.tx_hash, emergency_time_lock_input.index), 0),
                CellInput::new(OutPoint::new(emergency_reason_input.tx_hash, emergency_reason_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(emergency_time_lock_input.tx_hash, emergency_time_lock_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(request_emergency_code_outpoint.tx_hash, request_emergency_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: request_emergency_output_capacity, lock: request_emergency_lock.clone(), type_: None },
                CellOutput { capacity: request_emergency_output_capacity, lock: request_emergency_lock.clone(), type_: None },
            ],
            vec![
                emergency_release_molecule_cell_data([0; 32], emergency_owner, &emergency_reason, emergency_current_height + 1, &[]),
                emergency_time_lock_data.clone(),
            ],
            vec![request_emergency_witness.clone(), vec![]],
        )
        .expect("malformed timelock request_emergency_release transaction must be structurally valid"),
        &request_emergency_artifact.action,
        "malformed timelock request_emergency_release transaction",
    );
    let malformed_emergency_reason = rpc_client
        .submit_transaction((&malformed_emergency_tx).into(), false)
        .await
        .expect_err("malformed timelock request_emergency_release must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_emergency_reason, "timelock request_emergency_release");

    let valid_emergency_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(emergency_time_lock_input.tx_hash, emergency_time_lock_input.index), 0),
                CellInput::new(OutPoint::new(emergency_reason_input.tx_hash, emergency_reason_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(emergency_time_lock_input.tx_hash, emergency_time_lock_input.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(request_emergency_code_outpoint.tx_hash, request_emergency_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: request_emergency_output_capacity, lock: request_emergency_lock.clone(), type_: None },
                CellOutput { capacity: request_emergency_output_capacity, lock: request_emergency_lock, type_: None },
            ],
            vec![
                emergency_release_molecule_cell_data([0; 32], emergency_owner, &emergency_reason, emergency_current_height, &[]),
                emergency_time_lock_data,
            ],
            vec![request_emergency_witness, vec![]],
        )
        .expect("valid timelock request_emergency_release transaction must be structurally valid"),
        &request_emergency_artifact.action,
        "valid timelock request_emergency_release transaction",
    );
    let valid_emergency_tx_id = spora_hashes::Hash::from_bytes(valid_emergency_tx.id());
    rpc_client
        .submit_transaction((&valid_emergency_tx).into(), false)
        .await
        .expect("valid timelock request_emergency_release must be accepted by the scoped action verifier");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        valid_emergency_tx_id,
        "valid timelock request_emergency_release action",
    )
    .await;

    let approve_emergency_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        approve_emergency_artifact.artifact_bytes.len(),
        "timelock approve_emergency_release",
    );
    let approve_emergency_deploy_input_capacity = approve_emergency_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let approve_emergency_code_cell_capacity = approve_emergency_deploy_input_capacity
        .checked_sub(required_fee(approve_emergency_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock approve_emergency_release scoped action deployment must leave capacity for code cell");
    let approve_emergency_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &approve_emergency_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: approve_emergency_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![approve_emergency_artifact.artifact_bytes.clone()],
    );
    let approve_emergency_deploy_tx_id = spora_hashes::Hash::from_bytes(approve_emergency_deploy_tx.id());
    let approve_emergency_code_outpoint = TransactionOutpoint::new(approve_emergency_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&approve_emergency_deploy_tx).into(), false)
        .await
        .expect("timelock approve_emergency_release scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        approve_emergency_deploy_tx_id,
        "timelock approve_emergency_release scoped action deployment",
    )
    .await;

    let approve_emergency_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock approve_emergency_release fixture");
    let approve_emergency_fixture_input_capacity =
        approve_emergency_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let approve_emergency_lock = Script::new(approve_emergency_artifact.code_hash, 0, vec![]);
    let approve_emergency_type = Script::new(always_success_code_hash(), 0, b"timelock-approve-emergency-type".to_vec());
    let approve_emergency_fixture_capacity = approve_emergency_fixture_input_capacity
        .checked_sub(required_fee(approve_emergency_fixture_input.len(), 1).saturating_add(100_000))
        .expect("timelock approve_emergency_release fixture transaction must leave capacity");
    let approve_emergency_owner = [162; 32];
    let approve_emergency_reason = molecule_bytes_cell_data(b"approve emergency release");
    let approve_emergency_requested_at = 175;
    let approve_emergency_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &approve_emergency_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![CellOutput {
            capacity: approve_emergency_fixture_capacity,
            lock: approve_emergency_lock.clone(),
            type_: Some(approve_emergency_type.clone()),
        }],
        vec![emergency_release_molecule_cell_data(
            [0; 32],
            approve_emergency_owner,
            &approve_emergency_reason,
            approve_emergency_requested_at,
            &[],
        )],
    );
    let approve_emergency_fixture_tx_id = spora_hashes::Hash::from_bytes(approve_emergency_fixture_tx.id());
    let approve_emergency_input = TransactionOutpoint::new(approve_emergency_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&approve_emergency_fixture_tx).into(), false)
        .await
        .expect("timelock approve_emergency_release fixture cell must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        approve_emergency_fixture_tx_id,
        "timelock approve_emergency_release fixture cell",
    )
    .await;

    let approve_emergency_approver = [163; 32];
    let approve_emergency_required_approvals = 2u8;
    let approve_emergency_witness = approve_emergency_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(approve_emergency_approver),
            cellscript::EntryWitnessArg::U8(approve_emergency_required_approvals),
        ])
        .expect("timelock approve_emergency_release action witness must encode approver and required_approvals");
    let approve_emergency_output_capacity = approve_emergency_fixture_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("timelock approve_emergency_release action must leave fee");
    let malformed_approve_emergency_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(approve_emergency_input.tx_hash, approve_emergency_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(approve_emergency_code_outpoint.tx_hash, approve_emergency_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput {
                capacity: approve_emergency_output_capacity,
                lock: approve_emergency_lock.clone(),
                type_: Some(approve_emergency_type.clone()),
            }],
            vec![emergency_release_molecule_cell_data(
                [0; 32],
                approve_emergency_owner,
                &approve_emergency_reason,
                approve_emergency_requested_at,
                &[],
            )],
            vec![approve_emergency_witness.clone()],
        )
        .expect("malformed timelock approve_emergency_release transaction must be structurally valid"),
        &approve_emergency_artifact.action,
        "malformed timelock approve_emergency_release transaction",
    );
    let malformed_approve_emergency_reason = rpc_client
        .submit_transaction((&malformed_approve_emergency_tx).into(), false)
        .await
        .expect_err("malformed timelock approve_emergency_release must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_approve_emergency_reason, "timelock approve_emergency_release");

    let valid_approve_emergency_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(approve_emergency_input.tx_hash, approve_emergency_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(approve_emergency_code_outpoint.tx_hash, approve_emergency_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput {
                capacity: approve_emergency_output_capacity,
                lock: approve_emergency_lock,
                type_: Some(approve_emergency_type),
            }],
            vec![emergency_release_molecule_cell_data(
                [0; 32],
                approve_emergency_owner,
                &approve_emergency_reason,
                approve_emergency_requested_at,
                &[approve_emergency_approver],
            )],
            vec![approve_emergency_witness],
        )
        .expect("valid timelock approve_emergency_release transaction must be structurally valid"),
        &approve_emergency_artifact.action,
        "valid timelock approve_emergency_release transaction",
    );
    let valid_approve_emergency_tx_id = spora_hashes::Hash::from_bytes(valid_approve_emergency_tx.id());
    rpc_client
        .submit_transaction((&valid_approve_emergency_tx).into(), false)
        .await
        .expect("valid timelock approve_emergency_release must be accepted by the scoped action verifier");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        valid_approve_emergency_tx_id,
        "valid timelock approve_emergency_release action",
    )
    .await;

    let execute_emergency_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        execute_emergency_artifact.artifact_bytes.len(),
        "timelock execute_emergency_release",
    );
    let execute_emergency_deploy_input_capacity = execute_emergency_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let execute_emergency_code_cell_capacity = execute_emergency_deploy_input_capacity
        .checked_sub(required_fee(execute_emergency_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock execute_emergency_release scoped action deployment must leave capacity for code cell");
    let execute_emergency_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &execute_emergency_deploy_input,
        vec![],
        vec![CellOutput {
            capacity: execute_emergency_code_cell_capacity,
            lock: pay_to_acceptance_owner(prealloc_address),
            type_: None,
        }],
        vec![execute_emergency_artifact.artifact_bytes.clone()],
    );
    let execute_emergency_deploy_tx_id = spora_hashes::Hash::from_bytes(execute_emergency_deploy_tx.id());
    let execute_emergency_code_outpoint = TransactionOutpoint::new(execute_emergency_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&execute_emergency_deploy_tx).into(), false)
        .await
        .expect("timelock execute_emergency_release scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        execute_emergency_deploy_tx_id,
        "timelock execute_emergency_release scoped action deployment",
    )
    .await;

    let execute_emergency_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock execute_emergency_release fixture");
    let execute_emergency_fixture_input_capacity =
        execute_emergency_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let execute_emergency_lock = Script::new(execute_emergency_artifact.code_hash, 0, vec![]);
    let execute_emergency_type = Script::new(always_success_code_hash(), 0, b"timelock-execute-emergency-type".to_vec());
    let execute_emergency_cell_capacity = execute_emergency_fixture_input_capacity / 4;
    let execute_emergency_change_capacity = execute_emergency_fixture_input_capacity
        .checked_sub(execute_emergency_cell_capacity.saturating_mul(3))
        .and_then(|value| value.checked_sub(required_fee(execute_emergency_fixture_input.len(), 4).saturating_add(100_000)))
        .expect("timelock execute_emergency_release fixture transaction must leave change");
    let execute_emergency_owner = [164; 32];
    let execute_emergency_executor = execute_emergency_owner;
    let execute_emergency_current_height = 190;
    let execute_emergency_required_approvals = 2u8;
    let execute_emergency_approvers = [[165; 32], [166; 32]];
    let execute_emergency_reason = molecule_bytes_cell_data(b"executed emergency release");
    let execute_emergency_time_lock_data = timelock_cell_data(execute_emergency_owner, 0, 500, 1);
    let execute_emergency_locked_asset_data = locked_asset_molecule_cell_data(&[0], 42, [0; 32]);
    let execute_emergency_request_data = emergency_release_molecule_cell_data(
        [0; 32],
        execute_emergency_owner,
        &execute_emergency_reason,
        execute_emergency_current_height - 5,
        &execute_emergency_approvers,
    );
    let execute_emergency_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &execute_emergency_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput {
                capacity: execute_emergency_cell_capacity,
                lock: execute_emergency_lock.clone(),
                type_: Some(execute_emergency_type.clone()),
            },
            CellOutput {
                capacity: execute_emergency_cell_capacity,
                lock: execute_emergency_lock.clone(),
                type_: Some(execute_emergency_type.clone()),
            },
            CellOutput {
                capacity: execute_emergency_cell_capacity,
                lock: execute_emergency_lock.clone(),
                type_: Some(execute_emergency_type.clone()),
            },
            CellOutput { capacity: execute_emergency_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            execute_emergency_time_lock_data.clone(),
            execute_emergency_locked_asset_data.clone(),
            execute_emergency_request_data.clone(),
            vec![],
        ],
    );
    let execute_emergency_fixture_tx_id = spora_hashes::Hash::from_bytes(execute_emergency_fixture_tx.id());
    let execute_emergency_time_lock_input = TransactionOutpoint::new(execute_emergency_fixture_tx.id(), 0);
    let execute_emergency_locked_asset_input = TransactionOutpoint::new(execute_emergency_fixture_tx.id(), 1);
    let execute_emergency_request_input = TransactionOutpoint::new(execute_emergency_fixture_tx.id(), 2);
    rpc_client
        .submit_transaction((&execute_emergency_fixture_tx).into(), false)
        .await
        .expect("timelock execute_emergency_release fixture cells must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        execute_emergency_fixture_tx_id,
        "timelock execute_emergency_release fixture cells",
    )
    .await;

    let execute_emergency_witness = execute_emergency_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(execute_emergency_executor),
            cellscript::EntryWitnessArg::U8(execute_emergency_required_approvals),
            cellscript::EntryWitnessArg::U64(execute_emergency_current_height),
        ])
        .expect("timelock execute_emergency_release action witness must encode executor, required_approvals, and current_height");
    let execute_emergency_output_capacity = execute_emergency_cell_capacity
        .checked_sub(required_fee(3, 1).saturating_add(100_000))
        .expect("timelock execute_emergency_release action must leave fee");
    let malformed_execute_emergency_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(execute_emergency_time_lock_input.tx_hash, execute_emergency_time_lock_input.index), 0),
                CellInput::new(
                    OutPoint::new(execute_emergency_locked_asset_input.tx_hash, execute_emergency_locked_asset_input.index),
                    0,
                ),
                CellInput::new(OutPoint::new(execute_emergency_request_input.tx_hash, execute_emergency_request_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(execute_emergency_code_outpoint.tx_hash, execute_emergency_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: execute_emergency_output_capacity, lock: execute_emergency_lock.clone(), type_: None }],
            vec![release_record_cell_data([0; 32], execute_emergency_current_height + 1, execute_emergency_executor)],
            vec![execute_emergency_witness.clone()],
        )
        .expect("malformed timelock execute_emergency_release transaction must be structurally valid"),
        &execute_emergency_artifact.action,
        "malformed timelock execute_emergency_release transaction",
    );
    let malformed_execute_emergency_reason = rpc_client
        .submit_transaction((&malformed_execute_emergency_tx).into(), false)
        .await
        .expect_err("malformed timelock execute_emergency_release must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_execute_emergency_reason, "timelock execute_emergency_release");

    let valid_execute_emergency_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(execute_emergency_time_lock_input.tx_hash, execute_emergency_time_lock_input.index), 0),
                CellInput::new(
                    OutPoint::new(execute_emergency_locked_asset_input.tx_hash, execute_emergency_locked_asset_input.index),
                    0,
                ),
                CellInput::new(OutPoint::new(execute_emergency_request_input.tx_hash, execute_emergency_request_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(execute_emergency_code_outpoint.tx_hash, execute_emergency_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: execute_emergency_output_capacity, lock: execute_emergency_lock, type_: None }],
            vec![release_record_cell_data([0; 32], execute_emergency_current_height, execute_emergency_executor)],
            vec![execute_emergency_witness],
        )
        .expect("valid timelock execute_emergency_release transaction must be structurally valid"),
        &execute_emergency_artifact.action,
        "valid timelock execute_emergency_release transaction",
    );
    let valid_execute_emergency_tx_id = spora_hashes::Hash::from_bytes(valid_execute_emergency_tx.id());
    rpc_client
        .submit_transaction((&valid_execute_emergency_tx).into(), false)
        .await
        .expect("valid timelock execute_emergency_release must be accepted by the scoped action verifier");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        valid_execute_emergency_tx_id,
        "valid timelock execute_emergency_release action",
    )
    .await;

    let extend_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        extend_lock_artifact.artifact_bytes.len(),
        "timelock extend_lock",
    );
    let extend_deploy_input_capacity = extend_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let extend_code_cell_capacity = extend_deploy_input_capacity
        .checked_sub(required_fee(extend_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock extend_lock scoped action deployment must leave capacity for code cell");
    let extend_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &extend_deploy_input,
        vec![],
        vec![CellOutput { capacity: extend_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![extend_lock_artifact.artifact_bytes.clone()],
    );
    let extend_deploy_tx_id = spora_hashes::Hash::from_bytes(extend_deploy_tx.id());
    let extend_code_outpoint = TransactionOutpoint::new(extend_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&extend_deploy_tx).into(), false)
        .await
        .expect("timelock extend_lock scoped action deployment must be accepted");
    submit_next_template_containing(rpc_client, miner_address, extend_deploy_tx_id, "timelock extend_lock scoped action deployment")
        .await;

    let extend_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock extend_lock fixture");
    let extend_fixture_input_capacity = extend_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let extend_lock = Script::new(extend_lock_artifact.code_hash, 0, vec![]);
    let extend_fixture_capacity = extend_fixture_input_capacity
        .checked_sub(required_fee(extend_fixture_input.len(), 1).saturating_add(100_000))
        .expect("timelock extend_lock fixture transaction must leave capacity");
    let extend_type = Script::new(always_success_code_hash(), 0, b"timelock-extend-type".to_vec());
    let extend_owner = [158; 32];
    let extend_created_at = 1;
    let extend_initial_unlock_height = 100;
    let extend_input_data = timelock_cell_data(extend_owner, 0, extend_initial_unlock_height, extend_created_at);
    let extend_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &extend_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![CellOutput { capacity: extend_fixture_capacity, lock: extend_lock.clone(), type_: Some(extend_type.clone()) }],
        vec![extend_input_data],
    );
    let extend_fixture_tx_id = spora_hashes::Hash::from_bytes(extend_fixture_tx.id());
    let extend_input = TransactionOutpoint::new(extend_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&extend_fixture_tx).into(), false)
        .await
        .expect("timelock extend_lock fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, extend_fixture_tx_id, "timelock extend_lock fixture cell").await;

    let additional_period = 10;
    let extend_current_height = 50;
    let extend_witness = extend_lock_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::U64(additional_period),
            cellscript::EntryWitnessArg::Address(extend_owner),
            cellscript::EntryWitnessArg::U64(extend_current_height),
        ])
        .expect("timelock extend_lock action witness must encode additional_period, owner, and current_height");
    let extend_output_capacity = extend_fixture_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("timelock extend_lock action must leave fee");
    let malformed_extend_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(extend_input.tx_hash, extend_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(extend_code_outpoint.tx_hash, extend_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: extend_output_capacity, lock: extend_lock.clone(), type_: Some(extend_type.clone()) }],
            vec![timelock_cell_data(extend_owner, 0, extend_initial_unlock_height + additional_period + 1, extend_created_at)],
            vec![extend_witness.clone()],
        )
        .expect("malformed timelock extend_lock transaction must be structurally valid"),
        &extend_lock_artifact.action,
        "malformed timelock extend_lock transaction",
    );
    let malformed_extend_reason = rpc_client
        .submit_transaction((&malformed_extend_tx).into(), false)
        .await
        .expect_err("malformed timelock extend_lock must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_extend_reason, "timelock extend_lock");

    let valid_extend_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(extend_input.tx_hash, extend_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(extend_code_outpoint.tx_hash, extend_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: extend_output_capacity, lock: extend_lock, type_: Some(extend_type) }],
            vec![timelock_cell_data(extend_owner, 0, extend_initial_unlock_height + additional_period, extend_created_at)],
            vec![extend_witness],
        )
        .expect("valid timelock extend_lock transaction must be structurally valid"),
        &extend_lock_artifact.action,
        "valid timelock extend_lock transaction",
    );
    let valid_extend_tx_id = spora_hashes::Hash::from_bytes(valid_extend_tx.id());
    rpc_client
        .submit_transaction((&valid_extend_tx).into(), false)
        .await
        .expect("valid timelock extend_lock must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_extend_tx_id, "valid timelock extend_lock action").await;

    let batch_deploy_input = pop_code_deploy_cells(
        &mut spendable_cells,
        prealloc_address,
        batch_create_artifact.artifact_bytes.len(),
        "timelock batch_create_locks",
    );
    let batch_deploy_input_capacity = batch_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let batch_code_cell_capacity = batch_deploy_input_capacity
        .checked_sub(required_fee(batch_deploy_input.len(), 1).saturating_add(100_000))
        .expect("timelock batch_create_locks scoped action deployment must leave capacity for code cell");
    let batch_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &batch_deploy_input,
        vec![],
        vec![CellOutput { capacity: batch_code_cell_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
        vec![batch_create_artifact.artifact_bytes.clone()],
    );
    let batch_deploy_tx_id = spora_hashes::Hash::from_bytes(batch_deploy_tx.id());
    let batch_code_outpoint = TransactionOutpoint::new(batch_deploy_tx.id(), 0);
    rpc_client
        .submit_transaction((&batch_deploy_tx).into(), false)
        .await
        .expect("timelock batch_create_locks scoped action deployment must be accepted");
    submit_next_template_containing(
        rpc_client,
        miner_address,
        batch_deploy_tx_id,
        "timelock batch_create_locks scoped action deployment",
    )
    .await;

    let batch_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "timelock batch_create_locks fixture");
    let batch_fixture_input_capacity = batch_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let batch_lock = Script::new(batch_create_artifact.code_hash, 0, vec![]);
    let batch_fixture_capacity = batch_fixture_input_capacity
        .checked_sub(required_fee(batch_fixture_input.len(), 1).saturating_add(100_000))
        .expect("timelock batch_create_locks fixture transaction must leave capacity");
    let batch_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &batch_fixture_input,
        vec![],
        vec![CellOutput { capacity: batch_fixture_capacity, lock: batch_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let batch_fixture_tx_id = spora_hashes::Hash::from_bytes(batch_fixture_tx.id());
    let batch_input = TransactionOutpoint::new(batch_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&batch_fixture_tx).into(), false)
        .await
        .expect("timelock batch_create_locks fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, batch_fixture_tx_id, "timelock batch_create_locks fixture cell").await;

    let batch_current_height = 90;
    let batch_owners = [[153; 32], [154; 32], [155; 32], [156; 32]];
    let batch_unlock_heights = [120, 130, 140, 150];
    let batch_witness = batch_create_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Bytes(fixed_32_array_arg([[0; 32]; 4])),
            cellscript::EntryWitnessArg::Bytes(fixed_32_array_arg(batch_owners)),
            cellscript::EntryWitnessArg::Bytes(fixed_u64_array_arg(batch_unlock_heights)),
            cellscript::EntryWitnessArg::U64(batch_current_height),
        ])
        .expect("timelock batch_create_locks action witness must encode owners, unlock_heights, and current_height");
    let batch_output_capacity = batch_fixture_capacity
        .checked_sub(required_fee(1, 4).saturating_add(100_000))
        .expect("timelock batch_create_locks action must leave fee")
        / 4;
    let malformed_batch_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(batch_input.tx_hash, batch_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(batch_code_outpoint.tx_hash, batch_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            (0..4).map(|_| CellOutput { capacity: batch_output_capacity, lock: batch_lock.clone(), type_: None }).collect(),
            vec![
                timelock_cell_data(batch_owners[0], 0, batch_unlock_heights[0], batch_current_height),
                timelock_cell_data(batch_owners[1], 0, batch_unlock_heights[1] + 1, batch_current_height),
                timelock_cell_data(batch_owners[2], 0, batch_unlock_heights[2], batch_current_height),
                timelock_cell_data(batch_owners[3], 0, batch_unlock_heights[3], batch_current_height),
            ],
            vec![batch_witness.clone()],
        )
        .expect("malformed timelock batch_create_locks transaction must be structurally valid"),
        &batch_create_artifact.action,
        "malformed timelock batch_create_locks transaction",
    );
    let malformed_batch_reason = rpc_client
        .submit_transaction((&malformed_batch_tx).into(), false)
        .await
        .expect_err("malformed timelock batch_create_locks must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_batch_reason, "timelock batch_create_locks");

    let valid_batch_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(batch_input.tx_hash, batch_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(batch_code_outpoint.tx_hash, batch_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            (0..4).map(|_| CellOutput { capacity: batch_output_capacity, lock: batch_lock.clone(), type_: None }).collect(),
            vec![
                timelock_cell_data(batch_owners[0], 0, batch_unlock_heights[0], batch_current_height),
                timelock_cell_data(batch_owners[1], 0, batch_unlock_heights[1], batch_current_height),
                timelock_cell_data(batch_owners[2], 0, batch_unlock_heights[2], batch_current_height),
                timelock_cell_data(batch_owners[3], 0, batch_unlock_heights[3], batch_current_height),
            ],
            vec![batch_witness],
        )
        .expect("valid timelock batch_create_locks transaction must be structurally valid"),
        &batch_create_artifact.action,
        "valid timelock batch_create_locks transaction",
    );
    let valid_batch_tx_id = spora_hashes::Hash::from_bytes(valid_batch_tx.id());
    rpc_client
        .submit_transaction((&valid_batch_tx).into(), false)
        .await
        .expect("valid timelock batch_create_locks must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_batch_tx_id, "valid timelock batch_create_locks action").await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    for action in [
        "create_absolute_lock",
        "create_relative_lock",
        "lock_asset",
        "request_release",
        "execute_release",
        "request_emergency_release",
        "approve_emergency_release",
        "execute_emergency_release",
        "extend_lock",
        "batch_create_locks",
    ] {
        coverage.valid.insert(("timelock.cell".to_string(), action.to_string()));
        coverage.malformed.insert(("timelock.cell".to_string(), action.to_string()));
    }
    coverage
}

async fn run_invoice_financing_action_builder_matrix(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    prealloc_schnorr_key: secp256k1::Keypair,
    prealloc_address: &Address,
    always_success_code_outpoint: &TransactionOutpoint,
    deployments: &[CellScriptExampleDeployment],
) -> SporaActionBuilderMatrixCoverage {
    let Some(invoice_deployment) = deployments.iter().find(|deployment| deployment.name == "invoice_financing.cell") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(register_artifact) = invoice_deployment.action_artifacts.iter().find(|artifact| artifact.name == "register_invoice")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(approve_artifact) = invoice_deployment.action_artifacts.iter().find(|artifact| artifact.name == "approve_drawdown")
    else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(inspect_artifact) = invoice_deployment.action_artifacts.iter().find(|artifact| artifact.name == "inspect_invoice") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(settle_artifact) = invoice_deployment.action_artifacts.iter().find(|artifact| artifact.name == "settle_invoice") else {
        return SporaActionBuilderMatrixCoverage::default();
    };
    let Some(cancel_artifact) = invoice_deployment.action_artifacts.iter().find(|artifact| artifact.name == "cancel_invoice") else {
        return SporaActionBuilderMatrixCoverage::default();
    };

    submit_empty_blocks(rpc_client, miner_address, 10).await;
    let mut spendable_cells = fetch_spendable_cells(rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .filter(|(_, meta)| meta.data_bytes == 0 && meta.type_hash.is_none())
        .collect::<VecDeque<_>>();
    assert!(
        spendable_cells.len() >= 12,
        "invoice financing action builder matrix needs twelve matured plain prealloc cells for scoped deploys and executable fixtures"
    );

    let register_code_outpoint = deploy_cellscript_action_artifact(
        rpc_client,
        miner_address,
        prealloc_schnorr_key,
        prealloc_address,
        &mut spendable_cells,
        register_artifact,
        "invoice register_invoice",
    )
    .await;
    let register_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "invoice register_invoice fixture");
    let register_fixture_input_capacity = register_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let register_lock = Script::new(register_artifact.code_hash, 0, vec![]);
    let register_fixture_capacity = register_fixture_input_capacity
        .checked_sub(required_fee(register_fixture_input.len(), 1).saturating_add(100_000))
        .expect("invoice register_invoice fixture transaction must leave capacity");
    let register_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &register_fixture_input,
        vec![],
        vec![CellOutput { capacity: register_fixture_capacity, lock: register_lock.clone(), type_: None }],
        vec![vec![]],
    );
    let register_fixture_tx_id = spora_hashes::Hash::from_bytes(register_fixture_tx.id());
    let register_input = TransactionOutpoint::new(register_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&register_fixture_tx).into(), false)
        .await
        .expect("invoice register_invoice fixture cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, register_fixture_tx_id, "invoice register_invoice fixture cell").await;

    let seller_address =
        Address::new_std_single(NetworkType::Devnet.into(), &[201; 32]).expect("invoice seller address must be valid");
    let buyer_address = Address::new_std_single(NetworkType::Devnet.into(), &[202; 32]).expect("invoice buyer address must be valid");
    let seller_lock = pay_to_acceptance_owner(&seller_address);
    let buyer_lock = pay_to_acceptance_owner(&buyer_address);
    let seller = seller_lock.hash();
    let buyer = buyer_lock.hash();
    let invoice_id = [0xA1; 32];
    let face_value = 1_250_000;
    let due_timepoint = 9_000;
    let register_witness = register_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Hash(invoice_id),
            cellscript::EntryWitnessArg::Address(seller),
            cellscript::EntryWitnessArg::Address(buyer),
            cellscript::EntryWitnessArg::U64(face_value),
            cellscript::EntryWitnessArg::U64(due_timepoint),
        ])
        .expect("invoice register_invoice witness must encode invoice fields");
    let register_output_capacity = register_fixture_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("invoice register_invoice action must leave fee");
    let malformed_register_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(register_input.tx_hash, register_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(register_code_outpoint.tx_hash, register_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: register_output_capacity, lock: seller_lock.clone(), type_: None }],
            vec![invoice_cell_data(invoice_id, seller, buyer, face_value.saturating_add(1), 0, due_timepoint, INVOICE_STATE_ISSUED)],
            vec![register_witness.clone()],
        )
        .expect("malformed invoice register_invoice transaction must be structurally valid"),
        &register_artifact.action,
        "malformed invoice register_invoice transaction",
    );
    let malformed_register_reason = rpc_client
        .submit_transaction((&malformed_register_tx).into(), false)
        .await
        .expect_err("malformed invoice register_invoice must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_register_reason, "invoice register_invoice");

    let valid_register_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(register_input.tx_hash, register_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(register_code_outpoint.tx_hash, register_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: register_output_capacity, lock: seller_lock.clone(), type_: None }],
            vec![invoice_cell_data(invoice_id, seller, buyer, face_value, 0, due_timepoint, INVOICE_STATE_ISSUED)],
            vec![register_witness],
        )
        .expect("valid invoice register_invoice transaction must be structurally valid"),
        &register_artifact.action,
        "valid invoice register_invoice transaction",
    );
    let valid_register_tx_id = spora_hashes::Hash::from_bytes(valid_register_tx.id());
    rpc_client
        .submit_transaction((&valid_register_tx).into(), false)
        .await
        .expect("valid invoice register_invoice must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_register_tx_id, "valid invoice register_invoice action").await;

    let approve_code_outpoint = deploy_cellscript_action_artifact(
        rpc_client,
        miner_address,
        prealloc_schnorr_key,
        prealloc_address,
        &mut spendable_cells,
        approve_artifact,
        "invoice approve_drawdown",
    )
    .await;
    let approve_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "invoice approve_drawdown fixture");
    let approve_fixture_input_capacity = approve_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let approve_lock = Script::new(approve_artifact.code_hash, 0, vec![]);
    let approve_invoice_capacity = approve_fixture_input_capacity
        .checked_sub(required_fee(approve_fixture_input.len(), 1).saturating_add(100_000))
        .expect("invoice approve_drawdown fixture transaction must leave invoice capacity");
    let approve_invoice_id = [0xA2; 32];
    let approve_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &approve_fixture_input,
        vec![],
        vec![CellOutput { capacity: approve_invoice_capacity, lock: approve_lock.clone(), type_: None }],
        vec![invoice_cell_data(approve_invoice_id, seller, buyer, face_value, 0, due_timepoint, INVOICE_STATE_ISSUED)],
    );
    let approve_fixture_tx_id = spora_hashes::Hash::from_bytes(approve_fixture_tx.id());
    let approve_invoice_input = TransactionOutpoint::new(approve_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&approve_fixture_tx).into(), false)
        .await
        .expect("invoice approve_drawdown fixture invoice cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, approve_fixture_tx_id, "invoice approve_drawdown fixture invoice cell")
        .await;

    let lender_address =
        Address::new_std_single(NetworkType::Devnet.into(), &[203; 32]).expect("invoice lender address must be valid");
    let lender_lock = pay_to_acceptance_owner(&lender_address);
    let lender = lender_lock.hash();
    let position_type = Script::new(always_success_code_hash(), 0, b"invoice-position".to_vec());
    let principal = 900_000;
    let discount_bps = 275;
    let malformed_discount_bps = 10_001;
    let approve_witness = approve_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(lender),
            cellscript::EntryWitnessArg::U64(principal),
            cellscript::EntryWitnessArg::U16(discount_bps),
        ])
        .expect("invoice approve_drawdown witness must encode lender, principal, and discount");
    let malformed_approve_witness = approve_artifact
        .action
        .entry_witness_args(&[
            cellscript::EntryWitnessArg::Address(lender),
            cellscript::EntryWitnessArg::U64(principal),
            cellscript::EntryWitnessArg::U16(malformed_discount_bps),
        ])
        .expect("invoice malformed approve_drawdown witness must encode lender, principal, and discount");
    let approve_position_capacity = approve_invoice_capacity / 3;
    let approve_invoice_output_capacity = approve_invoice_capacity
        .checked_sub(approve_position_capacity)
        .and_then(|value| value.checked_sub(required_fee(1, 2).saturating_add(100_000)))
        .expect("invoice approve_drawdown action must leave invoice output capacity");
    let approve_after_data =
        invoice_cell_data(approve_invoice_id, seller, buyer, face_value, principal, due_timepoint, INVOICE_STATE_FUNDED);
    let malformed_approve_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(approve_invoice_input.tx_hash, approve_invoice_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(approve_code_outpoint.tx_hash, approve_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: approve_invoice_output_capacity, lock: approve_lock.clone(), type_: None },
                CellOutput { capacity: approve_position_capacity, lock: lender_lock.clone(), type_: Some(position_type.clone()) },
            ],
            vec![
                approve_after_data.clone(),
                financing_position_cell_data(approve_invoice_id, lender, principal, malformed_discount_bps, INVOICE_STATE_FUNDED),
            ],
            vec![malformed_approve_witness],
        )
        .expect("malformed invoice approve_drawdown transaction must be structurally valid"),
        &approve_artifact.action,
        "malformed invoice approve_drawdown transaction",
    );
    let malformed_approve_reason = rpc_client
        .submit_transaction((&malformed_approve_tx).into(), false)
        .await
        .expect_err("malformed invoice approve_drawdown must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_approve_reason, "invoice approve_drawdown");

    let valid_approve_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(approve_invoice_input.tx_hash, approve_invoice_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(approve_code_outpoint.tx_hash, approve_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: approve_invoice_output_capacity, lock: approve_lock.clone(), type_: None },
                CellOutput { capacity: approve_position_capacity, lock: lender_lock.clone(), type_: Some(position_type.clone()) },
            ],
            vec![
                approve_after_data,
                financing_position_cell_data(approve_invoice_id, lender, principal, discount_bps, INVOICE_STATE_FUNDED),
            ],
            vec![approve_witness],
        )
        .expect("valid invoice approve_drawdown transaction must be structurally valid"),
        &approve_artifact.action,
        "valid invoice approve_drawdown transaction",
    );
    let valid_approve_tx_id = spora_hashes::Hash::from_bytes(valid_approve_tx.id());
    rpc_client
        .submit_transaction((&valid_approve_tx).into(), false)
        .await
        .expect("valid invoice approve_drawdown must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_approve_tx_id, "valid invoice approve_drawdown action").await;

    let inspect_code_outpoint = deploy_cellscript_action_artifact(
        rpc_client,
        miner_address,
        prealloc_schnorr_key,
        prealloc_address,
        &mut spendable_cells,
        inspect_artifact,
        "invoice inspect_invoice",
    )
    .await;
    let inspect_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "invoice inspect_invoice fixture");
    let inspect_fixture_input_capacity = inspect_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let inspect_lock = Script::new(inspect_artifact.code_hash, 0, vec![]);
    let inspect_cell_capacity = inspect_fixture_input_capacity / 4;
    let inspect_change_capacity = inspect_fixture_input_capacity
        .checked_sub(inspect_cell_capacity.saturating_mul(3))
        .and_then(|value| value.checked_sub(required_fee(inspect_fixture_input.len(), 4).saturating_add(100_000)))
        .expect("invoice inspect_invoice fixture transaction must leave change");
    let inspect_active_invoice_id = [0xA3; 32];
    let inspect_inactive_invoice_id = [0xA4; 32];
    let inspect_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &inspect_fixture_input,
        vec![],
        vec![
            CellOutput { capacity: inspect_cell_capacity, lock: inspect_lock.clone(), type_: None },
            CellOutput { capacity: inspect_cell_capacity, lock: seller_lock.clone(), type_: None },
            CellOutput { capacity: inspect_cell_capacity, lock: seller_lock.clone(), type_: None },
            CellOutput { capacity: inspect_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            vec![],
            invoice_cell_data(inspect_active_invoice_id, seller, buyer, 0, 0, due_timepoint, INVOICE_STATE_ISSUED),
            invoice_cell_data(inspect_inactive_invoice_id, seller, buyer, 0, 0, due_timepoint, INVOICE_STATE_CANCELLED),
            vec![],
        ],
    );
    let inspect_fixture_tx_id = spora_hashes::Hash::from_bytes(inspect_fixture_tx.id());
    let inspect_action_input = TransactionOutpoint::new(inspect_fixture_tx.id(), 0);
    let inspect_active_dep = TransactionOutpoint::new(inspect_fixture_tx.id(), 1);
    let inspect_inactive_dep = TransactionOutpoint::new(inspect_fixture_tx.id(), 2);
    rpc_client
        .submit_transaction((&inspect_fixture_tx).into(), false)
        .await
        .expect("invoice inspect_invoice fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, inspect_fixture_tx_id, "invoice inspect_invoice fixture cells").await;

    let inspect_witness =
        inspect_artifact.action.entry_witness_args(&[]).expect("invoice inspect_invoice witness must encode empty args");
    let inspect_output_capacity = inspect_cell_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("invoice inspect_invoice action must leave change");
    let malformed_inspect_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(inspect_action_input.tx_hash, inspect_action_input.index), 0)],
            vec![
                CellDep {
                    out_point: OutPoint::new(inspect_inactive_dep.tx_hash, inspect_inactive_dep.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(inspect_code_outpoint.tx_hash, inspect_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: inspect_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
            vec![vec![]],
            vec![inspect_witness.clone()],
        )
        .expect("malformed invoice inspect_invoice transaction must be structurally valid"),
        &inspect_artifact.action,
        "malformed invoice inspect_invoice transaction",
    );
    let malformed_inspect_reason = rpc_client
        .submit_transaction((&malformed_inspect_tx).into(), false)
        .await
        .expect_err("malformed invoice inspect_invoice must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_inspect_reason, "invoice inspect_invoice");

    let valid_inspect_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(inspect_action_input.tx_hash, inspect_action_input.index), 0)],
            vec![
                CellDep { out_point: OutPoint::new(inspect_active_dep.tx_hash, inspect_active_dep.index), dep_type: DepType::Code },
                CellDep {
                    out_point: OutPoint::new(inspect_code_outpoint.tx_hash, inspect_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![CellOutput { capacity: inspect_output_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None }],
            vec![vec![]],
            vec![inspect_witness],
        )
        .expect("valid invoice inspect_invoice transaction must be structurally valid"),
        &inspect_artifact.action,
        "valid invoice inspect_invoice transaction",
    );
    let valid_inspect_tx_id = spora_hashes::Hash::from_bytes(valid_inspect_tx.id());
    rpc_client
        .submit_transaction((&valid_inspect_tx).into(), false)
        .await
        .expect("valid invoice inspect_invoice must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_inspect_tx_id, "valid invoice inspect_invoice action").await;

    let settle_code_outpoint = deploy_cellscript_action_artifact(
        rpc_client,
        miner_address,
        prealloc_schnorr_key,
        prealloc_address,
        &mut spendable_cells,
        settle_artifact,
        "invoice settle_invoice",
    )
    .await;
    let settle_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "invoice settle_invoice fixture");
    let settle_fixture_input_capacity = settle_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let settle_lock = Script::new(settle_artifact.code_hash, 0, vec![]);
    let settle_input_capacity = settle_fixture_input_capacity / 3;
    let settle_change_capacity = settle_fixture_input_capacity
        .checked_sub(settle_input_capacity.saturating_mul(2))
        .and_then(|value| value.checked_sub(required_fee(settle_fixture_input.len(), 3).saturating_add(100_000)))
        .expect("invoice settle_invoice fixture transaction must leave change");
    let settle_invoice_id = [0xA5; 32];
    let settle_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &settle_fixture_input,
        vec![CellDep {
            out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
            dep_type: DepType::Code,
        }],
        vec![
            CellOutput { capacity: settle_input_capacity, lock: settle_lock.clone(), type_: None },
            CellOutput { capacity: settle_input_capacity, lock: settle_lock.clone(), type_: Some(position_type.clone()) },
            CellOutput { capacity: settle_change_capacity, lock: pay_to_acceptance_owner(prealloc_address), type_: None },
        ],
        vec![
            invoice_cell_data(settle_invoice_id, seller, buyer, face_value, principal, due_timepoint, INVOICE_STATE_FUNDED),
            financing_position_cell_data(settle_invoice_id, lender, principal, discount_bps, INVOICE_STATE_FUNDED),
            vec![],
        ],
    );
    let settle_fixture_tx_id = spora_hashes::Hash::from_bytes(settle_fixture_tx.id());
    let settle_invoice_input = TransactionOutpoint::new(settle_fixture_tx.id(), 0);
    let settle_position_input = TransactionOutpoint::new(settle_fixture_tx.id(), 1);
    rpc_client
        .submit_transaction((&settle_fixture_tx).into(), false)
        .await
        .expect("invoice settle_invoice fixture cells must be accepted");
    submit_next_template_containing(rpc_client, miner_address, settle_fixture_tx_id, "invoice settle_invoice fixture cells").await;

    let payer_address = Address::new_std_single(NetworkType::Devnet.into(), &[204; 32]).expect("invoice payer address must be valid");
    let payer_lock = pay_to_acceptance_owner(&payer_address);
    let payer = payer_lock.hash();
    let paid_amount = principal;
    let malformed_paid_amount = principal.saturating_sub(1);
    let settle_witness = settle_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(payer), cellscript::EntryWitnessArg::U64(paid_amount)])
        .expect("invoice settle_invoice witness must encode payer and paid amount");
    let malformed_settle_witness = settle_artifact
        .action
        .entry_witness_args(&[cellscript::EntryWitnessArg::Address(payer), cellscript::EntryWitnessArg::U64(malformed_paid_amount)])
        .expect("invoice malformed settle_invoice witness must encode payer and paid amount");
    let receipt_output_capacity = settle_input_capacity / 3;
    let settle_invoice_output_capacity = settle_input_capacity
        .saturating_add(settle_input_capacity)
        .checked_sub(receipt_output_capacity)
        .and_then(|value| value.checked_sub(required_fee(2, 2).saturating_add(100_000)))
        .expect("invoice settle_invoice action must leave invoice output capacity");
    let settle_after_data =
        invoice_cell_data(settle_invoice_id, seller, buyer, face_value, principal, due_timepoint, INVOICE_STATE_SETTLED);
    let malformed_settle_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(settle_invoice_input.tx_hash, settle_invoice_input.index), 0),
                CellInput::new(OutPoint::new(settle_position_input.tx_hash, settle_position_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(settle_code_outpoint.tx_hash, settle_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: settle_invoice_output_capacity, lock: settle_lock.clone(), type_: None },
                CellOutput { capacity: receipt_output_capacity, lock: lender_lock.clone(), type_: None },
            ],
            vec![
                settle_after_data.clone(),
                settlement_receipt_cell_data(settle_invoice_id, payer, malformed_paid_amount, INVOICE_STATE_SETTLED),
            ],
            vec![malformed_settle_witness, vec![]],
        )
        .expect("malformed invoice settle_invoice transaction must be structurally valid"),
        &settle_artifact.action,
        "malformed invoice settle_invoice transaction",
    );
    let malformed_settle_reason = rpc_client
        .submit_transaction((&malformed_settle_tx).into(), false)
        .await
        .expect_err("malformed invoice settle_invoice must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_settle_reason, "invoice settle_invoice");

    let valid_settle_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![
                CellInput::new(OutPoint::new(settle_invoice_input.tx_hash, settle_invoice_input.index), 0),
                CellInput::new(OutPoint::new(settle_position_input.tx_hash, settle_position_input.index), 0),
            ],
            vec![
                CellDep {
                    out_point: OutPoint::new(settle_code_outpoint.tx_hash, settle_code_outpoint.index),
                    dep_type: DepType::Code,
                },
                CellDep {
                    out_point: OutPoint::new(always_success_code_outpoint.tx_hash, always_success_code_outpoint.index),
                    dep_type: DepType::Code,
                },
            ],
            vec![
                CellOutput { capacity: settle_invoice_output_capacity, lock: settle_lock.clone(), type_: None },
                CellOutput { capacity: receipt_output_capacity, lock: lender_lock.clone(), type_: None },
            ],
            vec![settle_after_data, settlement_receipt_cell_data(settle_invoice_id, payer, paid_amount, INVOICE_STATE_SETTLED)],
            vec![settle_witness, vec![]],
        )
        .expect("valid invoice settle_invoice transaction must be structurally valid"),
        &settle_artifact.action,
        "valid invoice settle_invoice transaction",
    );
    let valid_settle_tx_id = spora_hashes::Hash::from_bytes(valid_settle_tx.id());
    rpc_client
        .submit_transaction((&valid_settle_tx).into(), false)
        .await
        .expect("valid invoice settle_invoice must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_settle_tx_id, "valid invoice settle_invoice action").await;

    let cancel_code_outpoint = deploy_cellscript_action_artifact(
        rpc_client,
        miner_address,
        prealloc_schnorr_key,
        prealloc_address,
        &mut spendable_cells,
        cancel_artifact,
        "invoice cancel_invoice",
    )
    .await;
    let cancel_fixture_input = pop_plain_cells(&mut spendable_cells, 1, "invoice cancel_invoice fixture");
    let cancel_fixture_input_capacity = cancel_fixture_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let cancel_lock = Script::new(cancel_artifact.code_hash, 0, vec![]);
    let cancel_invoice_capacity = cancel_fixture_input_capacity
        .checked_sub(required_fee(cancel_fixture_input.len(), 1).saturating_add(100_000))
        .expect("invoice cancel_invoice fixture transaction must leave invoice capacity");
    let cancel_invoice_id = [0xA6; 32];
    let cancel_fixture_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        &cancel_fixture_input,
        vec![],
        vec![CellOutput { capacity: cancel_invoice_capacity, lock: cancel_lock.clone(), type_: None }],
        vec![invoice_cell_data(cancel_invoice_id, seller, buyer, face_value, 0, due_timepoint, INVOICE_STATE_ISSUED)],
    );
    let cancel_fixture_tx_id = spora_hashes::Hash::from_bytes(cancel_fixture_tx.id());
    let cancel_invoice_input = TransactionOutpoint::new(cancel_fixture_tx.id(), 0);
    rpc_client
        .submit_transaction((&cancel_fixture_tx).into(), false)
        .await
        .expect("invoice cancel_invoice fixture invoice cell must be accepted");
    submit_next_template_containing(rpc_client, miner_address, cancel_fixture_tx_id, "invoice cancel_invoice fixture invoice cell")
        .await;

    let cancel_witness =
        cancel_artifact.action.entry_witness_args(&[]).expect("invoice cancel_invoice witness must encode empty args");
    let cancel_output_capacity = cancel_invoice_capacity
        .checked_sub(required_fee(1, 1).saturating_add(100_000))
        .expect("invoice cancel_invoice action must leave fee");
    let malformed_cancel_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(cancel_invoice_input.tx_hash, cancel_invoice_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(cancel_code_outpoint.tx_hash, cancel_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: cancel_output_capacity, lock: cancel_lock.clone(), type_: None }],
            vec![invoice_cell_data(cancel_invoice_id, seller, buyer, face_value, 0, due_timepoint, INVOICE_STATE_FUNDED)],
            vec![cancel_witness.clone()],
        )
        .expect("malformed invoice cancel_invoice transaction must be structurally valid"),
        &cancel_artifact.action,
        "malformed invoice cancel_invoice transaction",
    );
    let malformed_cancel_reason = rpc_client
        .submit_transaction((&malformed_cancel_tx).into(), false)
        .await
        .expect_err("malformed invoice cancel_invoice must be rejected by the scoped action verifier")
        .to_string();
    assert_action_malformed_rejection(&malformed_cancel_reason, "invoice cancel_invoice");

    let valid_cancel_tx = with_compiled_action_scheduler_witness(
        CellTx::new(
            vec![CellInput::new(OutPoint::new(cancel_invoice_input.tx_hash, cancel_invoice_input.index), 0)],
            vec![CellDep {
                out_point: OutPoint::new(cancel_code_outpoint.tx_hash, cancel_code_outpoint.index),
                dep_type: DepType::Code,
            }],
            vec![CellOutput { capacity: cancel_output_capacity, lock: cancel_lock, type_: None }],
            vec![invoice_cell_data(cancel_invoice_id, seller, buyer, face_value, 0, due_timepoint, INVOICE_STATE_CANCELLED)],
            vec![cancel_witness],
        )
        .expect("valid invoice cancel_invoice transaction must be structurally valid"),
        &cancel_artifact.action,
        "valid invoice cancel_invoice transaction",
    );
    let valid_cancel_tx_id = spora_hashes::Hash::from_bytes(valid_cancel_tx.id());
    rpc_client
        .submit_transaction((&valid_cancel_tx).into(), false)
        .await
        .expect("valid invoice cancel_invoice must be accepted by the scoped action verifier");
    submit_next_template_containing(rpc_client, miner_address, valid_cancel_tx_id, "valid invoice cancel_invoice action").await;

    let mut coverage = SporaActionBuilderMatrixCoverage::default();
    for action in ["register_invoice", "approve_drawdown", "inspect_invoice", "settle_invoice", "cancel_invoice"] {
        coverage.valid.insert(("invoice_financing.cell".to_string(), action.to_string()));
        coverage.malformed.insert(("invoice_financing.cell".to_string(), action.to_string()));
    }
    coverage
}

async fn submit_next_template_containing(
    rpc_client: &spora_grpc_client::GrpcClient,
    miner_address: &Address,
    tx_id: spora_hashes::Hash,
    description: &str,
) {
    let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == tx_id),
        "expected block template to include {description}"
    );
    rpc_client.submit_block(template.block, false).await.unwrap();
}

async fn submit_empty_blocks(rpc_client: &spora_grpc_client::GrpcClient, miner_address: &Address, count: usize) {
    for _ in 0..count {
        let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
        rpc_client.submit_block(template.block, false).await.unwrap();
    }
}

async fn current_header_dep_hash_and_daa(rpc_client: &spora_grpc_client::GrpcClient, miner_address: &Address) -> ([u8; 32], u64) {
    let template = rpc_client
        .get_block_template(miner_address.clone(), vec![])
        .await
        .expect("get_block_template for current header dep must succeed");
    let selected_parent = template
        .block
        .header
        .parents_by_level
        .first()
        .and_then(|level| level.first())
        .copied()
        .expect("block template must expose a selected parent for header dep");
    let current_header =
        rpc_client.get_header(selected_parent).await.expect("selected parent header lookup for header dep must succeed");
    let current_daa = current_header.daa_score;
    (selected_parent.as_bytes(), current_daa)
}

fn token_cell_data(amount: u64, symbol: [u8; 8]) -> Vec<u8> {
    let mut data = amount.to_le_bytes().to_vec();
    data.extend_from_slice(&symbol);
    data
}

fn mint_authority_cell_data(symbol: [u8; 8], max_supply: u64, minted: u64) -> Vec<u8> {
    let mut data = symbol.to_vec();
    data.extend_from_slice(&max_supply.to_le_bytes());
    data.extend_from_slice(&minted.to_le_bytes());
    data
}

fn pool_cell_data(
    token_a_symbol: [u8; 8],
    token_b_symbol: [u8; 8],
    reserve_a: u64,
    reserve_b: u64,
    total_lp: u64,
    fee_rate_bps: u16,
) -> Vec<u8> {
    let mut data = token_a_symbol.to_vec();
    data.extend_from_slice(&token_b_symbol);
    data.extend_from_slice(&reserve_a.to_le_bytes());
    data.extend_from_slice(&reserve_b.to_le_bytes());
    data.extend_from_slice(&total_lp.to_le_bytes());
    data.extend_from_slice(&fee_rate_bps.to_le_bytes());
    data
}

fn lp_receipt_cell_data(pool_id: [u8; 32], lp_amount: u64, provider: [u8; 32]) -> Vec<u8> {
    let mut data = pool_id.to_vec();
    data.extend_from_slice(&lp_amount.to_le_bytes());
    data.extend_from_slice(&provider);
    data
}

fn vesting_config_cell_data(admin: [u8; 32], token_symbol: [u8; 8], cliff_period: u64, total_period: u64, revocable: bool) -> Vec<u8> {
    let mut data = admin.to_vec();
    data.extend_from_slice(&token_symbol);
    data.extend_from_slice(&cliff_period.to_le_bytes());
    data.extend_from_slice(&total_period.to_le_bytes());
    data.push(u8::from(revocable));
    data
}

fn vesting_grant_cell_data(
    state: u8,
    beneficiary: [u8; 32],
    total_amount: u64,
    claimed_amount: u64,
    grant_daa_score: u64,
    cliff_daa_score: u64,
    end_daa_score: u64,
    token_symbol: [u8; 8],
) -> Vec<u8> {
    let mut data = vec![state];
    data.extend_from_slice(&beneficiary);
    data.extend_from_slice(&total_amount.to_le_bytes());
    data.extend_from_slice(&claimed_amount.to_le_bytes());
    data.extend_from_slice(&grant_daa_score.to_le_bytes());
    data.extend_from_slice(&cliff_daa_score.to_le_bytes());
    data.extend_from_slice(&end_daa_score.to_le_bytes());
    data.extend_from_slice(&token_symbol);
    data
}

const INVOICE_STATE_ISSUED: u8 = 0;
const INVOICE_STATE_FUNDED: u8 = 1;
const INVOICE_STATE_SETTLED: u8 = 2;
const INVOICE_STATE_CANCELLED: u8 = 3;

fn invoice_cell_data(
    invoice_id: [u8; 32],
    seller: [u8; 32],
    buyer: [u8; 32],
    face_value: u64,
    financed_amount: u64,
    due_timepoint: u64,
    state: u8,
) -> Vec<u8> {
    let mut data = invoice_id.to_vec();
    data.extend_from_slice(&seller);
    data.extend_from_slice(&buyer);
    data.extend_from_slice(&face_value.to_le_bytes());
    data.extend_from_slice(&financed_amount.to_le_bytes());
    data.extend_from_slice(&due_timepoint.to_le_bytes());
    data.push(state);
    data
}

fn financing_position_cell_data(invoice_id: [u8; 32], lender: [u8; 32], principal: u64, discount_bps: u16, state: u8) -> Vec<u8> {
    let mut data = invoice_id.to_vec();
    data.extend_from_slice(&lender);
    data.extend_from_slice(&principal.to_le_bytes());
    data.extend_from_slice(&discount_bps.to_le_bytes());
    data.push(state);
    data
}

fn settlement_receipt_cell_data(invoice_id: [u8; 32], payer: [u8; 32], paid_amount: u64, state: u8) -> Vec<u8> {
    let mut data = invoice_id.to_vec();
    data.extend_from_slice(&payer);
    data.extend_from_slice(&paid_amount.to_le_bytes());
    data.push(state);
    data
}

fn nft_cell_data(token_id: u64, owner: [u8; 32], metadata_hash: [u8; 32], royalty_recipient: [u8; 32], royalty_bps: u16) -> Vec<u8> {
    let mut data = token_id.to_le_bytes().to_vec();
    data.extend_from_slice(&owner);
    data.extend_from_slice(&metadata_hash);
    data.extend_from_slice(&royalty_recipient);
    data.extend_from_slice(&royalty_bps.to_le_bytes());
    data
}

fn listing_cell_data(token_id: u64, seller: [u8; 32], price: u64, created_at: u64) -> Vec<u8> {
    let mut data = token_id.to_le_bytes().to_vec();
    data.extend_from_slice(&seller);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&created_at.to_le_bytes());
    data.push(0);
    data
}

fn offer_cell_data(token_id: u64, buyer: [u8; 32], price: u64, expires_at: u64) -> Vec<u8> {
    let mut data = token_id.to_le_bytes().to_vec();
    data.extend_from_slice(&buyer);
    data.extend_from_slice(&price.to_le_bytes());
    data.extend_from_slice(&expires_at.to_le_bytes());
    data.push(0);
    data
}

fn royalty_payment_cell_data(token_id: u64, recipient: [u8; 32], amount: u64) -> Vec<u8> {
    let mut data = token_id.to_le_bytes().to_vec();
    data.extend_from_slice(&recipient);
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

fn timelock_cell_data(owner: [u8; 32], lock_type: u8, unlock_height: u64, created_at: u64) -> Vec<u8> {
    let mut data = vec![0; 32];
    data.extend_from_slice(&owner);
    data.push(lock_type);
    data.extend_from_slice(&unlock_height.to_le_bytes());
    data.extend_from_slice(&created_at.to_le_bytes());
    data
}

fn release_request_cell_data(lock_hash: [u8; 32], requester: [u8; 32], requested_at: u64) -> Vec<u8> {
    let mut data = lock_hash.to_vec();
    data.extend_from_slice(&requester);
    data.extend_from_slice(&requested_at.to_le_bytes());
    data.push(0);
    data
}

fn release_record_cell_data(lock_hash: [u8; 32], released_at: u64, released_by: [u8; 32]) -> Vec<u8> {
    let mut data = lock_hash.to_vec();
    data.extend_from_slice(&released_at.to_le_bytes());
    data.extend_from_slice(&released_by);
    data
}

fn locked_asset_molecule_cell_data(asset_type: &[u8], amount: u64, lock_hash: [u8; 32]) -> Vec<u8> {
    molecule_table_cell_data(&[asset_type.to_vec(), amount.to_le_bytes().to_vec(), lock_hash.to_vec()])
}

fn emergency_release_molecule_cell_data(
    lock_hash: [u8; 32],
    requester: [u8; 32],
    reason: &[u8],
    requested_at: u64,
    approvers: &[[u8; 32]],
) -> Vec<u8> {
    let approver_items = approvers.iter().map(|approver| approver.to_vec()).collect::<Vec<_>>();
    molecule_table_cell_data(&[
        lock_hash.to_vec(),
        requester.to_vec(),
        reason.to_vec(),
        requested_at.to_le_bytes().to_vec(),
        molecule_fixvec_cell_data(&approver_items),
        vec![0],
    ])
}

fn multisig_wallet_molecule_cell_data(signers: &[[u8; 32]], threshold: u8, nonce: u64, created_at: u64) -> Vec<u8> {
    let signer_items = signers.iter().map(|signer| signer.to_vec()).collect::<Vec<_>>();
    molecule_table_cell_data(&[
        vec![0; 32],
        molecule_fixvec_cell_data(&signer_items),
        vec![threshold],
        nonce.to_le_bytes().to_vec(),
        created_at.to_le_bytes().to_vec(),
    ])
}

fn multisig_proposal_molecule_cell_data(
    wallet_hash: [u8; 32],
    proposal_id: u64,
    proposer: [u8; 32],
    operation: u8,
    target: [u8; 32],
    amount: u64,
    data: &[u8],
    required_signatures: u8,
    signatures: &[[u8; 96]],
    created_at: u64,
    expires_at: u64,
) -> Vec<u8> {
    let signature_items = signatures.iter().map(|signature| signature.to_vec()).collect::<Vec<_>>();
    molecule_table_cell_data(&[
        wallet_hash.to_vec(),
        proposal_id.to_le_bytes().to_vec(),
        proposer.to_vec(),
        vec![operation],
        target.to_vec(),
        amount.to_le_bytes().to_vec(),
        molecule_bytes_cell_data(data),
        vec![required_signatures],
        molecule_fixvec_cell_data(&signature_items),
        created_at.to_le_bytes().to_vec(),
        expires_at.to_le_bytes().to_vec(),
        vec![0],
    ])
}

fn multisig_signature_struct(signer: [u8; 32], signature: [u8; 64]) -> [u8; 96] {
    let mut out = [0u8; 96];
    out[..32].copy_from_slice(&signer);
    out[32..].copy_from_slice(&signature);
    out
}

fn multisig_signature_confirmation_cell_data(proposal_id: u64, signer: [u8; 32], timestamp: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(48);
    out.extend_from_slice(&proposal_id.to_le_bytes());
    out.extend_from_slice(&signer);
    out.extend_from_slice(&timestamp.to_le_bytes());
    out
}

fn multisig_execution_record_cell_data(proposal_id: u64, executor: [u8; 32], executed_at: u64, success: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(49);
    out.extend_from_slice(&proposal_id.to_le_bytes());
    out.extend_from_slice(&executor);
    out.extend_from_slice(&executed_at.to_le_bytes());
    out.push(u8::from(success));
    out
}

fn molecule_bytes_cell_data(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + data.len());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn molecule_fixvec_cell_data(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + items.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&(items.len() as u32).to_le_bytes());
    for item in items {
        out.extend_from_slice(item);
    }
    out
}

fn molecule_table_cell_data(fields: &[Vec<u8>]) -> Vec<u8> {
    let header_len = 4 + fields.len() * 4;
    let total_len = header_len + fields.iter().map(Vec::len).sum::<usize>();
    let mut data = Vec::with_capacity(total_len);
    data.extend_from_slice(&(total_len as u32).to_le_bytes());
    let mut offset = header_len as u32;
    for field in fields {
        data.extend_from_slice(&offset.to_le_bytes());
        offset = offset.saturating_add(field.len() as u32);
    }
    for field in fields {
        data.extend_from_slice(field);
    }
    data
}

fn collection_cell_data(
    name: &[u8],
    symbol: &[u8],
    creator: [u8; 32],
    total_supply: u64,
    max_supply: u64,
    base_uri: &[u8],
) -> Vec<u8> {
    let fields = [
        name.to_vec(),
        symbol.to_vec(),
        creator.to_vec(),
        total_supply.to_le_bytes().to_vec(),
        max_supply.to_le_bytes().to_vec(),
        base_uri.to_vec(),
    ];
    let header_len = 4 + fields.len() * 4;
    let total_len = header_len + fields.iter().map(Vec::len).sum::<usize>();
    let mut data = Vec::with_capacity(total_len);
    data.extend_from_slice(&(total_len as u32).to_le_bytes());
    let mut offset = header_len as u32;
    for field in &fields {
        data.extend_from_slice(&offset.to_le_bytes());
        offset = offset.saturating_add(field.len() as u32);
    }
    for field in fields {
        data.extend_from_slice(&field);
    }
    data
}

fn fixed_32_array_arg(values: [[u8; 32]; 4]) -> Vec<u8> {
    let mut data = Vec::with_capacity(128);
    for value in values {
        data.extend_from_slice(&value);
    }
    data
}

fn fixed_u64_array_arg(values: [u64; 4]) -> Vec<u8> {
    let mut data = Vec::with_capacity(32);
    for value in values {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data
}

fn fixed_recipient_tuple_array8_arg(addresses: &[[u8; 32]], amounts: [u64; 8]) -> Vec<u8> {
    assert_eq!(addresses.len(), 8, "simple_launch recipient tuple array must have eight addresses");
    let mut data = Vec::with_capacity(8 * 40);
    for (address, amount) in addresses.iter().zip(amounts) {
        data.extend_from_slice(address);
        data.extend_from_slice(&amount.to_le_bytes());
    }
    data
}

fn fixed_recipient_tuple_array4_arg(addresses: &[[u8; 32]], amounts: [u64; 4]) -> Vec<u8> {
    assert_eq!(addresses.len(), 4, "launch_token recipient tuple array must have four addresses");
    let mut data = Vec::with_capacity(4 * 40);
    for (address, amount) in addresses.iter().zip(amounts) {
        data.extend_from_slice(address);
        data.extend_from_slice(&amount.to_le_bytes());
    }
    data
}

fn assert_action_malformed_rejection(reason: &str, context: &str) {
    assert!(
        !reason.contains("not standard")
            && !reason.contains("storage mass")
            && !reason.contains("compute mass")
            && !reason.contains("transient")
            && !reason.contains("cycles exceeded")
            && !reason.contains("cycles limit"),
        "malformed {context} must fail in script validation, not policy, mass, or VM cycle limits: {reason}"
    );
}

struct CellScriptExampleDeployment {
    name: &'static str,
    artifact_size_bytes: usize,
    standard_deployment_storage_mass: u64,
    deployment_tx_id: spora_hashes::Hash,
    code_outpoint: TransactionOutpoint,
    locked_outpoint: TransactionOutpoint,
    locked_capacity: u64,
    deployment_probe_succeeded: bool,
    deployment_error: Option<String>,
    deployment_probe_status: &'static str,
    code_cell_indexed: bool,
    malformed_spend_rejected: bool,
    malformed_spend_probe_status: &'static str,
    malformed_spend_reject_reason: String,
    malformed_spend_rejected_by_standard_policy: bool,
    ckb_runtime_required: bool,
    action_artifacts: Vec<crate::common::cellscript_contracts::CompiledCellScriptActionArtifact>,
    action_names: Vec<String>,
    estimated_compute_mass: u64,
    estimated_storage_mass: u64,
    estimated_transient_mass: u64,
    estimated_code_deployment_mass: u64,
    requires_relaxed_mass_policy: bool,
}

#[derive(Default)]
struct SporaActionBuilderMatrixCoverage {
    valid: HashSet<(String, String)>,
    malformed: HashSet<(String, String)>,
}

#[derive(Serialize)]
struct BaseReport {
    profile: &'static str,
    network_id: &'static str,
    relaxed_mass_policy: RelaxedMassPolicyReport,
    prealloc_cells: u64,
    prealloc_amount_sau: u64,
    signed_transfer_confirmed: bool,
    multi_input_multi_output_confirmed: bool,
    parent_child_mempool_confirmed: bool,
    scheduler_tamper_rejected: bool,
    always_success_vm_spend_confirmed: bool,
    noop_cellscript_spend_confirmed: bool,
    cellscript_schema_output_spend_confirmed: bool,
    cellscript_parameterized_amount_spend_confirmed: bool,
    bundled_examples: Vec<BundledExampleReport>,
    production_gate: SporaProductionGateReport,
}

#[derive(Serialize)]
struct RelaxedMassPolicyReport {
    mode: &'static str,
    relay_non_standard: bool,
    block_max_mass: u64,
    applies_to_all_networks_when_explicitly_enabled: bool,
    standard_policy_preserved_by_default: bool,
}

#[derive(Serialize)]
struct BundledExampleReport {
    name: &'static str,
    artifact_size_bytes: usize,
    ckb_runtime_required: bool,
    action_names: Vec<String>,
    action_count: usize,
    estimated_compute_mass: u64,
    estimated_storage_mass: u64,
    estimated_transient_mass: u64,
    estimated_code_deployment_mass: u64,
    requires_relaxed_mass_policy: bool,
    estimated_standard_deployment_storage_mass: u64,
    fits_standard_relay_transaction_mass: bool,
    deployment_tx_id: String,
    code_cell_outpoint: OutPointReport,
    locked_probe_outpoint: OutPointReport,
    deployment_probe_status: &'static str,
    code_cell_indexed: bool,
    malformed_spend_rejected: bool,
    malformed_spend_probe_status: &'static str,
    malformed_spend_reject_reason: String,
    malformed_spend_rejected_by_standard_policy: bool,
}

#[derive(Default, Serialize)]
struct SporaProductionGateReport {
    status: &'static str,
    production_ready: bool,
    requires_standard_mass_policy: bool,
    standard_mass_policy_used: bool,
    standard_block_max_mass: u64,
    standard_relay_max_tx_mass: u64,
    relaxed_block_max_mass: u64,
    bundled_example_count: usize,
    standard_relay_deploy_compatible_example_count: usize,
    full_file_monolith_standard_relay_ready: bool,
    standard_relay_deploy_compatible_action_count: usize,
    scoped_action_standard_relay_ready: bool,
    requires_action_specific_builders: bool,
    scoped_action_artifact_count: usize,
    scheduler_witness_shape_count: usize,
    scheduler_witness_shape_malformed_count: usize,
    valid_action_specific_builder_count: usize,
    required_action_specific_builder_count: usize,
    malformed_action_matrix_count: usize,
    bundled_example_deployment_probe_count: usize,
    standard_relay_incompatible_examples: Vec<StandardRelayIncompatibleExampleReport>,
    blockers: Vec<String>,
    advisories: Vec<String>,
    action_builder_coverage: Vec<SporaActionBuilderCoverageReport>,
}

#[derive(Serialize)]
struct StandardRelayIncompatibleExampleReport {
    name: &'static str,
    artifact_size_bytes: usize,
    estimated_standard_deployment_storage_mass: u64,
    requires_relaxed_mass_policy: bool,
}

#[derive(Serialize)]
struct SporaActionBuilderCoverageReport {
    example: &'static str,
    action: String,
    effect_class: String,
    estimated_cycles: u64,
    deployment_probe_covered: bool,
    scoped_action_artifact_covered: bool,
    scoped_action_artifact_bytes: usize,
    scoped_action_artifact_hash: String,
    builder_requirements: SporaActionBuilderRequirementsReport,
    scheduler_witness_shape_covered: bool,
    scheduler_witness_shape_malformed_covered: bool,
    scheduler_witness_shape_error: Option<String>,
    valid_action_specific_builder_covered: bool,
    malformed_action_matrix_covered: bool,
}

#[derive(Serialize)]
struct SporaActionBuilderRequirementsReport {
    entry_param_count: usize,
    schema_backed_param_count: usize,
    fixed_byte_param_count: usize,
    scheduler_witness_bytes: usize,
    scheduler_access_count: usize,
    min_input_count: usize,
    min_cell_dep_count: usize,
    min_output_count: usize,
    consume_count: usize,
    read_ref_count: usize,
    create_count: usize,
    mutate_count: usize,
    transaction_runtime_input_requirement_count: usize,
    checked_runtime_obligation_count: usize,
    fail_closed_runtime_feature_count: usize,
    estimated_cycles: u64,
    estimated_standard_deployment_storage_mass: u64,
    fits_standard_relay_transaction_mass: bool,
    requires_action_specific_transaction_builder: bool,
}

#[derive(Serialize)]
struct OutPointReport {
    tx_hash: String,
    index: u32,
}

fn outpoint_report(outpoint: &TransactionOutpoint) -> OutPointReport {
    OutPointReport { tx_hash: hash_hex(&outpoint.tx_hash), index: outpoint.index }
}

fn hash_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn build_spora_production_gate(
    deployments: &[CellScriptExampleDeployment],
    standard_mass_policy_used: bool,
    action_builder_matrix: &SporaActionBuilderMatrixCoverage,
) -> SporaProductionGateReport {
    let action_builder_coverage = deployments
        .iter()
        .flat_map(|deployment| {
            deployment.action_artifacts.iter().map(|action_artifact| {
                let scheduler_shape = verify_action_scheduler_witness_shape(&action_artifact.action);
                SporaActionBuilderCoverageReport {
                    example: deployment.name,
                    action: action_artifact.name.clone(),
                    effect_class: action_artifact.action.effect_class.clone(),
                    estimated_cycles: action_artifact.action.estimated_cycles,
                    deployment_probe_covered: true,
                    scoped_action_artifact_covered: true,
                    scoped_action_artifact_bytes: action_artifact.artifact_bytes.len(),
                    scoped_action_artifact_hash: hash_hex(&action_artifact.code_hash),
                    builder_requirements: action_builder_requirements(
                        &action_artifact.action,
                        &scheduler_shape,
                        action_artifact.artifact_bytes.len(),
                    ),
                    scheduler_witness_shape_covered: scheduler_shape.valid_shape_covered,
                    scheduler_witness_shape_malformed_covered: scheduler_shape.malformed_shape_covered,
                    scheduler_witness_shape_error: scheduler_shape.error,
                    valid_action_specific_builder_covered: action_builder_matrix
                        .valid
                        .contains(&(deployment.name.to_string(), action_artifact.name.clone())),
                    malformed_action_matrix_covered: action_builder_matrix
                        .malformed
                        .contains(&(deployment.name.to_string(), action_artifact.name.clone())),
                }
            })
        })
        .collect::<Vec<_>>();
    let required_action_specific_builder_count = action_builder_coverage.len();
    let scoped_action_artifact_count =
        action_builder_coverage.iter().filter(|coverage| coverage.scoped_action_artifact_covered).count();
    let scheduler_witness_shape_count =
        action_builder_coverage.iter().filter(|coverage| coverage.scheduler_witness_shape_covered).count();
    let scheduler_witness_shape_malformed_count =
        action_builder_coverage.iter().filter(|coverage| coverage.scheduler_witness_shape_malformed_covered).count();
    let valid_action_specific_builder_count =
        action_builder_coverage.iter().filter(|coverage| coverage.valid_action_specific_builder_covered).count();
    let malformed_action_matrix_count =
        action_builder_coverage.iter().filter(|coverage| coverage.malformed_action_matrix_covered).count();
    let standard_relay_incompatible_examples = deployments
        .iter()
        .filter(|deployment| {
            deployment.requires_relaxed_mass_policy || deployment.standard_deployment_storage_mass > SPORA_STANDARD_RELAY_MAX_TX_MASS
        })
        .map(|deployment| StandardRelayIncompatibleExampleReport {
            name: deployment.name,
            artifact_size_bytes: deployment.artifact_size_bytes,
            estimated_standard_deployment_storage_mass: deployment.standard_deployment_storage_mass,
            requires_relaxed_mass_policy: deployment.requires_relaxed_mass_policy,
        })
        .collect::<Vec<_>>();
    let standard_relay_deploy_compatible_example_count = deployments.len().saturating_sub(standard_relay_incompatible_examples.len());
    let standard_relay_deploy_compatible_action_count =
        action_builder_coverage.iter().filter(|coverage| coverage.builder_requirements.fits_standard_relay_transaction_mass).count();
    let bundled_example_count = deployments.len();
    let full_file_monolith_standard_relay_ready = standard_relay_deploy_compatible_example_count == bundled_example_count;
    let scoped_action_standard_relay_ready = standard_relay_deploy_compatible_action_count == required_action_specific_builder_count;
    let production_ready = standard_mass_policy_used
        && required_action_specific_builder_count > 0
        && valid_action_specific_builder_count == required_action_specific_builder_count
        && malformed_action_matrix_count == required_action_specific_builder_count;
    let mut blockers = Vec::new();
    let mut advisories = Vec::new();
    if !standard_mass_policy_used {
        blockers.push(format!(
            "production Spora gate requires standard mass policy and scoped action standard relay compatibility, but this run used relaxed mass policy: {standard_relay_deploy_compatible_action_count}/{required_action_specific_builder_count} scoped actions"
        ));
    }
    if !standard_relay_incompatible_examples.is_empty() {
        let incompatible_examples =
            standard_relay_incompatible_examples.iter().map(|example| example.name).collect::<Vec<_>>().join(", ");
        advisories.push(format!(
            "full-file monolith bundled examples remain above standard relay deployment mass: {standard_relay_deploy_compatible_example_count}/{bundled_example_count} compatible, incompatible examples: [{incompatible_examples}]"
        ));
    }
    if scoped_action_artifact_count != required_action_specific_builder_count {
        blockers.push(format!(
            "missing scoped Spora action artifacts: {scoped_action_artifact_count}/{required_action_specific_builder_count}"
        ));
    }
    if valid_action_specific_builder_count != required_action_specific_builder_count {
        blockers.push(format!(
            "valid action-specific Spora transaction builder coverage incomplete: {valid_action_specific_builder_count}/{required_action_specific_builder_count}"
        ));
    }
    if scheduler_witness_shape_count != required_action_specific_builder_count {
        blockers.push(format!(
            "missing scheduler witness transaction-shape preflight coverage: {scheduler_witness_shape_count}/{required_action_specific_builder_count}"
        ));
    }
    if scheduler_witness_shape_malformed_count != required_action_specific_builder_count {
        blockers.push(format!(
            "missing malformed scheduler witness transaction-shape rejection coverage: {scheduler_witness_shape_malformed_count}/{required_action_specific_builder_count}"
        ));
    }
    if malformed_action_matrix_count != required_action_specific_builder_count {
        blockers.push(format!(
            "malformed action-specific Spora rejection matrix coverage incomplete: {malformed_action_matrix_count}/{required_action_specific_builder_count}"
        ));
    }

    SporaProductionGateReport {
        status: if production_ready { "passed" } else { "blocked" },
        production_ready,
        requires_standard_mass_policy: true,
        standard_mass_policy_used,
        standard_block_max_mass: DEVNET_PARAMS.max_block_mass,
        standard_relay_max_tx_mass: SPORA_STANDARD_RELAY_MAX_TX_MASS,
        relaxed_block_max_mass: DEVNET_ACCEPTANCE_BLOCK_MAX_MASS,
        bundled_example_count,
        standard_relay_deploy_compatible_example_count,
        full_file_monolith_standard_relay_ready,
        standard_relay_deploy_compatible_action_count,
        scoped_action_standard_relay_ready,
        requires_action_specific_builders: true,
        scoped_action_artifact_count,
        scheduler_witness_shape_count,
        scheduler_witness_shape_malformed_count,
        valid_action_specific_builder_count,
        required_action_specific_builder_count,
        malformed_action_matrix_count,
        bundled_example_deployment_probe_count: deployments.len(),
        standard_relay_incompatible_examples,
        blockers,
        advisories,
        action_builder_coverage,
    }
}

struct SchedulerWitnessShapeCoverage {
    valid_shape_covered: bool,
    malformed_shape_covered: bool,
    witness_bytes: usize,
    access_count: usize,
    required_shape: SchedulerWitnessShape,
    error: Option<String>,
}

fn verify_action_scheduler_witness_shape(action: &cellscript::ActionMetadata) -> SchedulerWitnessShapeCoverage {
    let witness_bytes = match action.scheduler_witness_bytes() {
        Ok(bytes) => bytes,
        Err(error) => {
            return SchedulerWitnessShapeCoverage {
                valid_shape_covered: false,
                malformed_shape_covered: false,
                witness_bytes: 0,
                access_count: 0,
                required_shape: SchedulerWitnessShape::default(),
                error: Some(format!("missing or invalid compiled scheduler witness: {error}")),
            };
        }
    };
    let witness = match decode_cellscript_scheduler_witness(&witness_bytes) {
        Ok(witness) => witness,
        Err(error) => {
            return SchedulerWitnessShapeCoverage {
                valid_shape_covered: false,
                malformed_shape_covered: false,
                witness_bytes: witness_bytes.len(),
                access_count: 0,
                required_shape: SchedulerWitnessShape::default(),
                error: Some(format!("compiled scheduler witness did not decode through Spora exec: {error}")),
            };
        }
    };

    let witness_len = witness_bytes.len();
    let access_count = witness.accesses.len();
    let shape = scheduler_witness_required_shape(&witness);
    let mut valid_tx = scheduler_shape_tx(shape);
    let valid_shape_covered = valid_tx.push_cellscript_compiled_scheduler_witness(witness_bytes.clone()).is_ok();
    if !valid_shape_covered {
        return SchedulerWitnessShapeCoverage {
            valid_shape_covered: false,
            malformed_shape_covered: false,
            witness_bytes: witness_len,
            access_count,
            required_shape: shape,
            error: Some("compiled scheduler witness did not admit against its minimum transaction shape".to_string()),
        };
    }

    let Some(malformed_shape) = malformed_scheduler_shape(shape) else {
        return SchedulerWitnessShapeCoverage {
            valid_shape_covered: true,
            malformed_shape_covered: true,
            witness_bytes: witness_len,
            access_count,
            required_shape: shape,
            error: None,
        };
    };
    let mut malformed_tx = scheduler_shape_tx(malformed_shape);
    let malformed_shape_covered = malformed_tx.push_cellscript_compiled_scheduler_witness(witness_bytes).is_err();

    SchedulerWitnessShapeCoverage {
        valid_shape_covered,
        malformed_shape_covered,
        witness_bytes: witness_len,
        access_count,
        required_shape: shape,
        error: (!malformed_shape_covered)
            .then(|| "compiled scheduler witness admitted against a transaction shape with a missing source slot".to_string()),
    }
}

fn with_compiled_action_scheduler_witness(mut tx: CellTx, action: &cellscript::ActionMetadata, context: &str) -> CellTx {
    let witness_bytes = action
        .scheduler_witness_bytes()
        .unwrap_or_else(|error| panic!("{context} must have compiled scheduler witness metadata: {error}"));
    let admitted = tx
        .push_cellscript_compiled_scheduler_witness(witness_bytes)
        .unwrap_or_else(|error| panic!("{context} scheduler witness must match the concrete transaction shape: {error}"));
    assert_eq!(
        admitted.estimated_cycles, action.estimated_cycles,
        "{context} scheduler witness cycle estimate must match action metadata"
    );
    tx
}

fn action_builder_requirements(
    action: &cellscript::ActionMetadata,
    scheduler_shape: &SchedulerWitnessShapeCoverage,
    artifact_size_bytes: usize,
) -> SporaActionBuilderRequirementsReport {
    let schema_backed_param_count = action.params.iter().filter(|param| param.schema_pointer_abi || param.schema_length_abi).count();
    let fixed_byte_param_count =
        action.params.iter().filter(|param| param.fixed_byte_pointer_abi || param.fixed_byte_length_abi).count();
    let checked_runtime_obligation_count =
        action.verifier_obligations.iter().filter(|obligation| obligation.status == "checked-runtime").count();

    SporaActionBuilderRequirementsReport {
        entry_param_count: action.params.len(),
        schema_backed_param_count,
        fixed_byte_param_count,
        scheduler_witness_bytes: scheduler_shape.witness_bytes,
        scheduler_access_count: scheduler_shape.access_count,
        min_input_count: scheduler_shape.required_shape.inputs,
        min_cell_dep_count: scheduler_shape.required_shape.cell_deps,
        min_output_count: scheduler_shape.required_shape.outputs,
        consume_count: action.consume_set.len(),
        read_ref_count: action.read_refs.len(),
        create_count: action.create_set.len(),
        mutate_count: action.mutate_set.len(),
        transaction_runtime_input_requirement_count: action.transaction_runtime_input_requirements.len(),
        checked_runtime_obligation_count,
        fail_closed_runtime_feature_count: action.fail_closed_runtime_features.len(),
        estimated_cycles: action.estimated_cycles,
        estimated_standard_deployment_storage_mass: standard_deployment_storage_mass(artifact_size_bytes),
        fits_standard_relay_transaction_mass: standard_deployment_storage_mass(artifact_size_bytes)
            <= SPORA_STANDARD_RELAY_MAX_TX_MASS,
        requires_action_specific_transaction_builder: true,
    }
}

fn standard_deployment_storage_mass(artifact_size_bytes: usize) -> u64 {
    u64::try_from(artifact_size_bytes).expect("artifact size must fit u64").saturating_mul(2)
}

#[derive(Clone, Copy, Default)]
struct SchedulerWitnessShape {
    inputs: usize,
    cell_deps: usize,
    outputs: usize,
}

fn scheduler_witness_required_shape(witness: &CellScriptSchedulerWitness) -> SchedulerWitnessShape {
    let mut shape = SchedulerWitnessShape { inputs: 0, cell_deps: 0, outputs: 0 };
    for access in &witness.accesses {
        let required = usize::try_from(access.index).unwrap_or(usize::MAX).saturating_add(1);
        match access.source {
            CELLSCRIPT_SCHEDULER_SOURCE_INPUT => shape.inputs = shape.inputs.max(required),
            CELLSCRIPT_SCHEDULER_SOURCE_CELL_DEP => shape.cell_deps = shape.cell_deps.max(required),
            CELLSCRIPT_SCHEDULER_SOURCE_OUTPUT => shape.outputs = shape.outputs.max(required),
            _ => {}
        }
    }
    shape
}

fn malformed_scheduler_shape(shape: SchedulerWitnessShape) -> Option<SchedulerWitnessShape> {
    if shape.inputs > 0 {
        return Some(SchedulerWitnessShape { inputs: shape.inputs - 1, ..shape });
    }
    if shape.cell_deps > 0 {
        return Some(SchedulerWitnessShape { cell_deps: shape.cell_deps - 1, ..shape });
    }
    if shape.outputs > 0 {
        return Some(SchedulerWitnessShape { outputs: shape.outputs - 1, ..shape });
    }
    None
}

fn scheduler_shape_tx(shape: SchedulerWitnessShape) -> CellTx {
    let inputs = (0..shape.inputs)
        .map(|index| CellInput::new(OutPoint::new([1; 32], u32::try_from(index).expect("test input index fits u32")), 0))
        .collect::<Vec<_>>();
    let cell_deps = (0..shape.cell_deps)
        .map(|index| CellDep {
            out_point: OutPoint::new([2; 32], u32::try_from(index).expect("test cell dep index fits u32")),
            dep_type: DepType::Code,
        })
        .collect::<Vec<_>>();
    let outputs = (0..shape.outputs)
        .map(|_| CellOutput { capacity: 1_000_000, lock: Script::new(always_success_code_hash(), 0, vec![]), type_: None })
        .collect::<Vec<_>>();
    let outputs_data = vec![vec![]; shape.outputs];
    CellTx::new(inputs, cell_deps, outputs, outputs_data, vec![]).expect("scheduler shape transaction must be structurally valid")
}

fn write_base_report(report: &BaseReport) {
    let Ok(path) = std::env::var(BASE_REPORT_ENV) else {
        return;
    };
    let bytes = serde_json::to_vec_pretty(report).expect("base report must serialize");
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).expect("base report directory must be created");
    }
    std::fs::write(path, [bytes, b"\n".to_vec()].concat()).expect("base report must be written");
}
