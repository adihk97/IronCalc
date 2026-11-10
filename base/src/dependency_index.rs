//! Runtime-only records of the cells and rectangles read by formulas.

use std::collections::{HashMap, HashSet};

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
    pub(crate) valid: bool,
    reads: HashMap<CellKey, FormulaReads>,
    frames: Vec<FormulaReads>,
    incomplete: usize,
    cells: HashMap<CellKey, HashSet<CellKey>>,
    // Rectangles are scanned per sheet; an interval structure can replace this later.
    // Keyed by the reading formula, so removing a formula's old record is direct instead of a
    // scan of every rectangle on the sheet (that scan made a full pass O(formulas squared) on
    // sheets where every formula holds a range).
    rectangles: HashMap<u32, HashMap<CellKey, Vec<Read>>>,
    // Number of rectangle edges per sheet, kept in step with `rectangles`.
    rectangle_edges: HashMap<u32, usize>,
}

impl DependencyIndex {
    #[allow(dead_code)] // used by tests now; the next stage uses it in evaluation
    pub(crate) fn enable(&mut self) {
        self.enabled = true;
        self.valid = false;
    }

    #[allow(dead_code)] // used by tests now; the next stage uses it in evaluation
    pub(crate) fn disable(&mut self) {
        self.enabled = false;
        self.valid = false;
        self.clear();
    }

    #[allow(dead_code)] // used by tests now; the next stage uses it in evaluation
    pub(crate) fn clear(&mut self) {
        self.reads.clear();
        self.frames.clear();
        self.cells.clear();
        self.rectangles.clear();
        self.rectangle_edges.clear();
        self.incomplete = 0;
        self.valid = false;
    }

    pub(crate) fn invalidate(&mut self) {
        self.valid = false;
    }

    pub(crate) fn reads_len(&self) -> usize {
        self.reads.len()
    }

    /// Rectangle reads are currently stored as a flat per-sheet list. The
    /// incremental driver uses this to refuse a scan before it becomes larger
    /// than a small, predictable bound.
    pub(crate) fn rectangle_edges_in_sheet(&self, sheet: u32) -> usize {
        self.rectangle_edges.get(&sheet).copied().unwrap_or(0)
    }
    pub(crate) fn reads_are_complete(&self) -> bool {
        self.reads.values().all(|r| r.complete)
    }
    pub(crate) fn incomplete_count(&self) -> usize {
        self.incomplete
    }

    pub(crate) fn remove(&mut self, cell: CellReferenceIndex) {
        if self.enabled {
            self.remove_key((cell.sheet, cell.row, cell.column));
        }
    }

