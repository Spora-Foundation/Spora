use crate::result::Result;
use crate::utxo::NetworkParams;
use js_sys::BigInt;
use tondi_consensus_core::network::{NetworkIdT, NetworkType, NetworkTypeT};
use wasm_bindgen::prelude::*;
use workflow_wasm::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "bigint | number | HexString")]
    #[derive(Clone, Debug)]
    pub type ISauToTondi;

    #[wasm_bindgen(typescript_type = "object")]
    #[derive(Clone, Debug)]
    pub type INetworkParams;
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

#[wasm_bindgen(js_name = "getNetworkParams")]
#[allow(non_snake_case)]
pub fn get_network_params(networkId: NetworkIdT) -> Result<INetworkParams> {
    let params = NetworkParams::from(*networkId.try_into_cast()?);
    // Convert NetworkParams to a JavaScript object
    let obj = js_sys::Object::new();
    obj.set("coinbaseTransactionMaturityPeriodDaa", &params.coinbase_transaction_maturity_period_daa().into())?;
    obj.set("coinbaseTransactionStasisPeriodDaa", &params.coinbase_transaction_stasis_period_daa().into())?;
    obj.set("userTransactionMaturityPeriodDaa", &params.user_transaction_maturity_period_daa().into())?;
    obj.set("additionalCompoundTransactionMass", &params.additional_compound_transaction_mass().into())?;
    Ok(JsValue::from(obj).into())
}

#[wasm_bindgen(js_name = "getTransactionMaturityProgress")]
#[allow(non_snake_case)]
pub fn get_transaction_maturity_progress(
    blockDaaScore: BigInt,
    currentDaaScore: BigInt,
    networkId: NetworkIdT,
    isCoinbase: bool,
) -> Result<String> {
    let network_id = *networkId.try_into_cast()?;
    let params = NetworkParams::from(network_id);
    let block_daa_score = blockDaaScore.try_as_u64()?;
    let current_daa_score = currentDaaScore.try_as_u64()?;
    let maturity =
        if isCoinbase { params.coinbase_transaction_maturity_period_daa() } else { params.user_transaction_maturity_period_daa() };

    if current_daa_score < block_daa_score + maturity {
        let progress = (current_daa_score - block_daa_score) as f64 / maturity as f64;
        Ok(format!("{}", (progress * 100.) as usize))
    } else {
        Ok("".to_string())
    }
}
