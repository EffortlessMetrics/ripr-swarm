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

// ---- #6673: the issue's exact asserted-Err match shape -------------------

const ISSUE_6673_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum PayError {
    Insufficient,
    Limit,
}

pub fn withdraw(balance: u64, amount: u64) -> Result<u64, PayError> {
    if amount > 500 { return Err(PayError::Limit); }
    if amount > balance { return Result::Err(PayError::Insufficient); }
    Ok(balance - amount)
}
"#;
const ISSUE_6673_CHANGED_LINE: usize = 9;
const ISSUE_6673_OLD_LINE: &str =
    "    if amount > balance { return Result::Err(PayError::Limit); }";

fn issue_6673_test(ok_arm: &str) -> String {
    format!(
        "use error_path_sibling_variant::{{PayError, withdraw}};\n\n#[test]\nfn overdraw_routes_to_insufficient() {{\n    match withdraw(10, 20) {{\n        {ok_arm},\n        Err(e) => assert_eq!(e, PayError::Insufficient),\n    }}\n}}\n"
    )
}

#[test]
fn issue_6673_diverging_ok_arm_and_exact_err_arm_expose_the_changed_error() -> Result<(), String> {
    let diff = single_line_diff(
        ISSUE_6673_SOURCE,
        ISSUE_6673_CHANGED_LINE,
        ISSUE_6673_OLD_LINE,
    );
    let repo = TempRepo::create(
        ISSUE_6673_SOURCE,
        &issue_6673_test("Ok(left) => panic!(\"overdraw left {left}\")"),
        &diff,
    )?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, ISSUE_6673_CHANGED_LINE, "PayError::Insufficient")?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the RIPR-SPEC-0175 asserted-Err form pins the changed error; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    Ok(())
}

#[test]
fn issue_6673_quiet_ok_arm_does_not_expose_the_changed_error() -> Result<(), String> {
    let diff = single_line_diff(
        ISSUE_6673_SOURCE,
        ISSUE_6673_CHANGED_LINE,
        ISSUE_6673_OLD_LINE,
    );
    let repo = TempRepo::create(ISSUE_6673_SOURCE, &issue_6673_test("Ok(_) => {}"), &diff)?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, ISSUE_6673_CHANGED_LINE, "PayError::Insufficient")?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a quiet Ok arm lets a success result pass, so nothing pins the error; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    let exposed = exposed_on_line(&output, ISSUE_6673_CHANGED_LINE);
    assert!(exposed.is_empty(), "{exposed:?}");
    Ok(())
}

// ---- #6695: the issue's exact `ok_or(Variant)?` shape --------------------

const ISSUE_6695_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum CodeError {
    NotDigit,
    Short,
}

fn digit(c: char) -> Option<u32> {
    c.to_digit(10)
}

pub fn parse_code(s: &str) -> Result<u32, CodeError> {
    if s.len() < 4 {
        return Err(CodeError::Short);
    }
    let mut value = 0;
    for c in s.chars() {
        let d = digit(c).ok_or(CodeError::NotDigit)?;
        value = value * 10 + d;
    }
    Ok(value)
}
"#;
const ISSUE_6695_CHANGED_LINE: usize = 17;
const ISSUE_6695_OLD_LINE: &str = "        let d = digit(c).ok_or(CodeError::Short)?;";

fn issue_6695_test(assertion: &str) -> String {
    format!(
        "use error_path_sibling_variant::{{CodeError, parse_code}};\n\n#[test]\nfn a_letter_is_not_a_digit() {{\n    {assertion};\n}}\n"
    )
}

#[test]
fn issue_6695_owner_call_pin_exposes_the_changed_ok_or() -> Result<(), String> {
    let diff = single_line_diff(
        ISSUE_6695_SOURCE,
        ISSUE_6695_CHANGED_LINE,
        ISSUE_6695_OLD_LINE,
    );
    let repo = TempRepo::create(
        ISSUE_6695_SOURCE,
        &issue_6695_test("assert_eq!(parse_code(\"12x4\"), Err(CodeError::NotDigit))"),
        &diff,
    )?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, ISSUE_6695_CHANGED_LINE, "CodeError::NotDigit")?;
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "the owner-call pin observes the error the `?` returns; discriminator={:?} propagate={:?}",
        finding.ripr.reveal.discriminate,
        finding.ripr.propagate
    );
    Ok(())
}

