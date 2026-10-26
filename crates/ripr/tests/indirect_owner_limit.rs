//! Public-API controls for owners reached without a call ripr can place
//! (RIPR-SPEC-0240, #9442, #9443).
//!
//! A test that passes the owner as a function value, a macro that generates
//! the owner, and an executor helper wrapping the owner's call read as
//! `static_unknown` under `rust_indirect_owner_call_unresolved`. A `cfg_attr`
//! that introduces `test` is an ordinary test, and a comment after the tail
//! does not change what the owner returns. Each control below changes one
//! thing about the shape and keeps the gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-indirect-owner-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"indirect-owner\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
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

/// A one-line edit of `line` in `src/lib.rs`.
fn diff(line: usize, before: &str, after: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{before}
+{after}
"
    )
}

/// Every finding on `line`: one edit can carry several probes.
fn findings_on(lib: &str, diff: &str, line: usize) -> Result<Vec<Finding>, String> {
    let findings = TempRepo::create(lib, diff)?.findings()?;
    let on_line: Vec<Finding> = findings
        .into_iter()
        .filter(|finding| finding.probe.location.line == line)
        .collect();
    if on_line.is_empty() {
        return Err(format!("premise: a finding on line {line}"));
    }
    Ok(on_line)
}

const LIMIT: &str = "rust_indirect_owner_call_unresolved";

fn is_gap(finding: &Finding) -> bool {
    matches!(
        finding.class,
        ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
    )
}

fn limited(finding: &Finding) -> bool {
    finding.static_limit_kind.map(|kind| kind.as_str()) == Some(LIMIT)
}

fn summary(findings: &[Finding]) -> Vec<(&'static str, &'static str, Option<&'static str>)> {
    findings
        .iter()
        .map(|f| {
            (
                f.probe.family.as_str(),
                f.class.as_str(),
                f.static_limit_kind.map(|kind| kind.as_str()),
            )
        })
        .collect()
}

/// The 1-based line of the first line of `lib` that is exactly `text`.
fn line_of(lib: &str, text: &str) -> Result<usize, String> {
    lib.lines()
        .position(|line| line == text)
        .map(|at| at + 1)
        .ok_or_else(|| format!("premise: `{text}` is a line of the source"))
}

/// Edit the line `before` of `lib` to `after` and score the finding on it.
fn edited(lib: &str, before: &str, after: &str) -> Result<Vec<Finding>, String> {
    let line = line_of(lib, before)?;
    let lib = lib.replacen(before, after, 1);
    findings_on(&lib, &diff(line, before, after), line)
}

/// Some finding on the line carries the limit and none is still a gap.
fn withheld(lib: &str, before: &str, after: &str, label: &str) -> Result<(), String> {
    let findings = edited(lib, before, after)?;
    if findings.iter().any(is_gap)
        || findings
            .iter()
            .any(|finding| finding.class == ExposureClass::Exposed)
        || !findings.iter().any(limited)
    {
        return Err(format!(
            "{label}: expected the indirect-owner limit, got {:?}",
            summary(&findings)
        ));
    }
    Ok(())
}

/// No finding carries the limit and some finding is still a gap.
fn keeps_gap(lib: &str, before: &str, after: &str, label: &str) -> Result<(), String> {
    let findings = edited(lib, before, after)?;
    if findings.iter().any(limited) || !findings.iter().any(is_gap) {
        return Err(format!(
            "{label}: expected the gap to stay, got {:?}",
            summary(&findings)
        ));
    }
    Ok(())
}

/// Every finding on the line is `exposed`.
fn credited(lib: &str, before: &str, after: &str, label: &str) -> Result<(), String> {
    let findings = edited(lib, before, after)?;
    if findings
        .iter()
        .any(|finding| finding.class != ExposureClass::Exposed)
    {
        return Err(format!(
            "{label}: expected exposed, got {:?}",
            summary(&findings)
        ));
    }
    Ok(())
}

const BEFORE: &str = "    x + 1";
const AFTER: &str = "    1 + x";

fn values(test: &str) -> String {
    format!(
        "pub fn inc(x: u32) -> u32 {{
    x + 1
}}

#[cfg(test)]
mod tests {{
    use super::*;

{test}}}
"
    )
}

