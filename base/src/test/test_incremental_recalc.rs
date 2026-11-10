#![allow(clippy::unwrap_used)]

use std::time::Instant;

use crate::expressions::types::Area;
use crate::model::DefinedNameEntry;
use crate::model::EvaluationPath;
use crate::test::util::new_empty_model;

fn seeded_model(incremental: bool) -> crate::Model<'static> {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("A2", "2");
    model._set("B1", "=A1+A2");
    model._set("B2", "=B1*2");
    model._set("C1", "=SUM(A1:A2)");
    model._set("C2", "=IF(A1>0,B2,A2)");
    if incremental {
        model.enable_incremental_recalculation();
    }
    model.evaluate();
    model
}

fn assert_same(left: &crate::Model<'_>, right: &crate::Model<'_>) {
    for row in 1..=4 {
        for column in 1..=4 {
            assert_eq!(
                left.get_cell_value_by_index(0, row, column),
                right.get_cell_value_by_index(0, row, column)
            );
        }
    }
}

#[test]
fn oracle_lcg_500_edits_three_seeds() {
    for mut state in [1_u64, 17, 0xdead_beef] {
        let mut incremental = seeded_model(true);
        let mut full = seeded_model(false);
        for _ in 0..500 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let row = (state % 2 + 1) as i32;
            let value = ((state >> 16) % 100) as f64;
            incremental
                .update_cell_with_number(0, row, 1, value)
                .unwrap();
            full.update_cell_with_number(0, row, 1, value).unwrap();
            incremental.evaluate();
            full.evaluate();
            assert_same(&incremental, &full);
        }
    }
}

#[test]
fn unrelated_formula_is_not_run() {
    let mut model = seeded_model(true);
    model._set("D1", "=42");
    model.evaluate();
    model._set("A1", "3");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
    assert_eq!(model.evaluation.formulas_evaluated, 4);
}

#[test]
fn branch_changes_dependency() {
    let mut model = seeded_model(true);
    model._set("A1", "-1");
    model.evaluate();
    model._set("A2", "9");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
}

#[test]
fn formula_replaced_by_value_remains_correct() {
    let mut model = seeded_model(true);
    model._set("B1", "9");
    model.evaluate();
    model._set("A1", "8");
    model.evaluate();
    assert_eq!(
        model.get_cell_value_by_index(0, 1, 2).unwrap(),
        crate::cell::CellValue::Number(9.0)
    );
}

#[test]
fn paused_style_batch_is_accumulated_by_model() {
    let mut model = seeded_model(true);
    model._set("A1", "7");
    model._set("A2", "8");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
}

#[test]
fn hyperlinks_refuse_incremental() {
    let mut model = seeded_model(true);
    model._set("D1", "=HYPERLINK(\"https://example.com\",\"x\")");
    model.evaluate();
    model._set("A1", "5");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
}

#[test]
fn disabled_mode_stays_full() {
    let mut model = seeded_model(false);
    model._set("A1", "4");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
}

#[test]
fn invalidation_forces_then_rebuilds() {
    let mut model = seeded_model(true);
    model.invalidate_dependency_index();
    model._set("A1", "4");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model._set("A1", "5");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
}

fn full_oracle_after_edit(incremental: &crate::Model<'_>, full: &crate::Model<'_>) {
    assert_same(incremental, full);
}

#[test]
fn structural_operations_force_full_then_incremental() {
    let mut model = seeded_model(true);
    model.insert_rows(0, 2, 1).unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model._set("A1", "9");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
    model.insert_columns(0, 2, 1).unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model.delete_columns(0, 2, 1).unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model.delete_rows(0, 2, 1).unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
}

#[test]
fn sheet_name_merge_and_defined_name_mutations_force_full() {
    let mut model = seeded_model(true);
    model.add_sheet("Data").unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model.rename_sheet("Data", "Renamed").unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model.move_sheet(1, 0).unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model.delete_sheet(0).unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model
        .new_defined_name(DefinedNameEntry::new("input", "Sheet1!$A$1"))
        .unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model
        .update_defined_name("input", None, DefinedNameEntry::new("input", "Sheet1!$A$2"))
        .unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model.delete_defined_name("input", None).unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model
        .merge_cells(&Area {
            sheet: 0,
            row: 3,
            column: 1,
            width: 2,
            height: 1,
        })
        .unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
}

