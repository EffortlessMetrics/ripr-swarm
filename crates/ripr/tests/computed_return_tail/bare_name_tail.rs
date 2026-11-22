//! #13037: a bare parameter or local as the changed returned tail (`amount`
//! in the `else` of the tail `if`) read `propagation_unknown`, so even an
//! exact pin of the branch that returns it was not credited. The name now
//! reaches the return sink, and the #14950 reached-tail pin decides credit.
//! A test binding that shares the tail's name confirms nothing by itself.
use super::{ExposureClass, Finding, TempRepo, test_fn};

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
    let found = finding(
        source,
        "amount + 5",
        "amount",
        "assert_eq!(doubled(100, false), 200);",
    );
    // Either no return_value finding is produced for the line, or it is not
    // credited: the sink must not claim the name is the returned value.
    if let Ok(found) = found {
        assert_ne!(found.class, ExposureClass::Exposed);
        assert!(
            found.flow_sinks.iter().all(|sink| sink.text != "amount"),
            "{:?}",
            found.flow_sinks
        );
    }
    Ok(())
}
