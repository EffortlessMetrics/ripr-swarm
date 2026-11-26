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
    // The constant is spelled like a local, so only the binding check (not
    // the casing rule) keeps it out.
    let constant = "#[allow(non_upper_case_globals)]
pub const fallback: u32 = 7;

pub fn price(amount: u32, member: bool) -> u32 {
    if member {
        amount
    } else {
        fallback
    }
}";
    let item = findings_on_edited_line(constant, "amount + 1", "fallback", test)?;
    assert!(!returns_name(&item, "fallback"), "{item:?}");
    Ok(())
}

#[test]
fn a_same_named_local_in_a_disjoint_scope_does_not_bind_the_tail() -> Result<(), String> {
    let test = "assert_eq!(price(100, false), 7);";
    // The `else` block imports the constant it returns; the item declared in
    // an enclosing block makes the name ambiguous to this reader.
    let imported = "pub mod defaults {
    #[allow(non_upper_case_globals)]
    pub const fallback: u32 = 7;
}

pub fn price(amount: u32, member: bool) -> u32 {
    if member {
        let fallback = amount;
        fallback + 0
    } else {
        use crate::defaults::fallback;
        fallback
    }
}";
    let item = findings_on_edited_line(imported, "amount + 1", "fallback", test)?;
    assert!(!returns_name(&item, "fallback"), "{item:?}");
    // Control: a preceding `let` in the returning block itself binds it.
    let same_block = "pub fn price(amount: u32, member: bool) -> u32 {
    if member {
        amount
    } else {
        let fallback = 7;
        fallback
    }
}";
    let bound = findings_on_edited_line(same_block, "amount + 1", "fallback", test)?;
    assert!(returns_name(&bound, "fallback"), "{bound:?}");
    Ok(())
}

#[test]
fn a_bare_name_in_a_preceding_let_else_is_not_read_as_a_local() -> Result<(), String> {
    // With the constant in scope, `let fallback = amount else { .. };` is a
    // constant pattern, not a new binding, so the tail returns the constant.
    let source = "#[allow(non_upper_case_globals)]
pub const fallback: u32 = 7;

#[allow(non_upper_case_globals)]
pub fn price(amount: u32, member: bool) -> u32 {
    if member {
        return amount;
    }
    let fallback = amount else {
        return 0;
    };
    fallback
}";
    let findings = findings_on_edited_line(
        source,
        "amount + 1",
        "fallback",
        "assert_eq!(price(7, false), 7);",
    )?;
    assert!(!returns_name(&findings, "fallback"), "{findings:?}");
    Ok(())
}

#[test]
fn a_name_spelled_like_an_item_is_not_read_as_a_local() -> Result<(), String> {
    // With `struct Fallback;` in scope, `let Fallback = input;` is an
    // irrefutable unit-struct pattern, not a binding; the casing rule
    // declines it.
    let source = "#[derive(Debug, PartialEq)]
pub struct Fallback;

pub fn pick(input: Fallback, member: bool) -> Fallback {
    if member {
        return input;
    }
    let Fallback = input;
    Fallback
}";
    let findings = findings_on_edited_line(
        source,
        "input",
        "Fallback",
        "assert_eq!(pick(Fallback, false), Fallback);",
    )?;
    assert!(!returns_name(&findings, "Fallback"), "{findings:?}");
    Ok(())
}

#[test]
fn an_item_in_an_enclosing_block_shadows_an_outer_local() -> Result<(), String> {
    // The `else` block's `use` shadows the outer `let fallback`, so the tail
    // returns the imported constant; the enclosing-block item makes the
    // reader decline.
    let source = "pub mod defaults {
    #[allow(non_upper_case_globals)]
    pub const fallback: u32 = 7;
}

#[allow(unused_variables)]
pub fn price(amount: u32, member: bool) -> u32 {
    let fallback = amount;
    if member {
        amount
    } else {
        use crate::defaults::fallback;
        fallback
    }
}";
    let findings = findings_on_edited_line(
        source,
        "amount + 1",
        "fallback",
        "assert_eq!(price(100, false), 7);",
    )?;
    assert!(!returns_name(&findings, "fallback"), "{findings:?}");
    Ok(())
}

#[test]
fn a_numeric_literal_tail_keeps_its_owner_bound_token_credit() -> Result<(), String> {
    // The tail-name refusal covers names only: a literal tail (`105`) that an
    // owner-bound assertion spells still confirms (#5830), even when the call
    // takes a local argument that the branch-reading pin does not fold.
    let source = "pub fn fee(amount: u32) -> u32 {
    if amount > 50 {
        7
    } else {
        105
    }
}";
    let found = finding(
        source,
        "104",
        "105",
        "let amount = 10;\n        assert_eq!(fee(amount), 105);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed);
    Ok(())
}