#[test]
fn a_path_value_in_an_argument_is_a_limit_not_a_gap() -> Result<(), String> {
    let lib = values(
        "    #[test]
    fn mapped() {
        assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);
    }
",
    );
    withheld(&lib, BEFORE, AFTER, "path value")
}

#[test]
fn a_typed_function_pointer_binding_is_credited_by_the_pointer_pin() -> Result<(), String> {
    let lib = values(
        "    #[test]
    fn through_pointer() {
        let f: fn(u32) -> u32 = inc;
        assert_eq!(f(2), 3);
    }
",
    );
    // #9478 reads a safe typed function-pointer call as a pin on the owner's
    // return, so the limit never needs to apply.
    credited(&lib, BEFORE, AFTER, "typed pointer binding")
}

#[test]
fn an_untyped_function_value_binding_is_a_limit_not_a_gap() -> Result<(), String> {
    let lib = values(
        "    #[test]
    fn through_value() {
        let f = inc;
        assert!(f(2) > 0);
    }
",
    );
    withheld(&lib, BEFORE, AFTER, "untyped value binding")
}

#[test]
fn a_function_value_outside_any_test_keeps_the_gap() -> Result<(), String> {
    let lib = "pub const STEP: fn(u32) -> u32 = inc;

pub fn inc(x: u32) -> u32 {
    x + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive() {
        assert!(inc(1) > 0);
    }
}
";
    keeps_gap(lib, BEFORE, AFTER, "value only in production code")
}

#[test]
fn a_local_named_like_the_owner_keeps_the_gap() -> Result<(), String> {
    let lib = values(
        "    #[test]
    fn positive() {
        let inc = 5;
        assert!(inc > 0);
        assert!(super::inc(1) > 0);
    }
",
    );
    keeps_gap(&lib, BEFORE, AFTER, "local shadows the owner name")
}

#[test]
fn a_direct_weak_assertion_keeps_the_gap() -> Result<(), String> {
    let lib = values(
        "    #[test]
    fn positive() {
        assert!(inc(1) > 0);
    }
",
    );
    keeps_gap(&lib, BEFORE, AFTER, "direct weak assertion")
}

fn with_other_module(lib: &str) -> String {
    lib.replacen(
        "#[cfg(test)]",
        "pub mod other {
    pub fn inc(x: u32) -> u32 {
        x + 9
    }
}

#[cfg(test)]",
        1,
    )
}

#[test]
fn an_unrelated_qualified_value_beside_a_direct_call_keeps_the_gap() -> Result<(), String> {
    let lib = with_other_module(&values(
        "    #[test]
    fn positive() {
        assert!(inc(1) > 0);
        assert_eq!([1, 2].map(other::inc), [10, 11]);
    }
",
    ));
    keeps_gap(
        &lib,
        BEFORE,
        AFTER,
        "value of a different same-leaf function",
    )
}

#[test]
fn the_owners_own_qualified_value_beside_a_direct_call_is_a_limit() -> Result<(), String> {
    let lib = with_other_module(&values(
        "    #[test]
    fn positive() {
        assert!(inc(1) > 0);
        assert_eq!([1, 2].map(super::inc), [2, 3]);
    }
",
    ));
    withheld(&lib, BEFORE, AFTER, "value of the owner through super::")
}

#[test]
fn an_ignored_test_naming_the_owner_as_a_value_keeps_the_gap() -> Result<(), String> {
    let lib = values(
        "    #[test]
    fn positive() {
        assert!(inc(1) > 0);
    }

    #[test]
    #[ignore]
    fn mapped() {
        assert_eq!(vec![1, 2].into_iter().map(inc).collect::<Vec<_>>(), vec![2, 3]);
    }
",
    );
    keeps_gap(&lib, BEFORE, AFTER, "ignored value use")
}

#[test]
fn an_unrelated_pattern_binding_of_the_owner_name_keeps_the_gap() -> Result<(), String> {
    let lib = values(
        "    #[test]
    fn positive() {
        assert!(inc(1) > 0);
    }

    #[test]
    fn unrelated() {
        for (inc, _) in [(1, 2)] {
            assert!(inc > 0);
        }
        if let Some(inc) = Some(3) {
            assert!(inc > 0);
        }
    }
",
    );
    keeps_gap(&lib, BEFORE, AFTER, "pattern bindings named like the owner")
}

