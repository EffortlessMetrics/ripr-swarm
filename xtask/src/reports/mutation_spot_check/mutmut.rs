//! Parser for a mutmut (3.x) mutant dump, producing the language-neutral
//! [`MutationSource`] values the synthetic-diff join scores (a second Python
//! tool beside [`super::cosmic_ray`]).
//!
//! mutmut keeps its results in `mutants/<path>.meta` files and has no
//! machine-readable export of a mutant's location, so the hosted workflow
//! writes the dump itself: for every mutant listed by `mutmut results --all
//! true` it records `mutmut show <name>`. One JSON object per line:
//!
//! ```text
//! {"name": "pkg.mod.x_fn__mutmut_3", "status": "...", "diff": "<mutmut show>"}
//! ```
//!
//! `mutmut show` diffs the original function against the mutant function
//! (both read from mutmut's own trampolined module), so the diff is the real
//! mutation, but its line numbers count from the function's first line and a
//! method is dedented to column 0. `mutmut apply`, which would give a file
//! diff, is not used: it replaces the first function of that name anywhere in
//! the module (any class's `__init__`), which garbles the file for a third of
//! the mutants of a class-heavy package.
//!
//! The path comes from the `+++ <path>` header. The hunk's original side
//! (context and removed lines) is found in the checkout as a block at one
//! constant indent (a line inside a multi-line string may also match
//! verbatim); it must occur exactly once, so a dump from another revision, or
//! code repeated verbatim in two places, is not scored. `mutmut show`
//! re-renders blocks at 4 spaces per level, so a package indented otherwise
//! finds no placement. The replaced range and replacement are the smallest
//! span between the dump's own removed and added lines (common prefix and
//! suffix removed), mapped onto the checkout (a line matched through the
//! indent shifts by it, a string-interior line does not), so a multi-line
//! mutant is located exactly too. A diff that cannot be located,
//! has several changed hunks, or only inserts or deletes whole lines keeps an
//! `unlocatable_*` outcome token: it stays countable in the receipt and is
//! never scored. A record with no diff at all has no path, so it cannot join
//! a source file; such records are counted by status in the receipt's tool
//! identity (`records_without_diff`) instead of disappearing.
//!
//! A line that is not a well-formed record fails the parse: a silently
//! skipped mutant would leave the population short of the real set.

use super::synthetic_diff::{MutationSource, SyntheticMutant};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const UNLOCATABLE_DIFF: &str = "unlocatable_diff";
const UNLOCATABLE_MULTIHUNK: &str = "unlocatable_multihunk";
const UNLOCATABLE_AMBIGUOUS: &str = "unlocatable_ambiguous";

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

/// Path of a unified diff, from its `+++ <path>` header (or `--- <path>`),
/// without a git `a/`/`b/` prefix.
fn diff_path(diff: &str) -> Option<String> {
    let mut old = None;
    for line in diff.lines() {
        if line.starts_with("@@") {
            break;
        }
        if let Some(path) = line.strip_prefix("+++ ") {
            let path = path.strip_prefix("b/").unwrap_or(path).trim_end();
            if path != "/dev/null" && !path.is_empty() {
                return Some(path.to_string());
            }
        } else if let Some(path) = line.strip_prefix("--- ") {
            let path = path.strip_prefix("a/").unwrap_or(path).trim_end();
            if path != "/dev/null" && !path.is_empty() {
                old = Some(path.to_string());
            }
        }
    }
    old
}

