//! Rust diff-mode join: cargo-mutants mutants scored through `ripr check
//! --diff`, the product path, alongside the seam span join.
//!
//! cargo-mutants 27 and later records a unified `diff` for every mutant in
//! `mutants.json`. [`rust_mutation_sources`] turns those records and the
//! `outcomes.json` results into [`MutationSource`] values carrying that diff,
//! and [`apply_recorded_diff`] applies it to the checkout text in-process, so
//! the shared synthetic-diff scorer writes the mutated file into its scratch
//! tree, runs `ripr check --diff` with the same hunks, and reads the verdict
//! on the mutated lines. A diff whose context or removed lines differ from the
//! checkout is an exclusion, never a guess.

use super::synthetic_diff::{MutationSource, SyntheticEdit, SyntheticMutant};
use serde_json::Value;
use std::collections::BTreeMap;

const RECORDED_DIFF_MISMATCH: &str = "recorded_diff_mismatch";
const RECORDED_DIFF_INVALID: &str = "recorded_diff_invalid";
/// A hunk that only removes lines has no new line of its own, so the verdict
/// would be read from the neighbouring statement; such a mutant is excluded.
const DELETION_ONLY_MUTANT: &str = "deletion_only_mutant";
const LINE_ENDINGS_UNSUPPORTED: &str = "source_line_endings_unsupported";
const NOOP_MUTANT: &str = "noop_mutant";
/// The outcome of a mutant `outcomes.json` has no result for.
const NO_OUTCOME: &str = "no_outcome";

/// Parse `@@ -start[,count] ...` into the old start line and line count.
fn hunk_old_range(header: &str) -> Option<(usize, usize)> {
    let range = header.strip_prefix("@@ -")?.split(' ').next()?;
    let (start, count) = match range.split_once(',') {
        Some((start, count)) => (start.parse().ok()?, count.parse().ok()?),
        None => (range.parse().ok()?, 1),
    };
    Some((start, count))
}

/// Apply a cargo-mutants unified `diff` for `path` to `source` and return the
/// mutated file plus a diff `ripr check --diff` reads (standard `a/` and `b/`
/// headers, the recorded hunks unchanged). The error is the exclusion reason.
pub(crate) fn apply_recorded_diff(
    path: &str,
    source: &str,
    diff: &str,
) -> Result<SyntheticEdit, &'static str> {
    if source.contains('\r') {
        return Err(LINE_ENDINGS_UNSUPPORTED);
    }
    let ends_with_newline = source.ends_with('\n');
    let mut lines: Vec<&str> = source.split('\n').collect();
    if ends_with_newline {
        lines.pop();
    }
    let mut out: Vec<&str> = Vec::new();
    let mut hunks = String::new();
    let mut cursor = 0usize;
    let mut in_hunk = false;
    let mut changed: Option<(usize, usize)> = None;
    let mut added_any = false;
    let mut mark = |line: usize| {
        changed = Some(changed.map_or((line, line), |(lo, hi)| (lo.min(line), hi.max(line))));
    };
    for line in diff.lines() {
        if line.starts_with("@@ -") {
            let (start, count) = hunk_old_range(line).ok_or(RECORDED_DIFF_INVALID)?;
            // A zero-length old range names the line the insertion follows.
            let target = if count == 0 {
                start
            } else {
                start.checked_sub(1).ok_or(RECORDED_DIFF_INVALID)?
            };
            if target < cursor || target > lines.len() {
                return Err(RECORDED_DIFF_INVALID);
            }
            out.extend_from_slice(&lines[cursor..target]);
            cursor = target;
            in_hunk = true;
        } else if !in_hunk {
            // File headers before the first hunk are rebuilt below.
            continue;
        } else if let Some(added) = line.strip_prefix('+') {
            out.push(added);
            added_any = true;
            mark(out.len());
        } else if line.starts_with('\\') {
            continue;
        } else {
            let (removed, original) = match line.strip_prefix('-') {
                Some(text) => (true, text),
                None if line.is_empty() => (false, ""),
                None => (false, line.strip_prefix(' ').ok_or(RECORDED_DIFF_INVALID)?),
            };
            if lines.get(cursor) != Some(&original) {
                return Err(RECORDED_DIFF_MISMATCH);
            }
            cursor += 1;
            if removed {
                // A deletion lands between new lines: mark the line after it.
                mark(out.len() + 1);
            } else {
                out.push(original);
            }
        }
        if in_hunk {
            hunks.push_str(line);
            hunks.push('\n');
        }
    }
    let (first, last) = changed.ok_or(if in_hunk {
        NOOP_MUTANT
    } else {
        RECORDED_DIFF_INVALID
    })?;
    if !added_any {
        return Err(DELETION_ONLY_MUTANT);
    }
    out.extend_from_slice(&lines[cursor..]);
    if out == lines {
        return Err(NOOP_MUTANT);
    }
    let mut mutated = out.join("\n");
    if ends_with_newline {
        mutated.push('\n');
    }
    let last = last.min(out.len().max(1));
    Ok(SyntheticEdit {
        diff: format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n{hunks}"),
        mutated,
        first_line: first.min(last) as u64,
        last_line: last as u64,
    })
}