const ASYNC_OWNER: &str = "pub async fn add_tax(p: u32) -> u32 {
    p + p / 10
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    fn block_on<F: Future>(fut: F) -> F::Output {
        let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }
    }

    #[test]
    fn tax_is_added() {
        assert_eq!(block_on(add_tax(100)), 110);
    }
}
";
const TAX_BEFORE: &str = "    p + p / 10";
const TAX_AFTER: &str = "    p / 10 + p";

/// RIPR-SPEC-0197 rule 1: `block_on` here only polls its argument to
/// completion and returns the output, so the assertion pins the owner's
/// return exactly as `add_tax(100).await` would. Runtime truth: `p + p / 5`
/// gives 120, `p - p / 10` gives 90, and either fails `== 110`.
#[test]
fn an_owner_called_inside_a_polling_helper_is_credited() -> Result<(), String> {
    credited(ASYNC_OWNER, TAX_BEFORE, TAX_AFTER, "polling helper")
}

#[test]
fn a_polling_helper_with_a_bound_expected_value_is_credited() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "let expected = 110;\n        assert_eq!(block_on(add_tax(100)), expected);",
    );
    credited(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "bound expected through a helper",
    )
}

/// A helper with any statement beyond the poll loop is not read as one: it
/// may count polls, time out or rewrite the output.
#[test]
fn a_helper_with_an_extra_statement_is_a_limit_not_a_gap() -> Result<(), String> {
    let lib = ASYNC_OWNER
        .replace(
            "        loop {\n            if let Poll::Ready",
            "        let _polls = 0;\n        loop {\n            if let Poll::Ready",
        )
        .replace(
            "assert_eq!(block_on(add_tax(100)), 110);",
            "let expected = 110;\n        assert_eq!(block_on(add_tax(100)), expected);",
        );
    withheld(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "helper with an extra statement",
    )
}

#[test]
fn a_helper_that_rewrites_the_output_is_a_limit_not_a_gap() -> Result<(), String> {
    let lib = ASYNC_OWNER
        .replace("<F: Future>", "<F: Future<Output = u32>>")
        .replace("return v;", "return v * 2;")
        .replace(
            "assert_eq!(block_on(add_tax(100)), 110);",
            "let expected = 110;\n        assert_eq!(block_on(add_tax(100)), expected);",
        );
    withheld(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "helper that rewrites the output",
    )
}

/// The helper is verified, but the assertion does not pin the changed value.
#[test]
fn a_polling_helper_with_a_weak_assertion_is_a_limit_not_a_gap() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "assert!(block_on(add_tax(100)) > 0);",
    );
    withheld(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "weak assertion through a helper",
    )
}

#[test]
fn a_polling_helper_result_used_after_a_method_call_is_a_limit_not_a_gap() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "assert_eq!(block_on(add_tax(100)).count_ones(), 6);",
    );
    withheld(&lib, TAX_BEFORE, TAX_AFTER, "method on the helper result")
}

#[test]
fn an_owner_called_inside_a_known_executor_path_is_a_limit_not_a_gap() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "assert_eq!(futures::executor::block_on(add_tax(100)), 110);",
    );
    withheld(&lib, TAX_BEFORE, TAX_AFTER, "executor path")
}

#[test]
fn a_helper_that_is_not_an_executor_keeps_the_gap() -> Result<(), String> {
    let lib = ASYNC_OWNER
        .replace(
            "fn block_on<F: Future>(fut: F) -> F::Output {",
            "fn block_on<F>(fut: F) -> F {",
        )
        .replace(
            "assert_eq!(block_on(add_tax(100)), 110);",
            "assert!(passthrough(add_tax(100)).is_some());",
        )
        .replace(
            "fn block_on<F>(fut: F) -> F {",
            "fn passthrough<F>(fut: F) -> Option<F> {",
        )
        .replace(
            "let mut fut = pin!(fut);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
        }",
            "Some(fut)",
        )
        .replace(
            "use std::future::Future;
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};
",
            "",
        );
    keeps_gap(&lib, TAX_BEFORE, TAX_AFTER, "non-executor helper")
}

