//! Parser for a mutmut (3.x) mutant dump, producing the language-neutral
//! [`MutationSource`] values the synthetic-diff join scores (a second Python
//! tool beside [`super::cosmic_ray`]).
//!
//! mutmut keeps its results in `mutants/<path>.meta` files and has no
//! machine-readable export of a mutant's location, so the hosted workflow
//! writes the dump itself: for every mutant listed by `mutmut results --all
//! true` it runs `mutmut apply <name>`, records `git diff -U0 -- <module>`,
//! and restores the tree. One JSON object per line:
//!
//! ```text
//! {"name": "pkg.mod.x_fn__mutmut_3", "status": "...", "diff": "<git diff>"}
//! ```
//!
//! The diff is a real file diff, so the path comes from its `+++ b/<path>`
//! header and the changed range from its one hunk. The replaced range and the
//! replacement are the smallest span that turns the hunk's removed lines into
//! its added lines (common prefix and suffix removed), so a multi-line mutant
//! is located exactly too. Every removed line must match the checkout at the
//! hunk header's line number, so a dump from another revision is not scored.
//! A mutant whose diff is empty, has several hunks or does not match the
//! checkout keeps an `unlocatable_*` outcome token: it stays countable in the
//! receipt and is never scored. A mutant with no diff at all has no path, so
//! it cannot join a source file; such records are counted by status in the
//! receipt's tool identity (`records_without_diff`) instead of disappearing.
//!
//! A line that is not a well-formed record fails the parse: a silently
//! skipped mutant would leave the population short of the real set.

use super::synthetic_diff::{MutationSource, SyntheticMutant};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const UNLOCATABLE_DIFF: &str = "unlocatable_diff";
const UNLOCATABLE_MULTIHUNK: &str = "unlocatable_multihunk";

/// Join outcome token for a mutmut status (the strings of mutmut's
/// `status_by_exit_code`). Only a test run that detected the mutant is
/// `caught` and only one that did not is `missed`. A mutant that mutmut's
/// type checker rejected never ran the tests, so it is not `caught`.
pub(crate) fn outcome_of(status: &str) -> &'static str {
    match status {
        "killed" => "caught",   // ripr-allow: static-language: mutmut status name
        "survived" => "missed", // ripr-allow: static-language: mutmut status name
        "timeout" => "timeout",
        "no tests" => "no_test",
        "skipped" => "ignored",
        "not checked" => "pending",
        "caught by type check" => "unviable", // ripr-allow: static-language: mutmut status name
        "segfault" | "suspicious" => "runtime_error",
        "check was interrupted by user" => "pending",
        _ => "unknown_status",
    }
}

/// Path of a unified diff, from its `+++ b/<path>` header (or `--- a/<path>`
/// for a deletion), without the `a/`/`b/` prefix.
fn diff_path(diff: &str) -> Option<String> {
    let mut old = None;
    for line in diff.lines() {
        if line.starts_with("@@") {
            break;
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            let path = path.strip_prefix("b/").unwrap_or(path).trim_end();
            if path != "/dev/null" {
                return Some(path.to_string());
            }
        } else if let Some(path) = line.strip_prefix("--- ") {
            let path = path.strip_prefix("a/").unwrap_or(path).trim_end();
            if path != "/dev/null" {
                old = Some(path.to_string());
            }
        }
    }
    old
}

/// One hunk's removed and added lines.
struct Hunk<'a> {
    removed: Vec<&'a str>,
    added: Vec<&'a str>,
    /// Context lines in hunk order, with their original line numbers.
    context: Vec<(usize, &'a str)>,
    /// Original line number of each removed line.
    removed_at: Vec<usize>,
}

