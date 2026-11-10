#![allow(clippy::unwrap_used)]

use crate::dependency_index::Read;
use crate::expressions::types::CellReferenceIndex;
use crate::model::DefinedNameEntry;
use crate::test::util::new_empty_model;
use crate::Model;

fn cell(sheet: u32, row: i32, column: i32) -> Read {
    Read::Cell { sheet, row, column }
}

fn rect(sheet: u32, row1: i32, column1: i32, row2: i32, column2: i32) -> Read {
    Read::Rect {
        sheet,
        row1,
        column1,
        row2,
        column2,
    }
}

fn reads(model: &Model<'_>, reference: &str) -> Vec<Read> {
    let CellReferenceIndex { sheet, row, column } = model._parse_reference(reference);
    model
        .formula_reads(sheet, row, column)
        .unwrap()
        .reads
        .clone()
}

#[test]
fn dependency_index_direct_cell_reference() {
    let mut model = new_empty_model();
    model._set("A1", "3");
    model._set("B1", "=A1");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1)]);
}

#[test]
fn dependency_index_empty_and_plain_value_cells() {
    let mut model = new_empty_model();
    model._set("B1", "=A1");
    model._set("B2", "=A2");
    model._set("A2", "plain");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1)]);
    assert_eq!(reads(&model, "B2"), vec![cell(0, 2, 1)]);
}

#[test]
fn dependency_index_relative_and_absolute_references() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("B2", "=A2+$A$1");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "B2"), vec![cell(0, 2, 1), cell(0, 1, 1)]);
}

#[test]
fn dependency_index_cross_sheet_reference() {
    let mut model = new_empty_model();
    model.add_sheet("Data").unwrap();
    model._set("Data!A1", "7");
    model._set("A1", "=Data!A1");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "A1"), vec![cell(1, 1, 1)]);
}

#[test]
fn dependency_index_defined_name_cell_and_range() {
    let mut model = new_empty_model();
    model._set("A1", "3");
    model._set("A2", "4");
    model
        .new_defined_name(DefinedNameEntry::new("one", "Sheet1!$A$1"))
        .unwrap();
    model
        .new_defined_name(DefinedNameEntry::new("many", "Sheet1!$A$1:$A$2"))
        .unwrap();
    model._set("B1", "=one");
    model._set("B2", "=SUM(many)");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1)]);
    assert_eq!(reads(&model, "B2"), vec![rect(0, 1, 1, 2, 1)]);
}

#[test]
fn dependency_index_range_is_one_original_rectangle() {
    let mut model = new_empty_model();
    model._set("AA1", "=SUM(A1:Z1000)");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "AA1"), vec![rect(0, 1, 1, 1000, 26)]);
}

#[test]
fn dependency_index_whole_column_and_row_are_not_clipped() {
    let mut model = new_empty_model();
    model._set("A1", "2");
    model._set("C1", "=SUM(A:A)");
    model._set("A2", "=SUM(1:1)");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "C1"), vec![rect(0, 1, 1, 1_048_576, 1)]);
    assert_eq!(reads(&model, "A2"), vec![rect(0, 1, 1, 1, 16_384)]);
}

#[test]
fn dependency_index_same_formula_at_two_positions() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("B1", "=A1");
    model._set("B2", "=A2");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1)]);
    assert_eq!(reads(&model, "B2"), vec![cell(0, 2, 1)]);
}

#[test]
fn dependency_index_branch_reads_are_replaced() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("C1", "3");
    model._set("D1", "4");
    model._set("B1", "=IF(A1=1,C1,D1)");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1), cell(0, 1, 3)]);
    model._set("A1", "0");
    model.evaluate();
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1), cell(0, 1, 4)]);
}

#[test]
fn dependency_index_error_keeps_reads_made_before_error() {
    let mut model = new_empty_model();
    model._set("A1", "2");
    model._set("B1", "=A1/0");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(model._get_text("B1"), "#DIV/0!");
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1)]);
}

#[test]
fn dependency_index_chain_records_own_reads_only() {
    let mut model = new_empty_model();
    model._set("C1", "1");
    model._set("B1", "=C1");
    model._set("A1", "=B1");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "A1"), vec![cell(0, 1, 2)]);
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 3)]);
}

#[test]
fn dependency_index_diamond_records_direct_inputs_only() {
    let mut model = new_empty_model();
    model._set("D1", "1");
    model._set("B1", "=D1");
    model._set("C1", "=D1");
    model._set("A1", "=B1+C1");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "A1"), vec![cell(0, 1, 2), cell(0, 1, 3)]);
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 4)]);
    assert_eq!(reads(&model, "C1"), vec![cell(0, 1, 4)]);
}

#[test]
fn dependency_index_cycle_does_not_leave_a_removed_formula_record() {
    let mut model = new_empty_model();
    model._set("A1", "=B1");
    model._set("B1", "=A1");
    model.enable_dependency_index();
    model.evaluate();
    assert!(model.formula_reads(0, 1, 1).is_some());
    model._set("A1", "1");
    model.evaluate();
    assert!(model.formula_reads(0, 1, 1).is_none());
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1)]);
}