#[test]
fn an_owner_among_several_executor_arguments_keeps_the_gap() -> Result<(), String> {
    let lib = ASYNC_OWNER
        .replace(
            "fn block_on<F: Future>(fut: F) -> F::Output {",
            "fn block_on<F: Future>(fut: F, _extra: u32) -> F::Output {",
        )
        .replace("block_on(add_tax(100))", "block_on(add_tax(100), 1)");
    keeps_gap(&lib, TAX_BEFORE, TAX_AFTER, "second argument")
}

const MACRO_OWNER: &str = "macro_rules! def_add {
    ($name:ident, $n:expr) => {
        pub fn $name(x: u32) -> u32 {
            x + $n
        }
    };
}

def_add!(add_five, 5);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_is_added() {
        assert_eq!(add_five(1), 6);
    }
}
";

#[test]
fn an_owner_generated_by_an_item_macro_is_a_limit_not_a_gap() -> Result<(), String> {
    withheld(
        MACRO_OWNER,
        "def_add!(add_five, 5);",
        "def_add!(add_five, 2 + 3);",
        "item macro",
    )
}

const CFG_ATTR_OWNER: &str = "pub fn inc(x: u32) -> u32 {
    x + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg_attr(all(), test)]
    fn pins_inc() {
        assert_eq!(inc(1), 2);
    }
}
";

#[test]
fn a_cfg_attr_that_introduces_test_is_an_ordinary_test() -> Result<(), String> {
    credited(CFG_ATTR_OWNER, BEFORE, AFTER, "cfg_attr(all(), test)")
}

fn assert_no_credit_for_cfg_attr(attribute: &str) -> Result<(), String> {
    let lib = CFG_ATTR_OWNER.replace("cfg_attr(all(), test)", attribute);
    let findings = edited(&lib, BEFORE, AFTER)?;
    if findings
        .iter()
        .any(|finding| finding.class == ExposureClass::Exposed)
    {
        return Err(format!(
            "expected no credit for {attribute}, got {:?}",
            summary(&findings)
        ));
    }
    Ok(())
}

#[test]
fn a_cfg_attr_that_introduces_test_beside_ignore_does_not_credit() -> Result<(), String> {
    assert_no_credit_for_cfg_attr("cfg_attr(all(), test, ignore)")?;
    assert_no_credit_for_cfg_attr("cfg_attr(all(), ignore, test)")
}

#[test]
fn a_cfg_attr_that_introduces_test_beside_should_panic_does_not_credit() -> Result<(), String> {
    assert_no_credit_for_cfg_attr("cfg_attr(all(), test, should_panic)")?;
    assert_no_credit_for_cfg_attr("cfg_attr(all(), should_panic, test)")
}

#[test]
fn a_separate_cfg_attr_ignore_beside_a_cfg_attr_test_does_not_credit() -> Result<(), String> {
    assert_no_credit_for_cfg_attr("cfg_attr(all(), test)]\n    #[cfg_attr(all(), ignore)")?;
    assert_no_credit_for_cfg_attr(
        "cfg_attr(all(), test)]\n    #[cfg_attr(all(), ignore = \"slow\")",
    )
}

#[test]
fn a_separate_cfg_attr_should_panic_beside_a_cfg_attr_test_does_not_credit() -> Result<(), String> {
    assert_no_credit_for_cfg_attr("cfg_attr(all(), test)]\n    #[cfg_attr(all(), should_panic)")?;
    assert_no_credit_for_cfg_attr(
        "cfg_attr(all(), test)]\n    #[cfg_attr(all(), should_panic(expected = \"boom\"))",
    )
}

#[test]
fn a_cfg_attr_with_an_unevaluated_predicate_does_not_credit() -> Result<(), String> {
    let lib = CFG_ATTR_OWNER.replace(
        "cfg_attr(all(), test)",
        "cfg_attr(feature = \"extra\", test)",
    );
    let findings = edited(&lib, BEFORE, AFTER)?;
    if findings
        .iter()
        .any(|finding| finding.class == ExposureClass::Exposed)
    {
        return Err(format!("expected no credit, got {:?}", summary(&findings)));
    }
    Ok(())
}

