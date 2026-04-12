//!
//! Implementation of [`CellContextBinding`] which allows binding of
//! [`CellContext`] to [`Account`] or custom developer-defined ids.
//!

use crate::cell::CellContextId;
use crate::imports::*;

#[derive(Clone)]
pub enum CellContextBinding {
    Internal(CellContextId),
    AccountId(AccountId),
    Id(CellContextId),
}

impl Default for CellContextBinding {
    fn default() -> Self {
        CellContextBinding::Internal(CellContextId::default())
    }
}

impl CellContextBinding {
    pub fn id(&self) -> CellContextId {
        match self {
            CellContextBinding::Internal(id) => *id,
            CellContextBinding::AccountId(id) => (*id).into(),
            CellContextBinding::Id(id) => *id,
        }
    }
}
