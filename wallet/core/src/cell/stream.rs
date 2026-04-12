//!
//! Implements an async stream of tracked cells.
//!

use super::{CellContext, CellEntryReference};
use crate::imports::*;

pub struct CellStream {
    cell_context: CellContext,
    cursor: usize,
}

impl CellStream {
    pub fn new(cell_context: &CellContext) -> Self {
        Self { cell_context: cell_context.clone(), cursor: 0 }
    }
}

impl Stream for CellStream {
    type Item = CellEntryReference;
    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let entry = self.cell_context.context().mature.get(self.cursor).cloned();
        self.cursor += 1;
        Poll::Ready(entry)
    }
}

impl CellStream {
    pub fn new_for_cells(cell_context: &CellContext) -> Self {
        Self::new(cell_context)
    }
}
