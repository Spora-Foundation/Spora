use thiserror::Error;
use tondi_notify::events::EventType;
use tondi_cellindex::errors::CellIndexError;

#[derive(Error, Debug)]
pub enum IndexError {
    #[error("{0}")]
    CellIndexError(#[from] CellIndexError),

    #[error("event type {0:?} is not supported")]
    NotSupported(EventType),
}
pub type IndexResult<T> = std::result::Result<T, IndexError>;
