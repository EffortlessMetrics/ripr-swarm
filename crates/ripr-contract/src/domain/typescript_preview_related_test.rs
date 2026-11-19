//! Family-aware TypeScript/JavaScript preview related-test selection (#5505).
//!
//! One row identity for the check headline, preview card, packet projection,
//! JSON oracle summary, verify-command inference, and miss explanation.

use super::{
    Finding, LanguageId, OracleKind, OracleStrength, ProbeFamily, RelatedTest,
    typescript_oracle_seam::ts_oracle_kind_matches_seam,
};

/// The one TypeScript/JavaScript preview related-test row shared by every
/// single-row projection surface (#11228 tie-break, #5505 family evidence).
///
/// Producer `typescript_oracle_evidence_ref` may name a different family-
/// relevant row than raw strength ranking, but it must not displace a related
/// test that already matches the missing-check family at equal strength
/// (RIPR-SPEC-0224 / #5505). The producer currently emits the last strongest
/// family-matching assertion, which is often a later same-file row that does
/// not observe the missing boundary better than the first listed match.
pub fn typescript_script_preview_related_test(finding: &Finding) -> Option<&RelatedTest> {
    if !matches!(
        finding.language,
        Some(LanguageId::TypeScript | LanguageId::JavaScript)
    ) {
        return None;
    }
    let tests = &finding.related_tests;
    if tests.is_empty() {
        return None;
    }
    let family = &finding.probe.family;
    let family_ranked = strongest_family_relevant_related_test(tests, family);
    if let Some(row) = related_test_for_typescript_oracle_evidence_ref(finding)
        && oracle_ref_row_matches_missing_check_family(finding, row, family_ranked)
    {
        return Some(row);
    }
    family_ranked.or_else(|| tests.first())
}

/// Strongest related-test row whose collapsed oracle can observe `family`.
/// Equal rank keeps the first listed row (#11228).
fn strongest_family_relevant_related_test<'a>(
    tests: &'a [RelatedTest],
    family: &ProbeFamily,
) -> Option<&'a RelatedTest> {
    let mut best: Option<&RelatedTest> = None;
    for test in tests {
        if !typescript_row_carries_family_oracle(test, family) {
            continue;
        }
        if best.is_none_or(|current| test.oracle_strength.rank() > current.oracle_strength.rank()) {
            best = Some(test);
        }
    }
    best
}