/// The body lines of each hunk, as `(marker, text)` with marker ' ', '-' or
/// '+'. Header line numbers are not used: they count from the function.
fn hunks(diff: &str) -> Vec<Vec<(char, &str)>> {
    let mut hunks: Vec<Vec<(char, &str)>> = Vec::new();
    for line in diff.split('\n') {
        if line.starts_with("@@") {
            hunks.push(Vec::new());
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        let mut chars = line.chars();
        if let Some(marker @ (' ' | '-' | '+')) = chars.next() {
            hunk.push((marker, chars.as_str()));
        }
    }
    hunks
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

/// Whether checkout line `source` holds function-relative line `line` at
/// `indent`: blank matches blank, and a line inside a multi-line string keeps
/// its own indentation, so it may also match verbatim.
fn line_matches(source: &str, indent: &str, line: &str) -> bool {
    if line.trim().is_empty() {
        return source.trim().is_empty();
    }
    source.strip_prefix(indent) == Some(line) || source == line
}

/// Every `(start index, indent)` at which `old` (the hunk's original side)
/// occurs in `source_lines`. Each non-blank line of `old` is tried as the
/// anchor that fixes the indent, because a line inside a multi-line string
/// keeps its absolute indent in `mutmut show` and would fix the wrong one.
fn placements<'s>(old: &[&str], source_lines: &[&'s str]) -> Vec<(usize, &'s str)> {
    let mut found: Vec<(usize, &str)> = Vec::new();
    for (anchor, anchor_text) in old.iter().enumerate() {
        if anchor_text.trim().is_empty() {
            continue;
        }
        for (index, source) in source_lines.iter().enumerate() {
            let Some(indent) = source.strip_suffix(*anchor_text) else {
                continue;
            };
            if !indent.chars().all(char::is_whitespace) {
                continue;
            }
            let Some(start) = index.checked_sub(anchor) else {
                continue;
            };
            if found.iter().any(|(at, _)| *at == start) {
                continue;
            }
            let fits = old.iter().enumerate().all(|(offset, line)| {
                source_lines
                    .get(start + offset)
                    .is_some_and(|source| line_matches(source, indent, line))
            });
            if fits {
                found.push((start, indent));
            }
        }
    }
    found
}

/// The replaced range (start/end `(line, utf16 column)`, end exclusive) and
/// replacement text of a one-hunk function-relative diff against
/// `source_lines`, or the reason it cannot be located.
fn locate(diff: &str, source_lines: &[&str]) -> Result<Location, &'static str> {
    let hunks = hunks(diff);
    let changed: Vec<&Vec<(char, &str)>> = hunks
        .iter()
        .filter(|hunk| hunk.iter().any(|(marker, _)| *marker != ' '))
        .collect();
    let [hunk] = changed.as_slice() else {
        return Err(if changed.is_empty() {
            UNLOCATABLE_DIFF
        } else {
            UNLOCATABLE_MULTIHUNK
        });
    };
    // One contiguous change block: context, then -/+ lines, then context.
    let first_change = hunk.iter().position(|(m, _)| *m != ' ');
    let last_change = hunk.iter().rposition(|(m, _)| *m != ' ');
    let (Some(first_change), Some(last_change)) = (first_change, last_change) else {
        return Err(UNLOCATABLE_DIFF);
    };
    if hunk[first_change..=last_change]
        .iter()
        .any(|(m, _)| *m == ' ')
    {
        return Err(UNLOCATABLE_MULTIHUNK);
    }
    let old: Vec<&str> = hunk
        .iter()
        .filter(|(m, _)| *m != '+')
        .map(|(_, text)| *text)
        .collect();
    let added: Vec<&str> = hunk
        .iter()
        .filter(|(m, _)| *m == '+')
        .map(|(_, text)| *text)
        .collect();
    // Index in `old` of the first removed line, and how many there are.
    let removed_at = hunk[..first_change].len();
    let removed_count = old.len() - removed_at - (hunk.len() - last_change - 1);
    // A pure insertion or deletion of whole lines has no in-line range the
    // join can express.
    if removed_count == 0 || added.is_empty() {
        return Err(UNLOCATABLE_DIFF);
    }
    let found = placements(&old, source_lines);
    let (start, indent) = match found.as_slice() {
        [one] => *one,
        [] => return Err(UNLOCATABLE_DIFF),
        _ => return Err(UNLOCATABLE_AMBIGUOUS),
    };
    let first = start + removed_at;
    // The edit is found on the dump's own lines, then mapped onto the
    // checkout: a line that matched through the indent shifts its columns by
    // the indent, a line inside a multi-line string (matched verbatim) does
    // not.
    let removed = &old[removed_at..removed_at + removed_count];
    let via_indent: Vec<bool> = removed
        .iter()
        .enumerate()
        .map(|(offset, line)| {
            !line.trim().is_empty()
                && source_lines[first + offset].strip_prefix(indent) == Some(*line)
                && !indent.is_empty()
        })
        .collect();
    let old = removed.join("\n");
    let new = added.join("\n");
    if old == new {
        return Err(UNLOCATABLE_DIFF);
    }
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
    let indent_width = utf16_len(indent);
    let to_source = |offset: usize| {
        let (line, column) = position_in(&old, offset, 0);
        let index = usize::try_from(line).unwrap_or(usize::MAX);
        let shift = if via_indent.get(index).copied().unwrap_or(false) {
            indent_width
        } else {
            0
        };
        ((first + index + 1) as u64, column + shift)
    };
    let start_pos = to_source(prefix);
    let end_pos = to_source(old.len() - suffix);
    // A replacement that spans lines re-indents its later lines when the
    // edit starts in code (not inside a multi-line string).
    let raw = &new[prefix..new.len() - suffix];
    let starts_in_code = via_indent
        .get(usize::try_from(position_in(&old, prefix, 0).0).unwrap_or(usize::MAX))
        .copied()
        .unwrap_or(false);
    let replacement = if starts_in_code {
        raw.split('\n')
            .enumerate()
            .map(|(index, part)| {
                if index == 0 || part.is_empty() {
                    part.to_string()
                } else {
                    format!("{indent}{part}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        raw.to_string()
    };
    Ok((start_pos, end_pos, replacement))
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
            "report_format": "mutmut show dump",
            "version": version,
            "records": records,
            "records_without_diff": without_diff,
        }),
    ))
}

