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

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(source: &str, tests: &str) -> Result<Self, String> {
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
        let lib = format!(
            "{source}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{tests}\n}}\n"
        );
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        std::fs::write(root.join("diff.patch"), PATCH)
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
    assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{finding:?}");

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
    assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{finding:?}");
    let named = finding.missing.iter().any(|line| {
        line.contains("`order_cents >= FREE_OVER_CENTS`")
            && line.contains("`express = false`")
            && line.contains("`FREE_OVER_CENTS` (5_000)")
    });
    assert!(named, "the guard and boundary value are named: {:?}", finding.missing);
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
    assert_eq!(finding.class, ExposureClass::InfectionUnknown, "{finding:?}");
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
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}

#[test]
fn a_rebound_guard_keeps_the_previous_reading() -> Result<(), String> {
    // `express` is reassigned before the comparison, so a test's argument
    // is not the guard's value: the guarded reading declines and the
    // finding is never credited from it.
    let source = AFTER.replace(
        "    if order_cents >= FREE_OVER_CENTS && !express {",
        "    let express = express || weight_g > 9_000; if order_cents >= FREE_OVER_CENTS && !express {",
    );
    let tests = test_fn(
        "pins",
        "assert_eq!(shipping_cost(100, 5000, false), 0);\n        assert_eq!(shipping_cost(100, 4999, false), 100);",
    );
    let finding = TempRepo::create(&source, &tests)?.guarded_finding()?;
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    Ok(())
}