#[test]
fn a_comment_after_the_tail_does_not_change_what_the_owner_returns() -> Result<(), String> {
    let lib = "pub fn inc(x: u32) -> u32 {
    x + 1 // assert_eq!(inc(1), 3);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pins_inc() {
        assert_eq!(inc(1), 2);
    }
}
";
    credited(
        lib,
        "    x + 1 // assert_eq!(inc(1), 3);",
        "    1 + x // assert_eq!(inc(1), 3);",
        "trailing comment",
    )
}

#[test]
fn a_shadowed_polling_helper_cannot_credit_the_owner() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "let block_on = |_| 110u32;\n        let expected = 110;\n        assert_eq!(block_on(add_tax(100)), expected);",
    );
    withheld(&lib, TAX_BEFORE, TAX_AFTER, "shadowed polling helper")
}

#[test]
fn counterfeit_polling_primitives_cannot_credit_the_owner() -> Result<(), String> {
    let lib =
        include_str!("../../../fixtures/rust_polling_helper_primitive_shadow/input/src/lib.rs");
    let lib = lib.replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
    );
    withheld(
        &lib,
        TAX_AFTER,
        TAX_BEFORE,
        "counterfeit Future and pin macro",
    )
}

#[test]
fn an_unrelated_helper_binding_does_not_withdraw_direct_await_credit() -> Result<(), String> {
    let lib = ASYNC_OWNER
        .replace(
            "    #[test]\n    fn tax_is_added()",
            "    #[tokio::test]\n    async fn tax_is_added()",
        )
        .replace(
            "assert_eq!(block_on(add_tax(100)), 110);",
            "let block_on = 0;\n        assert_eq!(add_tax(100).await, 110);",
        );
    credited(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "direct await beside unrelated binding",
    )
}

#[test]
fn a_polling_assertion_borrowed_from_a_helper_is_unresolved() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace(
        "    #[test]\n    fn tax_is_added() {\n        assert_eq!(block_on(add_tax(100)), 110);\n    }",
        "    fn check_tax() {\n        let block_on = |_| 110u32;\n        let expected = 110;\n        assert_eq!(block_on(add_tax(100)), expected);\n    }\n    #[test]\n    fn tax_is_added() { check_tax(); }",
    );
    let findings = edited(&lib, TAX_BEFORE, TAX_AFTER)?;
    assert!(
        findings.iter().all(|f| f.class != ExposureClass::Exposed),
        "borrowed helper: {:?}",
        summary(&findings)
    );
    Ok(())
}

#[test]
fn standard_path_and_match_polling_helpers_keep_credit() -> Result<(), String> {
    let qualified = ASYNC_OWNER
        .replace("F: Future", "F: core::future::Future")
        .replace("= pin!", "= core::pin::pin!")
        .replace("Context::", "core::task::Context::")
        .replace("Waker::", "core::task::Waker::")
        .replace("Poll::", "core::task::Poll::");
    credited(&qualified, TAX_BEFORE, TAX_AFTER, "canonical core paths")?;
    let matched = ASYNC_OWNER.replace(
        "if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {\n                return v;\n            }",
        "match fut.as_mut().poll(&mut cx) { Poll::Ready(v) => return v, Poll::Pending => {} }",
    );
    credited(&matched, TAX_BEFORE, TAX_AFTER, "match helper")
}

#[test]
fn raw_named_pin_macro_cannot_lend_standard_primitive_identity() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace("    fn block_on", "    macro_rules! r#pin { ($f:expr) => { ::std::pin::pin!({ ::std::process::exit(0); $f }) } }\n    fn block_on").replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
    );
    withheld(&lib, TAX_BEFORE, TAX_AFTER, "raw-named pin macro")
}

#[test]
fn raw_named_standard_root_cannot_lend_primitive_identity() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace("    use super::*;", "    use super::*;\n    mod r#std { pub use ::std::{future, pin}; pub mod task { pub use ::std::task::{Context, Poll}; pub struct Waker; impl Waker { pub fn noop() -> &'static ::std::task::Waker { ::std::process::exit(0) } } } }").replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
    );
    withheld(&lib, TAX_BEFORE, TAX_AFTER, "raw-named standard root")
}