fn related_test_for_typescript_oracle_evidence_ref(finding: &Finding) -> Option<&RelatedTest> {
    let reference = finding.evidence.iter().find_map(|line| {
        line.strip_prefix("typescript_oracle_evidence_ref: ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })?;
    let (file, line_text) = reference.rsplit_once(':')?;
    let evidence_line = line_text.parse::<usize>().ok()?;
    let file_norm = file.replace('\\', "/");
    let mut best: Option<&RelatedTest> = None;
    for test in finding.related_tests.iter() {
        if test.file.to_string_lossy().replace('\\', "/") != file_norm || test.line > evidence_line
        {
            continue;
        }
        if best.is_none_or(|current| test.line > current.line) {
            best = Some(test);
        }
    }
    best
}

/// Oracle-ref may override prior ranking only when the named row matches the
/// missing-check family and does not merely reorder equal-strength family
/// matches onto a later same-file test.
fn oracle_ref_row_matches_missing_check_family(
    finding: &Finding,
    row: &RelatedTest,
    family_ranked: Option<&RelatedTest>,
) -> bool {
    if !typescript_row_carries_family_oracle(row, &finding.probe.family) {
        return false;
    }
    match family_ranked {
        None => true,
        Some(best) if std::ptr::eq(best, row) => true,
        Some(best)
            if row.oracle_strength.rank() < best.oracle_strength.rank()
                && oracle_ref_observes_missing_check(finding) =>
        {
            // Producer named a weaker family-relevant row that actually
            // supplies the missing-check / gap evidence.
            true
        }
        Some(_) => false,
    }
}

fn oracle_ref_observes_missing_check(finding: &Finding) -> bool {
    let Some(missing) = finding
        .activation
        .missing_discriminators
        .first()
        .map(|fact| fact.value.as_str())
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    let Some(observed) = finding.evidence.iter().find_map(|line| {
        line.strip_prefix("typescript_oracle_observed: ")
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }) else {
        return false;
    };
    observed_call_matches_missing_check(observed, missing)
}

/// Cheap evidence-only check: the producer-observed call is a plausible
/// observer of the missing equality/boundary, not a far-from-boundary input.
fn observed_call_matches_missing_check(observed: &str, missing: &str) -> bool {
    let Some((_, args_text)) = observed.split_once('(') else {
        return observed.contains(missing);
    };
    let Some(args_text) = args_text.rsplit_once(')').map(|(args, _)| args) else {
        return observed.contains(missing);
    };
    let args: Vec<&str> = args_text
        .split(',')
        .map(str::trim)
        .filter(|arg| !arg.is_empty())
        .collect();
    if missing.contains("==") {
        let parts: Vec<&str> = missing.split("==").map(str::trim).collect();
        if parts.len() == 2 {
            if parts[0] == parts[1] {
                return true;
            }
            if args.len() >= 2 && args[0] == args[1] {
                return true;
            }
            if args.iter().any(|arg| *arg == parts[0] || *arg == parts[1]) {
                // A named constant or literal from the missing check appears
                // in the observed call (`discountedTotal(DISCOUNT_THRESHOLD)`).
                return true;
            }
        }
    }
    args.iter().any(|arg| missing.contains(arg)) || observed.contains(missing)
}

fn typescript_row_carries_family_oracle(test: &RelatedTest, family: &ProbeFamily) -> bool {
    if test.oracle_strength == OracleStrength::None {
        return false;
    }
    if test.oracle_strength == OracleStrength::Unknown && test.oracle_kind == OracleKind::Unknown {
        return false;
    }
    ts_oracle_kind_matches_seam(&test.oracle_kind, family)
}

#[cfg(test)]
mod observed_call_matches_missing_check_tests {
    use super::observed_call_matches_missing_check;

    #[test]
    fn far_from_boundary_input_does_not_match_equality_missing_check() {
        assert!(!observed_call_matches_missing_check(
            "applyDiscount(10000, 100)",
            "amount == threshold"
        ));
        assert!(!observed_call_matches_missing_check(
            "discountedTotal(20000)",
            "amount == DISCOUNT_THRESHOLD"
        ));
    }

    #[test]
    fn equal_literal_args_match_equality_missing_check() {
        assert!(observed_call_matches_missing_check(
            "applyDiscount(100, 100)",
            "amount == threshold"
        ));
    }

    #[test]
    fn named_constant_in_call_matches_missing_check() {
        assert!(observed_call_matches_missing_check(
            "discountedTotal(DISCOUNT_THRESHOLD)",
            "amount == DISCOUNT_THRESHOLD"
        ));
    }
}

#[cfg(test)]
mod preview_row_selection_tests {
    use super::{observed_call_matches_missing_check, typescript_script_preview_related_test};
    use crate::domain::{
        ActivationEvidence, AnalysisScope, Confidence, DeltaKind, DiscriminatorShape,
        ExposureClass, Finding, LanguageId, MissingDiscriminatorFact, OracleKind, OracleStrength,
        Probe, ProbeFamily, ProbeId, RelatedTest, RelatedTestRole, RevealEvidence, RiprEvidence,
        SourceCurrentness, SourceLocation, StageEvidence, StageState,
    };
    use std::path::PathBuf;

    const TEST_FILE: &str = "tests/discount.test.ts";
    const REF_LINE_20: &str = "typescript_oracle_evidence_ref: tests/discount.test.ts:20";
    const OBSERVED_AT_BOUNDARY: &str = "typescript_oracle_observed: applyDiscount(100, 100)";
    const OBSERVED_FAR_AWAY: &str = "typescript_oracle_observed: applyDiscount(10000, 100)";

    fn row(name: &str, line: usize, kind: OracleKind, strength: OracleStrength) -> RelatedTest {
        row_in(TEST_FILE, name, line, kind, strength)
    }

    fn row_in(
        file: &str,
        name: &str,
        line: usize,
        kind: OracleKind,
        strength: OracleStrength,
    ) -> RelatedTest {
        RelatedTest {
            name: name.to_string(),
            file: PathBuf::from(file),
            line,
            oracle: None,
            oracle_kind: kind,
            oracle_strength: strength,
            relation_reason: None,
            relation_confidence: None,
            miss: None,
            role: RelatedTestRole::Contributing,
            miss_support: None,
        }
    }

    fn exact(name: &str, line: usize, strength: OracleStrength) -> RelatedTest {
        row(name, line, OracleKind::ExactValue, strength)
    }

    fn stage(state: StageState, summary: &str) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, summary)
    }

    /// A TypeScript predicate finding that misses the `amount == threshold`
    /// boundary, with the given related-test rows and evidence lines.
    fn finding(related_tests: Vec<RelatedTest>, evidence: &[&str]) -> Finding {
        Finding {
            analysis_scope: AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: "finding:discount".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:discount".to_string()),
                location: SourceLocation::new("src/discount.ts", 4, 5),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "amount >= threshold".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::WeaklyExposed,
            ripr: RiprEvidence {
                reach: stage(StageState::Yes, "a related test calls applyDiscount"),
                infect: stage(StageState::Weak, "no boundary input"),
                propagate: stage(StageState::Yes, "value is returned"),
                reveal: RevealEvidence {
                    observe: stage(StageState::Yes, "assertion observes the result"),
                    discriminate: stage(StageState::Weak, "no boundary check"),
                },
            },
            confidence: 0.5,
            evidence: evidence.iter().map(|line| (*line).to_string()).collect(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence {
                observed_values: Vec::new(),
                missing_discriminators: vec![MissingDiscriminatorFact {
                    value: "amount == threshold".to_string(),
                    reason: "boundary".to_string(),
                    flow_sink: None,
                    drafted_boundary: None,
                    shape: DiscriminatorShape::Boundary,
                }],
            },
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests,
            recommended_next_step: None,
            language: Some(LanguageId::TypeScript),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    fn selected(finding: &Finding) -> Option<&str> {
        typescript_script_preview_related_test(finding).map(|test| test.name.as_str())
    }

    #[test]
    fn only_typescript_and_javascript_findings_with_related_tests_select_a_row() {
        let rows = || vec![exact("checks_total", 5, OracleStrength::Strong)];
        let mut subject = finding(rows(), &[]);
        assert_eq!(selected(&subject), Some("checks_total"));
        subject.language = Some(LanguageId::JavaScript);
        assert_eq!(selected(&subject), Some("checks_total"));
        subject.language = Some(LanguageId::Rust);
        assert_eq!(selected(&subject), None);
        subject.language = None;
        assert_eq!(selected(&subject), None);

        assert_eq!(selected(&finding(Vec::new(), &[])), None);
    }

    #[test]
    fn the_strongest_family_relevant_row_wins_and_ties_keep_the_first_listed() {
        let subject = finding(
            vec![
                row("throws", 3, OracleKind::BroadError, OracleStrength::Strong),
                exact("weak", 5, OracleStrength::Weak),
                exact("first_medium", 7, OracleStrength::Medium),
                exact("second_medium", 9, OracleStrength::Medium),
            ],
            &[],
        );
        assert_eq!(selected(&subject), Some("first_medium"));
    }

    #[test]
    fn rows_without_a_usable_oracle_are_not_family_relevant() {
        // No oracle at all, and an oracle ripr knows nothing about, are skipped;
        // an unknown kind with a graded strength still counts.
        let subject = finding(
            vec![
                exact("no_oracle", 3, OracleStrength::None),
                row("unknown", 5, OracleKind::Unknown, OracleStrength::Unknown),
                row("graded", 7, OracleKind::Unknown, OracleStrength::Weak),
            ],
            &[],
        );
        assert_eq!(selected(&subject), Some("graded"));

        // A known kind of unknown strength counts; an unknown kind of unknown
        // strength does not, so the first listed row is the fallback.
        let throws = || row("throws", 3, OracleKind::BroadError, OracleStrength::Strong);
        let known_kind = finding(
            vec![throws(), exact("ungraded", 5, OracleStrength::Unknown)],
            &[],
        );
        assert_eq!(selected(&known_kind), Some("ungraded"));
        let nothing_relevant = finding(
            vec![
                throws(),
                row("unknown", 5, OracleKind::Unknown, OracleStrength::Unknown),
            ],
            &[],
        );
        assert_eq!(selected(&nothing_relevant), Some("throws"));
    }

    #[test]
    fn a_named_weaker_row_that_observes_the_missing_check_wins() {
        let rows = || {
            vec![
                exact("strong", 5, OracleStrength::Strong),
                exact("at_boundary", 15, OracleStrength::Weak),
            ]
        };
        assert_eq!(
            selected(&finding(rows(), &[REF_LINE_20, OBSERVED_AT_BOUNDARY])),
            Some("at_boundary")
        );
        // The reference may spell the path with backslashes.
        assert_eq!(
            selected(&finding(
                rows(),
                &[
                    "typescript_oracle_evidence_ref: tests\\discount.test.ts:20",
                    OBSERVED_AT_BOUNDARY
                ]
            )),
            Some("at_boundary")
        );

        // Without an observation of the missing check the strongest row stays.
        for evidence in [
            &[REF_LINE_20][..],
            &[REF_LINE_20, OBSERVED_FAR_AWAY],
            &[REF_LINE_20, "typescript_oracle_observed: "],
            &[OBSERVED_AT_BOUNDARY],
            &["typescript_oracle_evidence_ref: ", OBSERVED_AT_BOUNDARY],
            &[
                "typescript_oracle_evidence_ref: tests/discount.test.ts:soon",
                OBSERVED_AT_BOUNDARY,
            ],
        ] {
            assert_eq!(
                selected(&finding(rows(), evidence)),
                Some("strong"),
                "{evidence:?}"
            );
        }

        // Nor when the finding names no missing check to observe.
        let mut no_missing_check = finding(rows(), &[REF_LINE_20, OBSERVED_AT_BOUNDARY]);
        no_missing_check.activation.missing_discriminators[0]
            .value
            .clear();
        assert_eq!(selected(&no_missing_check), Some("strong"));
    }

    #[test]
    fn a_named_row_never_displaces_an_equal_or_family_relevant_stronger_match() {
        // Equal strength: the first listed match stays, even when the later
        // row observes the missing check.
        let equal = finding(
            vec![
                exact("first", 5, OracleStrength::Strong),
                exact("later", 15, OracleStrength::Strong),
            ],
            &[REF_LINE_20, OBSERVED_AT_BOUNDARY],
        );
        assert_eq!(selected(&equal), Some("first"));

        // A named row whose oracle cannot observe the family is ignored.
        let wrong_family = finding(
            vec![
                exact("strong", 5, OracleStrength::Strong),
                row("throws", 15, OracleKind::BroadError, OracleStrength::Weak),
            ],
            &[REF_LINE_20, OBSERVED_AT_BOUNDARY],
        );
        assert_eq!(selected(&wrong_family), Some("strong"));
    }

    #[test]
    fn the_reference_names_the_last_row_at_or_before_its_line_in_its_file() {
        let weak = OracleStrength::Weak;
        let subject = finding(
            vec![
                row_in(
                    "tests/other.test.ts",
                    "other_file",
                    18,
                    OracleKind::ExactValue,
                    weak.clone(),
                ),
                exact("early", 5, OracleStrength::Strong),
                exact("on_the_line", 20, weak.clone()),
                exact("same_line_twin", 20, weak.clone()),
                exact("after", 25, weak),
            ],
            &[REF_LINE_20, OBSERVED_AT_BOUNDARY],
        );
        assert_eq!(selected(&subject), Some("on_the_line"));
    }

    #[test]
    fn an_observed_argument_named_in_the_missing_check_matches_it() {
        assert!(observed_call_matches_missing_check(
            "applyDiscount(amount)",
            "amount > 3"
        ));
        assert!(!observed_call_matches_missing_check(
            "applyDiscount(total)",
            "amount > 3"
        ));
        // Without a complete call, only the missing check's own text matches.
        assert!(observed_call_matches_missing_check(
            "amount > 3 holds",
            "amount > 3"
        ));
        assert!(observed_call_matches_missing_check(
            "check(amount > 3",
            "amount > 3"
        ));
        assert!(!observed_call_matches_missing_check(
            "check(amount",
            "amount > 3"
        ));
        assert!(observed_call_matches_missing_check("same(1, 2)", "x == x"));
    }
}
