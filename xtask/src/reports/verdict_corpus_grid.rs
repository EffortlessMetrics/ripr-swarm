//! Mutation-operator grid and false-credit trap scoreboard of the labeled
//! verdict corpora (RIPR-SPEC-0219).
//!
//! The grid crosses the operator classes ripr reasons about (boundary,
//! equality, logical, arithmetic, constant, return value, deletion) with the
//! three states a runtime-mutant label can take for them: a test that kills
//! every listed mutant (`credited`), one that kills none (`gap`) and one that
//! kills some (`partial`, the case where neither full credit nor a plain gap
//! is the honest reading, so `limited` is acceptable). A cell is covered when
//! at least one case lands in it; the floor in `spec-coverage.toml` keeps
//! covered cells from silently disappearing, and a waiver names a cell no
//! case can fill. Beside the grid sits the false-credit trap table: cases from
//! a `*trap*` subject whose test looks like it checks the change but cannot
//! notice it, scored by how often ripr credits them, with the paired controls
//! that do notice it scored by how often ripr still credits them.
//!
//! The grid says which operator-by-outcome cells carry runtime-labeled cases
//! and how ripr scored them. It does not measure a corpus-wide rate for any
//! language: authored cases are chosen to fill cells.

use super::{CaseRow, Outcome, TruthState, Verdict};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The operator classes, in report order.
pub(crate) const OPERATORS: [&str; 7] = [
    "boundary",
    "equality",
    "logical",
    "arithmetic",
    "constant",
    "return_value",
    "deletion",
];

/// The truth states a cell distinguishes, in report order.
pub(crate) const STATES: [&str; 3] = ["credited", "gap", "partial"];

/// The operator class a `behavior_family` belongs to, or `None` for a family
/// the grid does not score (error paths, match arms, field construction and
/// the like have their own cells in the spec-example ledger).
pub(crate) fn operator_of(family: &str) -> Option<&'static str> {
    match family {
        "predicate_boundary"
        | "predicate"
        | "result_threshold_predicate"
        | "error_guard_boundary"
        | "predicate_side_flip" => Some("boundary"),
        "predicate_equality" => Some("equality"),
        "predicate_boolean" => Some("logical"),
        "return_value_arithmetic" | "arithmetic_operator" => Some("arithmetic"),
        "arithmetic_constant"
        | "lookup_constant"
        | "behavior_selector_const"
        | "default_value"
        | "parameter_default" => Some("constant"),
        "return_value"
        | "scalar_return_value"
        | "value_return"
        | "ok_return"
        | "ok_value"
        | "option_return"
        | "return_value_selection"
        | "return_value_format"
        | "format_string"
        | "output_text" => Some("return_value"),
        "call_deletion"
        | "side_effect"
        | "side_effect_only"
        | "side_effect_push"
        | "side_effect_field_write"
        | "state_update"
        | "field_collection_push"
        | "field_compound_assign" => Some("deletion"),
        _ => None,
    }
}

fn state_of(truth: TruthState) -> &'static str {
    match truth {
        TruthState::Discriminated => "credited",
        TruthState::NotDiscriminated => "gap",
        TruthState::PartiallyDiscriminated => "partial",
    }
}

/// A trap subject is named for its job (`authored-trap-kit`,
/// `authored-py-trap-kit`, ...).
fn is_trap_subject(subject_id: &str) -> bool {
    subject_id.contains("trap")
}

/// A trap is named for its job in its subject or in its case id, because a
/// trap added to a spec-owned subject (`trap-iter-empty-slice`) lives in a
/// subject that carries no such name.
fn is_trap_row(row: &CaseRow) -> bool {
    is_trap_subject(&row.subject_id) || row.case_id.contains("trap")
}

