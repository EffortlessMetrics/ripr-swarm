//! End-to-end witness for RIPR-SPEC-0227 rule 3b by decision 4 (#7063).
//!
//! A `?` that is its function's only way to return `Err` reads `exposed`
//! when a test asserts the owner call returns `Err`: swallowing the `?`
//! would make that call return `Ok`. Every control below keeps the
//! RIPR-SPEC-0107 reading (not `exposed`).

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOLE_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum E {
    Bad,
}

fn digit(c: char) -> Result<u32, E> {
    c.to_digit(10).ok_or(E::Bad)
}

pub fn total(s: &str) -> Result<u32, E> {
    let mut sum = 0;
    for c in s.chars() {
        let d = (digit(c))?;
        sum += d;
    }
    Ok(sum)
}
"#;

/// The same `?` with an earlier `?` that can also return `Err`
/// (RIPR-SPEC-0227 example 13).
const EARLIER_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum E {
    Bad,
}

fn digit(c: char) -> Result<u32, E> {
    c.to_digit(10).ok_or(E::Bad)
}

pub fn total(s: &str) -> Result<u32, E> {
    digit(s.chars().next().unwrap_or('0'))?;
    let mut sum = 0;
    for c in s.chars() {
        let d = (digit(c))?;
        sum += d;
    }
    Ok(sum)
}
"#;

const SOLE_LINE: usize = 13;
const EARLIER_LINE: usize = 14;
const OLD_LINE: &str = "        let d = digit(c)?;";

fn test_file(body: &str) -> String {
    format!(
        "use question_mark_side_flip::total;\n\n#[test]\nfn non_digit_is_refused() {{\n    {body}\n}}\n"
    )
}

fn single_line_diff(source: &str, changed_line: usize, old_line: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut body = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index + 1 == changed_line {
            body.push_str(&format!("-{old_line}\n+{line}\n"));
        } else {
            body.push_str(&format!(" {line}\n"));
        }
    }
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs\nindex 1111111..2222222 100644\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,{count} +1,{count} @@\n{body}",
        count = lines.len()
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, test_source: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "ripr-question-mark-side-flip-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"question-mark-side-flip\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/refuse.rs"), test_source)
            .map_err(|error| format!("write test failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
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

/// The class of the single error-path finding on the changed `?` line,
/// after asserting the fixture produced it from the changed expression.
fn error_path_class(
    source: &str,
    line: usize,
    old_line: &str,
    test_body: &str,
) -> Result<ExposureClass, String> {
    let diff = single_line_diff(source, line, old_line);
    let repo = TempRepo::create(source, &test_file(test_body), &diff)?;
    let output = repo.check()?;
    let mut matches = output.findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ErrorPath
            && finding.probe.location.line == line
            && finding.probe.after.is_some()
    });
    let finding = matches
        .next()
        .ok_or_else(|| format!("missing error_path finding at line {line}"))?;
    if matches.next().is_some() {
        return Err(format!("duplicate error_path findings at line {line}"));
    }
    if !finding.probe.expression.contains("(digit(c))?") {
        return Err(format!(
            "error_path finding is `{}`, not the changed `?` line",
            finding.probe.expression
        ));
    }
    // A removed-line probe at the same coordinate must not read exposed
    // either: only the current `?` line can carry this credit.
    let class = finding.class.clone();
    if class != ExposureClass::Exposed
        && output
            .findings
            .iter()
            .any(|other| other.probe.location.line == line && other.class == ExposureClass::Exposed)
    {
        return Err(format!("another finding at line {line} reads exposed"));
    }
    Ok(class)
}

#[test]
fn an_err_side_assertion_exposes_a_sole_source_question_mark() -> Result<(), String> {
    for body in [
        "assert!(total(\"x\").is_err());",
        "assert!(matches!(total(\"x\"), Err(_)));",
    ] {
        assert_eq!(
            error_path_class(SOLE_SOURCE, SOLE_LINE, OLD_LINE, body)?,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn an_ok_side_assertion_keeps_the_gap() -> Result<(), String> {
    let class = error_path_class(
        SOLE_SOURCE,
        SOLE_LINE,
        OLD_LINE,
        "assert!(total(\"7\").is_ok());",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_earlier_error_source_keeps_the_gap() -> Result<(), String> {
    let class = error_path_class(
        EARLIER_SOURCE,
        EARLIER_LINE,
        OLD_LINE,
        "assert!(total(\"x\").is_err());",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

/// The edit swaps which fallible call the `?` propagates: an `is_err()`
/// test cannot tell the two errors apart (rule 1).
#[test]
fn a_changed_question_mark_operand_keeps_the_gap() -> Result<(), String> {
    let class = error_path_class(
        SOLE_SOURCE,
        SOLE_LINE,
        "        let d = strict_digit(c)?;",
        "assert!(total(\"x\").is_err());",
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_wrapped_or_shadowed_assertion_keeps_the_gap() -> Result<(), String> {
    for body in [
        "assert!(Some(total(\"x\")).unwrap().is_err());",
        "if false { assert!(total(\"x\").is_err()); }",
    ] {
        assert_ne!(
            error_path_class(SOLE_SOURCE, SOLE_LINE, OLD_LINE, body)?,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    let shadowed = format!(
        "macro_rules! assert {{ ($($t:tt)*) => {{}}; }}\n\n{}",
        test_file("assert!(total(\"x\").is_err());")
    );
    let diff = single_line_diff(SOLE_SOURCE, SOLE_LINE, OLD_LINE);
    let repo = TempRepo::create(SOLE_SOURCE, &shadowed, &diff)?;
    let output = repo.check()?;
    let exposed = output.findings.iter().any(|finding| {
        finding.probe.location.line == SOLE_LINE && finding.class == ExposureClass::Exposed
    });
    assert!(!exposed, "a file-local `assert!` macro may be a no-op");
    Ok(())
}
