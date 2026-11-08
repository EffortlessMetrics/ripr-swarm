//! Parser for cosmic-ray's `cosmic-ray dump <session.sqlite>` output, producing
//! the language-neutral [`MutationSource`] values the synthetic-diff join
//! scores (the Python counterpart of [`super::stryker`]).
//!
//! `dump` writes one JSON array per line: `[work_item, result]`. The work item
//! carries `job_id` and `mutations[]` (`module_path`, `operator_name`,
//! `occurrence`, `start_pos`, `end_pos`, each position `[line, column]`); the
//! result carries `worker_outcome`, `test_outcome`, `output` and a unified
//! `diff` of the mutation, or is `null` while the mutant is still pending.
//!
//! cosmic-ray does not record the replacement text, and its columns count
//! Python code points, not UTF-16 units. Both are recovered from the file the
//! mutant was made in: the single removed/added line pair of `diff` gives the
//! mutated line, and the columns are verified against it. A mutant whose
//! range does not reproduce its own diff (or that spans several lines) is not
//! guessed at: it keeps an `unlocatable_*` outcome token, so the join never
//! scores it and the receipt shows how many there were.
//!
//! A line that is not a well-formed `[work_item, result]` pair fails the parse:
//! a silently skipped mutant would leave the population short of the real set.

use super::synthetic_diff::{MutationSource, SyntheticMutant};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Outcome token for a result that has no usable mutation location.
const UNLOCATABLE_DIFF: &str = "unlocatable_diff";
const UNLOCATABLE_MULTILINE: &str = "unlocatable_multiline";

/// Join outcome token for a worker/test outcome pair. Only a normal run whose
/// tests detected the mutant is `caught` and only a normal run whose tests did
/// not is `missed`; every other combination keeps its own token and is never
/// scored.
pub(crate) fn outcome_of(worker: Option<&str>, test: Option<&str>, output: &str) -> String {
    match (worker, test) {
        (None, _) => "pending",
        // cosmic-ray reports a test timeout as a normal run, detected, with output "timeout".
        (Some("normal"), Some("killed")) if output == "timeout" => "timeout", // ripr-allow: static-language: cosmic-ray outcome name
        (Some("normal"), Some("killed")) => "caught", // ripr-allow: static-language: cosmic-ray outcome name
        (Some("normal"), Some("survived")) => "missed", // ripr-allow: static-language: cosmic-ray outcome name
        (Some("normal"), Some("incompetent")) => "unviable",
        (Some("abnormal"), _) if output.to_ascii_lowercase().contains("timeout") => "timeout",
        (Some("abnormal" | "exception"), _) => "runtime_error",
        (Some("no-test"), _) => "no_test",
        (Some("skipped"), _) => "ignored",
        _ => "unknown_status",
    }
    .to_string()
}

fn position(value: Option<&Value>, what: &str) -> Result<(u64, u64), String> {
    let pair = value
        .and_then(Value::as_array)
        .filter(|pair| pair.len() == 2)
        .ok_or_else(|| format!("{what} is not a [line, column] pair"))?;
    let line = pair[0]
        .as_u64()
        .filter(|line| *line >= 1)
        .ok_or_else(|| format!("{what} line is not a 1-based integer"))?;
    let column = pair[1]
        .as_u64()
        .ok_or_else(|| format!("{what} column is not a 0-based integer"))?;
    Ok((line, column))
}

/// The one removed and one added line of a mutation diff, without markers,
/// provided every original-side line of every hunk (context and removed, at
/// the hunk header's line numbers) matches `source_lines`. A dump made from
/// another revision whose mutated line is unchanged but whose surrounding
/// code moved or changed is therefore refused, not scored.
fn changed_line_pair<'a>(
    diff: &'a str,
    source_lines: &[&str],
) -> Option<(usize, &'a str, &'a str)> {
    let mut removed = None;
    let mut added = None;
    let mut cursor: Option<usize> = None;
    for line in diff.split('\n') {
        if let Some(header) = line.strip_prefix("@@ -") {
            let start = header.split([',', ' ']).next()?.parse::<usize>().ok()?;
            cursor = Some(start.saturating_sub(1));
            continue;
        }
        // File headers (`--- ...`, `+++ ...`) precede the first hunk.
        let Some(at) = cursor.as_mut() else {
            continue;
        };
        if let Some(text) = line.strip_prefix('+') {
            added.replace(text).map_or(Some(()), |_| None)?;
            continue;
        }
        let (text, is_removed) = if let Some(text) = line.strip_prefix('-') {
            (text, true)
        } else if let Some(text) = line.strip_prefix(' ') {
            (text, false)
        } else {
            continue;
        };
        if source_lines.get(*at) != Some(&text) {
            return None;
        }
        *at += 1;
        if is_removed {
            removed
                .replace((*at - 1, text))
                .map_or(Some(()), |_| None)?;
        }
    }
    let (index, removed) = removed?;
    Some((index, removed, added?))
}

