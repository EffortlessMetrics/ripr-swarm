//! #7542: a weak direct oracle must not erase an unresolved macro witness.
use ripr::domain::{RelationReason, StageState, StaticLimitKind, StopReason};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, check_workspace, render_check};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "macro_rules! call_inner { ($x:expr) => { inner($x) }; }
pub fn outer(x: i64) -> i64 { call_inner!(x) }
fn inner(x: i64) -> i64 {
    3 * x
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn outer_triples() { assert_eq!(outer(4), 12); }
    #[test] fn inner_is_positive() { assert!(inner(2) > 0); }
}
";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -3,3 +3,3 @@
 fn inner(x: i64) -> i64 {
-    x * 3
+    3 * x
 }
";
struct Fixture(PathBuf);
impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-mixed-macro-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).map_err(|e| e.to_string())?;
        let owned = Self(root);
        std::fs::create_dir(owned.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(
            owned.0.join("Cargo.toml"),
            "[package]\nname='mixed-macro'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        std::fs::write(owned.0.join("src/lib.rs"), source).map_err(|e| e.to_string())?;
        std::fs::write(owned.0.join("change.diff"), DIFF).map_err(|e| e.to_string())?;
        Ok(owned)
    }
    fn check(&self, mode: Mode) -> Result<ripr::CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("change.diff")),
            mode,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn owner(output: &ripr::CheckOutput) -> Result<&Finding, String> {
    let candidates: Vec<_> = output
        .findings
        .iter()
        .filter(|f| f.probe.location.file.ends_with("src/lib.rs") && f.probe.location.line == 4)
        .collect();
    match candidates.as_slice() {
        [finding] => Ok(finding),
        _ => Err(format!(
            "expected one changed owner at line4, got {candidates:?}; all findings: {:?}",
            output.findings
        )),
    }
}

fn assert_ordinary_weak_gap(finding: &Finding) {
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.static_limit_kind, None);
    assert!(finding.canonical_gap.is_some(), "{finding:?}");
    assert_eq!(
        finding.recommended_next_step.as_deref(),
        Some(
            "Replace broad assertions with exact equality or a property that constrains the changed returned value."
        )
    );
}
#[test]
fn weak_direct_oracle_preserves_the_macro_witness_in_ready_and_draft() -> Result<(), String> {
    let fixture = Fixture::new(SOURCE)?;
    for mode in [Mode::Ready, Mode::Draft] {
        let output = fixture.check(mode)?;
        let finding = owner(&output)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed);
        assert_eq!(finding.related_tests.len(), 2);
        assert_eq!(finding.related_tests[0].name, "inner_is_positive");
        assert_eq!(finding.related_tests[1].name, "outer_triples");
        assert_eq!(
            finding.related_tests[1].relation_reason,
            Some(RelationReason::SameTestFile)
        );
        assert_eq!(finding.ripr.reach.state, StageState::Yes);
        assert_ne!(finding.ripr.reveal.discriminate.state, StageState::Yes);
        assert_eq!(
            finding.static_limit_kind,
            Some(StaticLimitKind::RustMacroReachUnresolved)
        );
        assert!(
            finding
                .stop_reasons
                .contains(&StopReason::MacroReachUnresolved)
        );
        assert!(
            finding
                .related_tests
                .iter()
                .any(|t| t.name.ends_with("inner_is_positive")
                    && t.relation_reason == Some(RelationReason::DirectOwnerCall))
        );
        assert!(
            finding
                .related_tests
                .iter()
                .filter(|t| t.name.ends_with("outer_triples"))
                .all(|t| t.relation_reason != Some(RelationReason::DirectOwnerCall))
        );
        assert_eq!(
            finding.missing,
            vec![
                StaticLimitKind::RustMacroReachUnresolved
                    .describe()
                    .to_string()
            ]
        );
        assert!(finding.activation.missing_discriminators.is_empty());
        assert!(finding.canonical_gap.is_none());
        let next = finding.recommended_next_step.as_deref().unwrap_or("");
        assert!(
            next.contains("outer_triples") && next.contains("does not establish a missing test"),
            "{next}"
        );
        let json = render_check(&output, &OutputFormat::Json)?;
        assert!(json.contains("rust_macro_reach_unresolved"));
        let human = render_check(&output, &OutputFormat::HumanFull)?;
        assert!(
            human.contains("outer_triples") && human.contains("call_inner!"),
            "{human}"
        );
        assert!(!human.contains("Write a test for it"), "{human}");
    }
    Ok(())
}
#[test]
fn strong_direct_oracle_remains_credited_beside_the_macro() -> Result<(), String> {
    let fixture =
        Fixture::new(&SOURCE.replace("assert!(inner(2) > 0)", "assert_eq!(inner(4), 12)"))?;
    let output = fixture.check(Mode::Ready)?;
    let finding = owner(&output)?;
    assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert_eq!(finding.static_limit_kind, None);
    Ok(())
}
#[test]
fn weak_direct_without_a_test_macro_route_keeps_the_gap() -> Result<(), String> {
    let fixture = Fixture::new(&SOURCE.replace(
        "#[test] fn outer_triples() { assert_eq!(outer(4), 12); }",
        "fn outer_triples() { assert_eq!(outer(4), 12); }",
    ))?;
    let output = fixture.check(Mode::Ready)?;
    let finding = owner(&output)?;
    assert_ordinary_weak_gap(finding);
    Ok(())
}
#[test]
fn unrelated_macro_with_weak_direct_keeps_the_gap() -> Result<(), String> {
    let fixture = Fixture::new(&SOURCE.replace("=> { inner($x) }", "=> { $x * 3 }"))?;
    let output = fixture.check(Mode::Ready)?;
    let finding = owner(&output)?;
    assert_ordinary_weak_gap(finding);
    Ok(())
}
#[test]
fn lexical_transitive_route_with_weak_direct_keeps_the_gap() -> Result<(), String> {
    let fixture = Fixture::new(&SOURCE.replace("call_inner!(x)", "inner(x)"))?;
    let output = fixture.check(Mode::Ready)?;
    let finding = owner(&output)?;
    assert_ordinary_weak_gap(finding);
    Ok(())
}
