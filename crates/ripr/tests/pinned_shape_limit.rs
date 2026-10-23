//! Public-API controls for an exact pin on a shape the owner pin does not read
//! (#9440, RIPR-SPEC-0240).
//!
//! A test that calls the owner directly and pins its whole result with an
//! exact `assert_eq!` leaves a gap that reads "no assertion references the
//! change" when the changed code sits in a closure the call runs, a statement
//! acting on the returned value, or the `else` exit of a `let ... else`. The
//! owner pin cannot read those, so the finding reads `static_unknown` with
//! `rust_pinned_shape_unresolved`. Tails whose closure may not run, other
//! exits, pins that are not exact or not isolated, and RIPR-SPEC-0265's
//! slice-parameter adaptor keep their gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-pinned-shape-limit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"pinned-shape\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), diff)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The one finding of `family` on `line` after `old` becomes `new` there.
/// `lib` holds the new text.
fn finding(
    lib: &str,
    line: usize,
    old: &str,
    new: &str,
    family: ProbeFamily,
) -> Result<Finding, String> {
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{line},1 +{line},1 @@
-{old}
+{new}
"
    );
    let repo = TempRepo::create(lib, &diff)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut on_line = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.line == line && finding.probe.family == family);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one {family:?} finding on line {line}, got {other:?} in {:?}",
            output.findings
        )),
    }
}

fn limit(finding: &Finding) -> Option<&'static str> {
    finding.static_limit_kind.map(|kind| kind.as_str())
}

/// An owner on lines 1-3 and a test with `body`.
fn owner_with_test(owner: &str, test_attrs: &str, body: &str) -> String {
    format!(
        "{owner}

#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    {test_attrs}
    fn pinned() {{
        {body}
    }}
}}
"
    )
}

const SPAWNED: &str = "pub fn doubled(x: u32) -> u32 {
    std::thread::spawn(move || 2 * x).join().unwrap()
}";

fn spawned(test_attrs: &str, body: &str) -> Result<Finding, String> {
    finding(
        &owner_with_test(SPAWNED, test_attrs, body),
        2,
        "    std::thread::spawn(move || x * 2).join().unwrap()",
        "    std::thread::spawn(move || 2 * x).join().unwrap()",
        ProbeFamily::ReturnValue,
    )
}

#[test]
fn an_exact_pin_through_a_spawned_closure_reads_as_a_shape_limit() -> Result<(), String> {
    for body in [
        "assert_eq!(doubled(5), 10);",
        "assert_eq!(10, doubled(5));",
        "assert_eq!(doubled(5), 10, \"ten\");",
    ] {
        let finding = spawned("", body)?;
        assert_eq!(finding.class, ExposureClass::StaticUnknown, "{body}");
        assert_eq!(
            limit(&finding),
            Some("rust_pinned_shape_unresolved"),
            "{body}"
        );
        assert!(
            finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("`pinned`") && step.contains("assert_eq!")),
            "{body}: {:?}",
            finding.recommended_next_step
        );
    }
    Ok(())
}

#[test]
fn a_pin_that_is_not_exact_or_not_isolated_keeps_the_gap() -> Result<(), String> {
    for (attrs, body) in [
        // Not an equality pin.
        ("", "assert!(doubled(5) > 0);"),
        // The expected value is computed through the owner.
        ("", "assert_eq!(doubled(5), doubled(5));"),
        // No assertion calls the owner.
        ("", "assert_eq!(2 + 2, 4); doubled(5);"),
        // A test that never runs, or passes when the values differ.
        ("#[ignore]", "assert_eq!(doubled(5), 10);"),
        ("#[should_panic]", "assert_eq!(doubled(5), 10);"),
    ] {
        let finding = spawned(attrs, body)?;
        // A test that never runs leaves no live oracle, so those rows are a
        // different class; the rest are the weak gap this rule must not hide.
        if attrs.is_empty() {
            assert_eq!(
                finding.class,
                ExposureClass::WeaklyExposed,
                "{attrs} {body}"
            );
        } else {
            assert_ne!(
                finding.class,
                ExposureClass::StaticUnknown,
                "{attrs} {body}"
            );
        }
        assert_eq!(limit(&finding), None, "{attrs} {body}");
    }
    Ok(())
}