/// The `[grid]` table of `spec-coverage.toml`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GridGate {
    /// Covered, unwaived cells the gate protects.
    pub(crate) cell_floor: usize,
    /// Trap cases (a trap subject's cases the tests cannot discriminate) the
    /// gate protects.
    pub(crate) trap_floor: usize,
    /// Trap cases ripr may credit before the gate fails.
    pub(crate) false_credit_ceiling: usize,
    /// Cases whose `behavior_family` names a different operator than the one
    /// they edit (`config-duplicate-key-case` edits an equality inside a
    /// `predicate` family), each mapped to its true operator, or `none` for
    /// an edit the grid does not score (bitwise).
    #[serde(default)]
    pub(crate) operators: BTreeMap<String, String>,
    /// Cells no case can fill, each with the reason.
    #[serde(default)]
    pub(crate) waived: Vec<GridWaiver>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GridWaiver {
    pub(crate) operator: String,
    pub(crate) state: String,
    pub(crate) reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GridCell {
    pub(crate) operator: String,
    pub(crate) state: String,
    pub(crate) cases: usize,
    pub(crate) ideal: usize,
    pub(crate) abstained: usize,
    pub(crate) false_verdicts: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) waived: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub(crate) struct TrapSummary {
    /// Cases from a trap subject whose test cannot discriminate the change.
    pub(crate) traps: usize,
    /// Of those, the ones ripr credits: a false credit.
    pub(crate) credited: usize,
    pub(crate) gapped: usize,
    pub(crate) abstained: usize,
    pub(crate) silent: usize,
    /// Cases from a trap subject whose test does notice the change.
    pub(crate) controls: usize,
    /// Of those, the ones ripr credits.
    pub(crate) controls_credited: usize,
    /// Controls ripr calls a gap: a false actionable.
    pub(crate) controls_gapped: usize,
    pub(crate) controls_abstained: usize,
    pub(crate) controls_silent: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct GridReport {
    pub(crate) ledger: String,
    pub(crate) cells: Vec<GridCell>,
    /// Unwaived cells with at least one case.
    pub(crate) covered_cells: usize,
    /// Unwaived cells.
    pub(crate) denominator: usize,
    pub(crate) waived_cells: usize,
    /// `operator/state` of each unwaived cell no case fills.
    pub(crate) uncovered: Vec<String>,
    /// Waivers naming a cell that now has a case, or one the grid lacks.
    pub(crate) stale_waivers: Vec<String>,
    pub(crate) traps: TrapSummary,
    /// Armed only for a whole-corpus run, where counts are comparable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) cell_floor: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) trap_floor: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) false_credit_ceiling: Option<usize>,
}

/// Build the grid and trap tables from scored rows. `gate` is `None` for a
/// corpus without a `[grid]` table; `subset` disarms the floors and ceiling
/// for a `--cases` run, whose counts are not whole-corpus counts.
pub(crate) fn grid_report(
    rows: &[CaseRow],
    gate: &GridGate,
    ledger: String,
    subset: bool,
) -> GridReport {
    let mut counts: BTreeMap<(&str, &str), [usize; 4]> = BTreeMap::new();
    let mut traps = TrapSummary::default();
    for row in rows {
        let operator = match gate.operators.get(&row.case_id).map(String::as_str) {
            Some("none") => None,
            Some(named) => OPERATORS.iter().copied().find(|o| *o == named),
            None => operator_of(&row.behavior_family),
        };
        if let Some(operator) = operator {
            let slot = counts
                .entry((operator, state_of(row.truth)))
                .or_insert([0; 4]);
            slot[0] += 1;
            match row.outcome {
                Outcome::Ideal => slot[1] += 1,
                Outcome::Abstained => slot[2] += 1,
                _ => slot[3] += 1,
            }
        }
        if is_trap_row(row) {
            if row.truth == TruthState::Discriminated {
                traps.controls += 1;
                match row.observed_verdict {
                    Verdict::Credited => traps.controls_credited += 1,
                    Verdict::Gap => traps.controls_gapped += 1,
                    Verdict::Limited => traps.controls_abstained += 1,
                    Verdict::Silent => traps.controls_silent += 1,
                }
            } else {
                traps.traps += 1;
                match row.observed_verdict {
                    Verdict::Credited => traps.credited += 1,
                    Verdict::Gap => traps.gapped += 1,
                    Verdict::Limited => traps.abstained += 1,
                    Verdict::Silent => traps.silent += 1,
                }
            }
        }
    }
    let waiver_of = |operator: &str, state: &str| {
        gate.waived
            .iter()
            .find(|w| w.operator == operator && w.state == state)
            .map(|w| w.reason.clone())
    };
    let mut cells = Vec::new();
    let mut covered_cells = 0;
    let mut denominator = 0;
    let mut waived_cells = 0;
    let mut uncovered = Vec::new();
    let mut stale_waivers = Vec::new();
    for operator in OPERATORS {
        for state in STATES {
            let [cases, ideal, abstained, false_verdicts] =
                counts.get(&(operator, state)).copied().unwrap_or([0; 4]);
            let waived = waiver_of(operator, state);
            match (&waived, cases) {
                (Some(_), 0) => waived_cells += 1,
                (Some(_), _) => {
                    waived_cells += 1;
                    stale_waivers.push(format!(
                        "{operator}/{state} is waived but has {cases} case(s); drop the waiver"
                    ));
                }
                (None, 0) => {
                    denominator += 1;
                    uncovered.push(format!("{operator}/{state}"));
                }
                (None, _) => {
                    denominator += 1;
                    covered_cells += 1;
                }
            }
            cells.push(GridCell {
                operator: operator.to_string(),
                state: state.to_string(),
                cases,
                ideal,
                abstained,
                false_verdicts,
                waived,
            });
        }
    }
    for waiver in &gate.waived {
        if !OPERATORS.contains(&waiver.operator.as_str())
            || !STATES.contains(&waiver.state.as_str())
        {
            stale_waivers.push(format!(
                "waiver `{}/{}` names a cell the grid does not have",
                waiver.operator, waiver.state
            ));
        }
    }
    for (case, operator) in &gate.operators {
        let known = operator == "none" || OPERATORS.contains(&operator.as_str());
        if !known || !rows.iter().any(|row| &row.case_id == case) {
            stale_waivers.push(format!(
                "operator override `{case} = {operator}` names an unknown operator or a case the corpus lacks"
            ));
        }
    }
    GridReport {
        ledger,
        cells,
        covered_cells,
        denominator,
        waived_cells,
        uncovered,
        stale_waivers,
        traps,
        cell_floor: (!subset).then_some(gate.cell_floor),
        trap_floor: (!subset).then_some(gate.trap_floor),
        false_credit_ceiling: (!subset).then_some(gate.false_credit_ceiling),
    }
}

/// The grid gate. Fewer covered cells or trap cases than the floors, more
/// credited traps than the ceiling, or a stale waiver fails with what to do;
/// improvement passes and says which committed number to tighten.
pub(crate) fn grid_gate(grid: &GridReport) -> Result<Option<String>, String> {
    let (Some(cell_floor), Some(trap_floor), Some(ceiling)) =
        (grid.cell_floor, grid.trap_floor, grid.false_credit_ceiling)
    else {
        return Ok(None);
    };
    let ledger = &grid.ledger;
    let mut failures = Vec::new();
    if !grid.stale_waivers.is_empty() {
        failures.push(format!(
            "verdict-corpus: stale grid waivers in {ledger}: {}",
            grid.stale_waivers.join("; ")
        ));
    }
    if grid.covered_cells < cell_floor {
        failures.push(format!(
            "verdict-corpus: grid cells covered fell to {}/{} below the floor {cell_floor} in {ledger}; uncovered: {}. Restore the case that filled the cell, or lower `cell_floor` with the reason in the PR when a case was deliberately retired",
            grid.covered_cells,
            grid.denominator,
            grid.uncovered.join(", ")
        ));
    }
    if grid.traps.traps < trap_floor {
        failures.push(format!(
            "verdict-corpus: trap cases fell to {} below the floor {trap_floor} in {ledger}; restore the trap case, or lower `trap_floor` with the reason in the PR when one was deliberately retired",
            grid.traps.traps
        ));
    }
    if grid.traps.credited > ceiling {
        failures.push(format!(
            "verdict-corpus: false-credited traps rose to {}/{} above the ceiling {ceiling} in {ledger}; ripr now credits a test that cannot notice the change (the report's trap table and rows name each case), so restore the reading or fix the case the credit came from",
            grid.traps.credited, grid.traps.traps
        ));
    }
    if !failures.is_empty() {
        return Err(failures.join("\n"));
    }
    let mut notes = Vec::new();
    if grid.covered_cells > cell_floor {
        notes.push(format!(
            "verdict-corpus: grid cells covered {}/{} are above the floor {cell_floor}; raise `cell_floor` in {ledger} to {} to protect them",
            grid.covered_cells, grid.denominator, grid.covered_cells
        ));
    }
    if grid.traps.traps > trap_floor {
        notes.push(format!(
            "verdict-corpus: trap cases {} are above the floor {trap_floor}; raise `trap_floor` in {ledger} to {} to protect them",
            grid.traps.traps, grid.traps.traps
        ));
    }
    if grid.traps.credited < ceiling {
        notes.push(format!(
            "verdict-corpus: false-credited traps {}/{} are below the ceiling {ceiling}; lower `false_credit_ceiling` in {ledger} to {} to protect the improvement",
            grid.traps.credited, grid.traps.traps, grid.traps.credited
        ));
    }
    Ok((!notes.is_empty()).then(|| notes.join("\n")))
}

pub(crate) fn render_grid_markdown(grid: &GridReport) -> String {
    let mut out = String::new();
    out.push_str("\n## Operator grid\n\n");
    out.push_str(&format!(
        "Operator classes crossed with the runtime-mutant truth of the cases that land in them: `credited` (every listed mutant fails a test), `gap` (none does) and `partial` (some do). Covered {}/{} unwaived cells ({}); floor {}. Waived {}.\n",
        grid.covered_cells,
        grid.denominator,
        grid.ledger,
        grid.cell_floor
            .map_or_else(|| "not armed".to_string(), |f| f.to_string()),
        grid.waived_cells,
    ));
    out.push_str("\nEach cell reads `cases (ideal / abstained / false)`.\n");
    out.push_str("\n| Operator | credited | gap | partial |\n| --- | --- | --- | --- |\n");
    for operator in OPERATORS {
        let mut line = format!("| {operator} |");
        for state in STATES {
            let cell = grid
                .cells
                .iter()
                .find(|c| c.operator == operator && c.state == state);
            let text = match cell {
                Some(c) if c.waived.is_some() && c.cases == 0 => "waived".to_string(),
                Some(c) if c.cases == 0 => "**uncovered**".to_string(),
                Some(c) => format!(
                    "{} ({} / {} / {})",
                    c.cases, c.ideal, c.abstained, c.false_verdicts
                ),
                None => "-".to_string(),
            };
            line.push_str(&format!(" {text} |"));
        }
        out.push_str(&line);
        out.push('\n');
    }
    for cell in grid.cells.iter().filter(|c| c.waived.is_some()) {
        out.push_str(&format!(
            "\n- waived `{}/{}`: {}\n",
            cell.operator,
            cell.state,
            cell.waived.as_deref().unwrap_or_default()
        ));
    }
    let t = &grid.traps;
    out.push_str("\n## False-credit traps\n\n");
    out.push_str(&format!(
        "Cases from a trap subject. A trap is a test that looks like it checks the change but cannot notice it; ripr should not credit it. A control is the paired test that does notice it. Trap floor {}; false-credit ceiling {}.\n",
        grid.trap_floor
            .map_or_else(|| "not armed".to_string(), |f| f.to_string()),
        grid.false_credit_ceiling
            .map_or_else(|| "not armed".to_string(), |f| f.to_string()),
    ));
    out.push_str("\n| Kind | Cases | Credited | Gap | Limited | Silent |\n| --- | --- | --- | --- | --- | --- |\n");
    out.push_str(&format!(
        "| trap | {} | {} | {} | {} | {} |\n",
        t.traps, t.credited, t.gapped, t.abstained, t.silent
    ));
    out.push_str(&format!(
        "| control | {} | {} | {} | {} | {} |\n",
        t.controls, t.controls_credited, t.controls_gapped, t.controls_abstained, t.controls_silent
    ));
    out
}

#[cfg(test)]
#[path = "verdict_corpus_grid_tests.rs"]
mod tests;