#[test]
fn cross_sheet_range_name_and_lookup_edits_match_full_oracle() {
    let mut inc = seeded_model(true);
    let mut full = seeded_model(false);
    for model in [&mut inc, &mut full] {
        model.add_sheet("Data").unwrap();
        model._set("Data!A1", "key");
        model._set("Data!B1", "2");
        model._set("Data!A2", "other");
        model._set("Data!B2", "3");
        model
            .new_defined_name(DefinedNameEntry::new("nums", "Data!$B$1:$B$2"))
            .unwrap();
        model._set("D1", "=SUM(nums)");
        model._set("D2", "=VLOOKUP(A1,Data!A1:B2,2,0)");
        model._set("D3", "=INDEX(Data!B1:B2,MATCH(A2,Data!A1:A2,0))");
        model._set("D4", "=SUMIF(Data!A1:A2,A1,Data!B1:B2)");
        model.evaluate();
    }
    inc._set("Data!B1", "7");
    full._set("Data!B1", "7");
    inc.evaluate();
    full.evaluate();
    full_oracle_after_edit(&inc, &full);
    inc._set("A1", "other");
    full._set("A1", "other");
    inc.evaluate();
    full.evaluate();
    full_oracle_after_edit(&inc, &full);
}

#[test]
fn cycle_and_dynamic_spill_refuse_then_recover() {
    let mut model = seeded_model(true);
    model._set("A1", "=B1");
    model._set("B1", "=A1");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model._set("A1", "1");
    model._set("B1", "=A1+1");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    model._set("A1", "2");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
    model._set("E1", "=SEQUENCE(2,1)");
    model.evaluate();
    model._set("A1", "3");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
}

#[test]
fn index_stays_bounded_for_real_formula_edits() {
    let mut model = seeded_model(true);
    for value in 0..10_000 {
        model._set("B1", &format!("=A1+{value}"));
        model.evaluate();
    }
    assert_eq!(model.dependency_index.reads_len(), 4);
}

macro_rules! incremental_smoke_cases {
    ($($name:ident: $formula:expr),* $(,)?) => {$(
        #[test]
        fn $name() {
            let mut model = seeded_model(true);
            model._set("D1", $formula);
            model.evaluate();
            model._set("A1", "6");
            model.evaluate();
            assert!(matches!(model.last_evaluation_path, EvaluationPath::Incremental | EvaluationPath::Full));
        }
    )*};
}

incremental_smoke_cases!(
    scalar_add: "=A1+1",
    scalar_multiply: "=A1*2",
    sum_range: "=SUM(A1:A2)",
    average_range: "=AVERAGE(A1:A2)",
    count_range: "=COUNT(A1:A2)",
    minimum_range: "=MIN(A1:A2)",
    maximum_range: "=MAX(A1:A2)",
    if_true: "=IF(A1>0,A2,0)",
    if_false: "=IF(A1<0,A2,0)",
    text_result: "=\"text\"",
    boolean_result: "=A1>0",
    div_zero_error: "=1/0",
    cross_formula: "=B2+1",
    nested_sum: "=SUM(B1:C2)",
    whole_column: "=SUM(A:A)",
    lookup_match: "=MATCH(A1,A1:A2,0)",
    index_lookup: "=INDEX(A1:A2,1)",
    choose_value: "=CHOOSE(1,A1,A2)",
    concatenate: "=A1&\"x\"",
    round_value: "=ROUND(A1/3,0)",
    abs_value: "=ABS(-A1)",
    power_value: "=A1^2",
);

