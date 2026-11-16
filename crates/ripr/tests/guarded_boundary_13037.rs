//! #13037: a changed comparison joined by `&&` to a boolean parameter guard
//! (`order_cents >= FREE_OVER_CENTS && !express`) compares `order_cents`
//! with `FREE_OVER_CENTS`. The guard only selects which test rows reach that
//! boundary; before this, the `&& !express` tail was read as part of the
//! boundary operand and every such boundary stayed `infection_unknown`.
//!
//! Credit needs one test call that sits on the boundary value while holding
//! every guard, paired with an exact assertion on that call. A boundary row
//! that misses a guard never pairs, and a guarded boundary no row reaches
//! stays unknown with the guard named; it is never promoted to an actionable
//! gap.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const BEFORE: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents > FREE_OVER_CENTS && !express {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

const AFTER: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && !express {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,3 +3,3 @@ pub const FREE_OVER_CENTS: u32 = 5_000;
 pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
-    if order_cents > FREE_OVER_CENTS && !express {
+    if order_cents >= FREE_OVER_CENTS && !express {
         return 0;
";

const INSERTION_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,2 +3,5 @@ pub const FREE_OVER_CENTS: u32 = 5_000;
 pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
+    if order_cents >= FREE_OVER_CENTS && !express {
+        return 0;
+    }
     if express { weight_g * 2 } else { weight_g }
";

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, tests: &str) -> Result<Self, String> {
        Self::create_with(source, PATCH, tests)
    }

    fn create_with(source: &str, patch: &str, tests: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-guarded-boundary-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"guarded-boundary\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        let lib =
            format!("{source}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{tests}\n}}\n");
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), patch)
            .map_err(|error| format!("write diff failed: {error}"))?;
        Ok(Self { root })
    }

    fn guarded_finding(&self) -> Result<Finding, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            base: None,
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let mut guarded = output.findings.into_iter().filter(|finding| {
            finding.probe.family == ProbeFamily::Predicate
                && finding.probe.location.line == 4
                && finding.probe.expression.contains("&& !express")
        });
        match (guarded.next(), guarded.next()) {
            (Some(finding), None) => Ok(finding),
            other => Err(format!(
                "premise: exactly one predicate finding for the guarded boundary, got {other:?}"
            )),
        }
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn test_fn(name: &str, body: &str) -> String {
    format!("    #[test]\n    fn {name}() {{\n        {body}\n    }}\n")
}

#[test]
fn premise_fixture_source_differs_only_on_the_guarded_line() {
    let changed: Vec<(&str, &str)> = BEFORE
        .lines()
        .zip(AFTER.lines())
        .filter(|(before, after)| before != after)
        .collect();
    assert_eq!(
        changed,
        vec![(
            "    if order_cents > FREE_OVER_CENTS && !express {",
            "    if order_cents >= FREE_OVER_CENTS && !express {"
        )]
    );
}

#[test]
fn boundary_row_holding_the_guard_with_exact_pin_is_exposed() -> Result<(), String> {
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create(AFTER, &tests)?.guarded_finding()?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn compact_two_calls_per_line_still_pair_per_call() -> Result<(), String> {
    // One test line holding a guard-missing boundary call and a
    // guard-holding below-boundary call: neither call alone sits on the
    // boundary with the guard, so nothing pairs.
    let missing = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, true), 200); assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create(AFTER, &missing)?.guarded_finding()?;
    assert_eq!(
        finding.class,
        ExposureClass::InfectionUnknown,
        "{finding:?}"
    );

    let held = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0); assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create(AFTER, &held)?.guarded_finding()?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn boundary_rows_that_miss_the_guard_stay_unknown_with_the_guard_named() -> Result<(), String> {
    // With `express = true` the boundary never decides the result: these
    // rows sit on 5000 but cannot tell `>` from `>=`.
    let tests = test_fn(
        "express_only",
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n        assert_eq!(shipping_cost(100, 4999, true), 200);",
    );
    let finding = TempRepo::create(AFTER, &tests)?.guarded_finding()?;
    assert_eq!(
        finding.class,
        ExposureClass::InfectionUnknown,
        "{finding:?}"
    );
    let named = finding.missing.iter().any(|line| {
        line.contains("`order_cents >= FREE_OVER_CENTS`")
            && line.contains("`express = false`")
            && line.contains("`FREE_OVER_CENTS` (5_000)")
    });
    assert!(
        named,
        "the guard and boundary value are named: {:?}",
        finding.missing
    );
    assert!(
        !finding
            .missing
            .iter()
            .any(|line| line.contains("FREE_OVER_CENTS && !express`")),
        "the guard is not read as part of the operand: {:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn no_row_on_the_boundary_stays_unknown_not_weak() -> Result<(), String> {
    let tests = test_fn(
        "near",
        "assert_eq!(shipping_cost(100, 4999, false), 100);\n        assert_eq!(shipping_cost(100, 9000, false), 0);",
    );
    let finding = TempRepo::create(AFTER, &tests)?.guarded_finding()?;
    assert_eq!(
        finding.class,
        ExposureClass::InfectionUnknown,
        "{finding:?}"
    );
    Ok(())
}

#[test]
fn boundary_call_and_exact_pin_in_separate_tests_do_not_pair() -> Result<(), String> {
    let tests = format!(
        "{}{}",
        test_fn("feeds", "let _ = shipping_cost(100, 5000, false);"),
        test_fn("pins", "assert_eq!(shipping_cost(100, 9000, false), 0);")
    );
    let finding = TempRepo::create(AFTER, &tests)?.guarded_finding()?;
    // The boundary input is real but unpaired: the same partial reading an
    // unguarded boundary gets for split tests.
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{finding:?}");
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("same_test_pairing_missing")),
        "{:?}",
        finding.missing
    );
    Ok(())
}