/// Record the pytest arguments a repo's mutmut run used. Each
/// `--deselect`/`--ignore` target (either `--flag=value` or `--flag value`)
/// lands in `excluded_tests`, and any `-k`/`-m` selection also marks the
/// suite reduced, because a left-out test cannot kill a mutant.
pub(crate) fn record_test_args(tool: &mut Value, test_args: &str) {
    let mut excluded = Vec::new();
    let mut selects = false;
    let mut words = test_args.split_whitespace().peekable();
    while let Some(word) = words.next() {
        if let Some((flag, value)) = word.split_once('=')
            && matches!(flag, "--deselect" | "--ignore" | "--ignore-glob")
        {
            excluded.push(value.to_string());
        } else if matches!(word, "--deselect" | "--ignore" | "--ignore-glob") {
            if let Some(value) = words.next() {
                excluded.push(value.to_string());
            }
        } else if matches!(word, "-k" | "-m") || word.starts_with("-k=") || word.starts_with("-m=")
        {
            selects = true;
        }
    }
    if let Some(fields) = tool.as_object_mut() {
        fields.insert("test_args".into(), json!(test_args));
        fields.insert(
            "reduced_suite".into(),
            json!(selects || !excluded.is_empty()),
        );
        fields.insert("excluded_tests".into(), json!(excluded));
    }
}

#[cfg(test)]
mod tests {
    use super::super::synthetic_diff::synthesize_edit;
    use super::*;

    const SOURCE: &str = concat!(
        "def f(x):\n",
        "    return x >= 1\n",
        "\n",
        "class A:\n",
        "    def __init__(self, s):\n",
        "        \"\"\"Doc\n",
        "        more doc.\"\"\"\n",
        "        self.s = \"é\" + s + 'é'\n",
        "\n",
        "class B:\n",
        "    def __init__(self, s):\n",
        "        self.s = call(\n",
        "            s,\n",
        "            1,\n",
        "        )\n",
        "\n",
        "class C:\n",
        "    def __init__(self, s):\n",
        "        self.s = call(\n",
        "            s,\n",
        "            1,\n",
        "        )\n",
    );

    fn read(path: &str) -> Option<String> {
        (path == "src/m.py").then(|| SOURCE.to_string())
    }

    fn record(name: &str, status: &str, diff: &str) -> String {
        json!({"name": name, "status": status, "diff": diff}).to_string()
    }