#[test]
#[ignore]
fn incremental_benchmark_workload() {
    for rows in [10_000_i32, 100_000] {
        for incremental in [false, true] {
            let mut model = new_empty_model();
            for row in 1..=rows {
                model
                    .update_cell_with_number(0, row, 1, row as f64)
                    .unwrap();
                model
                    .update_cell_with_formula(0, row, 26, format!("=A{row}+B{row}*2"))
                    .unwrap();
            }
            model._set("AA1", &format!("=SUM(A1:A{rows})"));
            if incremental {
                model.enable_incremental_recalculation();
            }
            model.evaluate();
            let started = Instant::now();
            model.update_cell_with_number(0, rows / 2, 1, 7.0).unwrap();
            model.evaluate();
            eprintln!(
                "rows={rows} incremental={incremental} elapsed={:?} formulas={}",
                started.elapsed(),
                model.evaluation.formulas_evaluated
            );
        }
    }
}

fn assert_fresh(model: &crate::Model<'_>) {
    let mut oracle = crate::Model::from_bytes(&model.to_bytes(), "en").unwrap();
    oracle.evaluate();
    for (sheet, ws) in model.workbook.worksheets.iter().enumerate() {
        for (row, column, _) in ws.sheet_data.cells() {
            assert_eq!(
                model.get_cell_value_by_index(sheet as u32, row, column),
                oracle.get_cell_value_by_index(sheet as u32, row, column),
                "sheet {sheet} row {row} column {column}"
            );
        }
        for (row, column, _) in oracle.workbook.worksheets[sheet].sheet_data.cells() {
            assert_eq!(
                model.get_cell_value_by_index(sheet as u32, row, column),
                oracle.get_cell_value_by_index(sheet as u32, row, column)
            );
        }
    }
}

fn assert_all_cells_same(left: &crate::Model<'_>, right: &crate::Model<'_>) {
    assert_eq!(
        left.workbook.worksheets.len(),
        right.workbook.worksheets.len()
    );
    for sheet in 0..left.workbook.worksheets.len() as u32 {
        for (row, column, _) in left.workbook.worksheet(sheet).unwrap().sheet_data.cells() {
            assert_eq!(
                left.get_cell_value_by_index(sheet, row, column),
                right.get_cell_value_by_index(sheet, row, column),
                "sheet {sheet} row {row} column {column}"
            );
        }
        for (row, column, _) in right.workbook.worksheet(sheet).unwrap().sheet_data.cells() {
            assert_eq!(
                left.get_cell_value_by_index(sheet, row, column),
                right.get_cell_value_by_index(sheet, row, column),
                "sheet {sheet} row {row} column {column}"
            );
        }
    }
}

const LARGE_RECTANGLE_ROWS: i32 = 20_000;

fn large_rectangle_model(incremental: bool) -> crate::Model<'static> {
    let mut model = new_empty_model();
    for row in 1..=LARGE_RECTANGLE_ROWS {
        model
            .update_cell_with_number(0, row, 1, row as f64)
            .unwrap();
        model
            .update_cell_with_formula(
                0,
                row,
                2,
                format!("=A{row}/SUM($A$1:$A${LARGE_RECTANGLE_ROWS})"),
            )
            .unwrap();
    }
    if incremental {
        model.enable_incremental_recalculation();
    }
    model.evaluate();
    model
}