fn hunks(diff: &str) -> Option<Vec<Hunk<'_>>> {
    let mut hunks: Vec<Hunk<'_>> = Vec::new();
    let mut cursor = 0usize;
    for line in diff.split('\n') {
        if let Some(header) = line.strip_prefix("@@ -") {
            let start = header.split([',', ' ']).next()?.parse::<usize>().ok()?;
            // `-N,0` (pure insertion) names the line *before* the insertion.
            let count = header
                .split(' ')
                .next()
                .and_then(|range| range.split_once(','))
                .map_or(Some(1), |(_, count)| count.parse::<usize>().ok())?;
            let first = if count == 0 { start + 1 } else { start };
            cursor = first;
            hunks.push(Hunk {
                removed: Vec::new(),
                added: Vec::new(),
                context: Vec::new(),
                removed_at: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        if let Some(text) = line.strip_prefix('+') {
            hunk.added.push(text);
        } else if let Some(text) = line.strip_prefix('-') {
            hunk.removed.push(text);
            hunk.removed_at.push(cursor);
            cursor += 1;
        } else if let Some(text) = line.strip_prefix(' ') {
            hunk.context.push((cursor, text));
            cursor += 1;
        }
    }
    Some(hunks)
}

/// UTF-16 length of `text`.
fn utf16_len(text: &str) -> u64 {
    text.chars().map(|c| c.len_utf16() as u64).sum()
}

/// `(line, utf16 column)` of byte offset `offset` in `block`, whose first
/// line is file line `first_line`.
fn position_in(block: &str, offset: usize, first_line: usize) -> (u64, u64) {
    let before = &block[..offset];
    let line = first_line + before.matches('\n').count();
    let column = before.rfind('\n').map_or(before, |at| &before[at + 1..]);
    (line as u64, utf16_len(column))
}

/// Start and end `(line, utf16 column)` (end exclusive) and replacement.
type Location = ((u64, u64), (u64, u64), String);

/// The replaced range (start/end `(line, utf16 column)`, end exclusive) and
/// replacement text of a one-hunk diff against `source_lines`, or the reason
/// it cannot be located.
fn locate(diff: &str, source_lines: &[&str]) -> Result<Location, &'static str> {
    let hunks = hunks(diff).ok_or(UNLOCATABLE_DIFF)?;
    let changed: Vec<&Hunk<'_>> = hunks
        .iter()
        .filter(|hunk| !hunk.removed.is_empty() || !hunk.added.is_empty())
        .collect();
    let [hunk] = changed.as_slice() else {
        return Err(if changed.is_empty() {
            UNLOCATABLE_DIFF
        } else {
            UNLOCATABLE_MULTIHUNK
        });
    };
    // Every original-side line must match the checkout.
    let matches = |(at, text): (usize, &str)| {
        at.checked_sub(1)
            .and_then(|index| source_lines.get(index))
            .is_some_and(|line| *line == text)
    };
    if !hunk.context.iter().copied().all(matches)
        || !hunk
            .removed_at
            .iter()
            .copied()
            .zip(hunk.removed.iter().copied())
            .all(matches)
    {
        return Err(UNLOCATABLE_DIFF);
    }
    // A pure insertion or deletion of whole lines has no in-line range the
    // join can express; mutmut does not produce one for a viable mutant.
    if hunk.removed.is_empty() || hunk.added.is_empty() {
        return Err(UNLOCATABLE_DIFF);
    }
    let first = *hunk.removed_at.first().ok_or(UNLOCATABLE_DIFF)?;
    if hunk
        .removed_at
        .iter()
        .enumerate()
        .any(|(i, at)| *at != first + i)
    {
        return Err(UNLOCATABLE_DIFF);
    }
    let old = hunk.removed.join("\n");
    let new = hunk.added.join("\n");
    let prefix = old
        .char_indices()
        .zip(new.chars())
        .find(|((_, a), b)| a != b)
        .map_or(old.len().min(new.len()), |((at, _), _)| at);
    // `prefix` is a char boundary in both strings (their prefixes are
    // equal), and zipping the remainders bounds the suffix by the shorter one.
    let suffix: usize = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(a, _)| a.len_utf8())
        .sum();
    if old == new {
        return Err(UNLOCATABLE_DIFF);
    }
    let start = position_in(&old, prefix, first);
    let end = position_in(&old, old.len() - suffix, first);
    let replacement = new[prefix..new.len() - suffix].to_string();
    Ok((start, end, replacement))
}