#[test]
fn issue_6695_sibling_owner_call_pin_does_not_expose_the_changed_ok_or() -> Result<(), String> {
    let diff = single_line_diff(
        ISSUE_6695_SOURCE,
        ISSUE_6695_CHANGED_LINE,
        ISSUE_6695_OLD_LINE,
    );
    let repo = TempRepo::create(
        ISSUE_6695_SOURCE,
        &issue_6695_test("assert_eq!(parse_code(\"12\"), Err(CodeError::Short))"),
        &diff,
    )?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, ISSUE_6695_CHANGED_LINE, "CodeError::NotDigit")?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a CodeError::Short pin does not observe the changed NotDigit error; discriminator={:?}",
        finding.ripr.reveal.discriminate
    );
    let exposed = exposed_on_line(&output, ISSUE_6695_CHANGED_LINE);
    assert!(exposed.is_empty(), "{exposed:?}");
    Ok(())
}

// ---- PR #6786 round 6: refused `ok_or?` lines must not complete ----------
//
// When flow refuses the owner-level `ok_or?` sink (a converting owner error
// type, a closure, a same-named enum from another module), the fallback
// text sink must not complete the witness through the generic identity
// rule. Each test pins `Err(<owner enum>::Bad)` through a different branch,
// so the changed `ok_or` line is never what the test observes.
const CONVERTING_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum Inner {
    Bad,
    Gone,
}

#[derive(Debug, PartialEq)]
pub enum Outer {
    Bad,
    Missing,
}

impl From<Inner> for Outer {
    fn from(_: Inner) -> Self {
        Outer::Missing
    }
}

pub fn parse(x: Option<u8>, strict: bool) -> Result<u8, Outer> {
    if strict {
        return Err(Outer::Bad);
    }
    let v = x.ok_or(Inner::Bad)?;
    Ok(v)
}
"#;
const CLOSURE_SOURCE: &str = r#"#[derive(Debug, PartialEq)]
pub enum E {
    Bad,
    Gone,
}

pub fn parse(xs: &[Option<u8>]) -> Result<u8, E> {
    let ok_count = xs
        .iter()
        .map(|x| -> Result<u8, E> {
            let v = x.ok_or(E::Bad)?;
            Ok(v)
        })
        .filter(Result::is_ok)
        .count();
    if ok_count == 0 {
        return Err(E::Bad);
    }
    Ok(ok_count as u8)
}
"#;
const OTHER_MODULE_SOURCE: &str = r#"pub mod other {
    #[derive(Debug, PartialEq)]
    pub enum E {
        Bad,
        Gone,
    }
}

#[derive(Debug, PartialEq)]
pub enum E {
    Bad,
    Missing,
}

impl From<other::E> for E {
    fn from(_: other::E) -> Self {
        E::Missing
    }
}

pub fn parse(x: Option<u8>, strict: bool) -> Result<u8, E> {
    if strict {
        return Err(E::Bad);
    }
    let v = x.ok_or(other::E::Bad)?;
    Ok(v)
}
"#;

fn refused_ok_or_case(
    source: &str,
    fragment: &str,
    old_variant: &str,
    new_variant: &str,
    test_body: &str,
    uses: &str,
) -> Result<(), String> {
    let line = source
        .lines()
        .position(|text| text.contains(fragment))
        .map(|index| index + 1)
        .ok_or_else(|| format!("fixture has no `{fragment}` line"))?;
    let new_line = source.lines().nth(line - 1).unwrap_or_default();
    let old_line = new_line.replace(new_variant, old_variant);
    let diff = single_line_diff(source, line, &old_line);
    let test = format!(
        "use error_path_sibling_variant::{{{uses}}};\n\n#[test]\nfn pinned_through_another_branch() {{\n    {test_body};\n}}\n"
    );
    let repo = TempRepo::create(source, &test, &diff)?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, line, fragment)?;
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "`{fragment}` must not read exposed; discriminator={:?} propagate={:?}",
        finding.ripr.reveal.discriminate,
        finding.ripr.propagate
    );
    let exposed = exposed_on_line(&output, line);
    assert!(exposed.is_empty(), "{exposed:?}");
    Ok(())
}