#[test]
fn rectangle_edge_guard_uses_full_evaluator_and_matches_oracle() {
    let mut incremental = large_rectangle_model(true);
    assert_eq!(
        incremental.dependency_index.rectangle_edges_in_sheet(0),
        LARGE_RECTANGLE_ROWS as usize
    );
    let mut oracle = crate::Model::from_bytes(&incremental.to_bytes(), "en").unwrap();
    incremental.update_cell_with_number(0, 1, 1, 7.0).unwrap();
    oracle.update_cell_with_number(0, 1, 1, 7.0).unwrap();
    let started = Instant::now();
    incremental.evaluate();
    let elapsed = started.elapsed();
    oracle.evaluate();
    eprintln!("rectangle guard rows={LARGE_RECTANGLE_ROWS} elapsed={elapsed:?}");
    assert_eq!(incremental.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(
        incremental.evaluation.formulas_evaluated,
        LARGE_RECTANGLE_ROWS as usize
    );
    assert_all_cells_same(&incremental, &oracle);
}

#[test]
fn small_two_formula_workload_stays_incremental() {
    let mut model = new_empty_model();
    model._set("A1", "1");
    model._set("B1", "=A1+1");
    model._set("C1", "=SUM(A1:A1)");
    model.enable_incremental_recalculation();
    model.evaluate();
    model._set("A1", "2");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
    assert_eq!(model.evaluation.formulas_evaluated, 2);
    assert_fresh(&model);
}

#[test]
fn dirty_set_guard_uses_full_evaluator_for_many_point_readers() {
    let formulas = 1_001;
    let mut model = new_empty_model();
    model._set("A1", "1");
    for row in 1..=formulas {
        model
            .update_cell_with_formula(0, row, 2, format!("=A1+{row}"))
            .unwrap();
    }
    model.enable_incremental_recalculation();
    model.evaluate();
    model._set("A1", "2");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(model.evaluation.formulas_evaluated, formulas as usize);
    assert_fresh(&model);
}

#[test]
#[ignore]
fn rectangle_edge_guard_timing_is_not_slower_than_full() {
    let mut full = large_rectangle_model(false);
    let mut incremental = large_rectangle_model(true);
    full.update_cell_with_number(0, 1, 1, 7.0).unwrap();
    incremental.update_cell_with_number(0, 1, 1, 7.0).unwrap();
    let full_started = Instant::now();
    full.evaluate();
    let full_elapsed = full_started.elapsed();
    let incremental_started = Instant::now();
    incremental.evaluate();
    let incremental_elapsed = incremental_started.elapsed();
    eprintln!(
        "rectangle guard timing rows={LARGE_RECTANGLE_ROWS} full={full_elapsed:?} incremental={incremental_elapsed:?}"
    );
    assert_eq!(incremental.last_evaluation_path, EvaluationPath::Full);
    assert_all_cells_same(&incremental, &full);
    assert!(incremental_elapsed <= full_elapsed.mul_f64(1.10));
}

#[test]
fn user_model_paused_history_and_external_diffs() {
    let mut user = crate::UserModel::from_model(seeded_model(true));
    user.pause_evaluation();
    user.set_user_input(0, 1, 1, "5").unwrap();
    user.set_user_input(0, 2, 1, "6").unwrap();
    user.evaluate();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Incremental);
    assert_eq!(user.model.evaluation.formulas_evaluated, 4);
    assert_fresh(&user.model);
    user.resume_evaluation();
    user.undo().unwrap();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 4);
    assert_fresh(&user.model);
    user.redo().unwrap();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 4);
    assert_fresh(&user.model);
    let mut remote = crate::UserModel::from_model(seeded_model(true));
    remote.set_user_input(0, 1, 1, "11").unwrap();
    user.apply_external_diffs(&remote.flush_send_queue())
        .unwrap();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 4);
    assert_fresh(&user.model);
}

#[test]
fn clipboard_cut_formula_full_then_incremental() {
    let mut user = crate::UserModel::from_model(seeded_model(true));
    user.set_selected_cell(1, 2).unwrap();
    user.set_selected_range(1, 2, 1, 2).unwrap();
    let clip = user.copy_to_clipboard().unwrap();
    user.set_selected_cell(3, 2).unwrap();
    user.paste_from_clipboard(0, (1, 2, 1, 2), &clip.data, true)
        .unwrap();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 4);
    assert_fresh(&user.model);
    user.set_user_input(0, 1, 1, "12").unwrap();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Incremental);
    assert_eq!(user.model.evaluation.formulas_evaluated, 4);
    assert_fresh(&user.model);
}

fn moving_formula_model(incremental: bool) -> (crate::Model<'static>, i32, i32) {
    let source_row = 20;
    let source_column = 8;
    let mut model = new_empty_model();
    for (row, column, value) in [
        (source_row, source_column, 1.0),
        (source_row, source_column + 1, 2.0),
        (source_row + 1, source_column, 3.0),
        (source_row + 1, source_column + 1, 4.0),
    ] {
        model
            .update_cell_with_number(0, row, column, value)
            .unwrap();
    }
    // Keep formula cells well to the right so repeated deletes inside H:I
    // move them without ever deleting them.
    model
        .update_cell_with_formula(0, source_row, source_column + 100, "=H20+I20".to_string())
        .unwrap();
    model
        .update_cell_with_formula(
            0,
            source_row,
            source_column + 101,
            "=SUM(H20:I21)".to_string(),
        )
        .unwrap();
    if incremental {
        model.enable_incremental_recalculation();
    }
    model.evaluate();
    (model, source_row, source_column)
}

