//! Public-API controls for the per-line call-fact budget (#8600).
//!
//! `extract_call_facts` keeps at most 256 distinct callees per source line.
//! A related test with a line at that budget may have had a call dropped, and
//! the dropped call may be its assertion helper, so a negative claim over that
//! test's calls (`reachable_unrevealed`) reads as `static_unknown` with the
//! typed `rust_call_fact_budget` reason. One call under the budget, repeated
//! names, a saturated line in an unrelated test, and an independently
//! established positive keep their findings. These controls use the real
//! extractor with distinct callee names, so they prove production
//! reachability; the guard-discrimination negatives live beside the guards.

use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = "pub fn row(index: usize) -> usize {
    if index >= 100 { return 100; }
    index / 4
}
";

/// The guard on line 2 changed.
const GUARD_DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn row(index: usize) -> usize {
-    if index > 99 { return 100; }
+    if index >= 100 { return 100; }
     index / 4
";

/// `row(100)` first, then `extra` distinct helper calls, all on one line.
fn noisy_line(extra: usize) -> String {
    let mut line = String::from("row(100);");
    for index in 0..extra {
        line.push_str(&format!("h{index}();"));
    }
    line
}

/// The owner, `body` as the `reaches` test, `other` as further tests, and the
/// helpers the noisy lines call.
fn source(body: &str, other: &str) -> String {
    let helpers: String = (0..400).map(|i| format!("fn h{i}() {{}}\n")).collect();
    format!(
        "{OWNER}
#[cfg(test)]
mod tests {{
    use super::*;

    #[test]
    fn reaches() {{
        {body}
    }}
{other}}}

{helpers}"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, extra: Option<(&str, &str)>) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-call-budget-limit-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"call-budget\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
        )
        .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
        std::fs::write(root.join("src/lib.rs"), lib)
            .map_err(|error| format!("write source failed: {error}"))?;
        if let Some((path, text)) = extra {
            let file = root.join(path);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("create {path} directory failed: {error}"))?;
            }
            std::fs::write(&file, text).map_err(|error| format!("write {path} failed: {error}"))?;
        }
        std::fs::write(root.join("diff.patch"), GUARD_DIFF)
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

/// The finding on the changed guard (line 2), which must be the only one.
fn guard_finding(lib: &str) -> Result<Finding, String> {
    guard_finding_in(TempRepo::create(lib, None)?)
}

/// As [`guard_finding`], with one more source file beside the library.
fn guard_finding_with_extra_file(lib: &str, path: &str, text: &str) -> Result<Finding, String> {
    guard_finding_in(TempRepo::create(lib, Some((path, text)))?)
}

fn guard_finding_in(repo: TempRepo) -> Result<Finding, String> {
    let findings = repo.findings()?;
    let mut on_line = findings
        .iter()
        .filter(|finding| finding.probe.location.line == 2);
    match (on_line.next(), on_line.next()) {
        (Some(finding), None) => Ok(finding.clone()),
        other => Err(format!(
            "premise: exactly one finding on line 2, got {other:?} in {findings:?}"
        )),
    }
}

fn limit(finding: &Finding) -> Option<&'static str> {
    finding.static_limit_kind.map(|kind| kind.as_str())
}

#[test]
fn a_saturated_related_test_line_withholds_the_gap_under_the_budget_reason() -> Result<(), String> {
    // 255 distinct callees (row + 254 helpers) leave the budget unreached, and
    // the test has no assertion: the baseline gap the later cases withhold.
    let control = guard_finding(&source(&noisy_line(254), ""))?;
    assert_eq!(control.class, ExposureClass::ReachableUnrevealed);
    assert_eq!(limit(&control), None);
    // Exactly 256 may have dropped a call, so the absence is not established.
    // 257 and 300 distinct callees drop calls for certain; `row` comes first
    // so the owner call itself is retained.
    for extra in [255, 256, 299] {
        let finding = guard_finding(&source(&noisy_line(extra), ""))?;
        assert_eq!(
            finding.class,
            ExposureClass::StaticUnknown,
            "{extra} helpers"
        );
        assert_eq!(limit(&finding), Some("rust_call_fact_budget"), "{extra}");
        assert!(
            finding
                .missing
                .iter()
                .all(|line| !line.contains("no assertion")),
            "{extra}: the withheld absence claim must not feed repair routes: {:?}",
            finding.missing
        );
        assert!(
            finding
                .recommended_next_step
                .as_deref()
                .is_some_and(|step| step.contains("`reaches`")),
            "{extra}: {:?}",
            finding.recommended_next_step
        );
    }
    Ok(())
}

#[test]
fn saturation_that_does_not_limit_the_proof_keeps_the_finding() -> Result<(), String> {
    // Repeated names are one fact each, so 2000 calls stay under the budget.
    let repeated = format!("row(100);{}", "h0();".repeat(2000));
    let finding = guard_finding(&source(&repeated, ""))?;
    assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
    assert_eq!(limit(&finding), None);
    // A saturated line in a test of another file that never reaches the owner
    // is not a related test, so the reaching test's gap stands.
    let finding = guard_finding_with_extra_file(
        &source("row(100);", ""),
        "tests/other.rs",
        &format!(
            "#[test]\nfn unrelated() {{\n    {}\n}}\n",
            (0..300).map(|i| format!("h{i}();")).collect::<String>()
        ),
    )?;
    assert_eq!(finding.class, ExposureClass::ReachableUnrevealed);
    assert_eq!(limit(&finding), None);
    Ok(())
}

#[test]
fn an_independently_established_positive_keeps_its_class() -> Result<(), String> {
    // The strong assertion sits on its own line; the saturated line only adds
    // calls. A dropped call cannot make an established positive a gap.
    let clean = guard_finding(&source("assert_eq!(row(100), 100);", ""))?;
    assert_eq!(clean.class, ExposureClass::Exposed, "premise: {clean:?}");
    let body = format!("{}\n        assert_eq!(row(100), 100);", noisy_line(299));
    let noisy = guard_finding(&source(&body, ""))?;
    assert_eq!(noisy.class, ExposureClass::Exposed, "{noisy:?}");
    assert_eq!(limit(&noisy), None);
    Ok(())
}