    /// `mutmut show` output: a status comment, then a function-relative diff
    /// of the dedented function.
    fn show(body: &str) -> String {
        format!("# m.x: survived\n--- src/m.py\n+++ src/m.py\n@@ -1,4 +1,4 @@\n{body}") // ripr-allow: static-language: mutmut status name
    }

    #[test]
    fn test_args_record_left_out_tests_and_mark_the_suite_reduced() {
        let mut tool = json!({"name": "mutmut"});
        record_test_args(
            &mut tool,
            "-p no:cacheprovider -o addopts= --deselect=tests/a.py::t1 --deselect tests/a.py::t2 --ignore=tests/b.py",
        );
        assert_eq!(tool["reduced_suite"], true);
        assert_eq!(
            tool["excluded_tests"],
            json!(["tests/a.py::t1", "tests/a.py::t2", "tests/b.py"])
        );
        assert_eq!(tool["name"], "mutmut");

        let mut full = json!({});
        record_test_args(&mut full, "-p no:cacheprovider -o addopts=");
        assert_eq!(full["reduced_suite"], false);
        assert_eq!(full["excluded_tests"], json!([]));

        let mut keyword = json!({});
        record_test_args(&mut keyword, "-k not_slow");
        assert_eq!(keyword["reduced_suite"], true);
    }
    #[test]
    fn locates_a_top_level_function_mutant_by_its_text() -> Result<(), String> {
        let dump = record(
            "m.x_f__mutmut_1",
            "survived", // ripr-allow: static-language: mutmut status name
            &show(" def f(x):\n-    return x >= 1\n+    return x > 1"),
        );
        let (sources, tool) = parse_mutmut_dump(&dump, &read, Some("3.8.1"))?;
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, "src/m.py");
        let mutant = &sources[0].mutants[0];
        // The smallest span, not the operator token: `>=` -> `>` deletes `=`.
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
    fn a_dedented_method_is_re_indented_and_its_docstring_matches_verbatim() -> Result<(), String> {
        // mutmut renders a method at column 0; the docstring's continuation
        // line keeps its own indentation inside the string.
        let diff = show(concat!(
            " def __init__(self, s):\n",
            "     \"\"\"Doc\n",
            "         more doc.\"\"\"\n",
            "-    self.s = \"é\" + s + 'é'\n",
            "+    self.s = \"XXéXX\" + s + 'é'",
        ));
        let dump = record("m.xǁAǁ__init____mutmut_2", "killed", &diff); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read, None)?;
        let mutant = &sources[0].mutants[0];
        // Columns count UTF-16 units; the shared prefix ends before `é`.
        assert_eq!(mutant.replacement, "XXéXX");
        assert_eq!(
            (mutant.start_line, mutant.start_col, mutant.end_col),
            (8, 18, 19)
        );
        assert_eq!(mutant.outcome, "caught");
        let edit = synthesize_edit("src/m.py", SOURCE, mutant).map_err(str::to_string)?;
        assert!(
            edit.mutated
                .contains("        self.s = \"XXéXX\" + s + 'é'\n"),
            "{}",
            edit.mutated
        );
        Ok(())
    }

    #[test]
    fn docstring_interior_context_before_the_change_does_not_fix_the_indent() -> Result<(), String>
    {
        // The first context lines sit inside a docstring, so they keep their
        // absolute indent; the anchor must come from a code line.
        let src = "class K:\n    def m(self):\n        \"\"\"Doc.\n\n        Details.\n        \"\"\"\n        return 1 + 2\n";
        let read_k = |_: &str| Some(src.to_string());
        let diff = show(concat!(
            "         Details.\n",
            "         \"\"\"\n",
            "-    return 1 + 2\n",
            "+    return 1 - 2",
        ));
        let dump = record("m.xǁKǁm__mutmut_1", "survived", &diff); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read_k, None)?;
        let mutant = &sources[0].mutants[0];
        assert_eq!(mutant.outcome, "missed");
        assert_eq!(mutant.replacement, "-");
        assert_eq!(
            (mutant.start_line, mutant.start_col, mutant.end_col),
            (7, 17, 18)
        );
        Ok(())
    }

    #[test]
    fn an_edit_inside_a_multi_line_string_is_not_re_indented() -> Result<(), String> {
        let src = "def f():\n    s = \"\"\"a\n        b\"\"\"\n    return s\n";
        let read_f = |_: &str| Some(src.to_string());
        let diff = show(concat!(
            " def f():\n",
            "-    s = \"\"\"a\n",
            "-        b\"\"\"\n",
            "+    s = \"\"\"XXa\n",
            "+        bXX\"\"\"\n",
            "     return s",
        ));
        let dump = record("m.x_f__mutmut_1", "killed", &diff); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read_f, None)?;
        let mutant = &sources[0].mutants[0];
        let edit = synthesize_edit("src/m.py", src, mutant).map_err(str::to_string)?;
        assert_eq!(
            edit.mutated,
            "def f():\n    s = \"\"\"XXa\n        bXX\"\"\"\n    return s\n"
        );
        Ok(())
    }

    #[test]
    fn code_repeated_verbatim_is_ambiguous_and_never_scored() -> Result<(), String> {
        let diff = show(concat!(
            " def __init__(self, s):\n",
            "     self.s = call(\n",
            "         s,\n",
            "-        1,\n",
            "+        2,\n",
            "     )",
        ));
        let dump = record("m.xǁBǁ__init____mutmut_1", "survived", &diff); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read, None)?;
        assert_eq!(sources[0].mutants[0].outcome, UNLOCATABLE_AMBIGUOUS);
        Ok(())
    }

    #[test]
    fn multi_line_mutants_are_located_exactly() -> Result<(), String> {
        let src = "def h(a, b):\n    return call(\n        a,\n        b,\n    )\n";
        let read_h = |_: &str| Some(src.to_string());
        let diff = show(concat!(
            " def h(a, b):\n",
            "     return call(\n",
            "-        a,\n",
            "-        b,\n",
            "+        b,\n",
            "     )",
        ));
        let dump = record("m.x_h__mutmut_2", "killed", &diff); // ripr-allow: static-language: mutmut status name
        let (sources, _) = parse_mutmut_dump(&dump, &read_h, None)?;
        let mutant = &sources[0].mutants[0];
        assert_eq!(mutant.replacement, "");
        assert_eq!(
            (
                mutant.start_line,
                mutant.start_col,
                mutant.end_line,
                mutant.end_col
            ),
            (3, 8, 4, 8)
        );
        let edit = synthesize_edit("src/m.py", src, mutant).map_err(str::to_string)?;
        assert!(
            edit.mutated
                .contains("    return call(\n        b,\n    )\n")
        );
        Ok(())
    }

    #[test]
    fn stale_multihunk_and_empty_diffs_are_kept_but_never_scored() -> Result<(), String> {
        let stale = show(" def f(x):\n-    return x >= 2\n+    return x > 2");
        let two = show(concat!(
            " def f(x):\n-    return x >= 1\n+    return x > 1\n",
            "@@ -9 +9 @@\n-    y = 1\n+    y = 2",
        ));
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
            ("check was interrupted by user", "pending"),
            ("something new", "unknown_status"),
        ] {
            assert_eq!(outcome_of(status), outcome, "{status}");
        }
    }

    #[test]
    fn malformed_records_and_unreadable_files_fail_the_parse() -> Result<(), String> {
        let other = "--- src/other.py\n+++ src/other.py\n@@ -1 +1 @@\n-a\n+b\n";
        let no_diff = r#"{"name": "a", "status": "killed"}"#; // ripr-allow: static-language: mutmut status name
        let unreadable = record("o", "killed", other); // ripr-allow: static-language: mutmut status name
        for dump in ["", "not json", no_diff, unreadable.as_str()] {
            if let Ok((sources, _)) = parse_mutmut_dump(dump, &read, None) {
                return Err(format!(
                    "{dump:?} should fail the parse, got {} sources",
                    sources.len()
                ));
            }
        }
        Ok(())
    }
}