fn apply_moving_structure(
    model: &mut crate::Model<'_>,
    operation: u64,
    source_row: &mut i32,
    source_column: &mut i32,
) {
    match operation % 8 {
        0 => {
            model.insert_rows(0, *source_row - 5, 1).unwrap();
            *source_row += 1;
        }
        1 if *source_row > 10 => {
            model.delete_rows(0, *source_row - 5, 1).unwrap();
            *source_row -= 1;
        }
        2 => model.insert_rows(0, *source_row + 1, 1).unwrap(),
        3 => model.delete_rows(0, *source_row + 1, 1).unwrap(),
        4 => {
            model.insert_columns(0, *source_column - 3, 1).unwrap();
            *source_column += 1;
        }
        5 if *source_column > 5 => {
            model.delete_columns(0, *source_column - 3, 1).unwrap();
            *source_column -= 1;
        }
        6 => model.insert_columns(0, *source_column + 1, 1).unwrap(),
        _ => model.delete_columns(0, *source_column + 1, 1).unwrap(),
    }
}

#[test]
fn randomized_structural_and_value_edits_match_fresh() {
    for mut state in [1_u64, 17, 0xdead_beef, 0x1234_5678_9abc_def0] {
        let (mut model, mut source_row, mut source_column) = moving_formula_model(true);
        for step in 0..300 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            if step % 3 == 0 {
                apply_moving_structure(
                    &mut model,
                    state >> 32,
                    &mut source_row,
                    &mut source_column,
                );
                model.evaluate();
                assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
            } else {
                model
                    .update_cell_with_number(
                        0,
                        source_row,
                        source_column,
                        ((state >> 16) % 100) as f64,
                    )
                    .unwrap();
                model.evaluate();
                assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
                assert!(model.evaluation.formulas_evaluated <= 2);
            }
            assert_fresh(&model);
        }
    }
}

#[test]
fn mode_off_matches_plain_full_evaluation_for_moving_random_sequence() {
    for mut state in [3_u64, 29, 0xfeed_beef, 0x1010_0101] {
        let (mut mode_off, mut source_row, mut source_column) = moving_formula_model(false);
        let (mut stage_one, mut oracle_row, mut oracle_column) = moving_formula_model(false);
        for step in 0..300 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            if step % 3 == 0 {
                apply_moving_structure(
                    &mut mode_off,
                    state >> 32,
                    &mut source_row,
                    &mut source_column,
                );
                apply_moving_structure(
                    &mut stage_one,
                    state >> 32,
                    &mut oracle_row,
                    &mut oracle_column,
                );
            } else {
                let value = ((state >> 16) % 100) as f64;
                mode_off
                    .update_cell_with_number(0, source_row, source_column, value)
                    .unwrap();
                stage_one
                    .update_cell_with_number(0, oracle_row, oracle_column, value)
                    .unwrap();
            }
            mode_off.evaluate();
            stage_one.evaluate();
            assert_eq!(mode_off.last_evaluation_path, EvaluationPath::Full);
            assert_eq!(stage_one.last_evaluation_path, EvaluationPath::Full);
            assert_all_cells_same(&mode_off, &stage_one);
        }
    }
}

#[test]
fn clear_all_and_merge_invalidate_dependencies() {
    let mut model = seeded_model(true);
    model
        .range_clear_all(&Area {
            sheet: 0,
            row: 1,
            column: 1,
            width: 1,
            height: 1,
        })
        .unwrap();
    model._set("A2", "3");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(model.evaluation.formulas_evaluated, 4);
    assert_fresh(&model);
    model
        .merge_cells(&Area {
            sheet: 0,
            row: 3,
            column: 1,
            width: 2,
            height: 1,
        })
        .unwrap();
    model._set("A2", "4");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(model.evaluation.formulas_evaluated, 4);
    assert_fresh(&model);
}

