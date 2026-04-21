use clap::Parser;
use serde::Serialize;
use spora_addresses::Address;
use spora_consensus_core::network::{NetworkId, NetworkType};
use spora_grpc_client::GrpcClient;
use spora_rpc_core::{api::rpc::RpcApi, RpcCellsByAddressesEntry};
use spora_wrpc_client::{
    client::{ConnectOptions, ConnectStrategy},
    SporaRpcClient, WrpcEncoding,
};
use std::{
    error::Error,
    fs, io,
    path::PathBuf,
    str::FromStr,
    time::{Duration, Instant},
};

#[derive(Debug, Parser)]
#[command(name = "spora-devnet-probe")]
#[command(about = "Probe a booted devnet node through gRPC and wRPC acceptance endpoints")]
struct Args {
    #[arg(long, default_value = "grpc://127.0.0.1:16610")]
    grpc: String,

    #[arg(long, default_value = "ws://127.0.0.1:17610")]
    wrpc_borsh: String,

    #[arg(long, default_value = "ws://127.0.0.1:18610")]
    wrpc_json: String,

    #[arg(long)]
    address: String,

    #[arg(long, default_value_t = 101)]
    expected_prealloc_cells: usize,

    #[arg(long, default_value_t = 10_000_000_000)]
    expected_prealloc_amount_sau: u64,

    #[arg(long, default_value_t = 30_000)]
    timeout_ms: u64,

    #[arg(long)]
    out: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct ProbeReport {
    grpc: GrpcProbeReport,
    wrpc_borsh: WrpcProbeReport,
    wrpc_json: WrpcProbeReport,
    prealloc: PreallocReport,
    mined_block: MinedBlockReport,
}

#[derive(Debug, Serialize)]
struct GrpcProbeReport {
    url: String,
    network_id: String,
    has_cell_index: bool,
    is_cell_indexed: bool,
    mempool_size: u64,
    virtual_daa_score: u64,
}

#[derive(Debug, Serialize)]
struct WrpcProbeReport {
    url: String,
    encoding: String,
    network_id: String,
    has_cell_index: bool,
    virtual_daa_score: u64,
    block_count: u64,
}

#[derive(Debug, Serialize)]
struct PreallocReport {
    address: String,
    cells: usize,
    expected_cells: usize,
    total_capacity_sau: u64,
    expected_total_capacity_sau: u64,
}

#[derive(Debug, Serialize)]
struct MinedBlockReport {
    virtual_daa_score_before: u64,
    virtual_daa_score_after: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let address = Address::from_str(&args.address)?;
    let timeout = Duration::from_millis(args.timeout_ms);
    let expected_network = NetworkId::new(NetworkType::Devnet);

    let grpc_client = connect_grpc_with_retry(&args.grpc, timeout).await?;
    let info = grpc_client.get_info().await?;
    let server_info = grpc_client.get_server_info().await?;
    if server_info.network_id != expected_network {
        return Err(invalid_data(format!("gRPC network mismatch: expected {expected_network}, got {}", server_info.network_id)));
    }
    if !server_info.has_cell_index || !info.is_cell_indexed {
        return Err(invalid_data("devnet probe requires cellindex to be enabled"));
    }

    let prealloc_cells = wait_for_prealloc_cells(&grpc_client, address.clone(), args.expected_prealloc_cells, timeout).await?;
    let total_capacity_sau = prealloc_cells.iter().map(|cell| cell.cell_entry.capacity).sum::<u64>();
    let expected_total_capacity_sau = args.expected_prealloc_amount_sau.saturating_mul(args.expected_prealloc_cells as u64);
    if total_capacity_sau != expected_total_capacity_sau {
        return Err(invalid_data(format!(
            "prealloc capacity mismatch: expected {expected_total_capacity_sau}, got {total_capacity_sau}"
        )));
    }

    let before_daa = server_info.virtual_daa_score;
    let template = grpc_client.get_block_template(address.clone(), vec![]).await?;
    grpc_client.submit_block(template.block, false).await?;
    let after_daa = wait_for_daa_advance(&grpc_client, before_daa, timeout).await?;

