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
                has_test_target: false,
            })
            .collect::<Vec<_>>();
        let missing = self
            .activation
            .missing_discriminators
            .iter()
            .map(|fact| {
                format!(
                    "{}:{}",
                    normalize_text(&fact.value),
                    normalize_text(&fact.reason)
                )
            })
            .collect::<Vec<_>>();
        summary_from_parts(seam_id, entries, &missing)
    }
}

fn summary_from_parts(
    seam_id: String,
    mut entries: Vec<TestEvidenceEntry>,
    missing_discriminators: &[String],
) -> TestEvidenceSummary {
    entries.sort_by(|left, right| {
        oracle_strength_rank(&right.oracle_strength)
            .cmp(&oracle_strength_rank(&left.oracle_strength))
            .then_with(|| left.relation_reason.cmp(&right.relation_reason))
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.test_name.cmp(&right.test_name))
            .then_with(|| left.line.cmp(&right.line))
    });
    let base = TestEvidenceSummary::compute_fingerprint(&entries);
    let mut missing = missing_discriminators
        .iter()
        .map(|value| normalize_text(value))
        .collect::<Vec<_>>();
    missing.sort();
    missing.dedup();
    let fingerprint = if missing.is_empty() {
        base
    } else {
        format!("{base}|missing:{}", missing.join(";"))
    };
    TestEvidenceSummary {
        seam_id,
        missing_discriminator_count: missing.len(),
        strongest_oracle: TestEvidenceSummary::strongest_oracle_from(&entries),
        fingerprint,
        related_tests: entries,
    }
}

fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    normalized
        .strip_prefix("./")
        .unwrap_or(&normalized)
        .to_string()
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn oracle_strength_rank(strength: &str) -> u8 {
    match strength {
        "strong" => 5,
        "medium" => 4,
        "weak" => 3,
        "smoke" => 2,
        "none" => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, path: &str, strength: &str) -> TestEvidenceEntry {
        TestEvidenceEntry {
            test_name: name.to_string(),
            file: normalize_path(path),
            line: 10,
            oracle_kind: "exact_value".to_string(),
            oracle_strength: strength.to_string(),
            relation_reason: "direct_owner_call".to_string(),
            has_test_target: false,
        }
    }

    #[test]
    fn summary_orders_strongest_evidence_and_preserves_target_absence() {
        let summary = summary_from_parts(
            "gap:boundary".to_string(),
            vec![
                entry("weak_test", "tests\\pricing.rs", "weak"),
                entry("strong_test", "./tests/pricing.rs", "strong"),
            ],
            &[],
        );
        assert_eq!(summary.related_tests[0].test_name, "strong_test");
        assert_eq!(summary.related_tests[0].file, "tests/pricing.rs");
        assert_eq!(summary.strongest_oracle, "strong");
        assert!(
            summary
                .related_tests
                .iter()
                .all(|test| !test.has_test_target)
        );
    }

    #[test]
    fn missing_discriminator_changes_the_semantic_fingerprint() {
        let entries = vec![entry("boundary_test", "tests/pricing.rs", "strong")];
        let before = summary_from_parts(
            "gap:boundary".to_string(),
            entries.clone(),
            &["amount   == threshold".to_string()],
        );
        let after = summary_from_parts("gap:boundary".to_string(), entries, &[]);
        assert_eq!(before.missing_discriminator_count, 1);
        assert_eq!(after.missing_discriminator_count, 0);
        assert_ne!(before.fingerprint, after.fingerprint);
        assert!(before.fingerprint.contains("amount == threshold"));
    }

    #[test]
    fn missing_discriminator_order_and_whitespace_do_not_change_identity() {
        let entries = vec![entry("boundary_test", "tests/pricing.rs", "strong")];
        let left = summary_from_parts(
            "gap:boundary".to_string(),
            entries.clone(),
            &[
                " field   status ".to_string(),
                "amount == threshold".to_string(),
            ],
        );
        let right = summary_from_parts(
            "gap:boundary".to_string(),
            entries,
            &[
                "amount == threshold".to_string(),
                "field status".to_string(),
            ],
        );
        assert_eq!(left.fingerprint, right.fingerprint);
    }
}
