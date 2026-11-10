//! Runtime-only records of the cells and rectangles read by formulas.

use std::collections::HashMap;

use crate::evaluation::CellKey;
use crate::expressions::types::CellReferenceIndex;

/// One input observed while evaluating a formula.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Read {
    /// One cell.
    Cell { sheet: u32, row: i32, column: i32 },
    /// One inclusive, normalized rectangle.
    Rect {
        sheet: u32,
        row1: i32,
        column1: i32,
        row2: i32,
        column2: i32,
    },
}

impl Read {
    pub(crate) fn cell(cell: CellReferenceIndex) -> Self {
        Self::Cell {
            sheet: cell.sheet,
            row: cell.row,
            column: cell.column,
        }
    }

    pub(crate) fn rect(left: CellReferenceIndex, right: CellReferenceIndex) -> Self {
        Self::Rect {
            sheet: left.sheet,
            row1: left.row.min(right.row),
            column1: left.column.min(right.column),
            row2: left.row.max(right.row),
            column2: left.column.max(right.column),
        }
    }
}

/// The reads produced by one completed formula evaluation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FormulaReads {
    pub(crate) reads: Vec<Read>,
    pub(crate) complete: bool,
}

impl Default for FormulaReads {
    fn default() -> Self {
        Self::new()
    }
}

impl FormulaReads {
    fn new() -> Self {
        Self {
            reads: Vec::new(),
            complete: true,
        }
    }

    fn record(&mut self, read: Read) {
        // A range consumer may subsequently evaluate every member cell. The
        // range is the dependency, so keep it as one read rather than growing
        // an entry per member.
        if let Read::Cell { sheet, row, column } = read {
            if self.reads.iter().any(|existing| {
                matches!(
                    existing,
                    Read::Rect { sheet: rect_sheet, row1, column1, row2, column2 }
                        if *rect_sheet == sheet
                            && *row1 <= row && row <= *row2
                            && *column1 <= column && column <= *column2
                )
            }) {
                return;
            }
        }
        if !self.reads.contains(&read) {
            self.reads.push(read);
        }
    }
}

/// A disabled-by-default, non-serialized forward index and its active frames.
#[derive(Default)]
pub(crate) struct DependencyIndex {
    pub(crate) enabled: bool,
    reads: HashMap<CellKey, FormulaReads>,
    frames: Vec<FormulaReads>,
}

impl DependencyIndex {
    #[allow(dead_code)] // used by tests now; the next stage uses it in evaluation
    pub(crate) fn enable(&mut self) {
        self.enabled = true;
    }

    #[allow(dead_code)] // used by tests now; the next stage uses it in evaluation
    pub(crate) fn disable(&mut self) {
        self.enabled = false;
        self.clear();
    }

    #[allow(dead_code)] // used by tests now; the next stage uses it in evaluation
    pub(crate) fn clear(&mut self) {
        self.reads.clear();
        self.frames.clear();
    }

    pub(crate) fn remove(&mut self, cell: CellReferenceIndex) {
        if self.enabled {
            self.reads.remove(&(cell.sheet, cell.row, cell.column));
        }
    }

    #[allow(dead_code)] // used by tests now; the next stage uses it in evaluation
    pub(crate) fn formula_reads(&self, cell: CellReferenceIndex) -> Option<&FormulaReads> {
        self.reads.get(&(cell.sheet, cell.row, cell.column))
    }

    pub(crate) fn push(&mut self) {
        if self.enabled {
            self.frames.push(FormulaReads::new());
        }
    }

    pub(crate) fn finish(&mut self, cell: CellReferenceIndex, keep: bool) {
        if !self.enabled {
            return;
        }
        let Some(frame) = self.frames.pop() else {
            return;
        };
        if keep {
            self.reads
                .insert((cell.sheet, cell.row, cell.column), frame);
        }
    }

    pub(crate) fn record_cell(&mut self, cell: CellReferenceIndex) {
        if self.enabled {
            if let Some(frame) = self.frames.last_mut() {
                frame.record(Read::cell(cell));
            }
        }
    }

    pub(crate) fn record_rect(&mut self, left: CellReferenceIndex, right: CellReferenceIndex) {
        if self.enabled {
            if let Some(frame) = self.frames.last_mut() {
                frame.record(Read::rect(left, right));
            }
        }
    }

    pub(crate) fn mark_incomplete(&mut self) {
        if self.enabled {
            if let Some(frame) = self.frames.last_mut() {
                frame.complete = false;
            }
        }
    }
}
