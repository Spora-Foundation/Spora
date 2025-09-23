use crate::result::Result;
use js_sys::BigInt;
use tondi_consensus_core::network::{NetworkType, NetworkTypeT};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "bigint | number | HexString")]
    #[derive(Clone, Debug)]
    pub type ISauToTondi;
}

/// Convert a Tondi string to Sau represented by bigint.
/// This function provides correct precision handling and
/// can be used to parse user input.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "tondiToSau")]
pub fn tondi_to_sau(tondi: String) -> Option<BigInt> {
    crate::utils::try_tondi_str_to_sau(tondi).ok().flatten().map(Into::into)
}

///
/// Convert Sau to a string representation of the amount in Tondi.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "sauToTondiString")]
pub fn sau_to_tondi_string(sau: ISauToTondi) -> Result<String> {
    let sau = sau.try_as_u64()?;
    Ok(crate::utils::sau_to_tondi_string(sau))
}

///
/// Format a Sau amount to a string representation of the amount in Tondi with a suffix
/// based on the network type (e.g. `TONDI` for mainnet, `TTONDI` for testnet,
/// `STONDI` for simnet, `DTONDI` for devnet).
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "sauToTondiStringWithSuffix")]
pub fn sau_to_tondi_string_with_suffix(sau: ISauToTondi, network: &NetworkTypeT) -> Result<String> {
    let sau = sau.try_as_u64()?;
    let network_type = NetworkType::try_from(network)?;
    Ok(crate::utils::sau_to_tondi_string_with_suffix(sau, &network_type))
}
