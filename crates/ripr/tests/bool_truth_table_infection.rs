//! End-to-end witness for RIPR-SPEC-0353 (#6488): a pure bool `&&`/`||`
//! owner has no literal boundary, so its infection is read from the truth
//! table the directly-called literal rows give. Rows that separate the chain
//! from its one-step mutations read `yes`; anything else keeps its reading.

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
            "ripr-bool-truth-table-{}-{sequence}",
            std::process::id()
        ));
        // Built before any write so `Drop` removes a partly written root.
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
            "[package]\nname = \"truth_table\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The findings on `line` of `src/lib.rs` after applying `old` -> `new`.
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

fn lib_with(owner_body: &str, extra: &str) -> String {
    format!(
        "/// Ship gate.\n///\n/// Paid and in stock.\npub fn can_ship(paid: bool, in_stock: bool) -> bool {{\n{owner_body}}}\n{extra}\npub fn tail() {{}}\n"
    )
}

fn test(calls: &[(&str, bool)]) -> String {
    let body = calls
        .iter()
        .map(|(args, want)| format!("    assert_eq!(can_ship({args}), {want});\n"))
        .collect::<String>();
    format!("use truth_table::can_ship;\n\n#[test]\nfn ships() {{\n{body}}}\n")
}

/// The predicate findings on the reordered chain.
fn predicate_findings(lib: &str, test: &str) -> Result<Vec<Finding>, String> {
    let findings =
        findings_on_changed_line(lib, "    in_stock && paid", "    paid && in_stock", test)?;
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

const THREE_ROWS: &[(&str, bool)] = &[
    ("true, true", true),
    ("true, false", false),
    ("false, true", false),
];

#[test]
fn rows_that_separate_the_chain_from_every_one_step_mutant_credit_infection() -> Result<(), String>
{
    let findings = predicate_findings(&lib_with("    in_stock && paid\n", ""), &test(THREE_ROWS))?;
    assert!(infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn an_or_chain_pinned_on_its_own_table_credits_infection() -> Result<(), String> {
    let lib = "/// Review gate.\n///\n/// Flagged or large.\npub fn can_ship(paid: bool, in_stock: bool) -> bool {\n    in_stock || paid\n}\n\npub fn tail() {}\n";
    let findings = findings_on_changed_line(
        lib,
        "    in_stock || paid",
        "    paid || in_stock",
        &test(&[
            ("false, false", false),
            ("true, false", true),
            ("false, true", true),
        ]),
    )?;
    let predicates = findings
        .into_iter()
        .filter(|finding| finding.probe.family.as_str() == "predicate")
        .collect::<Vec<_>>();
    assert!(infection_credited(&predicates), "{predicates:?}");
    Ok(())
}

#[test]
fn rows_that_cannot_tell_the_connectives_apart_stay_unknown() -> Result<(), String> {
    let findings = predicate_findings(
        &lib_with("    in_stock && paid\n", ""),
        &test(&[("true, true", true), ("false, false", false)]),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_single_row_stays_unknown() -> Result<(), String> {
    let findings = predicate_findings(
        &lib_with("    in_stock && paid\n", ""),
        &test(&[("true, false", false)]),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn another_caller_of_the_owner_stays_unknown() -> Result<(), String> {
    let wrapper = "\npub fn wrapped(paid: bool) -> bool {\n    can_ship(paid, true)\n}\n";
    let findings = predicate_findings(
        &lib_with("    in_stock && paid\n", wrapper),
        &test(THREE_ROWS),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn rows_fed_through_a_table_stay_unknown() -> Result<(), String> {
    let table = "use truth_table::can_ship;\n\n#[test]\nfn ships() {\n    let rows = [(true, true, true), (true, false, false), (false, true, false)];\n    for (paid, in_stock, want) in rows {\n        assert_eq!(can_ship(paid, in_stock), want);\n    }\n}\n";
    let findings = predicate_findings(&lib_with("    in_stock && paid\n", ""), table)?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn a_body_with_a_leading_statement_stays_unknown() -> Result<(), String> {
    let findings = predicate_findings(
        &lib_with("    let paid = !paid;\n    in_stock && paid\n", ""),
        &test(THREE_ROWS),
    )?;
    assert!(!infection_credited(&findings), "{findings:?}");
    Ok(())
}
