//! Transient match-arm owner admission from the actual producer index.
//! No index clone, filesystem discovery, or serialized authority.
use super::consumed_source::{ConsumedRustSources, ContextCommitment};
use super::facts::{FunctionImplContext, RustIndex, source_digest};
use crate::domain::{Finding, ProbeFamily};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RustMatchArmScope {
    epoch: ContextCommitment,
    input_identity: Option<String>,
    receipts: BTreeMap<String, MatchArmOwnerReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MatchArmOwnerReceipt {
    finding_digest: String,
    callers: Vec<(PathBuf, usize, String)>,
    owner: String,
    sources: BTreeMap<PathBuf, String>,
    admitted: bool,
}

impl RustMatchArmScope {
    pub(crate) fn from_index(
        root: &Path,
        index: &RustIndex,
        complete: bool,
        findings: &[Finding],
        consumed: &ConsumedRustSources,
    ) -> Option<Self> {
        if !complete {
            return None;
        }
        let epoch = ConsumedRustSources::establish_agreement(consumed, consumed).ok()?;
        let syntax = super::classify::OwnerPinSyntax::default();
        let mut receipts = BTreeMap::new();
        for finding in findings
            .iter()
            .filter(|f| f.probe.family == ProbeFamily::MatchArm)
        {
            let Some(id) = finding.probe.owner.as_ref() else {
                continue;
            };
            let functions = index.functions();
            let Some(owner) = functions.iter().find(|f| f.id == *id) else {
                continue;
            };
            let mut sources = BTreeMap::new();
            let mut callers = Vec::new();
            let owner_facts = index.files().get(&owner.file)?;
            sources.insert(
                owner.file.clone(),
                source_digest(owner_facts.source.as_bytes()),
            );
            let mut admitted =
                super::test_grip_evidence::match_arm_owner_identity_is_admitted(owner, index)
                    && !finding.related_tests.is_empty();
            for related in &finding.related_tests {
                let path = if related.file.is_absolute() {
                    related.file.strip_prefix(root).ok()?.to_path_buf()
                } else {
                    related.file.clone()
                };
                let tests = index.tests();
                let mut matching = tests
                    .iter()
                    .filter(|t| t.file == path && t.start_line == related.line);
                let (Some(test), None) = (matching.next(), matching.next()) else {
                    admitted = false;
                    continue;
                };
                callers.push((
                    test.file.clone(),
                    test.start_line,
                    source_digest(test.body.as_bytes()),
                ));
                let facts = index.files().get(&test.file)?;
                sources.insert(test.file.clone(), source_digest(facts.source.as_bytes()));
                admitted &= super::test_grip_evidence::match_arm_owner_call_is_admitted(
                    test, owner, index, &syntax,
                );
            }
            // Unknown owner placement cannot borrow an inherent/free admission.
            admitted &= !matches!(owner.impl_context, FunctionImplContext::Unknown);
            receipts.insert(
                finding.id.clone(),
                MatchArmOwnerReceipt {
                    finding_digest: finding_digest(finding)?,
                    callers,
                    owner: owner.id.0.clone(),
                    sources,
                    admitted,
                },
            );
        }
        Some(Self {
            epoch,
            input_identity: None,
            receipts,
        })
    }

    /// Seal only the bytes this producer actually consumed; another producer's
    /// carrier or a conflicting/unavailable observation never supplies authority.
    pub(crate) fn bind_input(
        mut self,
        input: Option<&str>,
        consumed: &ConsumedRustSources,
    ) -> Option<Self> {
        if input.is_none()
            || ConsumedRustSources::establish_agreement(consumed, consumed)
                .ok()?
                .ne(&self.epoch)
        {
            return None;
        }
        self.input_identity = input.map(str::to_owned);
        Some(self)
    }

    pub(crate) fn receipt(
        &self,
        finding: &Finding,
        input: Option<&str>,
    ) -> Option<&MatchArmOwnerReceipt> {
        if input.is_none() || self.input_identity.as_deref() != input {
            return None;
        }
        let receipt = self.receipts.get(&finding.id)?;
        (receipt.admitted && finding_digest(finding).as_ref() == Some(&receipt.finding_digest))
            .then_some(receipt)
    }
}

impl MatchArmOwnerReceipt {
    /// Compare only already-loaded bounded facts, without rereading sources or
    /// treating those facts as a certificate of workspace-wide uniqueness.
    pub(crate) fn matches_index(&self, index: &RustIndex) -> bool {
        self.admitted
            && index
                .functions()
                .iter()
                .any(|owner| owner.id.0 == self.owner)
            && self.callers.iter().all(|(file, line, digest)| {
                index.tests().iter().any(|test| {
                    test.file == *file
                        && test.start_line == *line
                        && source_digest(test.body.as_bytes()) == *digest
                })
            })
            && self.sources.iter().all(|(path, expected)| {
                index
                    .files()
                    .get(path)
                    .is_some_and(|facts| source_digest(facts.source.as_bytes()) == *expected)
            })
    }
}

fn finding_digest(finding: &Finding) -> Option<String> {
    serde_json::to_vec(&(&finding.probe, &finding.related_tests))
        .ok()
        .map(|bytes| source_digest(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::language::{LanguageAdapter, RustAdapter};
    use crate::analysis::{AnalysisMode, AnalysisOptions};
    use std::sync::atomic::{AtomicU64, Ordering};

    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn receipt_requires_complete_matching_sources_and_live_authority() -> Result<(), String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Fixture(std::env::temp_dir().join(format!(
            "ripr-owner-receipt-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir_all(fixture.0.join("src")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(fixture.0.join("tests")).map_err(|e| e.to_string())?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname=\"ar\"\nversion=\"0.1.0\"\nedition=\"2021\"\n[workspace]\n",
        )
        .map_err(|e| e.to_string())?;
        let owner = "pub fn rate(kind: &str) -> u32 {\n    match kind {\n        \"gold\" => 10,\n        \"silver\" => 7,\n        _ => 0,\n    }\n}\n";
        let test = "use ar::rate;\n#[test]\nfn runs() {\n    let r = rate(\"silver\");\n    assert!(r < 100);\n}\n";
        std::fs::write(fixture.0.join("src/lib.rs"), owner).map_err(|e| e.to_string())?;
        std::fs::write(fixture.0.join("tests/t.rs"), test).map_err(|e| e.to_string())?;
        let options = AnalysisOptions {
            root: fixture.0.clone(),
            base: None,
            diff_file: None,
            mode: AnalysisMode::Deep,
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
        };
        let diff = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,7 +1,7 @@\n pub fn rate(kind: &str) -> u32 {\n     match kind {\n         \"gold\" => 10,\n-        \"silver\" => 5,\n+        \"silver\" => 7,\n         _ => 0,\n     }\n }\n";
        let changed = crate::analysis::diff::parse_unified_diff(diff);
        let result = RustAdapter.analyze_diff(
            &options,
            &crate::config::OraclePolicy::default(),
            &changed,
        )?;
        let finding = result
            .findings
            .iter()
            .find(|f| f.probe.family == ProbeFamily::MatchArm && f.probe.location.line == 4)
            .ok_or("fixture must produce the changed match arm")?;
        let unbound = result
            .rust_match_arm_scope
            .clone()
            .ok_or("complete producer must publish scope")?;
        let scope = unbound
            .clone()
            .bind_input(Some("subject"), &result.rust_consumed_sources)
            .ok_or("same producer sources must seal")?;
        let receipt = scope
            .receipt(finding, Some("subject"))
            .ok_or("actual-owner positive must be admitted")?;
        let paths = [PathBuf::from("src/lib.rs"), PathBuf::from("tests/t.rs")];
        let index = super::super::facts::build_index(&fixture.0, &paths)?;
        assert!(receipt.matches_index(&index));
        assert!(scope.receipt(finding, None).is_none());
        assert!(scope.receipt(finding, Some("another-subject")).is_none());
        let mut other = finding.clone();
        other.probe.expression.push_str(" changed");
        assert!(scope.receipt(&other, Some("subject")).is_none());
        assert!(
            RustMatchArmScope::from_index(
                &fixture.0,
                &index,
                false,
                &result.findings,
                &result.rust_consumed_sources
            )
            .is_none()
        );
        let mut moved = result.rust_consumed_sources.clone();
        moved.record(Path::new("src/lib.rs"), Some(b"different source"));
        assert!(unbound.bind_input(Some("subject"), &moved).is_none());
        std::fs::write(
            fixture.0.join("tests/t.rs"),
            format!("{test}// changed source\n"),
        )
        .map_err(|e| e.to_string())?;
        let changed_index = super::super::facts::build_index(&fixture.0, &paths)?;
        assert!(!receipt.matches_index(&changed_index));

        use crate::analysis_outcome::{
            AnalysisIdentity, AnalysisOutcome, AnalysisOutcomeCounts, AnalysisOutcomeKind,
        };
        let mut outcome = AnalysisOutcome::new(
            AnalysisOutcomeKind::CompleteWithFindings,
            AnalysisIdentity::default(),
            AnalysisOutcomeCounts {
                changed_file_count: 1,
                changed_line_count: 1,
                candidate_line_count: 1,
                probe_count: 1,
                finding_count: 1,
            },
            Vec::new(),
        )?;
        assert!(outcome.rust_match_arm_scope.is_none());
        outcome.rust_match_arm_scope = Some(scope);
        let json = serde_json::to_value(&outcome).map_err(|e| e.to_string())?;
        assert!(json.get("rust_match_arm_scope").is_none());
        let restored: AnalysisOutcome = serde_json::from_value(json).map_err(|e| e.to_string())?;
        assert!(restored.rust_match_arm_scope.is_none());
        assert_eq!(restored.semantic_digest()?, outcome.semantic_digest()?);
        Ok(())
    }
}