#[test]
fn a_rebound_guard_is_not_credited() -> Result<(), String> {
    // A `mut` guard may hold something other than the test's argument by the
    // compared line, so the guarded reading declines; these are the same
    // calls the exposed case credits.
    let source = AFTER.replace("express: bool", "mut express: bool");
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create(&source, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn a_guard_missing_pin_beside_a_guard_holding_row_in_one_test_does_not_pair() -> Result<(), String>
{
    // The unasserted call supplies the guard-holding boundary row; the only
    // exact pin observes a call with `express = true`, which cannot tell
    // `>` from `>=`.
    let tests = test_fn(
        "mixed",
        "let _ = shipping_cost(100, 5000, false);\n        assert_eq!(shipping_cost(100, 5000, true), 200);",
    );
    let finding = TempRepo::create(AFTER, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn a_bound_result_of_a_guard_missing_call_does_not_pair() -> Result<(), String> {
    // Literal boundary: the old split read `5000 && !express` and found the
    // literal 5000, so a bound result of any call at 5000 could pair.
    let source = AFTER.replace("FREE_OVER_CENTS &&", "5000 &&");
    let patch = PATCH.replace("FREE_OVER_CENTS &&", "5000 &&");
    let tests = test_fn(
        "bound",
        "let _ = shipping_cost(100, 5000, false);\n        let got = shipping_cost(100, 5000, true);\n        assert_eq!(got, 200);",
    );
    let finding = TempRepo::create_with(&source, &patch, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");

    // Control: the bound result of the guard-holding call pairs.
    let tests = test_fn(
        "bound",
        "let got = shipping_cost(100, 5000, false);\n        assert_eq!(got, 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create_with(&source, &patch, &tests)?.guarded_finding()?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn an_edit_that_adds_the_guard_is_not_credited_from_guard_holding_rows() -> Result<(), String> {
    // Before: `order_cents >= FREE_OVER_CENTS`. Rows with `express = false`
    // behave the same on both sides; only an `express = true` row sees the
    // added guard, so the guarded reading must not credit them.
    let patch = PATCH.replace(
        "-    if order_cents > FREE_OVER_CENTS && !express {",
        "-    if order_cents >= FREE_OVER_CENTS {",
    );
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create_with(AFTER, &patch, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn an_inserted_guard_whose_boundary_row_stays_outside_is_not_credited() -> Result<(), String> {
    // A pure insertion of `if order_cents < FREE_OVER_CENTS && !express`:
    // the boundary row 5000 never enters the new branch, so the pinned call
    // returns the same value with or without the insertion. An unasserted
    // entering call beside it must not lend it credit.
    let source = AFTER.replace(">= FREE_OVER_CENTS", "< FREE_OVER_CENTS");
    let patch = INSERTION_PATCH.replace(">= FREE_OVER_CENTS", "< FREE_OVER_CENTS");
    let tests = test_fn(
        "pins",
        "let _ = shipping_cost(100, 4999, false);\n        assert_eq!(shipping_cost(100, 5000, false), 100);",
    );
    let finding = TempRepo::create_with(&source, &patch, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn an_inserted_guard_whose_boundary_row_enters_is_exposed() -> Result<(), String> {
    // The A/B shipping shape: the whole guarded branch is new, and the
    // pinned boundary row 5000 enters it.
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create_with(AFTER, INSERTION_PATCH, &tests)?.guarded_finding()?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn a_moved_threshold_is_not_credited_from_the_new_boundary_row() -> Result<(), String> {
    // `>= FREE_OVER_CENTS - 1` to `>= FREE_OVER_CENTS`: both accept 5000,
    // so a pin there cannot tell the versions apart (4999 would). A literal
    // threshold move (`>= 4999` to `>= 5000`) is declined the same way.
    let source = AFTER;
    let patch = PATCH.replace("> FREE_OVER_CENTS &&", ">= FREE_OVER_CENTS - 1 &&");
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4000, false), 100);",
    );
    let finding = TempRepo::create_with(source, &patch, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn operators_that_agree_at_the_boundary_are_not_credited() -> Result<(), String> {
    // `<=` to `>=` both hold at 5000.
    let patch = PATCH.replace(
        "-    if order_cents > FREE_OVER_CENTS && !express {",
        "-    if order_cents <= FREE_OVER_CENTS && !express {",
    );
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create_with(AFTER, &patch, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn a_bound_result_does_not_borrow_a_same_line_sibling_guard() -> Result<(), String> {
    let source = AFTER.replace("FREE_OVER_CENTS &&", "5000 &&");
    let patch = PATCH.replace("FREE_OVER_CENTS &&", "5000 &&");
    let tests = test_fn(
        "bound",
        "let got = shipping_cost(100, 5000, true); let _ = shipping_cost(100, 5000, false);\n        assert_eq!(got, 200);",
    );
    let finding = TempRepo::create_with(&source, &patch, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

/// A guarded shape the rule declines is unresolved with its guards named,
/// rather than credited through the compound-predicate reading (#9637),
/// which reads the comparison without its guards.
fn assert_declined_and_named(finding: &Finding) {
    assert_eq!(
        finding.class,
        ExposureClass::InfectionUnknown,
        "{finding:?}"
    );
    assert!(
        finding.missing.iter().any(|line| {
            line.contains("`order_cents >= FREE_OVER_CENTS`")
                && line.contains("`express = false`")
                && line.contains("does not read which test rows reach it")
        }),
        "the declined guarded shape names its guards: {:?}",
        finding.missing
    );
}

#[test]
fn a_declined_guarded_shape_is_unknown_with_the_guards_named() -> Result<(), String> {
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    // Operators that agree at the boundary.
    let agree = PATCH.replace(
        "-    if order_cents > FREE_OVER_CENTS && !express {",
        "-    if order_cents <= FREE_OVER_CENTS && !express {",
    );
    assert_declined_and_named(&TempRepo::create_with(AFTER, &agree, &tests)?.guarded_finding()?);
    // A guard that is not the caller's argument by the changed line.
    let rebound = AFTER.replace("express: bool", "mut express: bool");
    let rebound_patch = PATCH.replace("express: bool", "mut express: bool");
    assert_declined_and_named(
        &TempRepo::create_with(&rebound, &rebound_patch, &tests)?.guarded_finding()?,
    );
    Ok(())
}

#[test]
fn a_changed_equality_value_is_credited_only_from_guard_holding_rows() -> Result<(), String> {
    // `order_cents == 4000` to `order_cents == 5000`: a row on 5000 takes
    // the other branch in each version, but only while `express = false`.
    let source = AFTER.replace(">= FREE_OVER_CENTS", "== 5000");
    let patch = PATCH
        .replace("> FREE_OVER_CENTS &&", "== 4000 &&")
        .replace(">= FREE_OVER_CENTS &&", "== 5000 &&");
    let held = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create_with(&source, &patch, &held)?.guarded_finding()?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");

    let missing = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n        assert_eq!(shipping_cost(100, 4999, true), 200);",
    );
    let finding = TempRepo::create_with(&source, &patch, &missing)?.guarded_finding()?;
    assert_eq!(
        finding.class,
        ExposureClass::InfectionUnknown,
        "{finding:?}"
    );
    Ok(())
}

// #14873: rustfmt wraps a long condition and puts the guard on its own
// line. The diff then changes only the comparison line, and the guard must
// still be read from the line below it.
const WRAPPED_AFTER: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS
        && !express
    {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

const WRAPPED_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,4 +3,4 @@ pub const FREE_OVER_CENTS: u32 = 5_000;
 pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
-    if order_cents > FREE_OVER_CENTS
+    if order_cents >= FREE_OVER_CENTS
         && !express
     {
";

fn wrapped_finding(tests: &str) -> Result<Finding, String> {
    let repo = TempRepo::create_with(WRAPPED_AFTER, WRAPPED_PATCH, tests)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let mut found = output.findings.into_iter().filter(|finding| {
        finding.probe.family == ProbeFamily::Predicate && finding.probe.location.line == 4
    });
    match (found.next(), found.next()) {
        (Some(finding), None) => Ok(finding),
        other => Err(format!(
            "premise: exactly one predicate finding on the wrapped line, got {other:?}"
        )),
    }
}

#[test]
fn a_wrapped_guard_is_read_from_the_line_below() -> Result<(), String> {
    let missing = test_fn(
        "express_only",
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n        assert_eq!(shipping_cost(100, 4999, true), 200);",
    );
    let finding = wrapped_finding(&missing)?;
    assert_eq!(
        finding.class,
        ExposureClass::InfectionUnknown,
        "{finding:?}"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("`express = false`")),
        "the wrapped guard is named: {:?}",
        finding.missing
    );

    let held = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = wrapped_finding(&held)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

const WRAPPED_GUARD_ADDED_BEFORE: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
    if order_cents > FREE_OVER_CENTS {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

const WRAPPED_GUARD_ADDED_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,3 +3,5 @@ pub const FREE_OVER_CENTS: u32 = 5_000;
 pub fn shipping_cost(weight_g: u32, order_cents: u32, express: bool) -> u32 {
-    if order_cents > FREE_OVER_CENTS {
+    if order_cents >= FREE_OVER_CENTS
+        && !express
+    {
         return 0;
";

#[test]
fn an_edit_that_adds_a_wrapped_guard_is_not_credited_from_guard_missing_rows() -> Result<(), String>
{
    assert!(WRAPPED_GUARD_ADDED_BEFORE.contains("if order_cents > FREE_OVER_CENTS {"));
    let tests = test_fn(
        "express_only",
        "assert_eq!(shipping_cost(100, 5000, true), 200);\n        assert_eq!(shipping_cost(100, 4999, true), 200);",
    );
    let repo = TempRepo::create_with(WRAPPED_AFTER, WRAPPED_GUARD_ADDED_PATCH, &tests)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let finding = output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::Predicate && finding.probe.location.line == 4
        })
        .ok_or("premise: a predicate finding on the wrapped line")?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

const TWO_GUARDS_WRAPPED_AFTER: &str = "pub const FREE_OVER_CENTS: u32 = 5_000;

pub fn shipping_cost(weight_g: u32, order_cents: u32, member: bool, express: bool) -> u32 {
    if order_cents >= FREE_OVER_CENTS && member
        && !express
    {
        return 0;
    }
    if express { weight_g * 2 } else { weight_g }
}
";

const TWO_GUARDS_WRAPPED_PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,4 +3,4 @@ pub const FREE_OVER_CENTS: u32 = 5_000;
 pub fn shipping_cost(weight_g: u32, order_cents: u32, member: bool, express: bool) -> u32 {
-    if order_cents > FREE_OVER_CENTS && member
+    if order_cents >= FREE_OVER_CENTS && member
         && !express
     {
";

fn two_guard_finding(tests: &str) -> Result<Finding, String> {
    let repo = TempRepo::create_with(TWO_GUARDS_WRAPPED_AFTER, TWO_GUARDS_WRAPPED_PATCH, tests)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    output
        .findings
        .into_iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::Predicate && finding.probe.location.line == 4
        })
        .ok_or_else(|| "premise: a predicate finding on the guarded head".to_string())
}

#[test]
fn a_second_guard_wrapped_under_a_guarded_head_is_read() -> Result<(), String> {
    let missing = test_fn(
        "member_express",
        "assert_eq!(shipping_cost(100, 5000, true, true), 200);\n        assert_eq!(shipping_cost(100, 4999, true, true), 200);",
    );
    let finding = two_guard_finding(&missing)?;
    assert_eq!(
        finding.class,
        ExposureClass::InfectionUnknown,
        "{finding:?}"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|line| line.contains("`express = false`")),
        "the wrapped second guard is named: {:?}",
        finding.missing
    );

    let held = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, true, false), 0);\n        assert_eq!(shipping_cost(100, 4999, true, false), 100);",
    );
    let finding = two_guard_finding(&held)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