    let wrpc_borsh = probe_wrpc_endpoint(&args.wrpc_borsh, WrpcEncoding::Borsh, "borsh", timeout, expected_network).await?;
    let wrpc_json = probe_wrpc_endpoint(&args.wrpc_json, WrpcEncoding::SerdeJson, "json", timeout, expected_network).await?;

    let report = ProbeReport {
        grpc: GrpcProbeReport {
            url: args.grpc,
            network_id: server_info.network_id.to_string(),
            has_cell_index: server_info.has_cell_index,
            is_cell_indexed: info.is_cell_indexed,
            mempool_size: info.mempool_size,
            virtual_daa_score: before_daa,
        },
        wrpc_borsh,
        wrpc_json,
        prealloc: PreallocReport {
            address: address.to_string(),
            cells: prealloc_cells.len(),
            expected_cells: args.expected_prealloc_cells,
            total_capacity_sau,
            expected_total_capacity_sau,
        },
        mined_block: MinedBlockReport { virtual_daa_score_before: before_daa, virtual_daa_score_after: after_daa },
    };
    let report_json = serde_json::to_string_pretty(&report)?;

    if let Some(path) = args.out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &report_json)?;
    }

    println!("{report_json}");
    Ok(())
}

async fn connect_grpc_with_retry(url: &str, timeout: Duration) -> Result<GrpcClient, Box<dyn Error>> {
    let deadline = Instant::now() + timeout;
    let mut last_error = None;

    loop {
        match GrpcClient::connect(url.to_string()).await {
            Ok(client) => match client.get_info().await {
                Ok(_) => return Ok(client),
                Err(error) => last_error = Some(error.to_string()),
            },
            Err(error) => last_error = Some(error.to_string()),
        }

        if Instant::now() >= deadline {
            let detail = last_error.unwrap_or_else(|| "no connection attempt completed".to_string());
            return Err(io::Error::new(io::ErrorKind::TimedOut, format!("gRPC probe timed out: {detail}")).into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_prealloc_cells(
    client: &GrpcClient,
    address: Address,
    expected_cells: usize,
    timeout: Duration,
) -> Result<Vec<RpcCellsByAddressesEntry>, Box<dyn Error>> {
    let deadline = Instant::now() + timeout;

    loop {
        let cells = client.get_cells_by_addresses(vec![address.clone()]).await?;
        if cells.len() == expected_cells {
            return Ok(cells);
        }

        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("prealloc cell probe timed out: expected {expected_cells}, got {}", cells.len()),
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn wait_for_daa_advance(client: &GrpcClient, before_daa: u64, timeout: Duration) -> Result<u64, Box<dyn Error>> {
    let deadline = Instant::now() + timeout;

    loop {
        let current = client.get_server_info().await?.virtual_daa_score;
        if current > before_daa {
            return Ok(current);
        }

        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("mined block did not advance virtual DAA: before={before_daa}, current={current}"),
            )
            .into());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn probe_wrpc_endpoint(
    url: &str,
    encoding: WrpcEncoding,
    label: &str,
    timeout: Duration,
    expected_network: NetworkId,
) -> Result<WrpcProbeReport, Box<dyn Error>> {
    let client = SporaRpcClient::new(encoding, Some(url), None, Some(expected_network), None)?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(timeout),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };

    client.connect(Some(options)).await?;
    let server_info = client.get_server_info().await?;
    let dag_info = client.get_block_dag_info().await?;
    client.disconnect().await?;

    if server_info.network_id != expected_network {
        return Err(invalid_data(format!(
            "wRPC {label} network mismatch: expected {expected_network}, got {}",
            server_info.network_id
        )));
    }
    if !server_info.has_cell_index {
        return Err(invalid_data(format!("wRPC {label} endpoint reports cellindex disabled")));
    }

    Ok(WrpcProbeReport {
        url: url.to_string(),
        encoding: label.to_string(),
        network_id: server_info.network_id.to_string(),
        has_cell_index: server_info.has_cell_index,
        virtual_daa_score: dag_info.virtual_daa_score,
        block_count: dag_info.block_count,
    })
}

fn invalid_data(message: impl Into<String>) -> Box<dyn Error> {
    io::Error::new(io::ErrorKind::InvalidData, message.into()).into()
}