#[test]
fn ok_or_converted_by_the_owner_error_type_does_not_read_exposed() -> Result<(), String> {
    refused_ok_or_case(
        CONVERTING_SOURCE,
        "x.ok_or(Inner::Bad)",
        "Inner::Gone",
        "Inner::Bad",
        "assert_eq!(parse(None, true), Err(Outer::Bad))",
        "Outer, parse",
    )
}

#[test]
fn ok_or_inside_a_closure_does_not_read_exposed() -> Result<(), String> {
    refused_ok_or_case(
        CLOSURE_SOURCE,
        "x.ok_or(E::Bad)",
        "E::Gone",
        "E::Bad",
        "assert_eq!(parse(&[None]), Err(E::Bad))",
        "E, parse",
    )
}

#[test]
fn ok_or_of_a_same_named_enum_in_another_module_does_not_read_exposed() -> Result<(), String> {
    refused_ok_or_case(
        OTHER_MODULE_SOURCE,
        "x.ok_or(other::E::Bad)",
        "other::E::Gone",
        "other::E::Bad",
        "assert_eq!(parse(None, true), Err(E::Bad))",
        "E, parse",
    )
}

// ---- RIPR-SPEC-0231 examples 19 and 20: an inequality excluding a sibling --
//
// `assert_ne!(withdraw(..), Err(PayError::Limit))` names the changed
// `PayError::Insufficient` only through a sibling, but it excludes that
// sibling from the owner's own result: a change that returned `Limit` would
// fail it. It observes the result as `relational_check` / weak (rule 1), so
// the finding is `weakly_exposed` with that oracle, never `exposed`. An
// inequality on anything but the owner call shares only the enum qualifier
// and stays refused.

fn pay_assertion_test(assertion: &str) -> String {
    format!(
        "use error_path_sibling_variant::{{PayError, withdraw}};\n\n#[test]\nfn overdraw_is_not_the_limit() {{\n    let other: Result<u64, PayError> = Ok(1);\n    let _ = &other;\n    {assertion};\n}}\n"
    )
}

/// The error-path finding's class and the oracle its one related test
/// carries, as `kind/strength`.
fn sibling_exclusion_reading(assertion: &str) -> Result<(ExposureClass, Vec<String>), String> {
    let diff = single_line_diff(PAY_SOURCE, PAY_CHANGED_LINE, PAY_OLD_LINE);
    let repo = TempRepo::create(PAY_SOURCE, &pay_assertion_test(assertion), &diff)?;
    let output = repo.check()?;
    let finding = changed_error_path(&output, PAY_CHANGED_LINE, "PayError::Insufficient")?;
    let oracles = finding
        .related_tests
        .iter()
        .map(|test| {
            format!(
                "{}/{}",
                test.oracle_kind.as_str(),
                test.oracle_strength.as_str()
            )
        })
        .collect();
    let exposed = exposed_on_line(&output, PAY_CHANGED_LINE);
    if !exposed.is_empty() {
        return Err(format!(
            "an inequality exposed the changed line: {exposed:?}"
        ));
    }
    Ok((finding.class.clone(), oracles))
}

#[test]
fn spec_0231_inequality_excluding_a_sibling_reads_relational_weak() -> Result<(), String> {
    for assertion in [
        "assert_ne!(withdraw(10, 20), Err(PayError::Limit))",
        "assert!(!matches!(withdraw(10, 20), Err(PayError::Limit)))",
        "assert_ne!(withdraw(10, 20), Err(PayError::Limit), \"not the limit\")",
    ] {
        let (class, oracles) = sibling_exclusion_reading(assertion)?;
        assert_eq!(
            class,
            ExposureClass::WeaklyExposed,
            "{assertion}: {oracles:?}"
        );
        assert_eq!(oracles, vec!["relational_check/weak"], "{assertion}");
    }
    Ok(())
}

