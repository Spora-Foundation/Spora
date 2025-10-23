//!
//! Spora value formatting and parsing utilities.
//!

use crate::result::Result;
use separator::{separated_float, separated_int, separated_uint_with_output, Separatable};
use spora_addresses::Address;
use spora_consensus_core::constants::*;
use spora_consensus_core::network::NetworkType;
use workflow_log::style;

pub fn try_spora_str_to_sau<S: Into<String>>(s: S) -> Result<Option<u64>> {
    let s: String = s.into();
    let amount = s.trim();
    if amount.is_empty() {
        return Ok(None);
    }

    Ok(Some(str_to_sau(amount)?))
}

pub fn try_spora_str_to_sau_i64<S: Into<String>>(s: S) -> Result<Option<i64>> {
    let s: String = s.into();
    let amount = s.trim();
    if amount.is_empty() {
        return Ok(None);
    }

    let amount = amount.parse::<f64>()? * SAU_PER_TONDI as f64;
    Ok(Some(amount as i64))
}

#[inline]
pub fn sau_to_spora(sau: u64) -> f64 {
    sau as f64 / SAU_PER_TONDI as f64
}

#[inline]
pub fn spora_to_sau(spora: f64) -> u64 {
    (spora * SAU_PER_TONDI as f64) as u64
}

#[inline]
pub fn sau_to_spora_string(sau: u64) -> String {
    sau_to_spora(sau).separated_string()
}

#[inline]
pub fn sau_to_spora_string_with_trailing_zeroes(sau: u64) -> String {
    separated_float!(format!("{:.8}", sau_to_spora(sau)))
}

pub fn spora_suffix(network_type: &NetworkType) -> &'static str {
    match network_type {
        NetworkType::Mainnet => "SPORA",
        NetworkType::Testnet => "TTONDI",
        NetworkType::Simnet => "STONDI",
        NetworkType::Devnet => "DTONDI",
    }
}

#[inline]
pub fn sau_to_spora_string_with_suffix(sau: u64, network_type: &NetworkType) -> String {
    let spora = sau_to_spora_string(sau);
    let suffix = spora_suffix(network_type);
    format!("{spora} {suffix}")
}

#[inline]
pub fn sau_to_spora_string_with_trailing_zeroes_and_suffix(sau: u64, network_type: &NetworkType) -> String {
    let spora = sau_to_spora_string_with_trailing_zeroes(sau);
    let suffix = spora_suffix(network_type);
    format!("{spora} {suffix}")
}

pub fn format_address_colors(address: &Address, range: Option<usize>) -> String {
    let address = address.to_string();

    let parts = address.split(':').collect::<Vec<&str>>();
    let prefix = style(parts[0]).dim();
    let payload = parts[1];
    let range = range.unwrap_or(6);
    let start = range;
    let finish = payload.len() - range;

    let left = &payload[0..start];
    let center = style(&payload[start..finish]).dim();
    let right = &payload[finish..];

    format!("{prefix}:{left}:{center}:{right}")
}

fn str_to_sau(amount: &str) -> Result<u64> {
    let Some(dot_idx) = amount.find('.') else {
        return Ok(amount.parse::<u64>()? * SAU_PER_TONDI);
    };
    let integer = amount[..dot_idx].parse::<u64>()? * SAU_PER_TONDI;
    let decimal = &amount[dot_idx + 1..];
    let decimal_len = decimal.len();
    let decimal = if decimal_len == 0 {
        0
    } else if decimal_len <= 8 {
        decimal.parse::<u64>()? * 10u64.pow(8 - decimal_len as u32)
    } else {
        // TODO - discuss how to handle values longer than 8 decimal places
        // (reject, truncate, ceil(), etc.)
        decimal[..8].parse::<u64>()?
    };
    Ok(integer + decimal)
}
