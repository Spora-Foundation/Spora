//!
//! Associative iterator over the tracked cell set.
//!

use crate::cell::{CellContext, CellEntryReference};

#[derive(Debug)]
pub struct CellIterator {
    entries: Vec<CellEntryReference>,
    cursor: usize,
}

impl CellIterator {
    pub fn new(cell_context: &CellContext) -> Self {
        Self { entries: cell_context.context().mature.clone(), cursor: 0 }
    }
}

impl Iterator for CellIterator {
    type Item = CellEntryReference;

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.entries.get(self.cursor).cloned();
        self.cursor += 1;
        entry
    }
}

impl CellIterator {
    pub fn new_for_cells(cell_context: &CellContext) -> Self {
        Self::new(cell_context)
    }
}