#[test]
fn dependency_index_cycle_members_are_incomplete() {
    // A formula caught in a cycle returns a circular-reference error without
    // reading its inputs. Recording that as "reads nothing, complete" would let
    // a later stage skip it once the cycle is broken, so it must be incomplete.
    let mut model = new_empty_model();
    model._set("A1", "=B1");
    model._set("B1", "=A1");
    model._set("C1", "=A1+1");
    model.enable_dependency_index();
    model.evaluate();
    for reference in ["A1", "B1"] {
        let CellReferenceIndex { sheet, row, column } = model._parse_reference(reference);
        let recorded = model.formula_reads(sheet, row, column).unwrap();
        assert!(!recorded.complete, "{reference} is on a cycle");
    }
    // C1 reads A1 normally; being downstream of a cycle does not make it incomplete.
    let CellReferenceIndex { sheet, row, column } = model._parse_reference("C1");
    let downstream = model.formula_reads(sheet, row, column).unwrap();
    assert!(downstream.complete);
    assert_eq!(downstream.reads, vec![cell(0, 1, 1)]);
}

#[test]
fn dependency_index_dynamic_and_volatile_functions_are_incomplete() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "=INDIRECT(\"A1\")");
    model._set("C1", "=OFFSET(A1,0,0)");
    model._set("D1", "=RAND()");
    model.enable_dependency_index();
    model.evaluate();
    for reference in ["B1", "C1", "D1"] {
        let CellReferenceIndex { sheet, row, column } = model._parse_reference(reference);
        assert!(!model.formula_reads(sheet, row, column).unwrap().complete);
    }
}

#[test]
fn dependency_index_formula_inspection_functions_are_incomplete() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("B1", "=ISFORMULA(A1)");
    model._set("B2", "=SUBTOTAL(9,A1:A2)");
    model.enable_dependency_index();
    model.evaluate();
    assert_eq!(reads(&model, "B1"), vec![cell(0, 1, 1)]);
    assert!(!model.formula_reads(0, 1, 2).unwrap().complete);
    assert!(!model.formula_reads(0, 2, 2).unwrap().complete);
}

#[test]
fn dependency_index_range_consumers_and_numeric_bulk_reads() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("B1", "one");
    model._set("B2", "two");
    model._set("D1", "1");
    model._set("D2", "0");
    model._set("E1", "0");
    model._set("E2", "1");
    model._set("G1", "=SUMIFS(A1:A2,B1:B2,\"one\")");
    model._set("G2", "=VLOOKUP(2,A1:B2,2,FALSE)");
    model._set("G3", "=INDEX(A1:B2,2,1)");
    model._set("G4", "=MMULT(A1:B2,D1:E2)");
    model.enable_dependency_index();
    model.evaluate();
    assert!(reads(&model, "G1").contains(&rect(0, 1, 1, 2, 1)));
    assert!(reads(&model, "G2").contains(&rect(0, 1, 1, 2, 2)));
    assert!(reads(&model, "G3").contains(&rect(0, 1, 1, 2, 2)));
    assert_eq!(
        reads(&model, "G4"),
        vec![rect(0, 1, 1, 2, 2), rect(0, 1, 4, 2, 5)]
    );
}

#[test]
fn dependency_index_disabled_records_nothing_and_values_match() {
    let mut off = new_empty_model();
    off._set("A1", "4");
    off._set("B1", "=A1*2");
    off.evaluate();
    assert!(off.formula_reads(0, 1, 2).is_none());
    let off_value = off._get_text("B1");

    let mut on = new_empty_model();
    on._set("A1", "4");
    on._set("B1", "=A1*2");
    on.enable_dependency_index();
    on.evaluate();
    assert_eq!(off_value, on._get_text("B1"));
    assert!(on.formula_reads(0, 1, 2).is_some());
}

#[test]
fn dependency_index_loaded_model_starts_empty_and_disabled() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "=A1");
    model.enable_dependency_index();
    model.evaluate();
    let bytes = model.to_bytes();
    let loaded = Model::from_bytes(&bytes, "en").unwrap();
    assert!(!loaded.dependency_index.enabled);
    assert!(loaded.formula_reads(0, 1, 2).is_none());
}

#[test]
fn dependency_index_mixed_workbook_values_match_tracking_off() {
    let mut off = new_empty_model();
    let mut on = new_empty_model();
    for model in [&mut off, &mut on] {
        model._set("A1", "2");
        model._set("A2", "3");
        model._set("B1", "=A1+A2");
        model._set("B2", "=SUM(A1:A2)");
        model._set("C1", "=IF(A1>0,B1,B2)");
        model._set("C2", "=VLOOKUP(3,A1:B2,1,FALSE)");
    }
    on.enable_dependency_index();
    off.evaluate();
    on.evaluate();
    for reference in ["A1", "A2", "B1", "B2", "C1", "C2"] {
        assert_eq!(off._get_text(reference), on._get_text(reference));
    }
}

#[test]
fn dependency_index_benchmark_workload_has_cells_and_sum_rectangle() {
    let mut model = new_empty_model();
    let rows = 20;
    for row in 1..=rows {
        model._set(&format!("A{row}"), "1");
        model._set(&format!("B{row}"), "2");
        model._set(&format!("C{row}"), &format!("=A{row}+B{row}*2"));
    }
    model._set("D1", &format!("=SUM(A1:A{rows})"));
    model.enable_dependency_index();
    model.evaluate();
    for row in 1..=rows {
        assert_eq!(
            reads(&model, &format!("C{row}")),
            vec![cell(0, row, 1), cell(0, row, 2)]
        );
    }
    assert_eq!(reads(&model, "D1"), vec![rect(0, 1, 1, rows, 1)]);
}
