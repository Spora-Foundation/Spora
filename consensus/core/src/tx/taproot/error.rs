use bitcoin::io::Error as BitcoinIoError;
use std::io::Error as StdIoError;

#[derive(Debug, thiserror::Error)]
pub enum TaprootError {
    #[error(transparent)]
    StdIoError(#[from] StdIoError),

    #[error(transparent)]
    BitcoinIoError(#[from] BitcoinIoError),

    #[error("PrevoutsError")]
    PrevoutsError,

    #[error("InvalidSighashTypeError")]
    InvalidSighashTypeError,
}
