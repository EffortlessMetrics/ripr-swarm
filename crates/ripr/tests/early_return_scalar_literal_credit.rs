//! #13037 Defect A: exact pins on scalar early returns and on scalar tails
//! beside early returns are credited when every sibling exit builds a
//! different scalar literal (RIPR-SPEC-0197 early-scalar return gate).
//!
//! A changed `return <lit>;` (or scalar tail) whose sibling exits produce
//! only different scalar literals is the owner's only source of `<lit>`, so a
//! passing exact `assert_eq!(owner(..), <lit>)` proves the changed exit ran
//! and observes it. Aliasing shapes stay fail-closed (no credit): a second
//! exit producing the same literal, a parameter/computed exit that may alias
//! the literal for some input, or a non-exact assertion.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn lib(function: &str, assertions: &str) -> String {
    format!(
        "{function}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn it_works() {{
        {assertions}
    }}
}}
"
    )
}

/// A unified diff covering the whole function, with correct hunk line counts,
/// so every changed line pairs at its real source line.
fn fn_diff(before_fn: &str, after_fn: &str) -> String {
    let before_lines: Vec<&str> = before_fn.lines().collect();
    let after_lines: Vec<&str> = after_fn.lines().collect();
    assert_eq!(
        before_lines.len(),
        after_lines.len(),
        "fixture functions must align line for line"
    );
    assert!(
        before_lines
            .iter()
            .zip(after_lines.iter())
            .any(|(before, after)| before != after),
        "fixture functions must differ somewhere"
    );
    let count = before_lines.len();
    let mut hunk = String::new();
    for index in 0..count {
        match before_lines[index] == after_lines[index] {
            true => {
                hunk.push(' ');
                hunk.push_str(before_lines[index]);
                hunk.push('\n');
            }
            false => {
                hunk.push('-');
                hunk.push_str(before_lines[index]);
                hunk.push('\n');
                hunk.push('+');
                hunk.push_str(after_lines[index]);
                hunk.push('\n');
            }
        }
    }
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,{count} +1,{count} @@
{hunk}"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, patch: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-early-return-scalar-credit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"early-return-scalar\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn findings(&self) -> Result<Vec<Finding>, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        Ok(output.findings)
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn finding_on_line<'a>(
    findings: &'a [Finding],
    line: usize,
    expression: &str,
) -> Result<&'a Finding, String> {
    let mut on_line = findings.iter().filter(|finding| {
        finding.probe.family == ProbeFamily::ReturnValue
            && finding.probe.location.line == line
            && finding.probe.expression.contains(expression)
    });
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding),
        other => Err(format!(
            "premise: exactly one return_value finding on line {line} for {expression}, got {other:?} in {findings:?}"
        )),
    }
}

fn confirmed_row(finding: &Finding, oracle: &str) -> Result<(), String> {
    let mut rows = finding.related_tests.iter().filter(|row| {
        row.miss.is_none()
            && row
                .oracle
                .as_deref()
                .is_some_and(|text| text.contains(oracle))
    });
    match (rows.next(), rows.next()) {
        (Some(_), None) => Ok(()),
        other => Err(format!(
            "premise: exactly one listed row containing `{oracle}`: {other:?} in {finding:?}"
        )),
    }
}

#[test]
fn scalar_early_return_and_scalar_tail_beside_it_are_exposed() -> Result<(), String> {
    let function = "pub fn gated(flag: bool) -> u64 {
    if flag {
        return 100;
    }
    7
}";
    let before = "pub fn gated(flag: bool) -> u64 {
    if flag {
        return 200;
    }
    8
}";
    let findings = TempRepo::create(
        &lib(
            function,
            "assert_eq!(gated(true), 100);\n        assert_eq!(gated(false), 7);",
        ),
        &fn_diff(before, function),
    )?
    .findings()?;

    let early = finding_on_line(&findings, 3, "return 100")?;
    assert_eq!(
        early.class,
        ExposureClass::Exposed,
        "early return: {early:?}"
    );
    let tail = finding_on_line(&findings, 5, "7")?;
    assert_eq!(tail.class, ExposureClass::Exposed, "tail: {tail:?}");

    // Each exit is listed with its own pin; the findings-level confirmation
    // (the discriminate stage) rides the matching pin alone, which the
    // sibling-pin-only scenario in
    // `sibling_exit_pin_does_not_confirm_the_early_return_probe` pins.
    confirmed_row(early, "gated(true), 100)")?;
    confirmed_row(tail, "gated(false), 7)")?;
    Ok(())
}