#[test]
fn introducing_dynamic_formula_forces_full_immediately() {
    let mut model = seeded_model(true);
    model._set("E1", "=SEQUENCE(2,1)");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(model.evaluation.formulas_evaluated, 5);
    assert_fresh(&model);
}

#[test]
fn duplicate_sheet_and_autofill_force_full() {
    let mut user = crate::UserModel::from_model(seeded_model(true));
    user.model.duplicate_sheet(0).unwrap();
    user.evaluate();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 8);
    assert_fresh(&user.model);
    user.model.new_sheet();
    user.evaluate();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 8);
    assert_fresh(&user.model);
    user.auto_fill_rows(
        &Area {
            sheet: 0,
            row: 1,
            column: 1,
            width: 1,
            height: 1,
        },
        2,
    )
    .unwrap();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 8);
    assert_fresh(&user.model);
}

#[test]
fn fixed_array_spill_readers_force_full() {
    let mut model = new_empty_model();
    model._set("A1", "2");
    model
        .set_user_array_formula(0, 1, 2, 1, 2, "=A1*2")
        .unwrap();
    model._set("C1", "=B2+1");
    model.enable_incremental_recalculation();
    model.evaluate();
    model._set("A1", "5");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(model.evaluation.formulas_evaluated, 2);
    assert_fresh(&model);
}

#[test]
fn outstanding_unwind_forces_full() {
    let mut model = seeded_model(true);
    model._set("A1", "7");
    model.evaluation.unwinding = true;
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(model.evaluation.formulas_evaluated, 4);
    assert_fresh(&model);
}

#[test]
fn workbook_rename_invalidates_cell_filename() {
    let mut model = seeded_model(true);
    model._set("D1", "=CELL(\"filename\")");
    model.evaluate();
    let mut user = crate::UserModel::from_model(model);
    user.pause_evaluation();
    user.set_user_input(0, 1, 1, "7").unwrap();
    user.set_name("Renamed");
    user.evaluate();
    assert_eq!(user.model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(user.model.evaluation.formulas_evaluated, 5);
    assert_fresh(&user.model);
}

#[test]
fn locale_change_forces_full_evaluation_and_preserves_values() {
    let mut model = seeded_model(true);
    model.update_cell_with_number(0, 1, 1, 7.0).unwrap();
    model.set_locale("fr").unwrap();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(
        model.get_cell_value_by_index(0, 1, 2).unwrap(),
        crate::cell::CellValue::Number(9.0)
    );
    assert_fresh(&model);
}

#[test]
fn timezone_change_forces_full_evaluation_and_preserves_values() {
    let mut model = seeded_model(true);
    model.update_cell_with_number(0, 1, 1, 7.0).unwrap();
    model.set_timezone("America/Panama").unwrap();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(
        model.get_cell_value_by_index(0, 1, 2).unwrap(),
        crate::cell::CellValue::Number(9.0)
    );
    assert_fresh(&model);
}

#[test]
fn language_change_forces_next_evaluation_full_and_preserves_values() {
    let mut model = seeded_model(true);
    model.update_cell_with_number(0, 1, 1, 7.0).unwrap();
    model.set_language("fr").unwrap();
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert_eq!(
        model.get_cell_value_by_index(0, 1, 2).unwrap(),
        crate::cell::CellValue::Number(9.0)
    );
    assert_fresh(&model);
}

#[test]
fn injected_incremental_abort_rebuilds_a_valid_index() {
    let mut model = seeded_model(true);
    model._set("A1", "7");
    model.evaluation.fail_incremental_pass_after_formula = true;
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Full);
    assert!(model.dependency_index.valid);
    assert_fresh(&model);
    model._set("A1", "8");
    model.evaluate();
    assert_eq!(model.last_evaluation_path, EvaluationPath::Incremental);
    assert_eq!(model.evaluation.formulas_evaluated, 4);
    assert_fresh(&model);
}
