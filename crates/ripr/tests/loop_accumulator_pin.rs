//! Public-API controls for the zero-initialized loop accumulator the owner
//! returns (#6485, RIPR-SPEC-0360).
//!
//! `for w in items { sum += w; }` with `sum` starting at zero and returned as
//! the tail is a `return_value` change. An exact pin credits it only when the
//! pinned call walks a non-empty literal sequence and pins a nonzero integer;
//! an empty or zero pin keeps its gap, and an exact pin ripr cannot read that
//! way is withheld as `rust_pinned_shape_unresolved`, never a gap.

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
            "ripr-loop-accumulator-pin-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"loop-accumulator\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

const OWNER: &str = "pub fn total(items: &[u32]) -> u32 {
    let mut sum = 0;
    for w in items {
        sum += w;
    }
    sum
}
";

/// `OWNER` with `test_body` as the one test, and the finding on line 4
/// where `sum = sum + w;` became `sum += w;`.
fn accumulator_finding(owner: &str, test_body: &str) -> Result<Finding, String> {
    accumulator_finding_from(
        owner,
        "        sum = sum + w;",
        "        sum += w;",
        test_body,
    )
}

/// `accumulator_finding` for an update line 4 that became `new` from `old`.
fn accumulator_finding_from(
    owner: &str,
    old: &str,
    new: &str,
    test_body: &str,
) -> Result<Finding, String> {
    let lib = format!(
        "{owner}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn pinned() {{
        {test_body}
    }}
}}
"
    );
    finding(&lib, 4, old, new, ProbeFamily::ReturnValue)
}