#[test]
fn an_applied_returned_closure_pin_is_no_longer_withheld() -> Result<(), String> {
    // RIPR-SPEC-0265 rule 5 (#9490): the test applies the returned closure on
    // distinct nonzero literals, so the owner pin reads it and the shape-limit
    // rule must not withhold this shape. The shapes the credit still refuses
    // (zero or repeated inputs, a closure that is never applied) stay
    // controlled by `returned_closure_pin.rs`.
    let lib = owner_with_test(
        "pub fn make_adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| n + x
}",
        "",
        "assert_eq!(make_adder(2)(3), 5);",
    );
    let finding = finding(
        &lib,
        2,
        "    move |x| x + n",
        "    move |x| n + x",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(limit(&finding), None);
    Ok(())
}

#[test]
fn a_collected_iterator_chain_reads_as_a_shape_limit() -> Result<(), String> {
    // `Result` collection: the closure runs per element and the test pins
    // the whole collected value.
    let results = owner_with_test(
        "pub fn parse_all(xs: &[&str]) -> Result<Vec<u32>, std::num::ParseIntError> {
    xs.iter().map(|s| s.parse::<u32>()).collect::<Result<Vec<_>, _>>()
}",
        "",
        "assert_eq!(parse_all(&[\"1\", \"2\"]), Ok(vec![1, 2]));",
    );
    let finding_a = finding(
        &results,
        2,
        "    xs.iter().map(|s| s.parse::<u32>()).collect()",
        "    xs.iter().map(|s| s.parse::<u32>()).collect::<Result<Vec<_>, _>>()",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(finding_a.class, ExposureClass::StaticUnknown);
    assert_eq!(limit(&finding_a), Some("rust_pinned_shape_unresolved"));
    // An opaque iterator return the test collects.
    let opaque = owner_with_test(
        "pub fn evens(limit: u32) -> impl Iterator<Item = u32> {
    (0..limit).filter(|n| n & 1 == 0)
}",
        "",
        "assert_eq!(evens(7).collect::<Vec<_>>(), vec![0, 2, 4, 6]);",
    );
    let finding_b = finding(
        &opaque,
        2,
        "    (0..limit).filter(|n| n % 2 == 0)",
        "    (0..limit).filter(|n| n & 1 == 0)",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(finding_b.class, ExposureClass::StaticUnknown);
    assert_eq!(limit(&finding_b), Some("rust_pinned_shape_unresolved"));
    Ok(())
}

#[test]
fn a_closure_that_may_not_run_keeps_the_gap() -> Result<(), String> {
    // `Option::map` applies its closure only to `Some`; the pinned `None`
    // never runs the changed closure, so the gap is real.
    let lib = owner_with_test(
        "pub fn bump(v: Option<u32>) -> Option<u32> {
    v.map(|x| 1 + x)
}",
        "",
        "assert_eq!(bump(None), None);",
    );
    let finding = finding(
        &lib,
        2,
        "    v.map(|x| x + 1)",
        "    v.map(|x| 1 + x)",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(limit(&finding), None);
    Ok(())
}

#[test]
fn another_exit_before_the_closure_tail_keeps_the_gap() -> Result<(), String> {
    let lib = owner_with_test(
        "pub fn doubled(x: u32) -> u32 {
    if x == 0 { return 0; }
    std::thread::spawn(move || 2 * x).join().unwrap()
}",
        "",
        "assert_eq!(doubled(0), 0);",
    );
    let finding = finding(
        &lib,
        3,
        "    std::thread::spawn(move || x * 2).join().unwrap()",
        "    std::thread::spawn(move || 2 * x).join().unwrap()",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(limit(&finding), None);
    Ok(())
}

#[test]
fn a_slice_adaptor_stays_with_the_closure_adaptor_rule() -> Result<(), String> {
    // RIPR-SPEC-0265 example 5: the pin refuses an empty slice, so the gap is
    // kept rather than withheld.
    let lib = owner_with_test(
        "pub fn count_large(xs: &[u32]) -> usize {
    xs.iter().filter(|&&x| x > 100).count()
}",
        "",
        "assert_eq!(count_large(&[]), 0);",
    );
    let finding = finding(
        &lib,
        2,
        "    xs.iter().filter(|x| **x > 100).count()",
        "    xs.iter().filter(|&&x| x > 100).count()",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(limit(&finding), None);
    Ok(())
}

