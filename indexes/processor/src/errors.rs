use thiserror::Error;
use spora_notify::events::EventType;
use spora_cellindex::errors::CellIndexError;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("{0}")]
    CellIndexError(#[from] CellIndexError),

    #[error("event type {0:?} is not supported")]
    NotSupported(EventType),
}
pub type IndexResult<T> = std::result::Result<T, IndexError>;
