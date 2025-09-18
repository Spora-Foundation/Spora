use tondi_bip32::Error as TondiBip32Error;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    TondiBip32Error(#[from] TondiBip32Error),

    #[error("{0}")]
    Generic(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
