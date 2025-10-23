use spora_bip32::Error as SporaBip32Error;
use spora_addresses::AddressError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    SporaBip32Error(#[from] SporaBip32Error),

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error("{0}")]
    Generic(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
