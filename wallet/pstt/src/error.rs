//! Error types for the pstt crate.

use tondi_txscript_errors::TxScriptError;

use crate::input::InputBuilderError;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),
    #[error(transparent)]
    ConstructorError(#[from] ConstructorError),
    #[error("OutputNotModifiable")]
    OutOfBounds,
    #[error("Missing UTXO entry")]
    MissingUtxoEntry,
    #[error("Missing redeem script")]
    MissingRedeemScript,
    #[error(transparent)]
    InputBuilder(#[from] crate::input::InputBuilderError),
    #[error(transparent)]
    OutputBuilder(#[from] crate::output::OutputBuilderError),
    #[error("Serialization error: {0}")]
    HexDecodeError(#[from] hex::FromHexError),
    #[error("Json deserialize error: {0}")]
    JsonDeserializeError(#[from] serde_json::Error),
    #[error("Serialize error")]
    PSTBSerializeError(String),
    #[error("Unlock utxo error")]
    MultipleUnlockUtxoError(Vec<Error>),
    #[error("Unlock fees exceed available amount")]
    ExcessUnlockFeeError,
    #[error("Transaction output to output conversion error")]
    TxToInnerConversionError(#[source] Box<Error>),
    #[error("Transaction input building error in conversion")]
    TxToInnerConversionInputBuildingError(#[source] InputBuilderError),
    #[error("P2SH extraction error")]
    P2SHExtractError(#[source] TxScriptError),
    #[error("PSTB hex serialization error: {0}")]
    PSTBSerializeToHexError(String),
    #[error("PSTB serialization requires 'PSTB' prefix")]
    PSTBPrefixError,
    #[error("PSTT serialization requires 'PSTT' prefix")]
    PSTTPrefixError,
    #[error("Cannot set payload on PSTT version {0}, payload requires version 1 or higher")]
    PayloadRequiresVersion1(crate::pstt::Version),
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