    fn remove_key(&mut self, key: CellKey) {
        if let Some(old) = self.reads.remove(&key) {
            if !old.complete {
                self.incomplete = self.incomplete.saturating_sub(1);
            }
            for read in old.reads {
                match read {
                    Read::Cell { sheet, row, column } => {
                        let input = (sheet, row, column);
                        if let Some(formulas) = self.cells.get_mut(&input) {
                            formulas.remove(&key);
                            if formulas.is_empty() {
                                self.cells.remove(&input);
                            }
                        }
                    }
                    Read::Rect { sheet, .. } => {
                        // All of this formula's rectangles on the sheet go at once; a second
                        // rectangle read on the same sheet then finds nothing left to remove.
                        if let Some(by_formula) = self.rectangles.get_mut(&sheet) {
                            if let Some(removed) = by_formula.remove(&key) {
                                if let Some(count) = self.rectangle_edges.get_mut(&sheet) {
                                    *count = count.saturating_sub(removed.len());
                                    if *count == 0 {
                                        self.rectangle_edges.remove(&sheet);
                                    }
                                }
                            }
                            if by_formula.is_empty() {
                                self.rectangles.remove(&sheet);
                            }
                        }
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    pub(crate) fn readers(&self, cell: CellReferenceIndex) -> HashSet<CellKey> {
        let mut result = self
            .cells
            .get(&(cell.sheet, cell.row, cell.column))
            .cloned()
            .unwrap_or_default();
        if let Some(rectangles) = self.rectangles.get(&cell.sheet) {
            for (formula, reads) in rectangles {
                for read in reads {
                    if let Read::Rect {
                        row1,
                        column1,
                        row2,
                        column2,
                        ..
                    } = read
                    {
                        if *row1 <= cell.row
                            && cell.row <= *row2
                            && *column1 <= cell.column
                            && cell.column <= *column2
                        {
                            result.insert(*formula);
                            break;
                        }
                    }
                }
            }
        }
        result
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
            let key = (cell.sheet, cell.row, cell.column);
            self.remove_key(key);
            for read in &frame.reads {
                match read {
                    Read::Cell { sheet, row, column } => {
                        self.cells
                            .entry((*sheet, *row, *column))
                            .or_default()
                            .insert(key);
                    }
                    Read::Rect { sheet, .. } => {
                        self.rectangles
                            .entry(*sheet)
                            .or_default()
                            .entry(key)
                            .or_default()
                            .push(read.clone());
                        *self.rectangle_edges.entry(*sheet).or_insert(0) += 1;
                    }
                }
            }
            self.reads.insert(key, frame);
            if !self.reads[&key].complete {
                self.incomplete += 1;
            }
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

#[cfg(test)]
mod reverse_tests {
    use super::*;

    fn pos(sheet: u32, row: i32, column: i32) -> CellReferenceIndex {
        CellReferenceIndex { sheet, row, column }
    }

    #[test]
    fn reverse_edges_replace_and_remove() {
        let mut index = DependencyIndex::default();
        index.enable();
        let formula = pos(0, 1, 3);
        index.push();
        index.record_cell(pos(0, 1, 1));
        index.record_rect(pos(1, 1, 1), pos(1, 10, 2));
        index.finish(formula, true);
        assert!(index.readers(pos(0, 1, 1)).contains(&(0, 1, 3)));
        assert!(index.readers(pos(1, 10, 2)).contains(&(0, 1, 3)));
        assert!(index.readers(pos(1, 11, 2)).is_empty());
        index.push();
        index.record_cell(pos(0, 1, 2));
        index.finish(formula, true);
        assert!(index.readers(pos(0, 1, 1)).is_empty());
        assert!(index.readers(pos(1, 10, 2)).is_empty());
        assert!(index.readers(pos(0, 1, 2)).contains(&(0, 1, 3)));
        index.remove(formula);
        assert!(index.readers(pos(0, 1, 2)).is_empty());
    }

    #[test]
    fn reverse_edges_stay_bounded_after_many_replacements() {
        let mut index = DependencyIndex::default();
        index.enable();
        let formula = pos(0, 1, 3);
        for row in 1..=10_000 {
            index.push();
            index.record_cell(pos(0, row, 1));
            index.record_rect(pos(0, row, 2), pos(0, row + 1, 2));
            index.finish(formula, true);
            assert_eq!(index.reads.len(), 1);
            assert_eq!(index.cells.len(), 1);
            assert_eq!(index.rectangle_edges.values().sum::<usize>(), 1);
        }
        index.clear();
        assert!(index.cells.is_empty());
        assert!(index.rectangles.is_empty());
    }

    #[test]
    fn abandoned_frame_keeps_old_edges() {
        let mut index = DependencyIndex::default();
        index.enable();
        let formula = pos(0, 1, 3);
        index.push();
        index.record_cell(pos(0, 1, 1));
        index.finish(formula, true);
        index.push();
        index.record_cell(pos(0, 1, 2));
        index.finish(formula, false);
        assert!(index.readers(pos(0, 1, 1)).contains(&(0, 1, 3)));
        assert!(index.readers(pos(0, 1, 2)).is_empty());
    }
}
