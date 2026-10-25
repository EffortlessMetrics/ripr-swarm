//! End-to-end witness for the emptiness-guard rule (RIPR-SPEC-0158, #9619): a
//! `text.len() == 0` guard has no literal boundary operand, so its infection
//! is read from the empty and non-empty rows the related tests feed it,
//! whether the rows are direct calls or a literal table loop. Rows on one side
//! only, other literals and unreadable rows keep the unknown reading.

use ripr::domain::StageState;
use ripr::{CheckInput, CheckOutput, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    /// `lib` is the new `src/lib.rs` and `files` the other sources by path;
    /// `diff` changes one line of one of them.
    fn create(lib: &str, files: &[(&str, &str)], test: &str, diff: &str) -> Result<Self, String> {
        // A process-wide sequence, not the clock: parallel tests share one
        // `SystemTime` on some platforms.
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-empty-guard-{}-{sequence}",
            std::process::id()
        ));
        // Exclusive create, so `Drop` never removes a directory this run did
        // not make; the guard owns the root before any other write.
        std::fs::create_dir(&root)
            .map_err(|error| format!("create root directory failed: {error}"))?;
        let repo = Self { root };
        let root = &repo.root;
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        let write = |path: &str, text: &str| {
            if let Some(parent) = root.join(path).parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(root.join(path), text)
                .map_err(|error| format!("write {path} failed: {error}"))
        };
        write(
            "Cargo.toml",
            "[package]\nname = \"empty_guard\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )?;
        write("src/lib.rs", lib)?;
        for (path, text) in files {
            write(path, text)?;
        }
        write("tests/it.rs", test)?;
        write("diff.patch", diff)?;
        Ok(repo)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn diff_at(path: &str, text: &str, new: &str, old: &str) -> Result<(String, usize), String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut matches = lines.iter().enumerate().filter(|(_, line)| **line == new);
    let (index, _) = matches
        .next()
        .ok_or_else(|| format!("{path} has no line {new:?}"))?;
    if matches.next().is_some() || index < 3 || index + 3 > lines.len() {
        return Err(format!(
            "{path} needs one {new:?} with three lines before and two after"
        ));
    }
    let context = |range: std::ops::Range<usize>| {
        lines[range]
            .iter()
            .map(|line| format!(" {line}\n"))
            .collect::<String>()
    };
    let start = index - 2;
    Ok((
        format!(
            "diff --git a/{path} b/{path}\nindex 0000000..1111111 100644\n--- a/{path}\n+++ b/{path}\n@@ -{start},6 +{start},6 @@\n{}-{old}\n+{new}\n{}",
            context(index - 3..index),
            context(index + 1..index + 3)
        ),
        index + 1,
    ))
}

/// The findings on the changed line of `src/lib.rs` after applying `old` -> `new`.
fn findings_on_changed_line(
    lib: &str,
    new: &str,
    old: &str,
    test: &str,
) -> Result<Vec<Finding>, String> {
    let (diff, line) = diff_at("src/lib.rs", lib, new, old)?;
    let repo = TempRepo::create(lib, &[], test, &diff)?;
    let output = repo.check()?;
    Ok(output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect())
}

fn lib_with(guard: &str) -> String {
    format!(
        "/// First letter.\n///\n/// Question mark for no name.\npub fn initial(name: &str) -> char {{\n{guard}        return '?';\n    }}\n    name.chars().next().unwrap_or('?').to_ascii_uppercase()\n}}\n\npub fn tail() {{}}\n"
    )
}

const OLD: &str = "    if name.is_empty() {";
const NEW: &str = "    if name.len() == 0 {";

fn guard_findings(new: &str, test: &str) -> Result<Vec<Finding>, String> {
    let findings = findings_on_changed_line(&lib_with(&format!("{new}\n")), new, OLD, test)?;
    let predicates = findings
        .into_iter()
        .filter(|finding| finding.probe.family.as_str() == "predicate")
        .collect::<Vec<_>>();
    assert!(!predicates.is_empty(), "no predicate finding");
    Ok(predicates)
}

fn infection_credited(findings: &[Finding]) -> bool {
    findings
        .iter()
        .any(|finding| finding.ripr.infect.state == StageState::Yes)
}

fn direct(calls: &[(&str, char)]) -> String {
    let body = calls
        .iter()
        .map(|(name, want)| format!("    assert_eq!(initial({name}), {want:?});\n"))
        .collect::<String>();
    format!("use empty_guard::initial;\n\n#[test]\nfn initials() {{\n{body}}}\n")
}