#[test]
fn a_nonempty_literal_input_and_a_nonzero_total_credit_the_update() -> Result<(), String> {
    let found = accumulator_finding(OWNER, "assert_eq!(total(&[3, 4, 5]), 12);")?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_negative_total_from_a_subtracting_update_credits_it() -> Result<(), String> {
    let owner = OWNER
        .replace("-> u32", "-> i32")
        .replace("&[u32]", "&[i32]")
        .replace("sum += w;", "sum -= w;");
    let found = accumulator_finding_from(
        &owner,
        "        sum = sum + w;",
        "        sum -= w;",
        "assert_eq!(total(&[2, 3]), -5);",
    )?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_zero_element_does_not_make_the_pin_degenerate() -> Result<(), String> {
    // The pinned input walks the loop even though one element is zero, so a
    // computed expectation is withheld like any other unreadable exact pin.
    let found = accumulator_finding(
        OWNER,
        "let want = 3 + 2;
        assert_eq!(total(&[0, 3, 2]), want);",
    )?;
    assert_eq!(found.class, ExposureClass::StaticUnknown, "{found:?}");
    assert_eq!(limit(&found), Some("rust_pinned_shape_unresolved"));
    Ok(())
}

#[test]
fn a_cancelling_run_keeps_the_zero_pin_gap() -> Result<(), String> {
    let owner = OWNER
        .replace("-> u32", "-> i32")
        .replace("&[u32]", "&[i32]");
    let found = accumulator_finding(&owner, "assert_eq!(total(&[2, -2]), 0);")?;
    assert_eq!(found.class, ExposureClass::WeaklyExposed, "{found:?}");
    assert_eq!(limit(&found), None);
    Ok(())
}

#[test]
fn a_mutable_slice_parameter_is_the_shape_but_its_pin_is_not_read() -> Result<(), String> {
    // The parameter qualifies, so the update keeps its `return_value` probe;
    // the literal reader does not read `&mut [..]` as a run, so the exact pin
    // is withheld instead of called a gap or credited.
    let owner = OWNER.replace("&[u32]", "&mut [u32]");
    let found = accumulator_finding(&owner, "assert_eq!(total(&mut [3, 4, 5]), 12);")?;
    assert_ne!(found.class, ExposureClass::WeaklyExposed, "{found:?}");
    assert_ne!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn an_empty_literal_input_never_reaches_the_update_and_keeps_the_gap() -> Result<(), String> {
    let found = accumulator_finding(OWNER, "assert_eq!(total(&[]), 0);")?;
    assert_eq!(found.class, ExposureClass::WeaklyExposed, "{found:?}");
    assert_eq!(limit(&found), None);
    Ok(())
}

#[test]
fn a_zero_total_separates_nothing_and_keeps_the_gap() -> Result<(), String> {
    let found = accumulator_finding(OWNER, "assert_eq!(total(&[0, 0]), 0);")?;
    assert_eq!(found.class, ExposureClass::WeaklyExposed, "{found:?}");
    assert_eq!(limit(&found), None);
    Ok(())
}

#[test]
fn a_call_that_is_never_asserted_stays_a_gap() -> Result<(), String> {
    let found = accumulator_finding(OWNER, "let _ = total(&[3, 4, 5]);")?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:?}");
    assert_ne!(found.class, ExposureClass::StaticUnknown, "{found:?}");
    Ok(())
}

#[test]
fn an_exact_pin_on_a_bound_input_is_withheld_not_called_a_gap() -> Result<(), String> {
    let found = accumulator_finding(
        OWNER,
        "let rolls = vec![3, 4, 5];
        assert_eq!(total(&rolls), 12);",
    )?;
    assert_eq!(found.class, ExposureClass::StaticUnknown, "{found:?}");
    assert_eq!(limit(&found), Some("rust_pinned_shape_unresolved"));
    Ok(())
}

#[test]
fn a_computed_expectation_is_withheld_not_called_a_gap() -> Result<(), String> {
    let found = accumulator_finding(
        OWNER,
        "let want = 3 + 4 + 5;
        assert_eq!(total(&[3, 4, 5]), want);",
    )?;
    assert_eq!(found.class, ExposureClass::StaticUnknown, "{found:?}");
    assert_eq!(limit(&found), Some("rust_pinned_shape_unresolved"));
    Ok(())
}

#[test]
fn a_helper_that_pins_literal_runs_credits_the_update() -> Result<(), String> {
    let lib = format!(
        "{OWNER}
#[cfg(test)]
mod tests {{
    use super::*;

    fn check(items: &[u32], want: u32) {{
        assert_eq!(total(items), want);
    }}

    #[test]
    fn pinned() {{
        check(&[6, 6, 1], 13);
        check(&[], 0);
    }}
}}
"
    );
    let found = finding(
        &lib,
        4,
        "        sum = sum + w;",
        "        sum += w;",
        ProbeFamily::ReturnValue,
    )?;
    assert_eq!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_helper_that_only_pins_empty_and_zero_runs_keeps_the_gap() -> Result<(), String> {
    let lib = format!(
        "{OWNER}
#[cfg(test)]
mod tests {{
    use super::*;

    fn check(items: &[u32], want: u32) {{
        assert_eq!(total(items), want);
    }}

    #[test]
    fn pinned() {{
        check(&[], 0);
        check(&[0], 0);
    }}
}}
"
    );
    let found = finding(
        &lib,
        4,
        "        sum = sum + w;",
        "        sum += w;",
        ProbeFamily::ReturnValue,
    )?;
    assert_ne!(found.class, ExposureClass::Exposed, "{found:?}");
    Ok(())
}

#[test]
fn a_nonzero_start_conditional_update_or_extra_use_is_not_the_shape() -> Result<(), String> {
    for owner in [
        OWNER.replace("let mut sum = 0;", "let mut sum = 10;"),
        OWNER.replace("sum += w;", "if *w > 1 { sum += w; }"),
        OWNER.replace("    sum\n}", "    sum + 1\n}"),
        OWNER.replace("sum += w;", "sum += w; sum += 1;"),
        // `w * 2` to `w + 2` also gives 4 for `[2]`, so a total pin cannot
        // separate a mutant inside the term.
        OWNER.replace("sum += w;", "sum += w * 2;"),
        // A shadowed walked parameter: the empty call input is not what loops.
        OWNER.replace(
            "let mut sum = 0;",
            "let items: &[u32] = &[3, 4, 5];\n    let mut sum = 0;",
        ),
        // Code before the initialization.
        OWNER.replace(
            "let mut sum = 0;",
            "let _ = items.len();\n    let mut sum = 0;",
        ),
        // A second parameter as the term could carry a user `AddAssign`.
        OWNER
            .replace("sum += w;", "sum += k;")
            .replace("items: &[u32]", "k: &W, items: &[u32]"),
        // Elements that are not a built-in integer type.
        OWNER.replace("&[u32]", "&[Money]"),
        OWNER.replace("&[u32]", "&[f64]"),
    ] {
        let lib = format!(
            "{owner}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn pinned() {{
        assert_eq!(total(&[3, 4, 5]), 12);
    }}
}}
"
        );
        // The update is not a returned accumulator, so it keeps its
        // `static_unknown` probe instead of a `return_value` one.
        let diff = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -4,1 +4,1 @@
-        sum = sum + w;
+        sum += w;
";
        let repo = TempRepo::create(&lib, diff)?;
        let output = check_workspace(CheckInput {
            root: repo.root.clone(),
            base: None,
            diff_file: Some(repo.root.join("diff.patch")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert!(
            output
                .findings
                .iter()
                .all(|found| found.probe.family != ProbeFamily::ReturnValue
                    || found.probe.location.line != 4),
            "{owner}: {:?}",
            output.findings
        );
    }
    Ok(())
}