#[test]
fn a_statement_acting_on_the_returned_value_reads_as_a_shape_limit() -> Result<(), String> {
    let lib = owner_with_test(
        "pub fn top_two(mut v: Vec<u32>) -> Vec<u32> {
    v.sort_by_key(|x| std::cmp::Reverse(*x));
    v.truncate(2);
    v
}",
        "",
        "assert_eq!(top_two(vec![3, 9, 5]), vec![9, 5]);",
    );
    let finding = finding(
        &lib,
        2,
        "    v.sort_by(|a, b| b.cmp(a));",
        "    v.sort_by_key(|x| std::cmp::Reverse(*x));",
        ProbeFamily::CallDeletion,
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    assert_eq!(limit(&finding), Some("rust_pinned_shape_unresolved"));
    Ok(())
}

#[test]
fn an_effect_on_a_value_the_owner_does_not_return_keeps_the_gap() -> Result<(), String> {
    let lib = owner_with_test(
        "pub fn tidy(mut v: Vec<u32>) -> usize {
    v.sort_by_key(|x| std::cmp::Reverse(*x));
    v.len()
}",
        "",
        "assert_eq!(tidy(vec![3, 9, 5]), 3);",
    );
    let finding = finding(
        &lib,
        2,
        "    v.sort_by(|a, b| b.cmp(a));",
        "    v.sort_by_key(|x| std::cmp::Reverse(*x));",
        ProbeFamily::CallDeletion,
    )?;
    assert_eq!(limit(&finding), None);
    Ok(())
}

#[test]
fn a_let_else_exit_reads_as_a_shape_limit() -> Result<(), String> {
    let lib = owner_with_test(
        "pub fn word_len(s: &str) -> usize {
    let Some(w) = s.split_whitespace().next() else {
        return 0usize;
    };
    w.len()
}",
        "",
        "assert_eq!(word_len(\"   \"), 0);",
    );
    let finding = finding(
        &lib,
        3,
        "        return 0;",
        "        return 0usize;",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown);
    assert_eq!(limit(&finding), Some("rust_pinned_shape_unresolved"));
    Ok(())
}

#[test]
fn a_plain_branch_exit_keeps_the_gap() -> Result<(), String> {
    // An `if` guard's exit is not a `let ... else`.
    let lib = owner_with_test(
        "pub fn word_len(s: &str) -> usize {
    if s.is_empty() {
        return 0usize;
    }
    s.len()
}",
        "",
        "assert_eq!(word_len(\"\"), 0);",
    );
    let finding = finding(
        &lib,
        3,
        "        return 0;",
        "        return 0usize;",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(limit(&finding), None);
    Ok(())
}

/// A gap the rule must leave alone: `lib` with the change on `line`.
fn keeps_gap(
    lib: &str,
    line: usize,
    old: &str,
    new: &str,
    family: ProbeFamily,
) -> Result<(), String> {
    let finding = finding(lib, line, old, new, family)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{lib}");
    assert_eq!(limit(&finding), None, "{lib}");
    Ok(())
}

#[test]
fn a_pin_of_less_than_the_whole_result_keeps_the_gap() -> Result<(), String> {
    for body in [
        "assert_eq!(doubled(5).wrapping_add(0), 10);",
        "assert_eq!(f(doubled(5)), 10);",
        "assert_eq!(doubled(5) + 1, 11);",
    ] {
        let lib = format!(
            "fn f(x: u32) -> u32 {{ x }}\n\n{}",
            owner_with_test(SPAWNED, "", body)
        );
        keeps_gap(
            &lib,
            4,
            "    std::thread::spawn(move || x * 2).join().unwrap()",
            "    std::thread::spawn(move || 2 * x).join().unwrap()",
            ProbeFamily::ReturnValue,
        )?;
    }
    Ok(())
}

#[test]
fn an_empty_or_zero_input_keeps_the_gap() -> Result<(), String> {
    // The closure never runs for an empty collection, and a zero argument
    // makes an arithmetic edit invisible.
    let lib = owner_with_test(
        "pub fn bump_all(v: Vec<u32>) -> Vec<u32> {
    v.into_iter().map(|x| 1 + x).collect()
}",
        "",
        "assert_eq!(bump_all(vec![]), vec![]);",
    );
    keeps_gap(
        &lib,
        2,
        "    v.into_iter().map(|x| x + 1).collect()",
        "    v.into_iter().map(|x| 1 + x).collect()",
        ProbeFamily::ReturnValue,
    )?;
    let lib = owner_with_test(
        "pub fn make_offset(n: i32) -> impl Fn(i32) -> i32 {
    move |x| n + x
}",
        "",
        "assert_eq!(make_offset(0)(7), 7);",
    );
    keeps_gap(
        &lib,
        2,
        "    move |x| x + n",
        "    move |x| n + x",
        ProbeFamily::ReturnValue,
    )
}