#[test]
fn sibling_exit_pin_does_not_confirm_the_early_return_probe() -> Result<(), String> {
    // Only the sibling-exit pin exists: `gated(false), 7` passes whether or
    // not the changed `return 100;` ran, so the early-return probe must stay
    // below exposed (observation_unconfirmed). The tail probe keeps its
    // `EarlyScalar(7)` credit through the same assertion.
    let function = "pub fn gated(flag: bool) -> u64 {
    if flag {
        return 100;
    }
    7
}";
    let before = "pub fn gated(flag: bool) -> u64 {
    if flag {
        return 200;
    }
    8
}";
    let findings = TempRepo::create(
        &lib(function, "assert_eq!(gated(false), 7);"),
        &fn_diff(before, function),
    )?
    .findings()?;

    let tail = finding_on_line(&findings, 5, "7")?;
    assert_eq!(
        tail.class,
        ExposureClass::Exposed,
        "tail keeps its own pin: {tail:?}"
    );
    let early = finding_on_line(&findings, 3, "return 100")?;
    assert_ne!(
        early.class,
        ExposureClass::Exposed,
        "a sibling-exit pin must not confirm the early-return probe: {early:?}"
    );
    Ok(())
}

#[test]
fn single_digit_early_return_literal_is_credited() -> Result<(), String> {
    // `0` is no token (the #10479 one-to-two-digit rule), so this probe used
    // to read `observation_unverified` with no way to name the changed
    // expression in an assertion.
    let function = "pub fn waived(flag: bool) -> u64 {
    if flag {
        return 0;
    }
    7
}";
    let before = "pub fn waived(flag: bool) -> u64 {
    if flag {
        return 5;
    }
    7
}";
    let findings = TempRepo::create(
        &lib(function, "assert_eq!(waived(true), 0);"),
        &fn_diff(before, function),
    )?
    .findings()?;
    let early = finding_on_line(&findings, 3, "return 0")?;
    assert_eq!(
        early.class,
        ExposureClass::Exposed,
        "single-digit early return: {early:?}"
    );
    Ok(())
}

#[test]
fn second_source_of_the_same_literal_stays_fail_closed() -> Result<(), String> {
    // The tail also builds `0`, so a passing pin at `0` never proves the
    // changed exit ran: `dup(false)` returns the tail's `0` without touching
    // the changed line.
    let function = "pub fn dup(pick: bool) -> u64 {
    if pick {
        return 0;
    }
    0
}";
    let before = "pub fn dup(pick: bool) -> u64 {
    if pick {
        return 5;
    }
    0
}";
    let findings = TempRepo::create(
        &lib(
            function,
            "assert_eq!(dup(true), 0);\n        assert_eq!(dup(false), 0);",
        ),
        &fn_diff(before, function),
    )?
    .findings()?;
    let early = finding_on_line(&findings, 3, "return 0")?;
    assert_ne!(
        early.class,
        ExposureClass::Exposed,
        "a second source of the same literal must not be credited: {early:?}"
    );
    Ok(())
}

#[test]
fn computed_sibling_exit_stays_fail_closed() -> Result<(), String> {
    // `x * 2` may be `0` for `x == 0`, so a passing pin at `0` does not
    // prove the changed exit ran.
    let function = "pub fn computed(flag: bool, x: u64) -> u64 {
    if flag {
        return 0;
    }
    x * 2
}";
    let before = "pub fn computed(flag: bool, x: u64) -> u64 {
    if flag {
        return 5;
    }
    x * 2
}";
    let findings = TempRepo::create(
        &lib(function, "assert_eq!(computed(true, 3), 0);"),
        &fn_diff(before, function),
    )?
    .findings()?;
    let early = finding_on_line(&findings, 3, "return 0")?;
    assert_ne!(
        early.class,
        ExposureClass::Exposed,
        "a computed sibling exit must not be credited: {early:?}"
    );
    Ok(())
}

#[test]
fn parameter_sibling_exit_stays_fail_closed() -> Result<(), String> {
    // `passthrough(0)` returns the tail parameter `x == 0` without running
    // the changed exit, so the pin at `0` cannot observe the change.
    let function = "pub fn passthrough(x: u64) -> u64 {
    if x > 1 {
        return 0;
    }
    x
}";
    let before = "pub fn passthrough(x: u64) -> u64 {
    if x > 1 {
        return 9;
    }
    x
}";
    let findings = TempRepo::create(
        &lib(function, "assert_eq!(passthrough(0), 0);"),
        &fn_diff(before, function),
    )?
    .findings()?;
    let early = finding_on_line(&findings, 3, "return 0")?;
    assert_ne!(
        early.class,
        ExposureClass::Exposed,
        "a parameter sibling exit must not be credited: {early:?}"
    );
    Ok(())
}

#[test]
fn non_exact_assertion_is_not_credited() -> Result<(), String> {
    let function = "pub fn waived(flag: bool) -> u64 {
    if flag {
        return 0;
    }
    7
}";
    let before = "pub fn waived(flag: bool) -> u64 {
    if flag {
        return 5;
    }
    7
}";
    let findings = TempRepo::create(
        &lib(function, "assert!(waived(true) > 3);"),
        &fn_diff(before, function),
    )?
    .findings()?;
    let early = finding_on_line(&findings, 3, "return 0")?;
    assert_ne!(
        early.class,
        ExposureClass::Exposed,
        "a relational assertion pins no exact value: {early:?}"
    );
    Ok(())
}
