use serde::{Deserialize, Serialize};
use spora_utils::mem_size::MemSizeEstimator;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum BlockStatus {
    /// StatusInvalid indicates that the block is invalid.
    StatusInvalid,

    /// StatusCellValid indicates the block is valid from any Cell related aspects and has passed all the other validations as well.
    StatusCellValid,

    /// StatusCellPendingVerification indicates that the block is pending verification against its past Cell set, either
    /// because it was not yet verified since the block was never in the selected parent chain, or if the
    /// block violates finality.
    StatusCellPendingVerification,

    /// StatusDisqualifiedFromChain indicates that the block is not eligible to be a selected parent.
    StatusDisqualifiedFromChain,

    /// StatusHeaderOnly indicates that the block transactions are not held (pruned or wasn't added yet)
    StatusHeaderOnly,
}

impl MemSizeEstimator for BlockStatus {}

impl BlockStatus {
    pub fn has_block_header(self) -> bool {
        matches!(
            self,
            Self::StatusHeaderOnly | Self::StatusCellValid | Self::StatusCellPendingVerification | Self::StatusDisqualifiedFromChain
        )
    }

    pub fn is_header_only(self) -> bool {
        self == Self::StatusHeaderOnly
    }

    pub fn has_block_body(self) -> bool {
        matches!(self, Self::StatusCellValid | Self::StatusCellPendingVerification | Self::StatusDisqualifiedFromChain)
    }

    pub fn is_cell_valid_or_pending(self) -> bool {
        matches!(self, Self::StatusCellValid | Self::StatusCellPendingVerification)
    }

    pub fn is_valid(self) -> bool {
        self != BlockStatus::StatusInvalid
    }

    pub fn is_invalid(self) -> bool {
        self == BlockStatus::StatusInvalid
    }
}
