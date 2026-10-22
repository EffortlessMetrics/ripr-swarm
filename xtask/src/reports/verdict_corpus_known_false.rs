//! The known-false-verdict ledger of a verdict corpus.
//!
//! `check` already fails when a row drifts from `expected/rows`, but a bless
//! can absorb a new false verdict as one more moved row. The `[[known_false]]`
//! tables in `spec-coverage.toml` close that: every false row must be listed
//! with the issue that owns its fix, and a listed row that is no longer false
//! must be removed. A fixed row therefore leaves the ledger, and its return
//! fails `check` as an unlisted false verdict.

use super::coverage::Ledger;
use super::{CaseRow, Outcome, TruthState, coverage};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KnownFalse {
    pub(crate) case: String,
    /// The ripr-swarm issue that owns the fix.
    pub(crate) issue: u64,
    pub(crate) reason: String,
}

fn is_false(outcome: Outcome) -> bool {
    matches!(
        outcome,
        Outcome::FalseActionable | Outcome::FalseExposed | Outcome::FalseSilent
    )
}

/// The outcome-independent checks on `[[known_false]]`: each case exists once
/// and carries an owning issue and a reason. `validate` and `bless` run these
/// without scoring; the gate runs them again so a direct caller cannot skip them.
pub(crate) fn known_false_ledger_violations(
    case_ids: &BTreeSet<&str>,
    ledger: &Ledger,
    ledger_label: &str,
) -> Vec<String> {
    let mut violations = Vec::new();
    let mut seen = BTreeSet::new();
    for entry in &ledger.known_false {
        if !seen.insert(entry.case.as_str()) {
            violations.push(format!(
                "{ledger_label} lists known_false case `{}` twice",
                entry.case
            ));
        }
        if entry.reason.trim().is_empty() || entry.issue == 0 {
            violations.push(format!(
                "known_false case `{}` in {ledger_label} needs an owning `issue` and a `reason`",
                entry.case
            ));
        }
        if !case_ids.contains(entry.case.as_str()) {
            violations.push(format!(
                "known_false case `{}` in {ledger_label} is not a corpus case; remove the entry",
                entry.case
            ));
        }
    }
    violations
}

