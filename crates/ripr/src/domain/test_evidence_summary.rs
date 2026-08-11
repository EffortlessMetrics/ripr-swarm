//! Portable test-evidence summary projected from the analyzer's internal
//! evidence types. Designed for consumption by cargo-allow's
//! requirement-relevance checks (#1658/#1679), the LSP's
//! `FixInstructionSummary` (#1752), and the shared witness migration (#3160).
//!
//! This is a RIPR-internal projection — the portable V1 JSON schema
//! (`TestGripSummaryV1`) waits on cargo-allow #2191 to freeze the
//! cross-tool contract. See #1783.

/// A per-related-test evidence summary entry.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TestEvidenceEntry {
    /// The test function name.
    pub test_name: String,
    /// The test file path (forward-slash normalized).
    pub file: String,
    /// The test source line.
    pub line: usize,
    /// The oracle kind (exact_value, broad_error, etc.).
    pub oracle_kind: String,
    /// The oracle strength (strong, weak, etc.).
    pub oracle_strength: String,
    /// The relation reason (direct_owner_call, token_coincidence, etc.).
    pub relation_reason: String,
    /// True when a producer-owned test target identity exists.
    pub has_test_target: bool,
}

/// A summary of test-grip evidence for one behavior item, projected from the
/// analyzer's internal types. This is derived, not authoritative — the source
/// `Finding` or `TestGripEvidence` remains the source of truth.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TestEvidenceSummary {
    /// The canonical gap/seam identifier when available.
    pub seam_id: String,
    /// The related-test evidence entries, strongest first.
    pub related_tests: Vec<TestEvidenceEntry>,
    /// Count of distinct missing discriminators.
    pub missing_discriminator_count: usize,
    /// The strongest oracle strength among related tests, or "none".
    pub strongest_oracle: String,
    /// A semantic fingerprint that changes when assertion strength or the
    /// missing-discriminator set changes, but is stable under formatting,
    /// root spelling, test naming, and input ordering changes.
    pub fingerprint: String,
}

impl TestEvidenceSummary {
    /// Build the shared summary from producer-owned entries and discriminator
    /// identities.
    ///
    /// Both diff and repo adapters must use this constructor so ordering,
    /// strongest-oracle selection, missing-fact deduplication, and fingerprint
    /// semantics cannot drift between analysis paths.
    pub(crate) fn from_parts(
        seam_id: impl Into<String>,
        mut entries: Vec<TestEvidenceEntry>,
        missing_discriminators: &[String],
    ) -> Self {
        sort_entries(&mut entries);
        let mut missing = missing_discriminators
            .iter()
            .map(|value| normalize_text(value))
            .collect::<Vec<_>>();
        missing.sort();
        missing.dedup();

        let base = Self::compute_fingerprint(&entries);
        let fingerprint = if missing.is_empty() {
            base
        } else {
            format!("{base}|missing:{}", missing.join(";"))
        };

        Self {
            seam_id: seam_id.into(),
            missing_discriminator_count: missing.len(),
            strongest_oracle: Self::strongest_oracle_from(&entries),
            fingerprint,
            related_tests: entries,
        }
    }

    /// Compute the related-test semantic fingerprint. The fingerprint captures
    /// the sorted set of `(oracle_kind, oracle_strength, relation_reason)`
    /// tuples. It changes when a test's assertion narrows or broadens, but is
    /// stable when test names, file paths, roots, or ordering change.
    pub fn compute_fingerprint(entries: &[TestEvidenceEntry]) -> String {
        let mut profile: Vec<(String, String, String)> = entries
            .iter()
            .map(|entry| {
                (
                    entry.oracle_kind.clone(),
                    entry.oracle_strength.clone(),
                    entry.relation_reason.clone(),
                )
            })
            .collect();
        profile.sort();
        let joined = profile
            .iter()
            .map(|(kind, strength, reason)| format!("{kind}:{strength}:{reason}"))
            .collect::<Vec<_>>()
            .join(";");
        format!("fp:{joined}")
    }

    /// Derive the strongest oracle strength from a list of entries.
    pub fn strongest_oracle_from(entries: &[TestEvidenceEntry]) -> String {
        entries
            .iter()
            .map(|entry| oracle_strength_rank(&entry.oracle_strength))
            .max()
            .map(oracle_strength_for_rank)
            .unwrap_or("none")
            .to_string()
    }
}