/// Parse a mutmut dump into sources (sorted by path, mutants by position) and
/// a tool-identity object for the receipt. `read_source` returns the
/// checkout's text for a diff path.
pub(crate) fn parse_mutmut_dump(
    dump: &str,
    read_source: &dyn Fn(&str) -> Option<String>,
    version: Option<&str>,
) -> Result<(Vec<MutationSource>, Value), String> {
    let mut by_path: BTreeMap<String, (String, Vec<SyntheticMutant>)> = BTreeMap::new();
    let mut without_diff: BTreeMap<String, usize> = BTreeMap::new();
    let mut records = 0usize;
    for (index, text) in dump.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        records += 1;
        let what = format!("dump line {}", index + 1);
        let record: Value =
            serde_json::from_str(text).map_err(|err| format!("{what} is not JSON: {err}"))?;
        let field = |key: &str| {
            record
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{what} has no string `{key}`"))
        };
        let name = field("name")?;
        let status = field("status")?;
        let diff = field("diff")?;
        let Some(path) = diff_path(diff) else {
            *without_diff.entry(status.to_string()).or_default() += 1;
            continue;
        };
        let path = path.strip_prefix("./").unwrap_or(&path).to_string();
        if !by_path.contains_key(&path) {
            let source = read_source(&path).ok_or_else(|| {
                format!("{what}: cannot read mutated file `{path}` in the checkout")
            })?;
            by_path.insert(path.clone(), (source, Vec::new()));
        }
        let Some(entry) = by_path.get_mut(&path) else {
            return Err(format!("{what}: internal error: `{path}` not tracked"));
        };
        let source_lines: Vec<&str> = entry.0.split('\n').collect();
        let mut outcome = outcome_of(status).to_string();
        let ((start_line, start_col), (end_line, end_col), replacement) =
            match locate(diff, &source_lines) {
                Ok(location) => location,
                Err(reason) => {
                    if matches!(outcome.as_str(), "caught" | "missed") {
                        outcome = reason.to_string();
                    }
                    ((1, 0), (1, 0), String::new())
                }
            };
        // mutmut names a mutant `<module>.<x_function>__mutmut_<n>`; the part
        // before the number names the operator site, not the operator.
        let mutator = name
            .rsplit_once("__mutmut_")
            .map_or(name, |(site, _)| site)
            .to_string();
        entry.1.push(SyntheticMutant {
            id: name.to_string(),
            mutator,
            replacement,
            start_line,
            start_col,
            end_line,
            end_col,
            status: status.to_string(),
            outcome,
        });
    }
    if records == 0 {
        return Err("mutmut dump has no records".to_string());
    }
    let mut sources: Vec<MutationSource> = by_path
        .into_iter()
        .map(|(path, (source, mut mutants))| {
            mutants.sort_by(|a, b| {
                (a.start_line, a.start_col, a.end_line, a.end_col, &a.id).cmp(&(
                    b.start_line,
                    b.start_col,
                    b.end_line,
                    b.end_col,
                    &b.id,
                ))
            });
            MutationSource {
                path,
                source,
                mutants,
            }
        })
        .collect();
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((
        sources,
        json!({
            "name": "mutmut",
            "report_format": "mutmut apply + git diff dump",
            "version": version,
            "records": records,
            "records_without_diff": without_diff,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "def f(x):\n    return x >= 1\n\ndef g(s):\n    return \"é\" + s + 'é'\n\ndef h(a, b):\n    return call(\n        a,\n        b,\n    )\n";

    fn read(path: &str) -> Option<String> {
        (path == "src/m.py").then(|| SOURCE.to_string())
    }

    fn record(name: &str, status: &str, diff: &str) -> String {
        json!({"name": name, "status": status, "diff": diff}).to_string()
    }

    fn one_line_diff(line: usize, old: &str, new: &str) -> String {
        format!(
            "diff --git a/src/m.py b/src/m.py\nindex 1..2 100644\n--- a/src/m.py\n+++ b/src/m.py\n@@ -{line} +{line} @@\n-{old}\n+{new}\n"
        )
    }

    #[test]
    fn recovers_the_smallest_replaced_span_from_a_real_file_diff() -> Result<(), String> {
        let dump = record(
            "m.x_f__mutmut_1",
            "survived", // ripr-allow: static-language: mutmut status name
            &one_line_diff(2, "    return x >= 1", "    return x > 1"),
        );
        let (sources, tool) = parse_mutmut_dump(&dump, &read, Some("3.8.1"))?;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, "src/m.py");
        let mutant = &sources[0].mutants[0];
        // The smallest span, not the operator token: `>=` -> `>` deletes `=`.
        // The join only needs the changed lines, which are identical.
        assert_eq!(mutant.replacement, "");
        assert_eq!(
            (
                mutant.start_line,
                mutant.start_col,
                mutant.end_line,
                mutant.end_col
            ),
            (2, 14, 2, 15)
        );
        assert_eq!(mutant.outcome, "missed");
        assert_eq!(mutant.mutator, "m.x_f");
        assert_eq!(tool["name"], "mutmut");
        assert_eq!(tool["version"], "3.8.1");
        assert_eq!(tool["records"], 1);
        Ok(())
    }

    #[test]
    fn the_span_reproduces_the_diff_through_the_synthetic_join() -> Result<(), String> {
        let diff = one_line_diff(
            5,
            "    return \"é\" + s + 'é'",
            "    return \"XXéXX\" + s + 'é'",
        );
        let dump = record("m.x_g__mutmut_2", "killed", &diff); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read, None)?;
        let mutant = &sources[0].mutants[0];
        // Columns count UTF-16 units; the shared prefix ends before `é`.
        assert_eq!(mutant.replacement, "XXéXX");
        assert_eq!((mutant.start_col, mutant.end_col), (12, 13));
        assert_eq!(mutant.outcome, "caught");
        let edit = super::super::synthetic_diff::synthesize_edit("src/m.py", SOURCE, mutant)
            .map_err(str::to_string)?;
        assert!(
            edit.mutated.contains("    return \"XXéXX\" + s + 'é'\n"),
            "{}",
            edit.mutated
        );
        Ok(())
    }

    #[test]
    fn multi_line_mutants_are_located_exactly() -> Result<(), String> {
        let diff = "--- a/src/m.py\n+++ b/src/m.py\n@@ -9,2 +9,2 @@\n-        a,\n-        b,\n+        a,\n+        None,\n";
        let dump = record("m.x_h__mutmut_1", "survived", diff); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read, None)?;
        let mutant = &sources[0].mutants[0];
        assert_eq!(mutant.replacement, "None");
        assert_eq!(
            (
                mutant.start_line,
                mutant.start_col,
                mutant.end_line,
                mutant.end_col
            ),
            (10, 8, 10, 9)
        );
        let spanning = "--- a/src/m.py\n+++ b/src/m.py\n@@ -9,2 +9,1 @@\n-        a,\n-        b,\n+        b,\n";
        let dump = record("m.x_h__mutmut_2", "killed", spanning); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read, None)?;
        let mutant = &sources[0].mutants[0];
        assert_eq!(mutant.replacement, "");
        assert_eq!(
            (
                mutant.start_line,
                mutant.start_col,
                mutant.end_line,
                mutant.end_col
            ),
            (9, 8, 10, 8)
        );
        let edit = super::super::synthetic_diff::synthesize_edit("src/m.py", SOURCE, mutant)
            .map_err(str::to_string)?;
        assert!(
            edit.mutated
                .contains("    return call(\n        b,\n    )\n")
        );
        Ok(())
    }

    #[test]
    fn stale_multihunk_and_empty_diffs_are_kept_but_never_scored() -> Result<(), String> {
        let stale = one_line_diff(1, "    return x >= 1", "    return x > 1");
        let two = format!(
            "--- a/src/m.py\n+++ b/src/m.py\n@@ -2 +2 @@\n-    return x >= 1\n+    return x > 1\n@@ -5 +5 @@\n-    return \"é\" + s + 'é'\n+    return \"é\" - s + 'é'\n"
        );
        let dump = [
            record("m.x_f__mutmut_1", "killed", &stale), // ripr-allow: static-language: mutmut status name
            record("m.x_f__mutmut_2", "survived", &two), // ripr-allow: static-language: mutmut status name
            record("m.x_f__mutmut_3", "timeout", &stale),
            record("m.x_f__mutmut_4", "survived", ""), // ripr-allow: static-language: mutmut status name
            record("m.x_f__mutmut_5", "no tests", ""),
        ]
        .join("\n");
        let (sources, tool) = parse_mutmut_dump(&dump, &read, None)?;
        let outcomes: Vec<&str> = sources[0]
            .mutants
            .iter()
            .map(|m| m.outcome.as_str())
            .collect();
        assert_eq!(outcomes.len(), 3);
        assert!(outcomes.contains(&UNLOCATABLE_DIFF));
        assert!(outcomes.contains(&UNLOCATABLE_MULTIHUNK));
        // A non-scoreable outcome keeps its own token even when unlocatable.
        assert!(outcomes.contains(&"timeout"));
        assert_eq!(tool["records"], 5);
        assert_eq!(tool["records_without_diff"]["no tests"], 1);
        assert_eq!(tool["records_without_diff"]["survived"], 1); // ripr-allow: static-language: mutmut status name
        Ok(())
    }

    #[test]
    fn statuses_map_only_test_runs_to_scoreable_tokens() {
        for (status, outcome) in [
            ("killed", "caught"),   // ripr-allow: static-language: mutmut status name
            ("survived", "missed"), // ripr-allow: static-language: mutmut status name
            ("timeout", "timeout"),
            ("no tests", "no_test"),
            ("skipped", "ignored"),
            ("not checked", "pending"),
            ("caught by type check", "unviable"), // ripr-allow: static-language: mutmut status name
            ("segfault", "runtime_error"),
            ("suspicious", "runtime_error"),
            ("something new", "unknown_status"),
        ] {
            assert_eq!(outcome_of(status), outcome, "{status}");
        }
    }

    #[test]
    fn malformed_records_and_unreadable_files_fail_the_parse() {
        assert!(parse_mutmut_dump("", &read, None).is_err());
        assert!(parse_mutmut_dump("not json", &read, None).is_err());
        assert!(parse_mutmut_dump(r#"{"name": "a", "status": "killed"}"#, &read, None).is_err()); // ripr-allow: static-language: mutmut status name
        let other = "--- a/src/other.py\n+++ b/src/other.py\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(parse_mutmut_dump(&record("o", "killed", other), &read, None).is_err()); // ripr-allow: static-language: mutmut status name
    }
}
