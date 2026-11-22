//! End-to-end witness for #15420: a condition and its block body both
//! change on one line.
//!
//! The producer keeps `probe.before` as the whole old line, because
//! classifiers rebuild the old owner from it. The diagnostic witness (the
//! `ripr context` packet, LSP diagnostic data and repair cards read it) adds
//! the old condition as `before_condition`, so the old and new sides can be
//! shown at one granularity.

use ripr::domain::DiagnosticWitness;
use ripr::{CheckInput, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

/// `src/lib.rs` with `line` as the body of `price` on line 2.
fn source(line: &str) -> String {
    format!(
        "pub fn price(total: u32) -> u32 {{
    {line}
}}

#[cfg(test)]
mod tests {{
    use super::price;

    #[test]
    fn small_order_is_not_discounted() {{
        assert_eq!(price(5), 5);
    }}
}}
"
    )
}

/// Line 2 changed from `old` to `new`.
fn diff(old: &str, new: &str) -> String {
    format!(
        "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 pub fn price(total: u32) -> u32 {{
-    {old}
+    {new}
 }}
"
    )
}

struct TempRepo {
    root: PathBuf,
}

impl TempRepo {
    fn create(lib: &str, diff: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-condition-granularity-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src"))
            .map_err(|error| format!("create source directory failed: {error}"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"condition-granularity\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
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

/// The witness of the predicate finding on line 2.
fn predicate_witness(old: &str, new: &str) -> Result<(Finding, DiagnosticWitness), String> {
    let findings = TempRepo::create(&source(new), &diff(old, new))?.findings()?;
    let finding = findings
        .iter()
        .find(|finding| {
            finding.probe.family == ProbeFamily::Predicate && finding.probe.location.line == 2
        })
        .ok_or_else(|| format!("premise: a predicate finding on line 2, got {findings:?}"))?;
    let witness = DiagnosticWitness::from_finding(finding)
        .ok_or_else(|| format!("premise: the predicate finding is a gap, got {finding:?}"))?;
    Ok((finding.clone(), witness))
}

#[test]
fn a_condition_and_body_edit_pairs_the_old_condition_with_the_new_one() -> Result<(), String> {
    let old = "if total >= 100 { total - 10 } else { total }";
    let (finding, witness) =
        predicate_witness(old, "if total > 200 { total - 25 } else { total }")?;

    // The producer keeps the whole old line for the classifiers.
    assert_eq!(finding.probe.before.as_deref(), Some(old));
    assert_eq!(witness.before.as_deref(), Some(old));
    assert_eq!(witness.after.as_deref(), Some("total > 200"));
    assert_eq!(witness.before_condition.as_deref(), Some("total >= 100"));
    let json = serde_json::to_value(&witness).map_err(|error| error.to_string())?;
    assert_eq!(json["before_condition"], "total >= 100");
    Ok(())
}

/// A condition-only edit is cut by the producer already, so `before` is the
/// old condition and the witness adds nothing.
#[test]
fn a_condition_only_edit_keeps_the_cut_before_and_adds_no_field() -> Result<(), String> {
    let (finding, witness) = predicate_witness(
        "if total >= 100 { total - 10 } else { total }",
        "if total > 200 { total - 10 } else { total }",
    )?;

    assert_eq!(finding.probe.before.as_deref(), Some("total >= 100"));
    assert_eq!(witness.before.as_deref(), Some("total >= 100"));
    assert_eq!(witness.after.as_deref(), Some("total > 200"));
    assert_eq!(witness.before_condition, None);
    let json = serde_json::to_value(&witness).map_err(|error| error.to_string())?;
    assert!(json.get("before_condition").is_none(), "{json}");
    Ok(())
}

/// A condition that ends in a string literal, and one that holds an array
/// repeat expression, are read too.
#[test]
fn a_condition_ending_in_a_literal_or_holding_an_array_is_paired() -> Result<(), String> {
    for (old, new, old_condition, new_condition) in [
        (
            "if total.to_string() == \"100\" { total - 10 } else { total }",
            "if total.to_string() == \"200\" { total - 25 } else { total }",
            "total.to_string() == \"100\"",
            "total.to_string() == \"200\"",
        ),
        (
            "if [total; 2] == [100; 2] { total - 10 } else { total }",
            "if [total; 2] == [200; 2] { total - 25 } else { total }",
            "[total; 2] == [100; 2]",
            "[total; 2] == [200; 2]",
        ),
    ] {
        let (_, witness) = predicate_witness(old, new)?;
        assert_eq!(witness.before.as_deref(), Some(old), "{old}");
        assert_eq!(witness.after.as_deref(), Some(new_condition), "{old}");
        assert_eq!(
            witness.before_condition.as_deref(),
            Some(old_condition),
            "{old}"
        );
    }
    Ok(())
}

/// The guard is unchanged and the comparison in the branch body changed
/// with the rest of the line. The probe is that comparison, so the old
/// guard must not be reported as its old side.
#[test]
fn a_branch_body_comparison_is_not_paired_with_the_unchanged_guard() -> Result<(), String> {
    let old = "if total > 5 { (total > 100) as u32 } else { 0 }";
    let findings = TempRepo::create(
        &source("if total > 5 { (total >= 200) as u32 } else { 1 }"),
        &diff(old, "if total > 5 { (total >= 200) as u32 } else { 1 }"),
    )?
    .findings()?;
    let witnesses: Vec<DiagnosticWitness> = findings
        .iter()
        .filter(|finding| finding.probe.family == ProbeFamily::Predicate)
        .filter_map(DiagnosticWitness::from_finding)
        .collect();
    assert!(
        witnesses
            .iter()
            .any(|witness| witness.before.as_deref() == Some(old)),
        "premise: a predicate gap whose before is the whole old line, got {findings:?}"
    );
    for witness in &witnesses {
        assert_eq!(witness.before_condition, None, "{witness:?}");
    }
    Ok(())
}

/// The new line gained a comparison in the branch body while the guard
/// stayed (#15420 review). The old line holds no other comparison, so only
/// the new line shows that the changed predicate is not the head's.
#[test]
fn a_comparison_the_new_body_gained_is_not_paired_with_the_unchanged_guard() -> Result<(), String> {
    let old = "if total > 5 { total - 10 } else { 0 }";
    let new = "if total > 5 { (total > 100) as u32 } else { 1 }";
    let findings = TempRepo::create(&source(new), &diff(old, new))?.findings()?;
    let predicates: Vec<&Finding> = findings
        .iter()
        .filter(|finding| finding.probe.family == ProbeFamily::Predicate)
        .collect();
    assert!(
        predicates.iter().any(
            |finding| finding.probe.after.as_deref() == Some("total > 100")
                && finding.probe.before.as_deref() == Some(old)
        ),
        "premise: the gained comparison is a predicate probe over the whole old line, got {findings:?}"
    );
    for finding in predicates {
        assert_eq!(finding.probe.before_condition, None, "{finding:?}");
        if let Some(witness) = DiagnosticWitness::from_finding(finding) {
            assert_eq!(witness.before_condition, None, "{witness:?}");
        }
    }
    Ok(())
}

/// Only the branch bodies changed: an unchanged condition is not reported
/// as its own old side.
#[test]
fn an_unchanged_condition_is_not_paired_with_itself() -> Result<(), String> {
    let old = "if total > 5 { total - 10 } else { total }";
    let new = "if total > 5 { total } else { total - 10 }";
    let findings = TempRepo::create(&source(new), &diff(old, new))?.findings()?;
    assert!(!findings.is_empty(), "premise: the arm swap is a finding");
    for finding in &findings {
        assert_eq!(finding.probe.before_condition, None, "{finding:?}");
    }
    Ok(())
}

/// A head nested in a call argument: which head changed is not established.
#[test]
fn a_nested_head_on_the_line_is_not_paired() -> Result<(), String> {
    let old = "if total >= 100 { u32::from(if total > 500 { 1u8 } else { 2 }) } else { 0 }";
    let new = "if total > 200 { u32::from(if total > 500 { 1u8 } else { 3 }) } else { 0 }";
    let findings = TempRepo::create(&source(new), &diff(old, new))?.findings()?;
    assert!(
        findings
            .iter()
            .any(|finding| finding.probe.family == ProbeFamily::Predicate),
        "premise: a predicate finding, got {findings:?}"
    );
    for finding in &findings {
        assert_eq!(finding.probe.before_condition, None, "{finding:?}");
    }
    Ok(())
}
