//! Error types for the psst crate.

use crate::input::InputBuilderError;
use spora_addresses::AddressError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),
    #[error(transparent)]
    ConstructorError(#[from] ConstructorError),
    #[error("OutputNotModifiable")]
    OutOfBounds,
    #[error("Missing cell entry")]
    MissingCellEntry,
    #[error("Missing witness template")]
    MissingWitnessTemplate,
    #[error(transparent)]
    InputBuilder(#[from] crate::input::InputBuilderError),
    #[error(transparent)]
    OutputBuilder(#[from] crate::output::OutputBuilderError),
    #[error("Serialization error: {0}")]
    HexDecodeError(#[from] hex::FromHexError),
    #[error("Json deserialize error: {0}")]
    JsonDeserializeError(#[from] serde_json::Error),
    #[error("Serialize error")]
    PSSBSerializeError(String),
    #[error("Transaction output to output conversion error")]
    TxToInnerConversionError(#[source] Box<Error>),
    #[error("Transaction input building error in conversion")]
    TxToInnerConversionInputBuildingError(#[source] InputBuilderError),
    #[error("PSSB hex serialization error: {0}")]
    PSSBSerializeToHexError(String),
    #[error("PSSB serialization requires 'PSSB' prefix")]
    PSSBPrefixError,
    #[error("PSST serialization requires 'PSST' prefix")]
    PSSTPrefixError,
    #[error("Cannot set payload on PSST version {0}, payload requires version 1 or higher")]
    PayloadRequiresVersion1(crate::psst::Version),
    #[error(transparent)]
    Address(#[from] AddressError),
}
#[derive(thiserror::Error, Debug)]
pub enum ConstructorError {
    #[error("InputNotModifiable")]
    InputNotModifiable,
    #[error("OutputNotModifiable")]
    OutputNotModifiable,
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::Custom(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self::Custom(err.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("Invalid output conversion")]
    InvalidOutput,
}
