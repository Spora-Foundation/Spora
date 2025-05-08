use crate::error::Error;
use crate::result::Result;
use std::fmt::Display;
use tondi_consensus_core::constants::SOMPI_PER_TONDI;

pub fn try_parse_required_nonzero_tondi_as_sompi_u64<S: ToString + Display>(tondi_amount: Option<S>) -> Result<u64> {
    if let Some(tondi_amount) = tondi_amount {
        let sompi_amount = tondi_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Tondi amount is not valid: '{tondi_amount}'")))?
            * SOMPI_PER_TONDI as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Tondi amount is not valid: '{tondi_amount}'"))
        } else {
            let sompi_amount = sompi_amount as u64;
            if sompi_amount == 0 {
                Err(Error::custom("Supplied required tondi amount must not be a zero: '{tondi_amount}'"))
            } else {
                Ok(sompi_amount)
            }
        }
    } else {
        Err(Error::custom("Missing Tondi amount"))
    }
}

pub fn try_parse_required_tondi_as_sompi_u64<S: ToString + Display>(tondi_amount: Option<S>) -> Result<u64> {
    if let Some(tondi_amount) = tondi_amount {
        let sompi_amount = tondi_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_| Error::custom(format!("Supplied Kasapa amount is not valid: '{tondi_amount}'")))?
            * SOMPI_PER_TONDI as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Tondi amount is not valid: '{tondi_amount}'"))
        } else {
            Ok(sompi_amount as u64)
        }
    } else {
        Err(Error::custom("Missing Tondi amount"))
    }
}

pub fn try_parse_optional_tondi_as_sompi_i64<S: ToString + Display>(tondi_amount: Option<S>) -> Result<Option<i64>> {
    if let Some(tondi_amount) = tondi_amount {
        let sompi_amount = tondi_amount
            .to_string()
            .parse::<f64>()
            .map_err(|_e| Error::custom(format!("Supplied Kasapa amount is not valid: '{tondi_amount}'")))?
            * SOMPI_PER_TONDI as f64;
        if sompi_amount < 0.0 {
            Err(Error::custom("Supplied Tondi amount is not valid: '{tondi_amount}'"))
        } else {
            Ok(Some(sompi_amount as i64))
        }
    } else {
        Ok(None)
    }
}
