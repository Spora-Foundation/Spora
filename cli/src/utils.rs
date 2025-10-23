use crate::error::Error;
use crate::result::Result;
use spora_consensus_core::constants::SAU_PER_TONDI;
use std::fmt::Display;

pub fn try_parse_required_nonzero_spora_as_sau_u64<S: ToString + Display>(spora_amount: Option<S>) -> Result<u64> {
    if let Some(spora_amount) = spora_amount {
        let sau_amount = spora_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Spora amount is not valid: '{spora_amount}'")))?
            * SAU_PER_TONDI as f64;
        if sau_amount < 0.0 {
            Err(Error::custom("Supplied Spora amount is not valid: '{spora_amount}'"))
        } else {
            let sau_amount = sau_amount as u64;
            if sau_amount == 0 {
                Err(Error::custom("Supplied required spora amount must not be a zero: '{spora_amount}'"))
            } else {
                Ok(sau_amount)
            }
        }
    } else {
        Err(Error::custom("Missing Spora amount"))
    }
}

pub fn try_parse_required_spora_as_sau_u64<S: ToString + Display>(spora_amount: Option<S>) -> Result<u64> {
    if let Some(spora_amount) = spora_amount {
        let sau_amount = spora_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Spora amount is not valid: '{spora_amount}'")))?
            * SAU_PER_TONDI as f64;
        if sau_amount < 0.0 {
            Err(Error::custom("Supplied Spora amount is not valid: '{spora_amount}'"))
        } else {
            Ok(sau_amount as u64)
        }
    } else {
        Err(Error::custom("Missing Spora amount"))
    }
}

pub fn try_parse_optional_spora_as_sau_i64<S: ToString + Display>(spora_amount: Option<S>) -> Result<Option<i64>> {
    if let Some(spora_amount) = spora_amount {
        let sau_amount = spora_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_e| Error::custom(format!("Supplied Spora amount is not valid: '{spora_amount}'")))?
            * SAU_PER_TONDI as f64;
        if sau_amount < 0.0 {
            Err(Error::custom("Supplied Spora amount is not valid: '{spora_amount}'"))
        } else {
            Ok(Some(sau_amount as i64))
        }
    } else {
        Ok(None)
    }
}
