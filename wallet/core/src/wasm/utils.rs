use crate::result::Result;
use js_sys::BigInt;
use tondi_consensus_core::network::{NetworkType, NetworkTypeT};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "bigint | number | HexString")]
    #[derive(Clone, Debug)]
    pub type ISompiToTondi;
}

/// Convert a Tondi string to Sompi represented by bigint.
/// This function provides correct precision handling and
/// can be used to parse user input.
/// @category Wallet SDK
#[wasm_bindgen(js_name = "tondiToSompi")]
pub fn tondi_to_sompi(tondi: String) -> Option<BigInt> {
    crate::utils::try_tondi_str_to_sompi(tondi).ok().flatten().map(Into::into)
}

///
/// Convert Sompi to a string representation of the amount in Tondi.
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "sompiToTondiString")]
pub fn sompi_to_tondi_string(sompi: ISompiToTondi) -> Result<String> {
    let sompi = sompi.try_as_u64()?;
    Ok(crate::utils::sompi_to_tondi_string(sompi))
}

///
/// Format a Sompi amount to a string representation of the amount in Tondi with a suffix
/// based on the network type (e.g. `TND` for mainnet, `TKAS` for testnet,
/// `SKAS` for simnet, `DKAS` for devnet).
///
/// @category Wallet SDK
///
#[wasm_bindgen(js_name = "sompiToTondiStringWithSuffix")]
pub fn sompi_to_tondi_string_with_suffix(sompi: ISompiToTondi, network: &NetworkTypeT) -> Result<String> {
    let sompi = sompi.try_as_u64()?;
    let network_type = NetworkType::try_from(network)?;
    Ok(crate::utils::sompi_to_tondi_string_with_suffix(sompi, &network_type))
}
