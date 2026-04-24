use serde::Serialize;
use spora_addresses::Address;
use spora_alloc::init_allocator_with_default_settings;
use spora_bip32::WordCount;
use spora_consensus::params::DEVNET_PARAMS;
use spora_consensus_core::network::NetworkType;
use spora_consensus_core::tx::{CellOutput, Script, TransactionOutpoint};
use spora_hashes::Hash;
use spora_rpc_core::api::rpc::RpcApi;
use spora_testing_integration::common::{
    args::ArgsBuilder,
    cellscript_contracts::compile_all_spora_full_example_contracts,
    daemon::Daemon,
    devnet_bootstrap::{generate_devnet_bootstrap, DEFAULT_PREALLOC_AMOUNT_SAU},
    utils::{fetch_spendable_cells, generate_signed_cell_tx, required_fee, wait_for},
};
use std::{collections::VecDeque, fs, path::PathBuf};

const SPORA_STANDARD_RELAY_MAX_TX_MASS: u64 = 500_000;

#[derive(Serialize)]
struct ProbeReport {
    prealloc_cells: u64,
    prealloc_amount_sau: u64,
    standard_relay_max_tx_mass: u64,
    examples: Vec<ExampleProbeReport>,
}

#[derive(Serialize)]
struct ExampleProbeReport {
    name: &'static str,
    artifact_size_bytes: usize,
    estimated_standard_deployment_storage_mass: u64,
    fits_standard_relay_transaction_mass: bool,
    requires_relaxed_mass_policy: bool,
    deployment_accepted: bool,
    deployment_error: Option<String>,
    code_cell_indexed: bool,
    deployment_tx_id: Option<String>,
}

fn pay_to_acceptance_owner(address: &Address) -> Script {
    Script::new(always_success_code_hash(), 0, pay_to_acceptance_owner_args(address))
}

fn pay_to_acceptance_owner_args(address: &Address) -> Vec<u8> {
    address.payload.to_vec()
}

fn hash_hex(bytes: &[u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn standard_deployment_storage_mass(artifact_size_bytes: usize) -> u64 {
    u64::try_from(artifact_size_bytes).expect("artifact size must fit u64").saturating_mul(2)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 1)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_allocator_with_default_settings();
    let _ = spora_core::log::try_init_logger("INFO");

    let prealloc_cells = 64;
    let prealloc_amount_spora = 10_000;
    let bootstrap = generate_devnet_bootstrap(
        NetworkType::Devnet,
        "standard-relay-probe",
        WordCount::Words12,
        prealloc_cells,
        prealloc_amount_spora * spora_consensus_core::constants::SAU_PER_SPORA,
    )?;
    assert!(bootstrap.manifest.node.prealloc_amount_sau >= DEFAULT_PREALLOC_AMOUNT_SAU);

    let prealloc_address = bootstrap.address.clone();
    let prealloc_schnorr_key = bootstrap.schnorr_keypair();
    let miner_address = Address::new_std_single(NetworkType::Devnet.into(), &[9; 32])?;

    let args = ArgsBuilder::devnet(prealloc_cells, prealloc_amount_spora)
        .prealloc_address(prealloc_address.clone())
        .cellindex(true)
        .resumable_virtual_state_step_cycles(10_000)
        .build();

    let mut sporad = Daemon::new_random_with_args(args, 10);
    let rpc_client = sporad.start().await;

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

    let mut spendable_cells = fetch_spendable_cells(&rpc_client, prealloc_address.clone(), DEVNET_PARAMS.coinbase_maturity())
        .await
        .into_iter()
        .collect::<VecDeque<_>>();
    let examples = compile_all_spora_full_example_contracts();
    let mut reports = Vec::with_capacity(examples.len());

    for example in examples {
        let artifact_size_bytes = example.artifact_bytes.len();
        let estimated_standard_deployment_storage_mass = standard_deployment_storage_mass(artifact_size_bytes);
        let fits_standard_relay_transaction_mass = estimated_standard_deployment_storage_mass <= SPORA_STANDARD_RELAY_MAX_TX_MASS;
        let deploy_input =
            spendable_cells.pop_front().ok_or_else(|| format!("ran out of matured prealloc cells while probing {}", example.name))?;
        let deploy_input = vec![deploy_input];
        let deploy_input_capacity = deploy_input.iter().map(|(_, meta)| meta.capacity()).sum::<u64>();
        let code_cell_capacity = deploy_input_capacity
            .checked_sub(required_fee(deploy_input.len(), 1).saturating_add(100_000))
            .ok_or_else(|| format!("{} deployment must leave capacity for code cell", example.name))?;
        let deploy_tx = generate_signed_cell_tx(
            prealloc_schnorr_key,
            &deploy_input,
            vec![],
            vec![CellOutput { capacity: code_cell_capacity, lock: pay_to_acceptance_owner(&prealloc_address), type_: None }],
            vec![example.artifact_bytes.clone()],
        );
        let deploy_tx_id = Hash::from_bytes(deploy_tx.id());
        let code_outpoint = TransactionOutpoint::new(deploy_tx.id(), 0);

        let mut deployment_accepted = false;
        let mut deployment_error = None;
        let mut code_cell_indexed = false;
        match rpc_client.submit_transaction((&deploy_tx).into(), false).await {
            Ok(_) => {
                let template = rpc_client.get_block_template(miner_address.clone(), vec![]).await.unwrap();
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
                    "bundled example code cell was not indexed after deployment block acceptance",
                )
                .await;
                deployment_accepted = true;
                code_cell_indexed = true;
            }
            Err(err) => {
                deployment_error = Some(err.to_string());
            }
        }

        reports.push(ExampleProbeReport {
            name: example.name,
            artifact_size_bytes,
            estimated_standard_deployment_storage_mass,
            fits_standard_relay_transaction_mass,
            requires_relaxed_mass_policy: example.requires_relaxed_mass_policy,
            deployment_accepted,
            deployment_error,
            code_cell_indexed,
            deployment_tx_id: deployment_accepted.then(|| hash_hex(&deploy_tx_id.as_bytes())),
        });

        let _ = code_outpoint;
    }

    let report = ProbeReport {
        prealloc_cells,
        prealloc_amount_sau: bootstrap.manifest.node.prealloc_amount_sau,
        standard_relay_max_tx_mass: SPORA_STANDARD_RELAY_MAX_TX_MASS,
        examples: reports,
    };

    let json = serde_json::to_string_pretty(&report)?;
    if let Ok(path) = std::env::var("SPORA_STANDARD_RELAY_PROBE_JSON") {
        let path = PathBuf::from(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &json)?;
    }
    println!("{json}");
    Ok(())
}

use spora_exec::scripts::always_success_code_hash;