/// The receipt outcome token for a cargo-mutants `summary`.
fn outcome_of_summary(summary: &str) -> String {
    match summary {
        "CaughtMutant" => "caught".to_string(),
        "MissedMutant" => "missed".to_string(),
        other => other.strip_suffix("Mutant").unwrap_or(other).to_lowercase(),
    }
}

/// Group cargo-mutants `mutants.json` records by file, each carrying its
/// recorded diff and its `outcomes.json` result. `read_source` returns the
/// checkout text for a file; an unreadable file gets empty text, so every
/// mutant of it is a `source_mismatch` exclusion rather than dropped.
pub(crate) fn rust_mutation_sources(
    mutants: &Value,
    outcomes: &Value,
    read_source: &dyn Fn(&str) -> Option<String>,
) -> Vec<MutationSource> {
    let results: BTreeMap<&str, &str> = outcomes
        .get("outcomes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            Some((
                record.pointer("/scenario/Mutant/name")?.as_str()?,
                record.get("summary")?.as_str()?,
            ))
        })
        .collect();
    let mut by_file: BTreeMap<String, Vec<SyntheticMutant>> = BTreeMap::new();
    for mutant in mutants.as_array().into_iter().flatten() {
        let text = |key: &str| mutant.get(key).and_then(Value::as_str).unwrap_or("");
        let position = |pointer: &str| mutant.pointer(pointer).and_then(Value::as_u64);
        let name = text("name");
        let status = results.get(name).copied().unwrap_or(NO_OUTCOME);
        let start_col = position("/span/start/column").map_or(0, |col| col.saturating_sub(1));
        by_file
            .entry(text("file").to_string())
            .or_default()
            .push(SyntheticMutant {
                id: name.to_string(),
                mutator: text("genre").to_string(),
                replacement: text("replacement").to_string(),
                start_line: position("/span/start/line").unwrap_or(0),
                start_col,
                end_line: position("/span/end/line").unwrap_or(0),
                end_col: position("/span/end/column").map_or(0, |col| col.saturating_sub(1)),
                status: status.to_string(),
                outcome: if status == NO_OUTCOME {
                    NO_OUTCOME.to_string()
                } else {
                    outcome_of_summary(status)
                },
                recorded_diff: mutant
                    .get("diff")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            });
    }
    by_file
        .into_iter()
        .map(|(path, mutants)| MutationSource {
            source: read_source(&path).unwrap_or_default(),
            path,
            mutants,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const SOURCE: &str = "// header\nfn f(a: u8) -> bool {\n    a > 1\n}\n";
    const DIFF: &str = "--- src/a.rs\n+++ replace > with < in f\n@@ -2,3 +2,3 @@\n fn f(a: u8) -> bool {\n-    a > 1\n+    a < 1\n }\n";

    #[test]
    fn recorded_diff_applies_to_the_checkout_text() -> Result<(), String> {
        let edit = apply_recorded_diff("src/a.rs", SOURCE, DIFF).map_err(str::to_string)?;
        assert_eq!(
            edit.mutated,
            "// header\nfn f(a: u8) -> bool {\n    a < 1\n}\n"
        );
        assert_eq!((edit.first_line, edit.last_line), (3, 3));
        assert_eq!(
            edit.diff,
            "diff --git a/src/a.rs b/src/a.rs\n--- a/src/a.rs\n+++ b/src/a.rs\n@@ -2,3 +2,3 @@\n fn f(a: u8) -> bool {\n-    a > 1\n+    a < 1\n }\n"
        );
        Ok(())
    }

    #[test]
    fn multi_line_body_replacement_marks_every_new_line() -> Result<(), String> {
        let diff = "--- src/a.rs\n+++ replace f -> bool with true\n@@ -2,3 +2,3 @@\n fn f(a: u8) -> bool {\n-    a > 1\n+    true /* ~ changed by cargo mutants ~ */\n }\n";
        let edit = apply_recorded_diff("src/a.rs", SOURCE, diff).map_err(str::to_string)?;
        assert!(edit.mutated.contains("    true /* ~ changed"));
        assert_eq!((edit.first_line, edit.last_line), (3, 3));
        let grow = "@@ -3,1 +3,2 @@\n-    a > 1\n+    let _ = a;\n+    true\n";
        let edit = apply_recorded_diff("src/a.rs", SOURCE, grow).map_err(str::to_string)?;
        assert_eq!((edit.first_line, edit.last_line), (3, 4));
        Ok(())
    }

    #[test]
    fn a_diff_that_no_longer_matches_the_checkout_is_excluded() {
        let stale = SOURCE.replace("a > 1", "a >= 1");
        assert_eq!(
            apply_recorded_diff("src/a.rs", &stale, DIFF),
            Err(RECORDED_DIFF_MISMATCH)
        );
        let shifted = format!("// new line\n{SOURCE}");
        assert_eq!(
            apply_recorded_diff("src/a.rs", &shifted, DIFF),
            Err(RECORDED_DIFF_MISMATCH)
        );
        assert_eq!(
            apply_recorded_diff("src/a.rs", SOURCE, "--- a\n+++ b\n"),
            Err(RECORDED_DIFF_INVALID)
        );
        assert_eq!(
            apply_recorded_diff("src/a.rs", SOURCE, "@@ -9,1 +9,1 @@\n-x\n+y\n"),
            Err(RECORDED_DIFF_INVALID)
        );
        assert_eq!(
            apply_recorded_diff("src/a.rs", &SOURCE.replace('\n', "\r\n"), DIFF),
            Err(LINE_ENDINGS_UNSUPPORTED)
        );
    }

    #[test]
    fn a_deletion_only_mutant_is_excluded_not_read_from_the_next_line() {
        let source = "fn f(a: u8) -> u8 {\n    match a {\n        0 => 1,\n        _ => 2,\n    }\n}\n";
        let diff = "--- src/a.rs\n+++ delete match arm 0 in f\n@@ -2,3 +2,2 @@\n     match a {\n-        0 => 1,\n         _ => 2,\n";
        assert_eq!(
            apply_recorded_diff("src/a.rs", source, diff).map(|_| ()),
            Err(DELETION_ONLY_MUTANT)
        );
    }

    #[test]
    fn sources_carry_each_mutant_diff_and_outcome() {
        let mutants = json!([
            {"name": "m1", "file": "src/a.rs", "genre": "BinaryOperator", "diff": DIFF,
             "span": {"start": {"line": 3, "column": 7}, "end": {"line": 3, "column": 8}}},
            {"name": "m2", "file": "src/a.rs", "genre": "FnValue", "diff": DIFF},
            {"name": "m3", "file": "src/gone.rs", "genre": "FnValue", "diff": DIFF},
        ]);
        let outcomes = json!({"outcomes": [
            {"scenario": "Baseline", "summary": "Success"},
            {"scenario": {"Mutant": {"name": "m1"}}, "summary": "MissedMutant"},
            {"scenario": {"Mutant": {"name": "m2"}}, "summary": "Timeout"},
        ]});
        let read = |path: &str| (path == "src/a.rs").then(|| SOURCE.to_string());
        let sources = rust_mutation_sources(&mutants, &outcomes, &read);
        assert_eq!(sources.len(), 2);
        assert_eq!(sources[0].path, "src/a.rs");
        assert_eq!(sources[0].source, SOURCE);
        let m1 = &sources[0].mutants[0];
        assert_eq!(
            (m1.outcome.as_str(), m1.start_line, m1.start_col),
            ("missed", 3, 6)
        );
        assert_eq!(m1.recorded_diff.as_deref(), Some(DIFF));
        assert_eq!(sources[0].mutants[1].outcome, "timeout");
        assert_eq!(sources[1].source, "");
        assert_eq!(sources[1].mutants[0].outcome, NO_OUTCOME);
    }
}