fn table(rows: &[(&str, char)], body: &str) -> String {
    let rows = rows
        .iter()
        .map(|(name, want)| format!("({name}, {want:?})"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "use empty_guard::initial;\n\n#[test]\nfn initials() {{\n    let cases = [{rows}];\n    for (name, want) in cases {{\n{body}    }}\n}}\n"
    )
}

const ASSERT_ROW: &str = "        assert_eq!(initial(name), want, \"{name:?}\");\n";
const BOTH_SIDES: &[(&str, char)] = &[("\"\"", '?'), ("\"ada\"", 'A'), ("\"Bob\"", 'B')];

#[test]
fn a_table_loop_with_an_empty_and_a_non_empty_row_credits_infection() -> Result<(), String> {
    let findings = guard_findings(NEW, &table(BOTH_SIDES, ASSERT_ROW))?;
    assert!(infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn direct_calls_with_an_empty_and_a_non_empty_input_credit_infection() -> Result<(), String> {
    let findings = guard_findings(NEW, &direct(&[("\"\"", '?'), ("\"ada\"", 'A')]))?;
    assert!(infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_not_empty_guard_with_both_kinds_of_row_credits_infection() -> Result<(), String> {
    let findings = guard_findings("    if name.len() != 0 {", &table(BOTH_SIDES, ASSERT_ROW))?;
    // `!= 0` with the same rows: the empty row and a non-empty row both sit on
    // the guard's two sides.
    assert!(infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn rows_that_are_all_empty_stay_unknown() -> Result<(), String> {
    let findings = guard_findings(NEW, &table(&[("\"\"", '?'), ("\"\"", '?')], ASSERT_ROW))?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn rows_that_are_all_non_empty_stay_unknown() -> Result<(), String> {
    let findings = guard_findings(
        NEW,
        &table(&[("\"ada\"", 'A'), ("\"Bob\"", 'B')], ASSERT_ROW),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_computed_cell_hides_the_rows_and_stays_unknown() -> Result<(), String> {
    let findings = guard_findings(
        NEW,
        &table(
            &[("String::new().as_str()", '?'), ("\"ada\"", 'A')],
            ASSERT_ROW,
        ),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_char_cell_is_no_text_row_and_stays_unknown() -> Result<(), String> {
    let findings = guard_findings(NEW, &table(&[("'a'", 'A'), ("\"ada\"", 'A')], ASSERT_ROW))?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_loop_that_skips_rows_stays_unknown() -> Result<(), String> {
    let body = "        if want == 'B' {\n            continue;\n        }\n        assert_eq!(initial(name), want, \"{name:?}\");\n";
    let findings = guard_findings(NEW, &table(BOTH_SIDES, body))?;
    // A loop that can leave a row early does not run every row, so the table
    // is unreadable and the guard keeps its unknown reading.
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_length_other_than_zero_stays_unknown() -> Result<(), String> {
    let findings = guard_findings("    if name.len() == 1 {", &table(BOTH_SIDES, ASSERT_ROW))?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_weakly_asserted_call_of_the_owner_stays_unknown() -> Result<(), String> {
    // A weakly asserted call of the owner is not read as pinning the guard
    // (`idiom-result-empty-guard`), even when the table rows are strong.
    let test = format!(
        "{}\n#[test]\nfn one_letter() {{\n    assert!(initial(\"a\") == 'A' || true);\n    let _ = initial(\"b\");\n}}\n",
        table(BOTH_SIDES, ASSERT_ROW)
    );
    let findings = guard_findings(NEW, &test)?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_compound_condition_stays_unknown() -> Result<(), String> {
    let findings = guard_findings(
        "    if name.len() == 0 || name == \"x\" {",
        &table(BOTH_SIDES, ASSERT_ROW),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn one_unreadable_row_among_readable_ones_stays_unknown() -> Result<(), String> {
    let findings = guard_findings(
        NEW,
        &direct(&[("\"\"", '?'), ("\"ada\"", 'A'), ("unknown_name", 'X')]),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_table_beside_an_unread_direct_call_stays_unknown() -> Result<(), String> {
    let test = format!(
        "{}\n#[test]\nfn other() {{\n    assert_eq!(initial(unknown_name), 'X');\n}}\n",
        table(BOTH_SIDES, ASSERT_ROW)
    );
    let findings = guard_findings(NEW, &test)?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn an_unasserted_empty_call_is_no_input_row() -> Result<(), String> {
    let test = "use empty_guard::initial;\n\n#[test]\nfn initials() {\n    let _ = initial(\"\");\n    assert_eq!(initial(\"ada\"), 'A');\n    assert_eq!(initial(\"Bob\"), 'B');\n}\n";
    let findings = guard_findings(NEW, test)?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}
