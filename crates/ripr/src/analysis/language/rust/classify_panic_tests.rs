//! #11116: a panic while classifying one probe is contained to that probe.
//! The run keeps every other file's result, the failed probe becomes a
//! fail-closed `static_unknown` stand-in (never `exposed`, never actionable),
//! and the analysis outcome names the file.
use super::{RustAdapter, injected_classify_panic};
use crate::analysis::classifier::CLASSIFICATION_INCOMPLETE_EVIDENCE;
use crate::analysis::language::{LanguageAdapter, LanguageDiffResult};
use crate::analysis::{AnalysisMode, AnalysisOptions, diff};
use crate::analysis_outcome::AnalysisLimitationKind;
use crate::config::OraclePolicy;
use crate::domain::{ExposureClass, Finding, StageState, StopReason};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const LIB: &str = "pub mod other;\npub fn a(x: i64) -> bool {\n    x >= 1\n}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn a_boundary() {\n        assert_eq!(a(1), true);\n        assert_eq!(a(0), false);\n    }\n}\n";
const OTHER: &str = "pub fn b(x: i64) -> bool {\n    x >= 2\n}\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn b_boundary() {\n        assert_eq!(b(2), true);\n        assert_eq!(b(1), false);\n    }\n}\n";
const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2,3 +2,3 @@\n pub fn a(x: i64) -> bool {\n-    x > 1\n+    x >= 1\n }\ndiff --git a/src/other.rs b/src/other.rs\n--- a/src/other.rs\n+++ b/src/other.rs\n@@ -1,3 +1,3 @@\n pub fn b(x: i64) -> bool {\n-    x > 2\n+    x >= 2\n }\n";

struct Fixture(PathBuf);

impl Fixture {
    fn new(label: &str) -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ripr-classify-panic-{label}-{stamp}"));
        fs::create_dir_all(root.join("src")).map_err(|e| e.to_string())?;
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\nedition='2024'\n",
        )
        .map_err(|e| e.to_string())?;
        fs::write(root.join("src/lib.rs"), LIB).map_err(|e| e.to_string())?;
        fs::write(root.join("src/other.rs"), OTHER).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    fn run(&self) -> Result<LanguageDiffResult, String> {
        RustAdapter.analyze_diff(
            &AnalysisOptions {
                root: self.0.clone(),
                base: None,
                diff_file: None,
                mode: AnalysisMode::Ready,
                resolved_subject_identity: None,
                open_rust_index_paths: Default::default(),
                include_unchanged_tests: true,
                resolve_tsconfig_paths: false,
                perl_facts_path: None,
                perl_producer_failure: None,
                git_timeout: None,
                git_candidate: None,
                production_like_targets: Default::default(),
                test_harnesses: Vec::new(),
            },
            &OraclePolicy::default(),
            &diff::parse_unified_diff(DIFF),
        )
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn is_stand_in(finding: &Finding) -> bool {
    finding
        .evidence
        .iter()
        .any(|line| line == CLASSIFICATION_INCOMPLETE_EVIDENCE)
}

fn in_file<'a>(findings: &'a [Finding], file: &str) -> Vec<&'a Finding> {
    findings
        .iter()
        .filter(|finding| finding.probe.location.file.ends_with(file))
        .collect()
}

