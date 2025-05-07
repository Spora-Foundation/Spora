use thiserror::Error;
use tondi_consensus_core::errors::{block::RuleError, coinbase::CoinbaseError};

#[derive(Error, Debug, Clone)]
pub enum BuilderError {
    /// A consensus rule error
    #[error(transparent)]
    ConsensusError(#[from] RuleError),

    /// A coinbase error
    #[error(transparent)]
    CoinbaseError(#[from] CoinbaseError),
}

pub type BuilderResult<T> = std::result::Result<T, BuilderError>;
