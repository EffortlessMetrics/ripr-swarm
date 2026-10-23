//! Admission gate: every Perl test framework the fact packet can name carries
//! an explicit disposition against the language capability table.
//!
//! The Perl cells are all framework-agnostic (`any`) and `not_proven`, while
//! the repair gate accepts six of the seven frameworks as strictly
//! actionable. This gate pins that relationship so it cannot change by
//! accident: a new framework must be classified here, and promoting the
//! repair-routing cell past `not_proven` is a deliberate edit of this file.
//!
//! Limit: Rust cannot enumerate enum variants, so `ALL_FRAMEWORKS` is a
//! hand-kept list. The exhaustive matches force classification and the
//! ordinal check catches duplicates, gaps and a stale count, but a variant
//! classified in the matches yet left out of both `FRAMEWORK_COUNT` and
//! `ALL_FRAMEWORKS` is not checked.

use super::TestFramework;
use crate::domain::LanguageId;
use crate::domain::language_capability::{
    AnalysisMode, CAPABILITY_CELLS, CapabilityCell, CapabilityFamily, FRAMEWORK_ANY, SupportState,
    TierCeiling, cells_for,
};

/// Hand-kept variant count: it must equal the length of `ALL_FRAMEWORKS` and
/// the ordinal range of its entries; nothing ties it to the enum itself.
const FRAMEWORK_COUNT: usize = 7;

const ALL_FRAMEWORKS: [TestFramework; FRAMEWORK_COUNT] = [
    TestFramework::TestMore,
    TestFramework::Test2V0,
    TestFramework::Test2V1,
    TestFramework::Test2Suite,
    TestFramework::TestException,
    TestFramework::TestFatal,
    TestFramework::Unknown,
];

fn ordinal(framework: TestFramework) -> usize {
    match framework {
        TestFramework::TestMore => 0,
        TestFramework::Test2V0 => 1,
        TestFramework::Test2V1 => 2,
        TestFramework::Test2Suite => 3,
        TestFramework::TestException => 4,
        TestFramework::TestFatal => 5,
        TestFramework::Unknown => 6,
    }
}

/// Whether the repair gate treats the framework as strictly actionable.
/// Exhaustive, so a new variant must be decided here rather than silently
/// defaulting to `false` the way the production `matches!` would.
fn expected_strict_actionability(framework: TestFramework) -> bool {
    match framework {
        TestFramework::TestMore
        | TestFramework::Test2V0
        | TestFramework::Test2V1
        | TestFramework::Test2Suite
        | TestFramework::TestException
        | TestFramework::TestFatal => true,
        TestFramework::Unknown => false,
    }
}

/// Every way the Perl cells and the actionable frameworks disagree.
fn disposition_violations(any_actionable: bool, cells: &[CapabilityCell]) -> Vec<String> {
    let perl: Vec<&CapabilityCell> = cells
        .iter()
        .filter(|cell| cell.language == LanguageId::Perl)
        .collect();
    let mut violations = Vec::new();
    for cell in &perl {
        if cell.framework != FRAMEWORK_ANY {
            violations.push(format!(
                "perl cell {}/{}/{} names framework `{}` but frameworks are classified language-wide",
                cell.mode.as_str(),
                cell.family.as_str(),
                cell.framework,
                cell.framework
            ));
        }
    }
    if any_actionable {
        let repair = perl.iter().find(|cell| {
            cell.mode == AnalysisMode::Diff && cell.family == CapabilityFamily::RepairRouting
        });
        match repair {
            None => violations
                .push("a Perl framework is strictly actionable but no repair cell exists".into()),
            Some(cell) => {
                if cell.state != SupportState::NotProven
                    || cell.tier_ceiling != TierCeiling::Preview
                {
                    violations.push(format!(
                        "perl repair routing is {} at {}: promoting it needs a deliberate edit of this gate",
                        cell.state.as_str(),
                        cell.tier_ceiling.as_str()
                    ));
                }
            }
        }
    }
    violations
}

fn any_actionable() -> bool {
    ALL_FRAMEWORKS
        .iter()
        .copied()
        .any(TestFramework::supports_strict_actionability)
}

#[test]
fn the_framework_list_is_a_gap_free_ordinal_sequence() {
    let ordinals: Vec<usize> = ALL_FRAMEWORKS.iter().copied().map(ordinal).collect();
    assert_eq!(ordinals, (0..FRAMEWORK_COUNT).collect::<Vec<_>>());
}

#[test]
fn production_actionability_matches_the_exhaustive_classification() {
    for framework in ALL_FRAMEWORKS {
        assert_eq!(
            framework.supports_strict_actionability(),
            expected_strict_actionability(framework),
            "{}",
            framework.as_str()
        );
    }
}

#[test]
fn the_perl_cells_agree_with_the_actionable_frameworks() {
    assert!(cells_for(LanguageId::Perl).next().is_some());
    assert_eq!(
        disposition_violations(any_actionable(), CAPABILITY_CELLS),
        Vec::<String>::new()
    );
}

#[test]
fn a_framework_specific_perl_cell_is_drift() {
    let mut cells = CAPABILITY_CELLS.to_vec();
    let mut extra = cells[0];
    extra.language = LanguageId::Perl;
    extra.framework = "Test::More";
    cells.push(extra);
    let violations = disposition_violations(any_actionable(), &cells);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("names framework `Test::More`")),
        "{violations:?}"
    );
}

#[test]
fn promoting_the_perl_repair_cell_is_drift() {
    let cells: Vec<CapabilityCell> = CAPABILITY_CELLS
        .iter()
        .map(|cell| {
            let mut cell = *cell;
            if cell.language == LanguageId::Perl && cell.family == CapabilityFamily::RepairRouting {
                cell.state = SupportState::SupportedWithRequiredFallback;
            }
            cell
        })
        .collect();
    let violations = disposition_violations(true, &cells);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("promoting it needs a deliberate edit")),
        "{violations:?}"
    );
}

#[test]
fn a_missing_perl_repair_cell_is_drift_when_a_framework_is_actionable() {
    let cells: Vec<CapabilityCell> = CAPABILITY_CELLS
        .iter()
        .filter(|cell| {
            !(cell.language == LanguageId::Perl && cell.family == CapabilityFamily::RepairRouting)
        })
        .copied()
        .collect();
    let violations = disposition_violations(true, &cells);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("no repair cell exists")),
        "{violations:?}"
    );
    assert!(disposition_violations(false, &cells).is_empty());
}
