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

/// One accepted missed credit: an abstained row whose tests do discriminate
/// the change. One `[[entry]]` block per row in the corpus's sorted
/// `missed-credit.toml`, so PRs that move different rows touch different
/// hunks and git usually merges them (adjacent edits can still conflict) (#10222).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MissedCredit {
    pub(crate) case: String,
    pub(crate) reason: String,
}

/// The accepted-missed-credit file inside a corpus directory.
pub(crate) const MISSED_CREDIT_FILE: &str = "missed-credit.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MissedCreditFile {
    #[serde(default)]
    entry: Vec<MissedCredit>,
}

/// An entry file's `case`, which must equal the file stem.
trait CaseEntry: for<'de> Deserialize<'de> {
    fn case(&self) -> &str;
}

impl CaseEntry for KnownFalse {
    fn case(&self) -> &str {
        &self.case
    }
}

/// Read every `<dir_name>/<case>.toml` of a corpus. The file stem must equal
/// the `case` it holds, so one case has one path and a duplicate cannot be added.
fn load_case_files<T: CaseEntry>(corpus_dir: &Path, dir_name: &str) -> Result<Vec<T>, String> {
    let dir = corpus_dir.join(dir_name);
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
            return Err(format!("{} is not a `.toml` entry file", path.display()));
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("read {}: {err}", path.display()))?;
        let entry: T =
            toml::from_str(&text).map_err(|err| format!("parse {}: {err}", path.display()))?;
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if stem != entry.case() {
            return Err(format!(
                "{} holds case `{}`; name the file `{}.toml`",
                path.display(),
                entry.case(),
                entry.case()
            ));
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// Read every `known-false/<case>.toml`.
pub(crate) fn load_entries(corpus_dir: &Path) -> Result<Vec<KnownFalse>, String> {
    load_case_files(corpus_dir, KNOWN_FALSE_DIR)
}

/// Read `missed-credit.toml`: `[[entry]]` blocks (case, reason), sorted by
/// case and unique, so an insertion lands at its sorted place and two PRs
/// adding different cases rarely touch the same lines. A corpus without the
/// file accepts no missed credit.
pub(crate) fn load_missed_credit_entries(corpus_dir: &Path) -> Result<Vec<MissedCredit>, String> {
    let path = corpus_dir.join(MISSED_CREDIT_FILE);
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text =
        std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let file: MissedCreditFile =
        toml::from_str(&text).map_err(|err| format!("parse {}: {err}", path.display()))?;
    for pair in file.entry.windows(2) {
        if pair[0].case >= pair[1].case {
            return Err(format!(
                "{} must list cases sorted and unique; `{}` does not follow `{}`",
                path.display(),
                pair[1].case,
                pair[0].case
            ));
        }
    }
    Ok(file.entry)
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
    let mut accepted = BTreeSet::new();
    for entry in &ledger.missed_credit {
        if !accepted.insert(entry.case.as_str()) {
            violations.push(format!(
                "{ledger_label} lists missed-credit case `{}` twice",
                entry.case
            ));
        }
        if entry.reason.trim().is_empty() {
            violations.push(format!(
                "missed-credit case `{}` in {ledger_label} needs a `reason`",
                entry.case
            ));
        }
        if !case_ids.contains(entry.case.as_str()) {
            violations.push(format!(
                "missed-credit case `{}` in {ledger_label} is not a corpus case; remove the entry",
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

/// Ids of rows where the tests discriminate the change but ripr abstained
/// instead of crediting it: a safe answer that still leaves credit unclaimed.
fn missed_credit(rows: &[CaseRow]) -> BTreeSet<&str> {
    rows.iter()
        .filter(|row| row.truth == TruthState::Discriminated && row.outcome == Outcome::Abstained)
        .map(|row| row.case_id.as_str())
        .collect()
}

/// Gate the accepted missed-credit rows (`missed-credit.toml`). A
/// missed-credit row without an entry is a regression and fails, naming it. An
/// entry whose row no longer misses credit is an improvement that passes with
/// a note to delete its entry: each entry is its own block, so two PRs moving
/// different rows never conflict, and a fix never deadlocks main on a shared
/// number. The count is derived from the entries, never stored (#10222).
fn missed_credit_gate(
    missed: &BTreeSet<&str>,
    ledger: &Ledger,
    ledger_label: &str,
) -> Result<Option<String>, String> {
    let accepted: BTreeSet<&str> = ledger
        .missed_credit
        .iter()
        .map(|e| e.case.as_str())
        .collect();
    let unlisted: Vec<&str> = missed.difference(&accepted).copied().collect();
    if !unlisted.is_empty() {
        return Err(format!(
            "verdict-corpus: {} abstained row(s) had tests that catch the change but {ledger_label} accepts no missed credit for them; a verdict stopped crediting a discriminating test. Fix the regression; accepting it adds an `[[entry]]` with a reason to {MISSED_CREDIT_FILE} and is a reviewed edit:\n{}",
            unlisted.len(),
            unlisted
                .iter()
                .map(|case| format!("  - {case}"))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }
    let stale: Vec<&str> = accepted.difference(missed).copied().collect();
    Ok((!stale.is_empty()).then(|| {
        format!(
            "verdict-corpus: {} accepted missed-credit row(s) in {ledger_label} no longer miss credit; delete its `[[entry]]` from {MISSED_CREDIT_FILE} so the improvement cannot regress: {}",
            stale.len(),
            stale.join(", ")
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
    match missed_credit_gate(&missed_credit(rows), &ledger, &label) {
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

    fn accepting(cases: &[&str]) -> Result<Ledger, String> {
        let mut ledger = ledger("")?;
        ledger.missed_credit = cases
            .iter()
            .map(|case| MissedCredit {
                case: (*case).to_string(),
                reason: "accepted".to_string(),
            })
            .collect();
        Ok(ledger)
    }

    fn set<'a>(cases: &[&'a str]) -> BTreeSet<&'a str> {
        cases.iter().copied().collect()
    }

    #[test]
    fn a_new_missed_credit_fails_and_an_improvement_notes() -> Result<(), String> {
        let accepted = accepting(&["a", "b"])?;
        // Exactly the accepted rows: clean.
        assert_eq!(
            missed_credit_gate(&set(&["a", "b"]), &accepted, "l.toml")?,
            None
        );
        // Negative experiment: a row that newly misses credit has no entry, so
        // the gate fails and names it.
        let err = missed_credit_gate(&set(&["a", "b", "c"]), &accepted, "l.toml")
            .err()
            .unwrap_or_default();
        assert!(err.contains("  - c") && !err.contains("  - a"), "{err}");
        // Swapping one row for another keeps the count and still fails.
        let swapped = missed_credit_gate(&set(&["a", "c"]), &accepted, "l.toml")
            .err()
            .unwrap_or_default();
        assert!(swapped.contains("  - c"), "{swapped}");
        // An improvement passes with a note naming the entry to delete, so
        // concurrent fix PRs do not collide.
        let note = missed_credit_gate(&set(&["a"]), &accepted, "l.toml")?.unwrap_or_default();
        assert!(note.contains("delete") && note.contains('b'), "{note}");
        // Nothing accepted and nothing missed is clean.
        assert_eq!(
            missed_credit_gate(&set(&[]), &accepting(&[])?, "l.toml")?,
            None
        );
        Ok(())
    }

    #[test]
    fn missed_credit_entries_are_structurally_checked() -> Result<(), String> {
        let mut ledger = accepting(&["a", "a", "ghost"])?;
        ledger.missed_credit[0].reason = " ".to_string();
        let ids: BTreeSet<&str> = ["a"].into_iter().collect();
        let found = known_false_ledger_violations(&ids, &ledger, "ledger.toml");
        for needle in ["twice", "needs a `reason`", "not a corpus case"] {
            assert!(
                found.iter().any(|v| v.contains(needle)),
                "{needle}: {found:?}"
            );
        }
        Ok(())
    }

    /// Entries that outnumber the real missed credit would pass the gate with
    /// only a note, so pin each committed entry set to its blessed rows. The
    /// accepted count is derived from the files, never stored on a shared line.
    #[test]
    fn committed_missed_credit_entries_equal_the_blessed_missed_credit() -> Result<(), String> {
        for language in ["rust", "typescript", "python", "perl"] {
            let dir = crate::dogfood::repo_rooted_fixture_path(&format!(
                "fixtures/{language}-verdict-corpus"
            ));
            let rows = super::super::expected_report(&dir)?.rows;
            let ledger = coverage::load_ledger(&dir)?;
            let missed = missed_credit(&rows);
            assert!(
                !missed.is_empty(),
                "{language} has no missed credit to count"
            );
            let accepted: BTreeSet<&str> = ledger
                .missed_credit
                .iter()
                .map(|e| e.case.as_str())
                .collect();
            assert_eq!(
                accepted, missed,
                "{language} missed-credit.toml entries must equal the blessed missed rows"
            );
        }
        Ok(())
    }

    #[test]
    fn an_inline_ceiling_line_is_refused() -> Result<(), String> {
        let dir = crate::tests::temp_dir("missed-credit-inline-ceiling");
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        std::fs::write(
            dir.join(coverage::LEDGER_FILE),
            "schema_version = \"ripr_verdict_corpus_spec_coverage.v1\"\nmissed_credit_ceiling = 3\n",
        )
        .map_err(|err| err.to_string())?;
        let err = coverage::load_ledger(&dir).err().unwrap_or_default();
        assert!(err.contains("missed-credit.toml"), "{err}");
        Ok(())
    }

    #[test]
    fn missed_credit_file_loads_sorted_unique_entries() -> Result<(), String> {
        let dir = crate::tests::temp_dir("missed-credit-entry-file");
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let write = |text: &str| {
            std::fs::write(dir.join(MISSED_CREDIT_FILE), text).map_err(|err| err.to_string())
        };
        assert!(load_missed_credit_entries(&dir)?.is_empty());
        write(
            "[[entry]]\ncase = \"a\"\nreason = \"why\"\n\n[[entry]]\ncase = \"b\"\nreason = \"why\"\n",
        )?;
        assert_eq!(load_missed_credit_entries(&dir)?.len(), 2);
        write(
            "[[entry]]\ncase = \"b\"\nreason = \"why\"\n\n[[entry]]\ncase = \"a\"\nreason = \"why\"\n",
        )?;
        let err = load_missed_credit_entries(&dir).err().unwrap_or_default();
        assert!(err.contains("sorted and unique"), "{err}");
        write(
            "[[entry]]\ncase = \"a\"\nreason = \"why\"\n\n[[entry]]\ncase = \"a\"\nreason = \"why\"\n",
        )?;
        assert!(load_missed_credit_entries(&dir).is_err());
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
    fn wiring_fails_a_missed_credit_row_that_has_no_entry_end_to_end() -> Result<(), String> {
        let (dir, rows) = committed_rust_rows()?;
        // The committed corpus is clean.
        assert_eq!(apply_known_false_gate(&dir, false, &rows, None)?, None);
        // The same rows against a ledger directory that accepts no missed
        // credit fail through `apply_known_false_gate` and name a missed row.
        let bare = crate::tests::temp_dir("missed-credit-bare-ledger");
        std::fs::create_dir_all(&bare).map_err(|err| err.to_string())?;
        std::fs::write(
            bare.join(coverage::LEDGER_FILE),
            "schema_version = \"ripr_verdict_corpus_spec_coverage.v1\"\n",
        )
        .map_err(|err| err.to_string())?;
        std::fs::write(bare.join(coverage::LOCK_FILE), "").map_err(|err| err.to_string())?;
        let missed = missed_credit(&rows);
        let named = missed.iter().next().copied().ok_or("no missed credit")?;
        let failure = apply_known_false_gate(&bare, false, &rows, None)?.unwrap_or_default();
        assert!(
            failure.contains("accepts no missed credit")
                && failure.contains(&format!("  - {named}")),
            "{failure}"
        );
        // Accepting every missed row through the file clears that failure.
        let entries: String = missed
            .iter()
            .map(|case| format!("[[entry]]\ncase = \"{case}\"\nreason = \"accepted\"\n\n"))
            .collect();
        std::fs::write(bare.join(MISSED_CREDIT_FILE), entries).map_err(|err| err.to_string())?;
        let accepted = apply_known_false_gate(&bare, false, &rows, None)?.unwrap_or_default();
        assert!(!accepted.contains("accepts no missed credit"), "{accepted}");
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
