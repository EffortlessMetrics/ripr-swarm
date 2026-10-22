//! Admission gate: every test framework package discovery can report carries
//! an explicit disposition against the language capability table.
//!
//! `disposition` and `ordinal` are exhaustive matches, so adding a
//! `TsFramework` variant fails to compile until it is classified here. A
//! framework is either governed by the framework-agnostic `any` cells or has
//! its own framework-specific cell; the checker rejects a disposition that
//! contradicts the table.
//!
//! Limit: Rust cannot enumerate enum variants, so `ALL_FRAMEWORKS` is a
//! second hand-kept list. The exhaustive matches force classification, and
//! the ordinal check catches duplicates, gaps and a stale count, but a
//! variant classified in `ordinal` and `disposition` yet left out of both
//! `FRAMEWORK_COUNT` and `ALL_FRAMEWORKS` is not checked against the table.

use super::package::TsFramework;
use crate::domain::LanguageId;
use crate::domain::language_capability::{
    CAPABILITY_CELLS, CapabilityCell, FRAMEWORK_ANY, cells_for,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disposition {
    /// No framework-specific cell: the `any` cells govern this framework.
    LanguageWide,
    /// A cell names this framework.
    FrameworkCell,
}

/// Number of `TsFramework` variants. `ordinal` must stay below it, so a new
/// variant forces this bump, which in turn forces `ALL_FRAMEWORKS` to grow.
const FRAMEWORK_COUNT: usize = 6;

const ALL_FRAMEWORKS: [TsFramework; FRAMEWORK_COUNT] = [
    TsFramework::Jest,
    TsFramework::Vitest,
    TsFramework::Bun,
    TsFramework::Mocha,
    TsFramework::NodeTest,
    TsFramework::Ava,
];

fn ordinal(framework: TsFramework) -> usize {
    match framework {
        TsFramework::Jest => 0,
        TsFramework::Vitest => 1,
        TsFramework::Bun => 2,
        TsFramework::Mocha => 3,
        TsFramework::NodeTest => 4,
        TsFramework::Ava => 5,
    }
}

fn disposition(framework: TsFramework) -> Disposition {
    match framework {
        TsFramework::Jest
        | TsFramework::Vitest
        | TsFramework::Bun
        | TsFramework::Mocha
        | TsFramework::NodeTest
        | TsFramework::Ava => Disposition::LanguageWide,
    }
}

/// Every way a disposition contradicts the table, one sentence each.
fn disposition_violations(
    entries: &[(&str, Disposition)],
    cells: &[CapabilityCell],
    language: LanguageId,
) -> Vec<String> {
    let language_cells: Vec<&CapabilityCell> = cells
        .iter()
        .filter(|cell| cell.language == language)
        .collect();
    let mut violations = Vec::new();
    for (framework, disposition) in entries {
        let named = language_cells
            .iter()
            .any(|cell| cell.framework == *framework);
        match disposition {
            Disposition::LanguageWide => {
                if named {
                    violations.push(format!(
                        "{framework} is classified language-wide but a cell names it"
                    ));
                }
                if !language_cells
                    .iter()
                    .any(|cell| cell.framework == FRAMEWORK_ANY)
                {
                    violations.push(format!(
                        "{framework} is classified language-wide but {} has no `any` cell",
                        language.as_str()
                    ));
                }
            }
            Disposition::FrameworkCell => {
                if !named {
                    violations.push(format!(
                        "{framework} is classified as having its own cell but none names it"
                    ));
                }
            }
        }
    }
    violations
}

fn current_entries() -> Vec<(&'static str, Disposition)> {
    ALL_FRAMEWORKS
        .iter()
        .map(|framework| (framework.as_str(), disposition(*framework)))
        .collect()
}

#[test]
fn the_framework_list_covers_every_variant() {
    let ordinals: Vec<usize> = ALL_FRAMEWORKS.iter().copied().map(ordinal).collect();
    assert_eq!(ordinals, (0..FRAMEWORK_COUNT).collect::<Vec<_>>());
}

#[test]
fn every_typescript_framework_agrees_with_the_capability_table() {
    assert_eq!(
        disposition_violations(&current_entries(), CAPABILITY_CELLS, LanguageId::TypeScript),
        Vec::<String>::new()
    );
    assert!(cells_for(LanguageId::TypeScript).next().is_some());
}

#[test]
fn a_framework_specific_cell_makes_a_language_wide_disposition_drift() {
    let mut cells = CAPABILITY_CELLS.to_vec();
    let mut vitest = cells[0];
    vitest.language = LanguageId::TypeScript;
    vitest.framework = "vitest";
    cells.push(vitest);
    let violations = disposition_violations(&current_entries(), &cells, LanguageId::TypeScript);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("vitest") && v.contains("a cell names it")),
        "{violations:?}"
    );
}

#[test]
fn a_framework_claiming_its_own_cell_without_one_is_drift() {
    let entries = [("ava", Disposition::FrameworkCell)];
    let violations = disposition_violations(&entries, CAPABILITY_CELLS, LanguageId::TypeScript);
    assert!(
        violations.iter().any(|v| v.contains("none names it")),
        "{violations:?}"
    );
}

#[test]
fn a_language_wide_disposition_needs_an_any_cell() {
    let cells: Vec<CapabilityCell> = CAPABILITY_CELLS
        .iter()
        .filter(|cell| {
            !(cell.language == LanguageId::TypeScript && cell.framework == FRAMEWORK_ANY)
        })
        .copied()
        .collect();
    let violations = disposition_violations(&current_entries(), &cells, LanguageId::TypeScript);
    assert!(
        violations.iter().any(|v| v.contains("has no `any` cell")),
        "{violations:?}"
    );
}