#[test]
fn an_option_combinator_after_an_iterator_keeps_the_gap() -> Result<(), String> {
    let lib = owner_with_test(
        "pub fn first_doubled(v: Vec<u32>) -> Option<u32> {
    v.iter().next().map(|x| 2 * x)
}",
        "",
        "assert_eq!(first_doubled(vec![]), None);",
    );
    keeps_gap(
        &lib,
        2,
        "    v.iter().next().map(|x| x * 2)",
        "    v.iter().next().map(|x| 2 * x)",
        ProbeFamily::ReturnValue,
    )
}

#[test]
fn a_let_else_initializer_or_a_second_exit_keeps_the_gap() -> Result<(), String> {
    // The change is in the initializer, which runs on every input.
    let lib = owner_with_test(
        "pub fn first_len(s: &str) -> usize {
    let Some(w) = s.split_whitespace().nth(0 + 0) else {
        return 0;
    };
    w.len()
}",
        "",
        "assert_eq!(first_len(\"ab cd\"), 2);",
    );
    let initializer = finding(
        &lib,
        2,
        "    let Some(w) = s.split_whitespace().nth(0) else {",
        "    let Some(w) = s.split_whitespace().nth(0 + 0) else {",
        ProbeFamily::ReturnValue,
    )?;
    // Not this rule's: the initializer runs on every input.
    assert_eq!(limit(&initializer), None);
    // Another early exit beside the let-else.
    let lib = owner_with_test(
        "pub fn first_len(s: &str, strict: bool) -> usize {
    if strict { return 7; }
    let Some(w) = s.split_whitespace().next() else {
        return 0usize;
    };
    w.len()
}",
        "",
        "assert_eq!(first_len(\"   \", false), 0);",
    );
    keeps_gap(
        &lib,
        4,
        "        return 0;",
        "        return 0usize;",
        ProbeFamily::ReturnValue,
    )
}

#[test]
fn a_shadowed_or_reassigned_receiver_keeps_the_gap() -> Result<(), String> {
    for rebind in ["let v = Vec::new();", "v = Vec::new();"] {
        let lib = owner_with_test(
            &format!(
                "pub fn top(mut v: Vec<u32>) -> Vec<u32> {{
    v.sort_by_key(|x| std::cmp::Reverse(*x));
    {rebind}
    v
}}"
            ),
            "",
            "assert_eq!(top(vec![3, 9, 5]), vec![]);",
        );
        keeps_gap(
            &lib,
            2,
            "    v.sort_by(|a, b| b.cmp(a));",
            "    v.sort_by_key(|x| std::cmp::Reverse(*x));",
            ProbeFamily::CallDeletion,
        )?;
    }
    Ok(())
}

#[test]
fn a_closure_the_tail_never_calls_keeps_the_gap() -> Result<(), String> {
    let lib = owner_with_test(
        "pub fn seven(x: u32) -> u32 {
    let _unused = || x + 1;
    7
}",
        "",
        "assert_eq!(seven(5), 7);",
    );
    keeps_gap(
        &lib,
        2,
        "    let _unused = || x + 2;",
        "    let _unused = || x + 1;",
        ProbeFamily::Predicate,
    )
}

#[test]
fn a_single_line_let_else_initializer_keeps_the_gap() -> Result<(), String> {
    let lib = owner_with_test(
        "pub fn first(xs: &[u32]) -> u32 {
    let Some(w) = xs.get(1) else { return 0; };
    *w
}",
        "",
        "assert_eq!(first(&[4, 4]), 4);",
    );
    keeps_gap(
        &lib,
        2,
        "    let Some(w) = xs.get(0) else { return 0; };",
        "    let Some(w) = xs.get(1) else { return 0; };",
        ProbeFamily::ReturnValue,
    )
}

#[test]
fn an_equality_that_may_not_run_does_not_withhold() -> Result<(), String> {
    let finding = spawned(
        "",
        "doubled(5);
        if false {
            assert_eq!(doubled(5), 10);
        }
        assert_eq!(1, 1);",
    )?;
    assert_eq!(limit(&finding), None);
    Ok(())
}
