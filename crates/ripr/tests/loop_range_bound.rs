//! Public-API controls for a changed `for` range bound (#11399,
//! RIPR-SPEC-0370).
//!
//! `for i in 1..=n { acc += i; }` edited to `1..n` changes how many times the
//! loop runs. Today ripr has no probe or seam for the header, so the finding
//! is `static_unknown`. The fail-closed floor below must hold before and after
//! any producer: no pin may grade the change `exposed` unless it walks the
//! loop with an argument for which the old and new ranges differ. The
//! `#[ignore]` tests state what the producer must add and fail today.

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
            "ripr-loop-range-bound-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"loop-range-bound\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// Line of the `for` header in `owner_with_test`.
const HEADER_LINE: usize = 3;

/// Every finding on the `for` header after `1..=n` became `1..n`, beside a
/// test whose body is `test_body`.
fn header_findings(test_body: &str) -> Result<Vec<Finding>, String> {
    let lib = format!(
        "pub fn sum_to(n: u32) -> u32 {{
    let mut acc = 0;
    for i in 1..n {{
        acc += i;
    }}
    acc
}}

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
    let diff = format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -{HEADER_LINE},1 +{HEADER_LINE},1 @@
-    for i in 1..=n {{
+    for i in 1..n {{
"
    );
    let repo = TempRepo::create(&lib, &diff)?;
    let output = check_workspace(CheckInput {
        root: repo.root.clone(),
        base: None,
        diff_file: Some(repo.root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let on_header: Vec<Finding> = output
        .findings
        .iter()
        .filter(|finding| finding.probe.location.line == HEADER_LINE)
        .cloned()
        .collect();
    if on_header.is_empty() {
        return Err(format!(
            "premise: a finding on line {HEADER_LINE}, got {:?}",
            output.findings
        ));
    }
    Ok(on_header)
}

fn none_exposed(findings: &[Finding]) {
    for found in findings {
        assert_ne!(
            found.class,
            ExposureClass::Exposed,
            "a changed range bound was graded exposed: {found:?}"
        );
    }
}

#[test]
fn a_pin_on_an_empty_range_never_grades_the_changed_bound_exposed() -> Result<(), String> {
    // `1..=0` and `1..0` are both empty: this pin cannot notice the change.
    none_exposed(&header_findings("assert_eq!(sum_to(0), 0);")?);
    Ok(())
}

#[test]
fn a_bound_or_computed_pin_never_grades_the_changed_bound_exposed() -> Result<(), String> {
    none_exposed(&header_findings(
        "let n = 0;
        assert_eq!(sum_to(n), 0);",
    )?);
    none_exposed(&header_findings(
        "let want = 5 + 10;
        assert_eq!(sum_to(5), want);",
    )?);
    none_exposed(&header_findings("assert!(sum_to(5) >= 1);")?);
    Ok(())
}

#[test]
#[ignore = "RIPR-SPEC-0370 producer not implemented: a changed range bound has no probe yet"]
fn producer_credits_a_pin_that_walks_the_loop_with_differing_ranges() -> Result<(), String> {
    // `1..=5` sums to 15, `1..5` to 10: this pin notices the change.
    let findings = header_findings("assert_eq!(sum_to(5), 15);")?;
    assert!(
        findings
            .iter()
            .any(|found| found.class == ExposureClass::Exposed),
        "{findings:?}"
    );
    Ok(())
}

#[test]
#[ignore = "RIPR-SPEC-0370 producer not implemented: a changed range bound has no probe yet"]
fn producer_keeps_the_empty_range_pin_a_gap() -> Result<(), String> {
    let findings = header_findings("assert_eq!(sum_to(0), 0);")?;
    assert!(
        findings
            .iter()
            .any(|found| found.class == ExposureClass::WeaklyExposed),
        "{findings:?}"
    );
    none_exposed(&findings);
    Ok(())
}