/// Compare the scored rows (case id, outcome) with the ledger. `Err` names every unlisted false row and every stale entry.
pub(crate) fn known_false_gate(
    rows: &[(&str, Outcome)],
    ledger: &Ledger,
    ledger_label: &str,
) -> Result<(), String> {
    let false_ids: BTreeSet<&str> = rows
        .iter()
        .filter(|(_, outcome)| is_false(*outcome))
        .map(|(id, _)| *id)
        .collect();
    let all_ids: BTreeSet<&str> = rows.iter().map(|(id, _)| *id).collect();
    let mut failures: Vec<String> = known_false_ledger_violations(&all_ids, ledger, ledger_label)
        .into_iter()
        .map(|violation| format!("verdict-corpus: {violation}"))
        .collect();
    let listed: BTreeSet<&str> = ledger.known_false.iter().map(|e| e.case.as_str()).collect();
    for entry in &ledger.known_false {
        if all_ids.contains(entry.case.as_str()) && !false_ids.contains(entry.case.as_str()) {
            failures.push(format!(
                "verdict-corpus: known_false case `{}` is no longer a false verdict; remove it from {ledger_label} so a return fails this check",
                entry.case
            ));
        }
    }
    for id in &false_ids {
        if !listed.contains(id) {
            failures.push(format!(
                "verdict-corpus: `{id}` is a false verdict that {ledger_label} does not list; fix the case, spec or analyzer. Listing it as known_false needs an owning issue and is a reviewed edit, never a bless side effect"
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// Rows where the tests discriminate the change but ripr abstained instead of
/// crediting it: a safe answer that still leaves credit unclaimed.
fn missed_credit(rows: &[CaseRow]) -> usize {
    rows.iter()
        .filter(|row| row.truth == TruthState::Discriminated && row.outcome == Outcome::Abstained)
        .count()
}

/// Ratchet `missed_credit_ceiling`: above it is a regression, below it is an
/// improvement the ledger must record so it cannot be given back unnoticed.
/// A ledger without a ceiling does not gate missed credit.
fn missed_credit_gate(missed: usize, ledger: &Ledger, ledger_label: &str) -> Result<(), String> {
    let Some(ceiling) = ledger.missed_credit_ceiling else {
        return Ok(());
    };
    if missed > ceiling {
        return Err(format!(
            "verdict-corpus: {missed} abstained rows had tests that catch the change, above the missed_credit_ceiling of {ceiling} in {ledger_label}; a verdict stopped crediting a discriminating test. Fix the regression; raising the ceiling is a reviewed edit with a reason"
        ));
    }
    if missed < ceiling {
        return Err(format!(
            "verdict-corpus: {missed} abstained rows had tests that catch the change, below the missed_credit_ceiling of {ceiling} in {ledger_label}; lower the ceiling to {missed} so the improvement cannot regress"
        ));
    }
    Ok(())
}

/// Join the ledger gate's failure to `existing`. A case subset cannot see the
/// whole corpus's false rows, and a corpus without a ledger has no gate.
pub(crate) fn apply_known_false_gate(
    dir: &Path,
    subset: bool,
    rows: &[CaseRow],
    existing: Option<String>,
) -> Result<Option<String>, String> {
    if subset || !dir.join(coverage::LEDGER_FILE).is_file() {
        return Ok(existing);
    }
    let ledger = coverage::load_ledger(dir)?;
    let scored: Vec<(&str, Outcome)> = rows
        .iter()
        .map(|row| (row.case_id.as_str(), row.outcome))
        .collect();
    let label = coverage::ledger_label(dir);
    let failures: Vec<String> = [
        known_false_gate(&scored, &ledger, &label),
        missed_credit_gate(missed_credit(rows), &ledger, &label),
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect();
    if failures.is_empty() {
        return Ok(existing);
    }
    let failure = failures.join("\n");
    Ok(Some(match existing {
        Some(existing) => format!("{existing}\n{failure}"),
        None => failure,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ledger(entries: &str) -> Result<Ledger, String> {
        let text = format!(
            "schema_version = \"ripr_verdict_corpus_spec_coverage.v1\"\nfloor = 0\n{entries}"
        );
        toml::from_str(&text).map_err(|err| err.to_string())
    }

    /// The gate's failure text; empty when the gate passes, so a `contains`
    /// assertion on it fails for a gate that should have refused.
    fn failure(rows: &[(&str, Outcome)], entries: &str) -> String {
        gate(rows, entries).err().unwrap_or_default()
    }

    const ENTRY: &str = "[[known_false]]\ncase = \"a\"\nissue = 7\nreason = \"why\"\n";

    fn gate(rows: &[(&str, Outcome)], entries: &str) -> Result<(), String> {
        known_false_gate(rows, &ledger(entries)?, "ledger.toml")
    }

    #[test]
    fn listed_false_row_passes() -> Result<(), String> {
        gate(
            &[("a", Outcome::FalseActionable), ("b", Outcome::Ideal)],
            ENTRY,
        )
    }

    #[test]
    fn unlisted_false_row_fails_for_each_false_kind() {
        for outcome in [
            Outcome::FalseActionable,
            Outcome::FalseExposed,
            Outcome::FalseSilent,
        ] {
            let err = failure(&[("a", Outcome::Ideal), ("b", outcome)], "");
            assert!(err.contains("`b` is a false verdict"), "{err}");
        }
    }

    #[test]
    fn abstained_and_ideal_rows_need_no_entry() -> Result<(), String> {
        gate(&[("a", Outcome::Abstained), ("b", Outcome::Ideal)], "")
    }

    #[test]
    fn fixed_row_must_leave_the_ledger() {
        let err = failure(&[("a", Outcome::Ideal)], ENTRY);
        assert!(err.contains("no longer a false verdict"), "{err}");
    }

    #[test]
    fn entry_for_unknown_case_fails() {
        let err = failure(&[("b", Outcome::Ideal)], ENTRY);
        assert!(err.contains("not a corpus case"), "{err}");
    }

    #[test]
    fn duplicate_and_ownerless_entries_fail() {
        let twice = format!("{ENTRY}{ENTRY}");
        let err = failure(&[("a", Outcome::FalseSilent)], &twice);
        assert!(err.contains("twice"), "{err}");
        let ownerless = "[[known_false]]\ncase = \"a\"\nissue = 0\nreason = \" \"\n";
        let err = failure(&[("a", Outcome::FalseSilent)], ownerless);
        assert!(err.contains("owning `issue`"), "{err}");
    }

    fn ceiling(value: Option<usize>) -> Result<Ledger, String> {
        let line = value.map_or(String::new(), |v| format!("missed_credit_ceiling = {v}\n"));
        ledger(&line)
    }

    #[test]
    fn missed_credit_ceiling_ratchets_both_ways() -> Result<(), String> {
        missed_credit_gate(3, &ceiling(Some(3))?, "l.toml")?;
        let up = missed_credit_gate(4, &ceiling(Some(3))?, "l.toml").err();
        assert!(
            up.is_some_and(|e| e.contains("above")),
            "regression must fail"
        );
        let down = missed_credit_gate(2, &ceiling(Some(3))?, "l.toml").err();
        assert!(down.is_some_and(|e| e.contains("lower the ceiling to 2")));
        // No ceiling, no gate.
        missed_credit_gate(99, &ceiling(None)?, "l.toml")
    }

    #[test]
    fn missed_credit_counts_only_abstained_discriminated_rows() -> Result<(), String> {
        let (_, rows) = committed_rust_rows()?;
        let expected = rows
            .iter()
            .filter(|r| r.outcome == Outcome::Abstained && r.truth == TruthState::Discriminated)
            .count();
        assert_eq!(missed_credit(&rows), expected);
        assert!(
            rows.iter()
                .any(|r| r.outcome == Outcome::Abstained && r.truth != TruthState::Discriminated)
                || expected > 0
        );
        Ok(())
    }

    fn committed_rust_rows() -> Result<(std::path::PathBuf, Vec<CaseRow>), String> {
        let dir = crate::dogfood::repo_rooted_fixture_path("fixtures/rust-verdict-corpus");
        let rows = super::super::expected_report(&dir)?.rows;
        Ok((dir, rows))
    }

    #[test]
    fn wiring_accepts_the_committed_ledger_against_blessed_rows() -> Result<(), String> {
        let (dir, rows) = committed_rust_rows()?;
        assert_eq!(apply_known_false_gate(&dir, false, &rows, None)?, None);
        Ok(())
    }

    #[test]
    fn wiring_fails_an_unlisted_false_row_and_keeps_the_earlier_failure() -> Result<(), String> {
        let (_, rows) = committed_rust_rows()?;
        let empty = crate::tests::temp_dir("known-false-empty-ledger");
        std::fs::create_dir_all(&empty).map_err(|err| err.to_string())?;
        std::fs::write(
            empty.join(coverage::LEDGER_FILE),
            "schema_version = \"ripr_verdict_corpus_spec_coverage.v1\"\nfloor = 0\n",
        )
        .map_err(|err| err.to_string())?;
        let alone = apply_known_false_gate(&empty, false, &rows, None)?.unwrap_or_default();
        assert!(alone.contains("is a false verdict"), "{alone}");
        let joined = apply_known_false_gate(&empty, false, &rows, Some("oracle failed".into()))?
            .unwrap_or_default();
        assert!(
            joined.starts_with("oracle failed\n") && joined.contains("is a false verdict"),
            "{joined}"
        );
        // A case subset cannot see the whole corpus, so it is never gated.
        assert_eq!(apply_known_false_gate(&empty, true, &rows, None)?, None);
        Ok(())
    }

    #[test]
    fn structural_violations_need_no_scored_rows() -> Result<(), String> {
        let twice =
            format!("{ENTRY}{ENTRY}[[known_false]]\ncase = \"zz\"\nissue = 0\nreason = \"\"\n");
        let ids: BTreeSet<&str> = ["a"].into_iter().collect();
        let found = known_false_ledger_violations(&ids, &ledger(&twice)?, "ledger.toml");
        for needle in ["twice", "owning `issue`", "not a corpus case"] {
            assert!(
                found.iter().any(|v| v.contains(needle)),
                "{needle}: {found:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn committed_ledgers_parse_with_owners() -> Result<(), String> {
        for language in ["rust", "typescript", "python", "perl"] {
            let dir = crate::dogfood::repo_rooted_fixture_path(&format!(
                "fixtures/{language}-verdict-corpus"
            ));
            let ledger = super::super::coverage::load_ledger(&dir)?;
            for entry in &ledger.known_false {
                assert!(
                    entry.issue > 0 && !entry.reason.trim().is_empty(),
                    "{language}"
                );
            }
        }
        Ok(())
    }
}