fn sort_entries(entries: &mut [TestEvidenceEntry]) {
    entries.sort_by(|left, right| {
        oracle_strength_rank(&right.oracle_strength)
            .cmp(&oracle_strength_rank(&left.oracle_strength))
            .then_with(|| left.relation_reason.cmp(&right.relation_reason))
            .then_with(|| left.file.cmp(&right.file))
            .then_with(|| left.test_name.cmp(&right.test_name))
            .then_with(|| left.line.cmp(&right.line))
    });
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

fn oracle_strength_for_rank(rank: u8) -> &'static str {
    match rank {
        5 => "strong",
        4 => "medium",
        3 => "weak",
        2 => "smoke",
        1 => "none",
        _ => "unknown",
    }
}

fn normalize_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: &str, strength: &str, reason: &str) -> TestEvidenceEntry {
        TestEvidenceEntry {
            test_name: "test_foo".to_string(),
            file: "src/lib.rs".to_string(),
            line: 10,
            oracle_kind: kind.to_string(),
            oracle_strength: strength.to_string(),
            relation_reason: reason.to_string(),
            has_test_target: true,
        }
    }

    #[test]
    fn fingerprint_stable_under_reordering() {
        let a = vec![
            entry("exact_value", "strong", "direct_owner_call"),
            entry("broad_error", "weak", "same_test_file"),
        ];
        let b = vec![
            entry("broad_error", "weak", "same_test_file"),
            entry("exact_value", "strong", "direct_owner_call"),
        ];
        assert_eq!(
            TestEvidenceSummary::compute_fingerprint(&a),
            TestEvidenceSummary::compute_fingerprint(&b),
        );
    }

    #[test]
    fn fingerprint_changes_when_assertion_narrows() {
        let broad = vec![entry("broad_error", "weak", "direct_owner_call")];
        let exact = vec![entry("exact_value", "strong", "direct_owner_call")];
        assert_ne!(
            TestEvidenceSummary::compute_fingerprint(&broad),
            TestEvidenceSummary::compute_fingerprint(&exact),
        );
    }

    #[test]
    fn fingerprint_stable_under_name_change() {
        let a = vec![entry("exact_value", "strong", "direct_owner_call")];
        let mut b = entry("exact_value", "strong", "direct_owner_call");
        b.test_name = "different_name".to_string();
        assert_eq!(
            TestEvidenceSummary::compute_fingerprint(&a),
            TestEvidenceSummary::compute_fingerprint(&[b]),
        );
    }

    #[test]
    fn strongest_oracle_picks_highest_rank() {
        let entries = vec![
            entry("broad_error", "weak", "direct_owner_call"),
            entry("exact_value", "strong", "direct_owner_call"),
            entry("smoke_only", "smoke", "same_test_file"),
        ];
        assert_eq!(
            TestEvidenceSummary::strongest_oracle_from(&entries),
            "strong"
        );
    }

    #[test]
    fn strongest_oracle_none_when_empty() {
        assert_eq!(TestEvidenceSummary::strongest_oracle_from(&[]), "none");
    }

    #[test]
    fn empty_entries_produce_empty_fingerprint() {
        assert_eq!(TestEvidenceSummary::compute_fingerprint(&[]), "fp:");
    }

    #[test]
    fn from_parts_orders_entries_and_tracks_missing_identity() {
        let weak = entry("broad_error", "weak", "direct_owner_call");
        let strong = entry("exact_value", "strong", "direct_owner_call");
        let summary = TestEvidenceSummary::from_parts(
            "gap:boundary",
            vec![weak, strong],
            &[
                " amount   == threshold ".to_string(),
                "amount == threshold".to_string(),
            ],
        );
        assert_eq!(summary.related_tests[0].oracle_strength, "strong");
        assert_eq!(summary.strongest_oracle, "strong");
        assert_eq!(summary.missing_discriminator_count, 1);
        assert!(summary.fingerprint.ends_with("missing:amount == threshold"));
    }

    #[test]
    fn resolving_missing_discriminator_changes_fingerprint() {
        let entries = vec![entry("exact_value", "strong", "direct_owner_call")];
        let before = TestEvidenceSummary::from_parts(
            "gap:boundary",
            entries.clone(),
            &["amount == threshold".to_string()],
        );
        let after = TestEvidenceSummary::from_parts("gap:boundary", entries, &[]);
        assert_ne!(before.fingerprint, after.fingerprint);
    }
}
