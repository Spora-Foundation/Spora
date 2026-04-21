#![cfg(test)]
#![cfg(all(feature = "integration-tests", feature = "devnet-prealloc"))]

use crate::common::{
    args::ArgsBuilder,
    cellscript_contracts::{
        compile_all_spora_example_contracts, compile_fixed_output_spora_contract, compile_noop_spora_lock_contract,
        compile_parameterized_amount_spora_contract,
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
use spora_consensus_core::network::{NetworkId, NetworkType};
use spora_consensus_core::tx::{CellDep, CellInput, CellOutput, CellTx, DepType, OutPoint, Script, TransactionOutpoint};
use spora_exec::scripts::{always_success_code_hash, ALWAYS_SUCCESS_SCRIPT};
use spora_rpc_core::api::rpc::RpcApi;

const DEVNET_ACCEPTANCE_BLOCK_MAX_MASS: u64 = 100_000_000;
const SMOKE_REPORT_ENV: &str = "DEVNET_ACCEPTANCE_SMOKE_REPORT_JSON";

/// Real devnet acceptance smoke:
/// bootstrap wallet -> prealloc cells -> mine confirmations -> signed transfer -> mempool -> template -> block -> cellindex
/// -> deploy script code cell -> spend VM-locked cell with cell dep -> deploy/spend a compiled CellScript ELF contract
/// through a real code cell dependency -> deploy every bundled CellScript example under the explicit
/// non-standard relay profile, whose CLI opt-in applies to every network.
///
/// `cargo test -p spora-testing-integration --lib --features "integration-tests devnet-prealloc vm" -- devnet_acceptance_tests::devnet_acceptance_smoke --exact --nocapture --test-threads=1`
#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn devnet_acceptance_smoke() {
    init_allocator_with_default_settings();
    spora_core::log::try_init_logger("INFO");

    let prealloc_cells = 20;
    let prealloc_amount_spora = 500;
    let bootstrap = generate_devnet_bootstrap(
        NetworkType::Devnet,
        "acceptance-smoke",
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

    let args = ArgsBuilder::devnet(prealloc_cells, prealloc_amount_spora)
        .prealloc_address(prealloc_address.clone())
        .cellindex(true)
        .relay_non_standard(true)
        .block_max_mass(DEVNET_ACCEPTANCE_BLOCK_MAX_MASS)
        .resumable_virtual_state_step_cycles(10_000)
        .build();

    let mut sporad = Daemon::new_random_with_args(args, 10);
    let rpc_client = sporad.start().await;

    let info = rpc_client.get_info().await.expect("get_info must succeed");
    assert!(info.is_cell_indexed, "devnet acceptance requires cellindex");
    assert_eq!(info.mempool_size, 0);

    let server_info = rpc_client.get_server_info().await.expect("get_server_info must succeed");
    assert!(server_info.has_cell_index, "server must advertise cellindex");
    assert_eq!(server_info.network_id, NetworkId::new(NetworkType::Devnet));
    let mut smoke_report = SmokeReport {
        profile: "smoke",
        network_id: "devnet",
        relaxed_mass_policy: RelaxedMassPolicyReport {
            relay_non_standard: true,
            block_max_mass: DEVNET_ACCEPTANCE_BLOCK_MAX_MASS,
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
    smoke_report.signed_transfer_confirmed = true;

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
    smoke_report.multi_input_multi_output_confirmed = true;

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
        deploy_input,
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
        parent_input,
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
    smoke_report.parent_child_mempool_confirmed = true;

    let remaining_prealloc = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity()).await;
    assert!(remaining_prealloc.len() >= 4, "CellScript deployment acceptance needs four remaining spendable prealloc cells");

    let cellscript_contract = compile_noop_spora_lock_contract();
    let cellscript_code_deploy_input = &remaining_prealloc[..1];
    let cellscript_code_deploy_input_capacity = cellscript_code_deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
    let cellscript_code_cell_capacity = cellscript_code_deploy_input_capacity
        .checked_sub(required_fee(cellscript_code_deploy_input.len(), 1).saturating_add(100_000))
        .expect("CellScript code deployment transaction must leave capacity for the code cell");
    let cellscript_lock = Script::new(cellscript_contract.code_hash, 0, vec![]);
    let cellscript_code_deploy_tx = generate_signed_cell_tx(
        prealloc_schnorr_key,
        cellscript_code_deploy_input,
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
        cellscript_lock_create_input,
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
        fixed_output_deploy_input,
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
        parameterized_amount_deploy_input,
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
    let parameterized_amount_deploy_tx_id = spora_hashes::Hash::from_bytes(parameterized_amount_deploy_tx.id());
    let parameterized_amount_code_outpoint = TransactionOutpoint::new(parameterized_amount_deploy_tx.id(), 0);
    let parameterized_amount_locked_outpoint = TransactionOutpoint::new(parameterized_amount_deploy_tx.id(), 1);

    rpc_client.submit_transaction((&cellscript_code_deploy_tx).into(), false).await.unwrap();
    rpc_client.submit_transaction((&cellscript_lock_create_tx).into(), false).await.unwrap();
    rpc_client.submit_transaction((&fixed_output_deploy_tx).into(), false).await.unwrap();
    rpc_client.submit_transaction((&parameterized_amount_deploy_tx).into(), false).await.unwrap();
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
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == parameterized_amount_deploy_tx_id),
        "expected block template to include the CellScript parameterized amount deployment transaction"
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
                        .filter(|cell| cell.outpoint.transaction_id == parameterized_amount_deploy_tx_id)
                        .any(|cell| cell.outpoint.index == 0)
                }
            }
        },
        "CellScript parameterized amount script code cell was not indexed after deployment block acceptance",
    )
    .await;

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
        let example_locked_capacity = example_input_capacity
            .checked_sub(example_code_cell_capacity)
            .and_then(|value| value.checked_sub(required_fee(example_deploy_input.len(), 2).saturating_add(100_000)))
            .unwrap_or_else(|| panic!("{} deployment transaction must leave capacity for its locked probe cell", example.name));
        let example_lock = Script::new(example.code_hash, 0, vec![]);
        let example_deploy_tx = generate_signed_cell_tx(
            prealloc_schnorr_key,
            example_deploy_input,
            vec![],
            vec![
                CellOutput { capacity: example_code_cell_capacity, lock: pay_to_acceptance_owner(&prealloc_address), type_: None },
                CellOutput { capacity: example_locked_capacity, lock: example_lock, type_: None },
            ],
            vec![example.artifact_bytes.clone(), vec![]],
        );
        let example_deploy_tx_id = spora_hashes::Hash::from_bytes(example_deploy_tx.id());
        let example_code_outpoint = TransactionOutpoint::new(example_deploy_tx.id(), 0);
        let example_locked_outpoint = TransactionOutpoint::new(example_deploy_tx.id(), 1);

        rpc_client
            .submit_transaction((&example_deploy_tx).into(), false)
            .await
            .unwrap_or_else(|err| panic!("{} deployment must pass explicit relaxed relay policy: {err}", example.name));
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

        example_deployments.push(CellScriptExampleDeployment {
            name: example.name,
            artifact_size_bytes: example.artifact_bytes.len(),
            deployment_tx_id: example_deploy_tx_id,
            code_outpoint: example_code_outpoint,
            locked_outpoint: example_locked_outpoint,
            locked_capacity: example_locked_capacity,
            ckb_runtime_required: example.ckb_runtime_required,
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

    for (index, deployment) in example_deployments.iter().enumerate() {
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
        smoke_report.bundled_examples.push(BundledExampleReport {
            name: deployment.name,
            artifact_size_bytes: deployment.artifact_size_bytes,
            ckb_runtime_required: deployment.ckb_runtime_required,
            deployment_tx_id: hash_hex(&deployment.deployment_tx_id.as_bytes()),
            code_cell_outpoint: outpoint_report(&deployment.code_outpoint),
            locked_probe_outpoint: outpoint_report(&deployment.locked_outpoint),
            code_cell_indexed: true,
            malformed_spend_rejected: true,
            malformed_spend_reject_reason: reject_reason,
            malformed_spend_rejected_by_standard_policy: false,
        });
    }

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
    smoke_report.scheduler_tamper_rejected = true;

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
        vec![parameterized_amount.to_le_bytes().to_vec()],
        vec![parameterized_amount_contract.entry_witness_for_amount(parameterized_amount)],
    )
    .expect("valid CellScript parameterized amount spend transaction with code dep and witness args");
    let parameterized_amount_spend_tx_id = spora_hashes::Hash::from_bytes(parameterized_amount_spend_tx.id());
    let parameterized_amount_data_hash = spora_cell_data_hash(&parameterized_amount.to_le_bytes());
    rpc_client
        .submit_transaction((&parameterized_amount_spend_tx).into(), false)
        .await
        .expect("CellScript parameterized amount spend should verify witness-bound output cell data");

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
    assert!(
        template
            .block
            .transactions
            .iter()
            .skip(1)
            .filter_map(|rpc_tx| CellTx::try_from(rpc_tx.clone()).ok())
            .any(|tx| spora_hashes::Hash::from_bytes(tx.id()) == parameterized_amount_spend_tx_id),
        "expected block template to include the CellScript parameterized amount spend transaction"
    );
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
    smoke_report.always_success_vm_spend_confirmed = true;

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
    smoke_report.noop_cellscript_spend_confirmed = true;

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
    smoke_report.cellscript_schema_output_spend_confirmed = true;

    wait_for(
        50,
        40,
        {
            let client = rpc_client.clone();
            let address = parameterized_amount_recipient.clone();
            let data_hash = parameterized_amount_data_hash;
            move || {
                let client = client.clone();
                let address = address.clone();
                let data_hash = data_hash;
                async move {
                    client.get_cells_by_addresses(vec![address]).await.unwrap().iter().any(|cell| {
                        cell.outpoint.transaction_id == parameterized_amount_spend_tx_id
                            && cell.cell_entry.data_bytes == 8
                            && cell.cell_entry.data_hash == data_hash
                    })
                }
            }
        },
        "recipient cell from the CellScript parameterized amount spend was not indexed with witness-bound schema output data",
    )
    .await;
    smoke_report.cellscript_parameterized_amount_spend_confirmed = true;
    write_smoke_report(&smoke_report);
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

struct CellScriptExampleDeployment {
    name: &'static str,
    artifact_size_bytes: usize,
    deployment_tx_id: spora_hashes::Hash,
    code_outpoint: TransactionOutpoint,
    locked_outpoint: TransactionOutpoint,
    locked_capacity: u64,
    ckb_runtime_required: bool,
}

#[derive(Serialize)]
struct SmokeReport {
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
}

#[derive(Serialize)]
struct RelaxedMassPolicyReport {
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
    deployment_tx_id: String,
    code_cell_outpoint: OutPointReport,
    locked_probe_outpoint: OutPointReport,
    code_cell_indexed: bool,
    malformed_spend_rejected: bool,
    malformed_spend_reject_reason: String,
    malformed_spend_rejected_by_standard_policy: bool,
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

fn write_smoke_report(report: &SmokeReport) {
    let Ok(path) = std::env::var(SMOKE_REPORT_ENV) else {
        return;
    };
    let bytes = serde_json::to_vec_pretty(report).expect("smoke report must serialize");
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).expect("smoke report directory must be created");
    }
    std::fs::write(path, [bytes, b"\n".to_vec()].concat()).expect("smoke report must be written");
}
