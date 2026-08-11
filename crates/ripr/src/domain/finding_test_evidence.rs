//! Producer-backed `TestEvidenceSummary` projection for diff findings.
//!
//! This is the first bounded convergence slice for #1658/#3160. It gives the
//! diff path a real portable summary while preserving the exact limitation
//! that a `Finding` does not carry repo-seam `TestTargetEvidence`.

use super::{Finding, TestEvidenceEntry, TestEvidenceSummary};

impl Finding {
    /// Project this finding's related-test and discriminator facts into the
    /// shared portable test-evidence summary.
    ///
    /// The canonical gap ID is preferred when available. Related-test paths
    /// are normalized for transport, and no test target is invented from a
    /// name/path/line tuple: `has_test_target` remains false until a producer
    /// with target authority supplies that identity.
    pub fn test_evidence_summary(&self) -> TestEvidenceSummary {
        let seam_id = self
            .canonical_gap
            .as_ref()
            .map(|gap| gap.id.clone())
            .unwrap_or_else(|| self.id.clone());
        let entries = self
            .related_tests
            .iter()
            .map(|test| TestEvidenceEntry {
                test_name: test.name.clone(),
                file: normalize_path(&test.file.to_string_lossy()),
                line: test.line,
                oracle_kind: test.oracle_kind.as_str().to_string(),
                oracle_strength: test.oracle_strength.as_str().to_string(),
                relation_reason: test
                    .relation_reason
                    .as_ref()
                    .map_or("unknown", |reason| reason.as_str())
                    .to_string(),
                // A diff Finding has no producer-owned TestTargetEvidence.
                // Preserve that absence instead of inferring authority from a
                // path/name/line tuple.
                has_test_target: false,
            })
            .collect::<Vec<_>>();
        let missing = self
            .activation
            .missing_discriminators
            .iter()
            .map(|fact| fact.value.clone())
            .collect::<Vec<_>>();

        TestEvidenceSummary::from_parts(seam_id, entries, &missing)
    }
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_path;

    #[test]
    fn related_test_paths_are_transport_normalized() {
        assert_eq!(normalize_path("tests\\pricing.rs"), "tests/pricing.rs");
        assert_eq!(normalize_path("./tests/pricing.rs"), "tests/pricing.rs");
    }
}