/// The replacement text for `[start, end)` (Python code-point columns on
/// `original`) that turns `original` into `mutated`, or `None` when the range
/// does not reproduce the diff.
fn replacement_for(original: &str, mutated: &str, start: usize, end: usize) -> Option<String> {
    let old: Vec<char> = original.chars().collect();
    let new: Vec<char> = mutated.chars().collect();
    if start > end || end > old.len() {
        return None;
    }
    let tail = old.len() - end;
    if new.len() < start + tail
        || old[..start] != new[..start]
        || old[end..] != new[new.len() - tail..]
    {
        return None;
    }
    Some(new[start..new.len() - tail].iter().collect())
}

/// UTF-16 offset of code-point column `col` in `line`.
fn code_points_to_utf16(line: &str, col: usize) -> u64 {
    line.chars().take(col).map(|c| c.len_utf16() as u64).sum()
}

/// Parse a `cosmic-ray dump` into sources (sorted by path, mutants by
/// position) and a tool-identity object for the receipt. `read_source`
/// returns the checkout's text for a report path.
pub(crate) fn parse_cosmic_ray_dump(
    dump: &str,
    read_source: &dyn Fn(&str) -> Option<String>,
    version: Option<&str>,
) -> Result<(Vec<MutationSource>, Value), String> {
    let mut by_path: BTreeMap<String, (String, Vec<SyntheticMutant>)> = BTreeMap::new();
    let mut lines = 0usize;
    for (index, text) in dump.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        lines += 1;
        let what = format!("dump line {}", index + 1);
        let pair: Value =
            serde_json::from_str(text).map_err(|err| format!("{what} is not JSON: {err}"))?;
        let pair = pair
            .as_array()
            .filter(|pair| pair.len() == 2)
            .ok_or_else(|| format!("{what} is not a [work_item, result] pair"))?;
        let item = &pair[0];
        let result = &pair[1];
        let job_id = item
            .get("job_id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{what} work item has no string `job_id`"))?;
        let mutations = item
            .get("mutations")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{what} work item has no `mutations` array"))?;
        let [mutation] = mutations.as_slice() else {
            return Err(format!(
                "{what} (job {job_id}) has {} mutations; the join scores one mutation per job",
                mutations.len()
            ));
        };
        let text_of = |key: &str| {
            mutation
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{what} mutation has no string `{key}`"))
        };
        let raw_path = text_of("module_path")?;
        let path = raw_path.strip_prefix("./").unwrap_or(raw_path).to_string();
        let operator = text_of("operator_name")?.to_string();
        let (start_line, start_cp) =
            position(mutation.get("start_pos"), &format!("{what} start_pos"))?;
        let (end_line, end_cp) = position(mutation.get("end_pos"), &format!("{what} end_pos"))?;

        let (worker, test, output, diff) = if result.is_null() {
            (None, None, "", None)
        } else {
            (
                Some(
                    result
                        .get("worker_outcome")
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("{what} result has no string `worker_outcome`"))?,
                ),
                result.get("test_outcome").and_then(Value::as_str),
                result.get("output").and_then(Value::as_str).unwrap_or(""),
                result.get("diff").and_then(Value::as_str),
            )
        };
        let mut outcome = outcome_of(worker, test, output);
        let status = format!("{}/{}", worker.unwrap_or("pending"), test.unwrap_or("none"));

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
        let line_text = usize::try_from(start_line)
            .ok()
            .and_then(|line| source_lines.get(line - 1).copied());

        // Columns and replacement are recovered from the mutated file; any
        // failure keeps the mutant visible but unscoreable.
        let location = if start_line != end_line {
            outcome = UNLOCATABLE_MULTILINE.to_string();
            None
        } else {
            line_text.zip(diff).and_then(|(original, diff)| {
                let (index, removed, added) = changed_line_pair(diff, &source_lines)?;
                // The removed line must be the mutant's own line, not an equal
                // line elsewhere in the file.
                if removed != original || u64::try_from(index + 1).ok()? != start_line {
                    return None;
                }
                let (start, end) = (
                    usize::try_from(start_cp).ok()?,
                    usize::try_from(end_cp).ok()?,
                );
                let replacement = replacement_for(original, added, start, end)?;
                Some((
                    replacement,
                    code_points_to_utf16(original, start),
                    code_points_to_utf16(original, end),
                ))
            })
        };
        if location.is_none() && matches!(outcome.as_str(), "caught" | "missed") {
            outcome = UNLOCATABLE_DIFF.to_string();
        }
        let (replacement, start_col, end_col) =
            location.unwrap_or((String::new(), start_cp, end_cp));
        entry.1.push(SyntheticMutant {
            id: job_id.to_string(),
            mutator: operator,
            replacement,
            start_line,
            start_col,
            end_line,
            end_col,
            status,
            outcome,
        });
    }
    if lines == 0 {
        return Err("cosmic-ray dump has no work items".to_string());
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
        // The dump does not name its producer, so the version is whatever
        // the caller states (`--cosmic-ray-version`), or null.
        json!({"name": "cosmic-ray", "report_format": "cosmic-ray dump", "version": version}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "def f(x):\n    return x >= 1\n\ndef g(s):\n    return \"é\" + s + 'é'\n";

    fn diff(old: &str, new: &str) -> String {
        format!(
            "--- mutation diff ---\n+++ mutation diff ---\n@@ -1,3 +1,3 @@\n def f(x):\n-{old}\n+{new}\n \n"
        )
    }

    fn pair(job: &str, start: (u64, u64), end: (u64, u64), result: &Value) -> String {
        json!([
            {"job_id": job, "mutations": [{
                "module_path": "src/m.py", "operator_name": "core/ReplaceComparisonOperator_GtE_Gt",
                "occurrence": 0, "start_pos": [start.0, start.1], "end_pos": [end.0, end.1],
                "operator_args": {}
            }]},
            result
        ])
        .to_string()
    }

    fn result(worker: &str, test: &str, old: &str, new: &str) -> Value {
        json!({"worker_outcome": worker, "test_outcome": test, "output": "", "diff": diff(old, new)})
    }

    fn read(path: &str) -> Option<String> {
        (path == "src/m.py").then(|| SOURCE.to_string())
    }

    #[test]
    fn recovers_replacement_and_zero_based_columns_from_the_diff() -> Result<(), String> {
        let dump = pair(
            "a",
            (2, 13),
            (2, 15),
            &result(
                "normal",
                "survived", // ripr-allow: static-language: cosmic-ray outcome name
                "    return x >= 1",
                "    return x > 1",
            ),
        );
        let (sources, tool) = parse_cosmic_ray_dump(&dump, &read, None)?;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, "src/m.py");
        let mutant = &sources[0].mutants[0];
        assert_eq!(mutant.replacement, ">");
        assert_eq!(
            (mutant.start_line, mutant.start_col, mutant.end_col),
            (2, 13, 15)
        );
        assert_eq!(mutant.outcome, "missed");
        assert_eq!(mutant.status, "normal/survived"); // ripr-allow: static-language: cosmic-ray outcome name
        assert_eq!(tool["name"], "cosmic-ray");
        Ok(())
    }

    #[test]
    fn code_point_columns_become_utf16_columns() -> Result<(), String> {
        // A non-BMP character before the range widens UTF-16 columns by one.
        let src = "x = \"😀\" + 1\n";
        let read_wide = |_: &str| Some(src.to_string());
        let dump = json!([
            {"job_id": "w", "mutations": [{"module_path": "m.py", "operator_name": "o",
                "occurrence": 0, "start_pos": [1, 8], "end_pos": [1, 9]}]},
            {"worker_outcome": "normal", "test_outcome": "killed", // ripr-allow: static-language: cosmic-ray outcome name
             "diff": "--- a\n+++ b\n@@ -1 +1 @@\n-x = \"😀\" + 1\n+x = \"😀\" - 1\n"}
        ])
        .to_string();
        let (sources, _) = parse_cosmic_ray_dump(&dump, &read_wide, None)?;
        let mutant = &sources[0].mutants[0];
        assert_eq!(mutant.replacement, "-");
        assert_eq!((mutant.start_col, mutant.end_col), (9, 10));
        assert_eq!(mutant.outcome, "caught");
        Ok(())
    }

    #[test]
    fn outcomes_map_only_normal_runs_to_scoreable_tokens() {
        for ((worker, test, output), outcome) in [
            ((Some("normal"), Some("killed"), ""), "caught"), // ripr-allow: static-language: cosmic-ray outcome name
            ((Some("normal"), Some("survived"), ""), "missed"), // ripr-allow: static-language: cosmic-ray outcome name
            ((Some("normal"), Some("incompetent"), ""), "unviable"),
            ((Some("normal"), Some("killed"), "timeout"), "timeout"), // ripr-allow: static-language: cosmic-ray outcome name
            ((Some("abnormal"), None, "boom"), "runtime_error"),
            ((Some("exception"), None, ""), "runtime_error"),
            ((Some("no-test"), None, ""), "no_test"),
            ((Some("skipped"), None, ""), "ignored"),
            ((None, None, ""), "pending"),
            ((Some("normal"), None, ""), "unknown_status"),
            ((Some("new"), Some("killed"), ""), "unknown_status"), // ripr-allow: static-language: cosmic-ray outcome name
        ] {
            assert_eq!(
                outcome_of(worker, test, output),
                outcome,
                "{worker:?}/{test:?}"
            );
        }
    }

    #[test]
    fn a_range_that_does_not_reproduce_the_diff_is_never_scored() -> Result<(), String> {
        // Range shifted left: the unchanged suffix no longer matches.
        let dump = pair(
            "off",
            (2, 12),
            (2, 14),
            &result("normal", "killed", "    return x >= 1", "    return x > 1"), // ripr-allow: static-language: cosmic-ray outcome name
        );
        let (sources, _) = parse_cosmic_ray_dump(&dump, &read, None)?;
        assert_eq!(sources[0].mutants[0].outcome, UNLOCATABLE_DIFF);
        // The diff's removed line is not the file's line (stale checkout).
        let stale = pair(
            "stale",
            (2, 13),
            (2, 15),
            &result("normal", "killed", "    return y >= 1", "    return y > 1"), // ripr-allow: static-language: cosmic-ray outcome name
        );
        let (sources, _) = parse_cosmic_ray_dump(&stale, &read, None)?;
        assert_eq!(sources[0].mutants[0].outcome, UNLOCATABLE_DIFF);
        // A multi-line range is not synthesized.
        let multi = pair(
            "multi",
            (2, 4),
            (3, 0),
            &result("normal", "killed", "    return x >= 1", "    return x > 1"), // ripr-allow: static-language: cosmic-ray outcome name
        );
        let (sources, _) = parse_cosmic_ray_dump(&multi, &read, None)?;
        assert_eq!(sources[0].mutants[0].outcome, UNLOCATABLE_MULTILINE);
        Ok(())
    }

    #[test]
    fn a_diff_whose_context_differs_from_the_checkout_is_never_scored() -> Result<(), String> {
        // Same removed line, but the dump's context line is from another
        // revision (`def f(y):`), so its outcome belongs to other code.
        let stale_context = "--- mutation diff ---\n--- a/src/m.py\n+++ b/src/m.py\n@@ -1,2 +1,2 @@\n def f(y):\n-    return x >= 1\n+    return x > 1\n";
        let moved = "--- a/src/m.py\n+++ b/src/m.py\n@@ -4,1 +4,1 @@\n-    return x >= 1\n+    return x > 1\n";
        for diff in [stale_context, moved] {
            let dump = pair(
                "ctx",
                (2, 13),
                (2, 15),
                &json!({"worker_outcome": "normal", "test_outcome": "killed", "diff": diff}), // ripr-allow: static-language: cosmic-ray outcome name
            );
            let (sources, _) = parse_cosmic_ray_dump(&dump, &read, None)?;
            assert_eq!(sources[0].mutants[0].outcome, UNLOCATABLE_DIFF, "{diff}");
        }
        Ok(())
    }

    #[test]
    fn a_hunk_at_another_equal_line_is_never_scored() -> Result<(), String> {
        let src = "a = 1\nb = 1\na = 1\n";
        let read_twice = |_: &str| Some(src.to_string());
        // The mutant claims line 1, but its diff changes the equal line 3.
        let dump = json!([
            {"job_id": "t", "mutations": [{"module_path": "m.py", "operator_name": "o",
                "occurrence": 0, "start_pos": [1, 4], "end_pos": [1, 5]}]},
            {"worker_outcome": "normal", "test_outcome": "survived", // ripr-allow: static-language: cosmic-ray outcome name
             "diff": "--- a/m.py\n+++ b/m.py\n@@ -2,2 +2,2 @@\n b = 1\n-a = 1\n+a = 2\n"}
        ])
        .to_string();
        let (sources, _) = parse_cosmic_ray_dump(&dump, &read_twice, None)?;
        assert_eq!(sources[0].mutants[0].outcome, UNLOCATABLE_DIFF);
        Ok(())
    }

    #[test]
    fn a_removed_line_that_starts_with_dashes_is_still_located() -> Result<(), String> {
        let src = "x = 1\n--y\n";
        let read_dashes = |_: &str| Some(src.to_string());
        let dump = json!([
            {"job_id": "d", "mutations": [{"module_path": "m.py", "operator_name": "o",
                "occurrence": 0, "start_pos": [2, 0], "end_pos": [2, 1]}]},
            {"worker_outcome": "normal", "test_outcome": "survived", // ripr-allow: static-language: cosmic-ray outcome name
             "diff": "--- mutation diff ---\n--- a/m.py\n+++ b/m.py\n@@ -1,2 +1,2 @@\n x = 1\n---y\n+ -y\n"}
        ])
        .to_string();
        let (sources, tool) = parse_cosmic_ray_dump(&dump, &read_dashes, Some("8.8.0"))?;
        let mutant = &sources[0].mutants[0];
        assert_eq!(mutant.replacement, " ");
        assert_eq!(mutant.outcome, "missed");
        assert_eq!(tool["version"], "8.8.0");
        Ok(())
    }

    #[test]
    fn a_pending_result_has_no_outcome_to_score_and_sources_sort() -> Result<(), String> {
        let dump = format!(
            "{}\n\n{}\n",
            pair("p2", (2, 13), (2, 15), &Value::Null),
            pair("p1", (2, 4), (2, 10), &Value::Null)
        );
        let (sources, _) = parse_cosmic_ray_dump(&dump, &read, None)?;
        let ids: Vec<_> = sources[0].mutants.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["p1", "p2"]);
        assert!(sources[0].mutants.iter().all(|m| m.outcome == "pending"));
        Ok(())
    }

    #[test]
    fn malformed_dumps_are_refused_not_skipped() -> Result<(), String> {
        let good = pair("g", (2, 13), (2, 15), &Value::Null);
        for (name, dump) in [
            ("empty", String::new()),
            ("not json", "nope".to_string()),
            ("not a pair", "[1]".to_string()),
            (
                "no mutations",
                json!([{"job_id": "x", "mutations": []}, null]).to_string(),
            ),
            (
                "bad position",
                json!([{"job_id": "x", "mutations": [{"module_path": "src/m.py",
                "operator_name": "o", "start_pos": [0, 1], "end_pos": [1, 2]}]}, null])
                .to_string(),
            ),
            ("unreadable file", good.replace("src/m.py", "src/gone.py")),
        ] {
            assert!(parse_cosmic_ray_dump(&dump, &read, None).is_err(), "{name}");
        }
        parse_cosmic_ray_dump(&good, &read, None)?;
        Ok(())
    }
}
