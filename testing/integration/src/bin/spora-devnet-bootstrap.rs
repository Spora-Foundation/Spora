use clap::Parser;
use spora_bip32::WordCount;
use spora_consensus_core::network::NetworkType;
use spora_testing_integration::common::devnet_bootstrap::{
    generate_devnet_bootstrap, DEFAULT_PREALLOC_AMOUNT_SAU, DEFAULT_PREALLOC_CELLS, DEFAULT_WALLET_NAME,
};
use std::{error::Error, fs, io, path::PathBuf, str::FromStr};

#[derive(Debug, Parser)]
#[command(name = "spora-devnet-bootstrap")]
#[command(about = "Generate a non-interactive devnet/simnet acceptance wallet manifest")]
struct Args {
    #[arg(long, default_value = "devnet")]
    network: String,

    #[arg(long, default_value = DEFAULT_WALLET_NAME)]
    wallet_name: String,

    #[arg(long)]
    wallet_dir: Option<PathBuf>,

    #[arg(long)]
    out: PathBuf,

    #[arg(long, default_value_t = 12)]
    word_count: usize,

    #[arg(long, default_value_t = DEFAULT_PREALLOC_CELLS)]
    prealloc_cells: u64,

    #[arg(long, default_value_t = DEFAULT_PREALLOC_AMOUNT_SAU)]
    prealloc_amount_sau: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let network_type = NetworkType::from_str(&args.network)?;
    if matches!(network_type, NetworkType::Mainnet | NetworkType::Testnet) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "spora-devnet-bootstrap only supports devnet and simnet acceptance wallets",
        )
        .into());
    }

    let word_count = WordCount::try_from(args.word_count)?;
    let bootstrap =
        generate_devnet_bootstrap(network_type, args.wallet_name, word_count, args.prealloc_cells, args.prealloc_amount_sau)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err))?;
    let manifest_json = serde_json::to_string_pretty(&bootstrap.manifest)?;

    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.out, &manifest_json)?;

    if let Some(wallet_dir) = args.wallet_dir {
        fs::create_dir_all(&wallet_dir)?;
        fs::write(wallet_dir.join("wallet.json"), &manifest_json)?;
    }

    println!("{manifest_json}");
    Ok(())
}