#[test]
fn spec_0231_sibling_inequality_off_the_owner_call_stays_refused() -> Result<(), String> {
    // Each test also calls the owner, so it stays related; only the
    // inequality's reading is under test.
    for assertion in [
        // Not the owner's result: only the enum qualifier is shared.
        "let _ = withdraw(1, 0);\n    assert_ne!(other, Err(PayError::Limit))",
        // The owner call is a nested argument, not the compared operand.
        "assert_ne!(other.or(withdraw(10, 20)), Err(PayError::Limit))",
        // The owner call sits only in the message arguments.
        "assert_ne!(other, Err(PayError::Limit), \"{:?}\", withdraw(10, 20))",
        // A negated `matches!` that is only part of the condition.
        "assert!(!matches!(withdraw(10, 20), Err(PayError::Limit)) || other.is_ok())",
        "assert!(!matches!(withdraw(10, 20), Err(PayError::Limit)) ^ other.is_ok())",
        // A guarded pattern excludes the sibling only when the guard holds.
        "assert!(!matches!(withdraw(10, 20), Err(PayError::Limit) if other.is_ok()))",
        // A local closure shadows the owner's name.
        "let _ = withdraw(1, 0);\n    let withdraw = |_: u64, _: u64| other;\n    assert_ne!(withdraw(10, 20), Err(PayError::Limit))",
        // So does a nested function.
        "let _ = withdraw(1, 0);\n    fn withdraw(_: u64, _: u64) -> Result<u64, PayError> { Ok(0) }\n    assert_ne!(withdraw(10, 20), Err(PayError::Limit))",
        // A test-visible macro rebinding the assertion may check nothing.
        "macro_rules! assert_ne { ($l:expr, $r:expr) => { let _ = ($l, $r); }; }\n    assert_ne!(withdraw(10, 20), Err(PayError::Limit))",
        "macro_rules! assert { ($c:expr) => { let _ = $c; }; }\n    assert!(!matches!(withdraw(10, 20), Err(PayError::Limit)))",
        "macro_rules! matches { ($v:expr, $p:pat) => { false }; }\n    assert!(!matches!(withdraw(10, 20), Err(PayError::Limit)))",
    ] {
        let (class, oracles) = sibling_exclusion_reading(assertion)?;
        assert_ne!(class, ExposureClass::WeaklyExposed, "{assertion}");
        assert_eq!(oracles.len(), 1, "{assertion}: {oracles:?}");
        assert_ne!(oracles, vec!["relational_check/weak"], "{assertion}");
    }
    Ok(())
}

// RIPR-SPEC-0227 decision 2 end to end: an uncompared `unwrap_err()` read
// directly off the owner call observes the error side as `broad_error` /
// weak. An adapter that replaces the call's result, written plainly or
// inside a grouping, passes whatever the owner returns and reads no
// error-side oracle (#7872).

#[test]
fn spec_0227_uncompared_unwrap_err_reads_broad_error_only_off_the_owner_call() -> Result<(), String>
{
    for assertion in [
        "withdraw(10, 20).unwrap_err()",
        "(withdraw(10, 20)).unwrap_err()",
        "(withdraw(10, 20).map_err(|e| e)).unwrap_err()",
        "r#withdraw(10, 20).unwrap_err()",
    ] {
        let (class, oracles) = sibling_exclusion_reading(assertion)?;
        assert_eq!(
            class,
            ExposureClass::WeaklyExposed,
            "{assertion}: {oracles:?}"
        );
        assert_eq!(oracles, vec!["broad_error/weak"], "{assertion}");
    }
    for assertion in [
        "withdraw(10, 20).and(Err::<u64, _>(PayError::Insufficient)).unwrap_err()",
        "(withdraw(10, 20).and(Err::<u64, _>(PayError::Insufficient))).unwrap_err()",
        "((withdraw(10, 20).and(Err::<u64, _>(PayError::Insufficient)))).unwrap_err()",
        "(withdraw(10, 20).or(Err::<u64, PayError>(PayError::Insufficient))).unwrap_err()",
    ] {
        let (class, oracles) = sibling_exclusion_reading(assertion)?;
        assert_ne!(
            class,
            ExposureClass::WeaklyExposed,
            "{assertion}: {oracles:?}"
        );
        assert_eq!(oracles.len(), 1, "{assertion}: {oracles:?}");
        assert_ne!(oracles, vec!["broad_error/weak"], "{assertion}");
    }
    Ok(())
}
