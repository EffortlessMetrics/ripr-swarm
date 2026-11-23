//! #13037: a bare parameter or local as the changed returned tail (`amount`
//! in the `else` of the tail `if`) read `propagation_unknown`, so even an
//! exact pin of the branch that returns it was not credited. The name now
//! reaches the return sink, and the #14950 reached-tail pin decides credit.
//! A test binding that shares the tail's name confirms nothing by itself.
use super::{ExposureClass, Finding, TempRepo, test_fn};
use ripr::domain::FlowSinkKind;
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};

const DISCOUNTED: &str = "pub fn discounted(amount: u32, member: bool) -> u32 {
    if member {
        amount - 10
    } else {
        amount
    }
}";

fn finding(source: &str, old: &str, new: &str, body: &str) -> Result<Finding, String> {
    let (at, text) = source
        .lines()
        .enumerate()
        .find(|(_, line)| line.trim() == new)
        .ok_or_else(|| format!("missing edited expression {new}"))?;
    let line = at + 1;
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line} +{line} @@\n-{}\n+{text}\n",
        text.replace(new, old)
    );
    TempRepo::with_patch(source, &test_fn(body), &patch)?.return_finding(line, new)
}

fn amount(body: &str) -> Result<Finding, String> {
    finding(DISCOUNTED, "amount + 5", "amount", body)
}

#[test]
fn an_exact_pin_of_the_branch_returning_a_bare_parameter_is_exposed() -> Result<(), String> {
    let pinned = amount("assert_eq!(discounted(100, false), 100);")?;
    assert_eq!(pinned.class, ExposureClass::Exposed);
    // A let-bound result named like the parameter reads the same when the
    // call reaches the changed branch.
    let bound = amount("let amount = discounted(100, false);\n        assert_eq!(amount, 100);")?;
    assert_eq!(bound.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_bare_local_tail_reached_by_the_pin_is_exposed() -> Result<(), String> {
    let source = "pub fn total(amount: u32, member: bool) -> u32 {
    let fee = amount / 10;
    if member {
        amount
    } else {
        fee
    }
}";
    let found = finding(
        source,
        "amount + 1",
        "fee",
        "assert_eq!(total(100, false), 10);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_pin_of_the_other_branch_or_a_far_input_is_not_credited() -> Result<(), String> {
    let other = amount("assert_eq!(discounted(100, true), 90);")?;
    assert_eq!(other.class, ExposureClass::WeaklyExposed);
    let far = amount(
        "for member in [true] {\n            assert_eq!(discounted(100, member), 90);\n        }",
    )?;
    assert_eq!(far.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_test_binding_named_like_the_tail_does_not_confirm_it() -> Result<(), String> {
    // The call takes the other branch; only the local's name matches.
    let named = amount("let amount = discounted(100, true);\n        assert_eq!(amount, 90);")?;
    assert_eq!(named.class, ExposureClass::WeaklyExposed);
    Ok(())
}

/// Every finding the analysis reports on the edited line, from an analysis
/// that must succeed. The edit must be on exactly one source line.
fn findings_on_edited_line(
    source: &str,
    old: &str,
    new: &str,
    body: &str,
) -> Result<Vec<Finding>, String> {
    let mut edited = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim() == new);
    let (at, text) = match (edited.next(), edited.next()) {
        (Some(found), None) => found,
        other => return Err(format!("premise: one line reads `{new}`, got {other:?}")),
    };
    let line = at + 1;
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line} +{line} @@\n-{}\n+{text}\n",
        text.replace(new, old)
    );
    let repo = TempRepo::with_patch(source, &test_fn(body), &patch)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let on_line: Vec<Finding> = output
        .findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect();
    // The analysis must have read the edited line, or the absence of a
    // return sink below would say nothing.
    if on_line.is_empty() {
        return Err(format!("premise: a finding on edited line {line}"));
    }
    Ok(on_line)
}

fn returns_name(findings: &[Finding], name: &str) -> bool {
    findings.iter().any(|finding| {
        finding
            .flow_sinks
            .iter()
            .any(|sink| sink.kind == FlowSinkKind::ReturnValue && sink.text == name)
    })
}

#[test]
fn a_bare_name_whose_value_is_not_returned_gets_no_return_sink() -> Result<(), String> {
    // The name is a branch of a `let` initializer; the owner returns `v * 2`.
    let source = "pub fn doubled(amount: u32, member: bool) -> u32 {
    let v = if member {
        amount - 10
    } else {
        amount
    };
    v * 2
}";
    let findings = findings_on_edited_line(
        source,
        "amount + 5",
        "amount",
        "assert_eq!(doubled(100, false), 200);",
    )?;
    assert!(!returns_name(&findings, "amount"), "{findings:?}");
    assert!(
        findings
            .iter()
            .all(|finding| finding.class != ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
fn only_a_name_the_owner_binds_gets_the_return_sink() -> Result<(), String> {
    let test = "assert_eq!(price(100, false), 7);";
    // Control: a `let`-bound local returned as the tail reaches the sink.
    let local = "pub fn price(amount: u32, member: bool) -> u32 {
    let fallback = 7;
    if member {
        amount
    } else {
        fallback
    }
}";
    let bound = findings_on_edited_line(local, "amount + 1", "fallback", test)?;
    assert!(returns_name(&bound, "fallback"), "{bound:?}");
    // A same-file constant spelled as one identifier is not owner-bound, so
    // this reader gives it no return sink.
    let constant = "pub const FALLBACK: u32 = 7;

pub fn price(amount: u32, member: bool) -> u32 {
    if member {
        amount
    } else {
        FALLBACK
    }
}";
    let item = findings_on_edited_line(constant, "amount + 1", "FALLBACK", test)?;
    assert!(!returns_name(&item, "FALLBACK"), "{item:?}");
    Ok(())
}
