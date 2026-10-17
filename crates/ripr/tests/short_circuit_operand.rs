//! End-to-end witness for a changed `||`/`&&` operand that no test input
//! evaluates (#7970): an input at the unchanged operands boundary does not
//! reach the edit when the earlier operand short-circuits.

use ripr::domain::StageState;
use ripr::{CheckInput, CheckOutput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
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
            "ripr-short-circuit-operand-{}-{sequence}",
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
            "[package]\nname = \"short_circuit\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

fn lib(operator: &str, right: &str) -> String {
    format!(
        "/// Either operand.\n///\n/// Zero on either side.\npub fn either(x: i32, y: i32) -> bool {{\n    x == 0 {operator} {right}\n}}\n\npub fn tail() {{}}\n"
    )
}

fn test(calls: &[(&str, bool)]) -> String {
    let body = calls
        .iter()
        .map(|(args, want)| format!("    assert_eq!(either({args}), {want});\n"))
        .collect::<String>();
    format!("use short_circuit::either;\n\n#[test]\nfn either_cases() {{\n{body}}}\n")
}

fn predicate_findings(
    operator: &str,
    new: &str,
    old: &str,
    calls: &[(&str, bool)],
) -> Result<Vec<Finding>, String> {
    let source = lib(operator, new);
    let findings = findings_on_changed_line(
        &source,
        &format!("    x == 0 {operator} {new}"),
        &format!("    x == 0 {operator} {old}"),
        &test(calls),
    )?;
    let predicates = findings
        .into_iter()
        .filter(|finding| finding.probe.family.as_str() == "predicate")
        .collect::<Vec<_>>();
    assert!(!predicates.is_empty(), "no predicate finding");
    Ok(predicates)
}

/// No finding reads `exposed`, and none credits infection: the changed
/// operand is never evaluated, so no test input reaches it.
fn none_exposed(findings: &[Finding]) -> bool {
    findings.iter().all(|finding| {
        finding.class != ExposureClass::Exposed && finding.ripr.infect.state != StageState::Yes
    })
}

#[test]
fn an_or_whose_first_operand_always_short_circuits_is_not_exposed() -> Result<(), String> {
    // Every call has `x == 0`, so `||` never evaluates `y == 1`.
    let findings = predicate_findings("||", "y == 1", "y == 0", &[("0, 3", true), ("0, 0", true)])?;
    assert!(none_exposed(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn an_and_whose_first_operand_always_short_circuits_is_not_exposed() -> Result<(), String> {
    let findings = predicate_findings(
        "&&",
        "y == 1",
        "y == 0",
        &[("1, 3", false), ("2, 1", false)],
    )?;
    assert!(none_exposed(&findings), "{findings:?}");
    Ok(())
}

#[test]
fn an_edit_to_the_first_operand_is_always_evaluated() -> Result<(), String> {
    // Control: `x == 1` is the first operand, so it runs on every input and
    // `x = 1` is a boundary input for it.
    let source = "/// Either operand.\n///\n/// Zero on either side.\npub fn either(x: i32, y: i32) -> bool {\n    x == 1 || y == 0\n}\n\npub fn tail() {}\n";
    let findings = findings_on_changed_line(
        source,
        "    x == 1 || y == 0",
        "    x == 0 || y == 0",
        &test(&[("1, 3", true), ("2, 3", false)]),
    )?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.ripr.infect.state == StageState::Yes),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn an_unreadable_argument_keeps_the_ordinary_reading() -> Result<(), String> {
    // Control: `next()` is not a literal, so ripr cannot show the call
    // short-circuits and does not claim it.
    let source = lib("||", "y == 1");
    let findings = findings_on_changed_line(
        &source,
        "    x == 0 || y == 1",
        "    x == 0 || y == 0",
        "use short_circuit::either;\n\n#[test]\nfn either_cases() {\n    assert_eq!(either(next(), 3), true);\n    assert_eq!(either(0, 0), true);\n}\n\nfn next() -> i32 {\n    0\n}\n",
    )?;
    assert!(!findings.is_empty());
    assert!(
        findings
            .iter()
            .all(|finding| !finding.ripr.infect.summary.contains("short-circuits")),
        "{findings:?}"
    );
    Ok(())
}

fn assert_no_skipped_operand_claim(findings: &[Finding]) {
    assert!(!findings.is_empty());
    assert!(
        findings
            .iter()
            .all(|finding| !finding.ripr.infect.summary.contains("short-circuits")),
        "{findings:?}"
    );
}

#[test]
fn a_wrapper_test_may_evaluate_the_changed_operand() -> Result<(), String> {
    // Control: `wrap(1, 1)` reaches `either` with `x != 0`, so the direct
    // `either(0, _)` calls are not every input.
    let findings = findings_on_changed_line(
        &lib("||", "y == 1"),
        "    x == 0 || y == 1",
        "    x == 0 || y == 0",
        "use short_circuit::either;\n\nfn wrap(x: i32, y: i32) -> bool {\n    either(x, y)\n}\n\n#[test]\nfn direct() {\n    assert_eq!(either(0, 3), true);\n}\n\n#[test]\nfn wrapped() {\n    assert_eq!(wrap(1, 1), true);\n}\n",
    )?;
    assert_no_skipped_operand_claim(&findings);
    Ok(())
}

#[test]
fn a_helper_test_may_evaluate_the_changed_operand() -> Result<(), String> {
    let findings = findings_on_changed_line(
        &lib("||", "y == 1"),
        "    x == 0 || y == 1",
        "    x == 0 || y == 0",
        "use short_circuit::either;\n\nfn check(x: i32, y: i32) {\n    assert!(either(x, y));\n}\n\n#[test]\nfn direct() {\n    assert_eq!(either(0, 3), true);\n}\n\n#[test]\nfn helped() {\n    check(1, 1);\n}\n",
    )?;
    assert_no_skipped_operand_claim(&findings);
    Ok(())
}

fn only_the_test_differs(test: &str) -> Result<(), String> {
    let findings = findings_on_changed_line(
        &lib("||", "y == 1"),
        "    x == 0 || y == 1",
        "    x == 0 || y == 0",
        test,
    )?;
    assert_no_skipped_operand_claim(&findings);
    Ok(())
}

#[test]
fn a_hex_literal_argument_is_not_read_as_decimal() -> Result<(), String> {
    // Control: `0x1` is 1, so `either(0x1, 1)` skips `x == 0` and evaluates
    // `y == 1`; the argument extractor reads it as `0`.
    only_the_test_differs(
        "use short_circuit::either;\n\n#[test]\nfn hex() {\n    assert_eq!(either(0x0, 3), true);\n    assert_eq!(either(0x1, 1), true);\n}\n",
    )
}

#[test]
fn two_owner_calls_on_one_line_are_not_read_as_one() -> Result<(), String> {
    // Control: the second call, `either(1, 1)`, evaluates `y == 1`.
    only_the_test_differs(
        "use short_circuit::either;\n\n#[test]\nfn pair() {\n    assert!(either(0, 3) && either(1, 1));\n}\n",
    )
}

#[test]
fn a_test_macro_may_expand_to_an_owner_call() -> Result<(), String> {
    // Control: `chk!(1, 1)` expands to `either(1, 1)`.
    only_the_test_differs(
        "use short_circuit::either;\n\nmacro_rules! chk {\n    ($a:expr, $b:expr) => {\n        assert!(either($a, $b));\n    };\n}\n\n#[test]\nfn through_macro() {\n    assert_eq!(either(0, 3), true);\n    chk!(1, 1);\n}\n",
    )
}

#[test]
fn an_owner_reached_through_a_local_binding_is_not_a_direct_call() -> Result<(), String> {
    // Control: `call(1, 1)` is `either(1, 1)`, but no `either(` call names it
    // and no other function calls `either` by name.
    only_the_test_differs(
        "use short_circuit::either;\n\n#[test]\nfn bound() {\n    assert_eq!(either(0, 3), true);\n    let call = either;\n    assert!(call(1, 1));\n}\n",
    )
}

#[test]
fn a_shadowed_parameter_is_not_the_called_value() -> Result<(), String> {
    // Control: `let x = x + 1;` makes `x == 0` false for `either(0, _)`, so
    // the call does evaluate `y == 1`.
    let source = "/// Either operand.\n///\n/// Zero on either side.\npub fn either(x: i32, y: i32) -> bool {\n    let x = x + 1;\n    x == 0 || y == 1\n}\n\npub fn tail() {}\n";
    let findings = findings_on_changed_line(
        source,
        "    x == 0 || y == 1",
        "    x == 0 || y == 0",
        &test(&[("0, 1", false), ("0, 2", false)]),
    )?;
    assert_no_skipped_operand_claim(&findings);
    Ok(())
}

#[test]
fn an_or_with_an_input_that_evaluates_the_changed_operand_reads_exposed() -> Result<(), String> {
    // Positive twin: `(1, 1)` skips `x == 0` and reaches `y == 1` on its
    // boundary.
    let findings = predicate_findings(
        "||",
        "y == 1",
        "y == 0",
        &[("0, 3", true), ("1, 1", true), ("1, 2", false)],
    )?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.ripr.infect.state == StageState::Yes),
        "{findings:?}"
    );
    Ok(())
}
