use thiserror::Error;
use crate::utxo::utxo_error::UtxoAlgebraError as CoreUtxoAlgebraError;

#[derive(Error, Debug, Clone)]
pub enum UtxoAlgebraError {
    #[error("UTXO set is empty")]
    EmptyUtxoSet,
    
    #[error("UTXO set is invalid")]
    InvalidUtxoSet,
    
    #[error("UTXO entry not found")]
    UtxoNotFound,
    
    #[error("UTXO entry already exists")]
    UtxoAlreadyExists,
    
    #[error("UTXO set is inconsistent")]
    InconsistentUtxoSet,

    #[error("Core UTXO algebra error: {0}")]
    Core(String),
} 

impl From<CoreUtxoAlgebraError> for UtxoAlgebraError {
    fn from(err: CoreUtxoAlgebraError) -> Self {
        UtxoAlgebraError::Core(err.to_string())
    }
} 