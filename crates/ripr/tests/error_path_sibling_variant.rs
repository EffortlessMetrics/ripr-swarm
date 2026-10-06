//! End-to-end witness for RIPR-SPEC-0106 sibling-variant binding (#6673,
//! #6695).
//!
//! A test that pins a SIBLING variant of the changed error's enum
//! (`assert!(matches!(e, PayError::Limit))` when the changed line returns
//! `PayError::Insufficient`) shares only the enum qualifier with the changed
//! line. That qualifier is not the changed behavior's identity, so the
//! sibling pin must never certify the changed error path as `exposed`. The
//! exact-variant pin of the same shape stays the positive control.

use ripr::{
    CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, ProbeFamily, check_workspace,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const PAY_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum PayError {
    Insufficient,
    Limit,
}

pub fn withdraw(balance: u64, amount: u64) -> Result<u64, PayError> {
    if amount > 100 {
        return Err(PayError::Limit);
    }
    if amount > balance {
        return Err(PayError::Insufficient);
    }
    Ok(balance - amount)
}
"#;
const PAY_CHANGED_LINE: usize = 12;
const PAY_OLD_LINE: &str = "        return Err(PayError::Limit);";

const CODE_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum CodeError {
    NotDigit,
    TooLong,
}

fn digit(c: char) -> Option<u32> {
    c.to_digit(10)
}

pub fn parse_code(s: &str) -> Result<u32, CodeError> {
    if s.len() > 4 {
        return Err(CodeError::TooLong);
    }
    let mut value = 0;
    for c in s.chars() {
        let d = digit(c).ok_or_else(|| CodeError::NotDigit)?;
        value = value * 10 + d;
    }
    Ok(value)
}
"#;
const CODE_CHANGED_LINE: usize = 17;
const CODE_OLD_LINE: &str = "        let d = digit(c).ok_or_else(|| CodeError::TooLong)?;";

fn pay_test(pattern: &str) -> String {
    format!(
        "use error_path_sibling_variant::{{PayError, withdraw}};\n\n#[test]\nfn withdraw_error_is_pinned() {{\n    match withdraw(10, 20) {{\n        Ok(left) => panic!(\"unexpected {{left}}\"),\n        Err(e) => assert!(matches!(e, {pattern})),\n    }}\n}}\n"
    )
}

fn code_test(pattern: &str) -> String {
    format!(
        "use error_path_sibling_variant::{{CodeError, parse_code}};\n\n#[test]\nfn parse_code_error_is_pinned() {{\n    match parse_code(\"12x4\") {{\n        Ok(value) => panic!(\"unexpected {{value}}\"),\n        Err(e) => assert!(matches!(e, {pattern})),\n    }}\n}}\n"
    )
}

/// A whole-file unified diff that replaces exactly `changed_line` (1-based)
/// of `source` with its new text, from `old_line`.
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
            "ripr-error-path-sibling-variant-{}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::create_dir_all(root.join("tests"))
            .map_err(|error| format!("create test directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"error-path-sibling-variant\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), source)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("tests/pin.rs"), test_source)
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

/// The single error-path finding on the changed line, after asserting the
/// fixture produced it from the intended changed expression.
fn changed_error_path<'a>(
    output: &'a CheckOutput,
    line: usize,
    expression_fragment: &str,
) -> Result<&'a ripr::Finding, String> {
    let mut matches = output.findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ErrorPath && finding.probe.location.line == line
    });
    let finding = matches.next().ok_or_else(|| {
        let observed = output
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{}:{}:{}:{}",
                    finding.probe.family.as_str(),
                    finding.probe.location.line,
                    finding.class.as_str(),
                    finding.probe.expression
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");
        format!("missing error_path finding at line {line}; observed {observed}")
    })?;
    if matches.next().is_some() {
        return Err(format!("duplicate error_path findings at line {line}"));
    }
    if !finding.probe.expression.contains(expression_fragment) {
        return Err(format!(
            "error_path finding at line {line} is `{}`, not the changed `{expression_fragment}`",
            finding.probe.expression
        ));
    }
    Ok(finding)
}

/// Every finding on the changed line, so a sibling pin cannot certify the
/// changed error through a sibling family either.
fn exposed_on_line(output: &CheckOutput, line: usize) -> Vec<String> {
    output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.location.line == line && finding.class == ExposureClass::Exposed
        })
        .map(|finding| {
            format!(
                "{}:{}",
                finding.probe.family.as_str(),
                finding.probe.expression
            )
        })
        .collect()
}

#[test]
fn exact_variant_matches_pin_exposes_the_changed_return_err() -> Result<(), String> {
    let diff = single_line_diff(PAY_SOURCE, PAY_CHANGED_LINE, PAY_OLD_LINE);
    let repo = TempRepo::create(PAY_SOURCE, &pay_test("PayError::Insufficient"), &diff)?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, PAY_CHANGED_LINE, "PayError::Insufficient")?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the exact-variant pin is the positive control; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    Ok(())
}

#[test]
fn sibling_variant_matches_pin_cannot_expose_the_changed_return_err() -> Result<(), String> {
    let diff = single_line_diff(PAY_SOURCE, PAY_CHANGED_LINE, PAY_OLD_LINE);
    let repo = TempRepo::create(PAY_SOURCE, &pay_test("PayError::Limit"), &diff)?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, PAY_CHANGED_LINE, "PayError::Insufficient")?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a PayError::Limit pin shares only the enum qualifier with the changed PayError::Insufficient; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    let exposed = exposed_on_line(&output, PAY_CHANGED_LINE);
    assert!(
        exposed.is_empty(),
        "no family may borrow the sibling pin: {exposed:?}"
    );
    Ok(())
}

#[test]
fn exact_variant_matches_pin_exposes_the_changed_ok_or_else() -> Result<(), String> {
    let diff = single_line_diff(CODE_SOURCE, CODE_CHANGED_LINE, CODE_OLD_LINE);
    let repo = TempRepo::create(CODE_SOURCE, &code_test("CodeError::NotDigit"), &diff)?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, CODE_CHANGED_LINE, "CodeError::NotDigit")?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the exact-variant pin is the positive control; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    Ok(())
}

#[test]
fn sibling_variant_matches_pin_cannot_expose_the_changed_ok_or_else() -> Result<(), String> {
    let diff = single_line_diff(CODE_SOURCE, CODE_CHANGED_LINE, CODE_OLD_LINE);
    let repo = TempRepo::create(CODE_SOURCE, &code_test("CodeError::TooLong"), &diff)?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, CODE_CHANGED_LINE, "CodeError::NotDigit")?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a CodeError::TooLong pin shares only the enum qualifier with the changed CodeError::NotDigit; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    let exposed = exposed_on_line(&output, CODE_CHANGED_LINE);
    assert!(
        exposed.is_empty(),
        "no family may borrow the sibling pin: {exposed:?}"
    );
    Ok(())
}