#[test]
fn a_parent_module_primitive_import_does_not_bind_the_child_helper() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace("    use std::task::{Context, Poll, Waker};", "    use std::task::{Context, Poll};\n    struct r#Waker; impl r#Waker { fn noop() -> &'static std::task::Waker { std::process::exit(0) } }").replace("#[cfg(test)]", "use std::task::Waker;\n#[cfg(test)]").replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
    );
    withheld(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "parent import beside a child counterfeit",
    )
}

#[test]
fn an_imported_alias_cannot_borrow_another_helpers_verification() -> Result<(), String> {
    let lib = ASYNC_OWNER.replace("    #[test]", "    fn constant<F>(_: F) -> u32 { 110 }\n    #[test]").replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "use self::constant as block_on; let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
    );
    withheld(&lib, TAX_BEFORE, TAX_AFTER, "imported helper alias")
}

#[test]
fn a_trait_cannot_intercept_the_polling_helpers_pin_method() -> Result<(), String> {
    let lib = include_str!("../../../fixtures/rust_polling_helper_method_shadow/input/src/lib.rs")
        .replace(
            "assert_eq!(block_on(add_tax(100)), 110);",
            "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
        );
    withheld(&lib, TAX_AFTER, TAX_BEFORE, "trait-intercepted as_mut")
}

#[test]
fn a_same_file_parent_glob_delivers_a_standard_future_import() -> Result<(), String> {
    let lib = format!(
        "use std::future::Future;\n{}",
        ASYNC_OWNER.replace("    use std::future::Future;\n", "")
    );
    credited(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "Future supplied by verified parent glob",
    )
}

#[test]
fn an_anonymous_foreign_trait_import_keeps_method_dispatch_unresolved() -> Result<(), String> {
    let lib = ASYNC_OWNER
        .replace(
            "    use super::*;",
            "    use super::*;\n    use external::Hijack as _;",
        )
        .replace(
            "assert_eq!(block_on(add_tax(100)), 110);",
            "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
        );
    withheld(
        &lib,
        TAX_BEFORE,
        TAX_AFTER,
        "anonymous foreign trait import",
    )
}

#[test]
fn a_sibling_attribute_expansion_cannot_lend_method_authority() -> Result<(), String> {
    let lib = ASYNC_OWNER
        .replace(
            "    use super::*;",
            "    use super::*;\n    #[external::emit_methods] struct Marker;",
        )
        .replace(
            "assert_eq!(block_on(add_tax(100)), 110);",
            "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
        );
    withheld(&lib, TAX_BEFORE, TAX_AFTER, "opaque sibling attribute")
}

#[test]
fn associated_item_macros_cannot_hide_a_polling_method() -> Result<(), String> {
    let lib =
        include_str!("../../../fixtures/rust_polling_helper_associated_macro/input/src/lib.rs")
            .replace(
                "assert_eq!(block_on(add_tax(100)), 110);",
                "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
            );
    withheld(
        &lib,
        TAX_AFTER,
        TAX_BEFORE,
        "associated-item method expansion",
    )
}

#[test]
fn a_counterfeit_import_root_cannot_hide_a_polling_method() -> Result<(), String> {
    let lib = include_str!(
        "../../../fixtures/rust_polling_helper_counterfeit_import_root/input/src/lib.rs"
    )
    .replace(
        "assert_eq!(block_on(add_tax(100)), 110);",
        "let expected = 110; assert_eq!(block_on(add_tax(100)), expected);",
    );
    let repo = TempRepo::create(&lib, &diff(2, TAX_BEFORE, TAX_AFTER))?;
    std::fs::create_dir_all(repo.root.join("src/tests")).map_err(|e| e.to_string())?;
    std::fs::write(
        repo.root.join("src/tests/std.rs"),
        include_str!(
            "../../../fixtures/rust_polling_helper_counterfeit_import_root/input/src/tests/std.rs"
        ),
    )
    .map_err(|e| e.to_string())?;
    let findings: Vec<_> = repo
        .findings()?
        .into_iter()
        .filter(|finding| finding.probe.location.line == 2)
        .collect();
    assert!(!findings.is_empty(), "changed-owner subject must exist");
    assert!(
        findings
            .iter()
            .all(|finding| !is_gap(finding) && finding.class != ExposureClass::Exposed)
            && findings.iter().any(limited),
        "counterfeit import root: {:?}",
        summary(&findings)
    );
    Ok(())
}
