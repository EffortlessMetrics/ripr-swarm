//! Admission gate: every Python test framework the adapter classifies tests
//! under is bound to the language capability table.
//!
//! `ordinal` is an exhaustive match, so adding a `PythonFramework` variant
//! fails to compile until it is classified here (the production consumers in
//! `related_tests.rs` are exhaustive too). The checker then requires a
//! `python/<framework>/diff/repair_routing` cell per variant and rejects any
//! Python framework-specific cell that no variant names.
//!
//! Limit: Rust cannot enumerate enum variants, so `ALL_FRAMEWORKS` is a
//! second hand-kept list. The exhaustive matches force classification, and
//! the ordinal check catches duplicates, gaps and a stale count, but a
//! variant classified in `ordinal` yet left out of both `FRAMEWORK_COUNT` and
//! `ALL_FRAMEWORKS` is not checked against the table.

use super::PythonFramework;
use crate::domain::LanguageId;
use crate::domain::language_capability::{
    AnalysisMode, CAPABILITY_CELLS, CapabilityCell, CapabilityFamily, FRAMEWORK_ANY, cells_for,
};

/// Hand-kept variant count. It must equal the length of `ALL_FRAMEWORKS` and
/// the ordinal range of its entries; nothing ties it to the enum itself.
const FRAMEWORK_COUNT: usize = 2;

const ALL_FRAMEWORKS: [PythonFramework; FRAMEWORK_COUNT] =
    [PythonFramework::Pytest, PythonFramework::Unittest];

fn ordinal(framework: PythonFramework) -> usize {
    match framework {
        PythonFramework::Pytest => 0,
        PythonFramework::Unittest => 1,
    }
}

/// Every way the framework names disagree with the table, one sentence each.
fn framework_violations(frameworks: &[&str], cells: &[CapabilityCell]) -> Vec<String> {
    let python_cells: Vec<&CapabilityCell> = cells
        .iter()
        .filter(|cell| cell.language == LanguageId::Python)
        .collect();
    let mut violations = Vec::new();
    for framework in frameworks {
        let routed = python_cells.iter().any(|cell| {
            cell.framework == *framework
                && cell.mode == AnalysisMode::Diff
                && cell.family == CapabilityFamily::RepairRouting
        });
        if !routed {
            violations.push(format!(
                "{framework} has no python/{framework}/diff/repair_routing cell"
            ));
        }
    }
    for cell in &python_cells {
        if cell.framework != FRAMEWORK_ANY && !frameworks.contains(&cell.framework) {
            violations.push(format!(
                "cell {} names a framework no PythonFramework variant carries",
                cell.framework
            ));
        }
    }
    violations
}

fn current_names() -> Vec<&'static str> {
    ALL_FRAMEWORKS
        .iter()
        .map(|framework| framework.as_str())
        .collect()
}

#[test]
fn the_framework_list_is_a_gap_free_ordinal_sequence() {
    let ordinals: Vec<usize> = ALL_FRAMEWORKS.iter().copied().map(ordinal).collect();
    assert_eq!(ordinals, (0..FRAMEWORK_COUNT).collect::<Vec<_>>());
}

#[test]
fn every_framework_agrees_with_the_capability_table() {
    assert_eq!(
        framework_violations(&current_names(), CAPABILITY_CELLS),
        Vec::<String>::new()
    );
    assert!(cells_for(LanguageId::Python).next().is_some());
}

#[test]
fn a_framework_without_a_repair_routing_cell_is_drift() {
    let mut names = current_names();
    names.push("nose");
    let violations = framework_violations(&names, CAPABILITY_CELLS);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("nose") && v.contains("repair_routing")),
        "{violations:?}"
    );
}

#[test]
fn a_missing_repair_routing_cell_for_an_existing_framework_is_drift() {
    let cells: Vec<CapabilityCell> = CAPABILITY_CELLS
        .iter()
        .filter(|cell| {
            !(cell.language == LanguageId::Python
                && cell.framework == "unittest"
                && cell.mode == AnalysisMode::Diff
                && cell.family == CapabilityFamily::RepairRouting)
        })
        .copied()
        .collect();
    let violations = framework_violations(&current_names(), &cells);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("unittest") && v.contains("repair_routing")),
        "{violations:?}"
    );
}

#[test]
fn a_framework_specific_cell_without_a_variant_is_drift() {
    let mut cells = CAPABILITY_CELLS.to_vec();
    let mut extra = cells[0];
    extra.language = LanguageId::Python;
    extra.framework = "nose";
    cells.push(extra);
    let violations = framework_violations(&current_names(), &cells);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("nose") && v.contains("no PythonFramework variant")),
        "{violations:?}"
    );
}

#[test]
fn another_languages_framework_cell_is_not_a_python_violation() {
    let mut cells = CAPABILITY_CELLS.to_vec();
    let mut extra = cells[0];
    extra.language = LanguageId::TypeScript;
    extra.framework = "vitest";
    cells.push(extra);
    assert_eq!(
        framework_violations(&current_names(), &cells),
        Vec::<String>::new()
    );
}
