use secp256k1::{Keypair, SecretKey};
use serde::{Deserialize, Serialize};
use spora_addresses::{Address, Prefix as AddressPrefix};
use spora_bip32::{DerivationPath, ExtendedPrivateKey, Language, Mnemonic, Prefix as KeyPrefix, SecretKeyExt, WordCount};
use spora_consensus_core::network::{NetworkId, NetworkType};
use std::str::FromStr;

pub const DEFAULT_WALLET_NAME: &str = "acceptance";
pub const DEFAULT_PREALLOC_CELLS: u64 = 101;
pub const DEFAULT_PREALLOC_AMOUNT_SAU: u64 = 10_000_000_000;
pub const DEFAULT_DERIVATION_PATH: &str = "m/44'/7890'/0'/0/0";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevnetBootstrapManifest {
    pub schema_version: u32,
    pub warning: String,
    pub network: String,
    pub wallet: DevnetWalletManifest,
    pub node: DevnetNodeManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevnetWalletManifest {
    pub name: String,
    pub mnemonic: String,
    pub word_count: usize,
    pub derivation_path: String,
    pub default_address: String,
    pub public_key_hex: String,
    pub xonly_public_key_hex: String,
    pub secret_key_hex: String,
    pub root_xprv: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DevnetNodeManifest {
    pub prealloc_cells: u64,
    pub prealloc_amount_sau: u64,
}

#[derive(Debug, Clone)]
pub struct GeneratedDevnetBootstrap {
    pub manifest: DevnetBootstrapManifest,
    pub address: Address,
    pub secret_key: SecretKey,
}

impl GeneratedDevnetBootstrap {
    pub fn schnorr_keypair(&self) -> Keypair {
        Keypair::from_secret_key(secp256k1::SECP256K1, &self.secret_key)
    }
}

pub fn generate_devnet_bootstrap(
    network_type: NetworkType,
    wallet_name: impl Into<String>,
    word_count: WordCount,
    prealloc_cells: u64,
    prealloc_amount_sau: u64,
) -> Result<GeneratedDevnetBootstrap, String> {
    let mnemonic = Mnemonic::random(word_count, Language::English).map_err(|err| err.to_string())?;
    devnet_bootstrap_from_mnemonic(network_type, wallet_name, mnemonic, prealloc_cells, prealloc_amount_sau)
}

pub fn devnet_bootstrap_from_mnemonic(
    network_type: NetworkType,
    wallet_name: impl Into<String>,
    mnemonic: Mnemonic,
    prealloc_cells: u64,
    prealloc_amount_sau: u64,
) -> Result<GeneratedDevnetBootstrap, String> {
    if matches!(network_type, NetworkType::Mainnet) {
        return Err("devnet acceptance bootstrap must not generate mainnet wallets".to_string());
    }

    let network_id = NetworkId::try_from(network_type).map_err(|err| err.to_string())?;
    let address_prefix = AddressPrefix::from(network_type);
    let derivation_path = DerivationPath::from_str(DEFAULT_DERIVATION_PATH).map_err(|err| err.to_string())?;
    let root_xprv = ExtendedPrivateKey::<SecretKey>::new(mnemonic.to_seed("")).map_err(|err| err.to_string())?;
    let derived_xprv = root_xprv.clone().derive_path(&derivation_path).map_err(|err| err.to_string())?;
    let secret_key = *derived_xprv.private_key();
    let public_key = secret_key.get_public_key();
    let xonly_public_key = public_key.x_only_public_key().0.serialize();
    let address = Address::new_std_single(address_prefix, &xonly_public_key).map_err(|err| err.to_string())?;
    let key_prefix = match network_type {
        NetworkType::Mainnet => KeyPrefix::KPRV,
        NetworkType::Testnet | NetworkType::Devnet | NetworkType::Simnet => KeyPrefix::KTRV,
    };

    let wallet_name = wallet_name.into();
    let word_count_value = mnemonic.phrase().split_whitespace().count();
    let manifest = DevnetBootstrapManifest {
        schema_version: 1,
        warning: "devnet/simnet acceptance wallet only; never fund this wallet on mainnet".to_string(),
        network: network_id.to_string(),
        wallet: DevnetWalletManifest {
            name: wallet_name,
            mnemonic: mnemonic.phrase_string(),
            word_count: word_count_value,
            derivation_path: derivation_path.to_string(),
            default_address: address.to_string(),
            public_key_hex: bytes_to_hex(&public_key.serialize()),
            xonly_public_key_hex: bytes_to_hex(&xonly_public_key),
            secret_key_hex: bytes_to_hex(&secret_key.secret_bytes()),
            root_xprv: root_xprv.to_string(key_prefix).to_string(),
        },
        node: DevnetNodeManifest { prealloc_cells, prealloc_amount_sau },
    };

    Ok(GeneratedDevnetBootstrap { manifest, address, secret_key })
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut result, "{byte:02x}").expect("writing to a String cannot fail");
    }
    result
}