#[test]
fn a_panicking_probe_does_not_abort_the_run_and_fails_closed() -> Result<(), String> {
    let fixture = Fixture::new("contained")?;

    // Control: unarmed, both files classify for real and nothing is a
    // stand-in, so the assertions below observe the containment and not a
    // fixture that never reached classification.
    let control = fixture.run()?;
    let control_lib = in_file(&control.findings, "src/lib.rs");
    let control_other = in_file(&control.findings, "src/other.rs");
    assert!(
        !control_lib.is_empty() && !control_other.is_empty(),
        "both changed files must reach classification: {:?}",
        control.findings
    );
    assert!(control.findings.iter().all(|finding| !is_stand_in(finding)));
    assert!(
        control
            .limitations
            .iter()
            .all(|limitation| limitation.kind != AnalysisLimitationKind::ProducerFailure),
        "control run carries no producer failure: {:?}",
        control.limitations
    );

    let _armed = injected_classify_panic::arm(&fixture.0, &fixture.0.join("src/lib.rs"));
    let result = fixture.run()?;

    // The other file's results are intact and unchanged.
    let other = in_file(&result.findings, "src/other.rs");
    assert_eq!(other.len(), control_other.len());
    for (armed, unarmed) in other.iter().zip(&control_other) {
        assert!(!is_stand_in(armed));
        assert_eq!(armed.class, unarmed.class);
    }

    // The panicking file yields one explicit, non-actionable stand-in per probe.
    let lib = in_file(&result.findings, "src/lib.rs");
    assert_eq!(lib.len(), control_lib.len());
    for finding in lib {
        assert!(is_stand_in(finding), "{finding:?}");
        assert_eq!(finding.class, ExposureClass::StaticUnknown);
        assert!(finding.related_tests.is_empty());
        assert!(finding.canonical_gap.is_none());
        assert!(finding.activation.missing_discriminators.is_empty());
        assert_eq!(finding.stop_reasons, vec![StopReason::StaticProbeUnknown]);
        assert_eq!(finding.ripr.reach.state, StageState::Unknown);
        assert_eq!(finding.ripr.reveal.discriminate.state, StageState::Unknown);
        assert!(finding.recommended_next_step.is_some());
    }

    // The analysis outcome names the file, so the run cannot read complete.
    let limitation = result
        .limitations
        .iter()
        .find(|limitation| {
            limitation.kind == AnalysisLimitationKind::ProducerFailure
                && limitation.path.as_deref() == Some("src/lib.rs")
        })
        .ok_or_else(|| {
            format!(
                "no producer_failure for src/lib.rs: {:?}",
                result.limitations
            )
        })?;
    assert!(
        limitation
            .bounded_detail
            .as_deref()
            .unwrap_or_default()
            .contains("classification_incomplete")
    );
    assert!(
        !result
            .limitations
            .iter()
            .any(|limitation| limitation.path.as_deref() == Some("src/other.rs")),
        "the healthy file carries no limitation"
    );
    Ok(())
}

#[test]
fn every_panicking_probe_is_contained_on_a_single_thread_pool() -> Result<(), String> {
    let fixture = Fixture::new("single-thread")?;
    let _armed = injected_classify_panic::arm(&fixture.0, &fixture.0.join("src/other.rs"));
    super::CLASSIFY_THREADS_FOR_TEST.with(|pinned| pinned.set(Some(1)));
    let result = fixture.run();
    super::CLASSIFY_THREADS_FOR_TEST.with(|pinned| pinned.set(None));
    let result = result?;
    let other = in_file(&result.findings, "src/other.rs");
    assert!(!other.is_empty() && other.iter().all(|finding| is_stand_in(finding)));
    let lib = in_file(&result.findings, "src/lib.rs");
    assert!(!lib.is_empty() && lib.iter().all(|finding| !is_stand_in(finding)));
    Ok(())
}

#[test]
fn a_panicking_macro_prime_fails_all_affected_probes_closed() -> Result<(), String> {
    let fixture = Fixture::new("prime")?;
    let control = fixture.run()?;
    assert!(!control.findings.is_empty());
    assert!(control.findings.iter().all(|finding| !is_stand_in(finding)));
    let armed = injected_classify_panic::arm(&fixture.0, &fixture.0);
    let result = fixture.run()?;
    drop(armed);
    assert_eq!(result.findings.len(), control.findings.len());
    for file in ["src/lib.rs", "src/other.rs"] {
        let findings = in_file(&result.findings, file);
        assert!(!findings.is_empty());
        assert!(findings.iter().all(|finding| is_stand_in(finding)
            && finding.class == ExposureClass::StaticUnknown
            && finding.related_tests.is_empty()
            && finding.canonical_gap.is_none()));
        assert!(result.limitations.iter().any(|limit| limit.kind
            == AnalysisLimitationKind::ProducerFailure
            && limit.path.as_deref() == Some(file)));
    }
    let recovered = fixture.run()?;
    assert_eq!(recovered.findings.len(), control.findings.len());
    assert!(
        recovered
            .findings
            .iter()
            .all(|finding| !is_stand_in(finding))
    );
    Ok(())
}
