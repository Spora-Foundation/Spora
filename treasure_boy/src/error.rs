use tondi_bip32::Error as TondiBip32Error;
use tondi_addresses::AddressError;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TondiBip32Error(#[from] TondiBip32Error),

    #[error(transparent)]
    AddressError(#[from] AddressError),

    #[error("{0}")]
    Generic(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
