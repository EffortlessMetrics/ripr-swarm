//! The known-false-verdict ledger of a verdict corpus.
//!
//! `check` already fails when a row drifts from `expected/rows`, but a bless
//! can absorb a new false verdict as one more moved row. The `[[known_false]]`
//! tables in `spec-coverage.toml` close that: every false row must be listed
//! with the issue that owns its fix, and a listed row that is no longer false
//! must be removed. A fixed row therefore leaves the ledger, and its return
//! fails `check` as an unlisted false verdict.
//!
//! Each entry is its own file, `known-false/<case>.toml` in the corpus
//! directory, the way cases are split: a fix deletes one file and a new false
//! row adds one, so parallel PRs never edit neighbouring lines of one list.

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

/// The directory of entry files inside a corpus directory.
pub(crate) const KNOWN_FALSE_DIR: &str = "known-false";

/// Read every `known-false/<case>.toml`. The file stem must equal the `case`
/// it holds, so one case has one path and a duplicate cannot be added.
pub(crate) fn load_entries(corpus_dir: &Path) -> Result<Vec<KnownFalse>, String> {
    let dir = corpus_dir.join(KNOWN_FALSE_DIR);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
        .map_err(|err| format!("read {}: {err}", dir.display()))?
        .map(|entry| {
            entry
                .map(|e| e.path())
                .map_err(|err| format!("read {}: {err}", dir.display()))
        })
        .collect::<Result<_, _>>()?;
    paths.sort();
    let mut entries = Vec::new();
    for path in paths {
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            return Err(format!(
                "{} is not a `.toml` known-false entry",
                path.display()
            ));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("read {}: {err}", path.display()))?;
        let entry: KnownFalse =
            toml::from_str(&text).map_err(|err| format!("parse {}: {err}", path.display()))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem != entry.case {
            return Err(format!(
                "{} holds case `{}`; name the file `{}.toml`",
                path.display(),
                entry.case,
                entry.case
            ));
        }
        entries.push(entry);
    }
    Ok(entries)
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

/// Gate `missed_credit_ceiling`: above it is a regression and fails. Below it
/// is an improvement that passes with a note to lower the ceiling; failing
/// there would make every concurrent fix PR edit the same number and
/// deadlock main. A ledger without a ceiling does not gate missed credit.
fn missed_credit_gate(
    missed: usize,
    ledger: &Ledger,
    ledger_label: &str,
) -> Result<Option<String>, String> {
    let Some(ceiling) = ledger.missed_credit_ceiling else {
        return Ok(None);
    };
    if missed > ceiling {
        return Err(format!(
            "verdict-corpus: {missed} abstained rows had tests that catch the change, above the missed_credit_ceiling of {ceiling} in {ledger_label}; a verdict stopped crediting a discriminating test. Fix the regression; raising the ceiling is a reviewed edit with a reason"
        ));
    }
    Ok((missed < ceiling).then(|| {
        format!(
            "verdict-corpus: {missed} abstained rows had tests that catch the change, below the missed_credit_ceiling of {ceiling} in {ledger_label}; lower the ceiling to {missed} so the improvement cannot regress"
        )
    }))
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
    let mut failures: Vec<String> = known_false_gate(&scored, &ledger, &label)
        .err()
        .into_iter()
        .collect();
    match missed_credit_gate(missed_credit(rows), &ledger, &label) {
        Ok(Some(note)) => println!("{note}"),
        Ok(None) => {}
        Err(failure) => failures.push(failure),
    }
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
        let text = format!("schema_version = \"ripr_verdict_corpus_spec_coverage.v1\"\n{entries}");
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
    fn missed_credit_ceiling_fails_up_and_notes_down() -> Result<(), String> {
        assert_eq!(missed_credit_gate(3, &ceiling(Some(3))?, "l.toml")?, None);
        let up = missed_credit_gate(4, &ceiling(Some(3))?, "l.toml").err();
        assert!(
            up.is_some_and(|e| e.contains("above")),
            "regression must fail"
        );
        // An improvement passes with a note, so concurrent fix PRs do not collide.
        let down = missed_credit_gate(2, &ceiling(Some(3))?, "l.toml")?;
        assert!(down.is_some_and(|n| n.contains("lower the ceiling to 2")));
        // No ceiling, no gate.
        assert_eq!(missed_credit_gate(99, &ceiling(None)?, "l.toml")?, None);
        Ok(())
    }

    /// A ceiling set above the real count would pass the gate with only a
    /// note, so pin each committed ceiling to its blessed rows.
    #[test]
    fn committed_ceilings_equal_the_blessed_missed_credit() -> Result<(), String> {
        for language in ["rust", "typescript", "python", "perl"] {
            let dir = crate::dogfood::repo_rooted_fixture_path(&format!(
                "fixtures/{language}-verdict-corpus"
            ));
            let rows = super::super::expected_report(&dir)?.rows;
            let ledger = coverage::load_ledger(&dir)?;
            let missed = missed_credit(&rows);
            assert!(missed > 0, "{language} has no missed credit to count");
            assert_eq!(
                ledger.missed_credit_ceiling,
                Some(missed),
                "{language} missed_credit_ceiling must equal the blessed count"
            );
        }
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
        let (_, mut rows) = committed_rust_rows()?;
        // The committed corpus can have no false row left; the negative control
        // must not depend on one, so it makes a committed row false itself.
        let row = rows.first_mut().ok_or("the committed corpus has no rows")?;
        row.outcome = Outcome::FalseActionable;
        let empty = crate::tests::temp_dir("known-false-empty-ledger");
        std::fs::create_dir_all(&empty).map_err(|err| err.to_string())?;
        std::fs::write(
            empty.join(coverage::LEDGER_FILE),
            "schema_version = \"ripr_verdict_corpus_spec_coverage.v1\"\n",
        )
        .map_err(|err| err.to_string())?;
        std::fs::write(empty.join(coverage::LOCK_FILE), "").map_err(|err| err.to_string())?;
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
    fn entry_files_load_by_case_name_and_reject_a_mismatch() -> Result<(), String> {
        let dir = crate::tests::temp_dir("known-false-entry-files");
        let entries = dir.join(KNOWN_FALSE_DIR);
        std::fs::create_dir_all(&entries).map_err(|err| err.to_string())?;
        std::fs::write(
            entries.join("a.toml"),
            "case = \"a\"\nissue = 7\nreason = \"why\"\n",
        )
        .map_err(|err| err.to_string())?;
        let loaded = load_entries(&dir)?;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].issue, 7);
        std::fs::write(
            entries.join("b.toml"),
            "case = \"c\"\nissue = 7\nreason = \"why\"\n",
        )
        .map_err(|err| err.to_string())?;
        let err = load_entries(&dir).err().unwrap_or_default();
        assert!(err.contains("name the file `c.toml`"), "{err}");
        assert!(load_entries(&dir.join("absent"))?.is_empty());
        Ok(())
    }

    #[test]
    fn inline_known_false_tables_are_rejected() -> Result<(), String> {
        let dir = crate::tests::temp_dir("known-false-inline");
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        std::fs::write(
            dir.join(coverage::LEDGER_FILE),
            format!("schema_version = \"ripr_verdict_corpus_spec_coverage.v1\"\n{ENTRY}"),
        )
        .map_err(|err| err.to_string())?;
        let err = coverage::load_ledger(&dir).err().unwrap_or_default();
        assert!(err.contains("one file per row"), "{err}");
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
