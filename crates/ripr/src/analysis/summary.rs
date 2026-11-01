use crate::domain::{Finding, Summary};

pub(crate) fn summarize_findings(changed_rust_files: usize, findings: &[Finding]) -> Summary {
    let mut summary = Summary {
        changed_rust_files,
        probes: findings.len(),
        findings: findings.len(),
        ..Summary::default()
    };

    for finding in findings {
        summary.increment_exposure_class(&finding.class);
    }
    summary.conflicting_expressions = conflicting_expression_count(findings.iter());

    summary
}

/// Expressions (file, line, text) whose findings carry more than one class
/// (#11554). Takes an iterator so a caller can exclude suppressed findings.
pub(crate) fn conflicting_expression_count<'a>(
    findings: impl Iterator<Item = &'a Finding>,
) -> usize {
    let mut classes_by_expression: std::collections::BTreeMap<
        _,
        Vec<&crate::domain::ExposureClass>,
    > = std::collections::BTreeMap::new();
    for finding in findings {
        let classes = classes_by_expression
            .entry(finding.expression_key())
            .or_default();
        if !classes.contains(&&finding.class) {
            classes.push(&finding.class);
        }
    }
    classes_by_expression
        .values()
        .filter(|classes| classes.len() > 1)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::ExposureClass;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, Probe, ProbeFamily, ProbeId, RevealEvidence,
        RiprEvidence, SourceLocation, StageEvidence, StageState,
    };

    fn stage() -> StageEvidence {
        StageEvidence::new(StageState::Unknown, Confidence::Unknown, "summary test")
    }

    fn finding(class: ExposureClass) -> Finding {
        Finding {
            analysis_scope: crate::domain::AnalysisScope::Diff,
            confidence_basis: Vec::new(),
            id: format!("finding-{}", class.as_str()),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId(format!("probe-{}", class.as_str())),
                location: SourceLocation::new("src/lib.rs", 1, 1),
                owner: None,
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: None,
                expression: "value > threshold".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class,
            ripr: RiprEvidence {
                reach: stage(),
                infect: stage(),
                propagate: stage(),
                reveal: RevealEvidence {
                    observe: stage(),
                    discriminate: stage(),
                },
            },
            confidence: 0.0,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests_matched_total: None,
            related_tests: Vec::new(),
            recommended_next_step: None,
            language: None,
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: crate::domain::SourceCurrentness::CandidateCurrent,
        }
    }

    #[test]
    fn summarize_findings_sets_changed_files_probes_and_findings() {
        // Test with empty findings - verifies basic accounting works.
        let findings: Vec<Finding> = vec![];
        let summary = summarize_findings(5, &findings);

        assert_eq!(summary.changed_rust_files, 5);
        assert_eq!(summary.probes, 0);
        assert_eq!(summary.findings, 0);
    }

    #[test]
    fn summarize_findings_counts_every_exposure_class() {
        let findings = [
            finding(ExposureClass::Exposed),
            finding(ExposureClass::WeaklyExposed),
            finding(ExposureClass::ReachableUnrevealed),
            finding(ExposureClass::NoStaticPath),
            finding(ExposureClass::InfectionUnknown),
            finding(ExposureClass::PropagationUnknown),
            finding(ExposureClass::StaticUnknown),
        ];

        let summary = summarize_findings(3, &findings);

        assert_eq!(summary.changed_rust_files, 3);
        assert_eq!(summary.probes, 7);
        assert_eq!(summary.findings, 7);
        assert_eq!(summary.exposed, 1);
        assert_eq!(summary.weakly_exposed, 1);
        assert_eq!(summary.reachable_unrevealed, 1);
        assert_eq!(summary.no_static_path, 1);
        assert_eq!(summary.infection_unknown, 1);
        assert_eq!(summary.propagation_unknown, 1);
        assert_eq!(summary.static_unknown, 1);
    }
    #[test]
    fn summarize_findings_accumulates_repeated_classes() {
        let findings = [
            finding(ExposureClass::Exposed),
            finding(ExposureClass::Exposed),
            finding(ExposureClass::NoStaticPath),
            finding(ExposureClass::NoStaticPath),
            finding(ExposureClass::NoStaticPath),
        ];

        let summary = summarize_findings(1, &findings);

        assert_eq!(summary.changed_rust_files, 1);
        assert_eq!(summary.probes, 5);
        assert_eq!(summary.findings, 5);
        assert_eq!(summary.exposed, 2);
        assert_eq!(summary.no_static_path, 3);
        assert_eq!(summary.weakly_exposed, 0);
        assert_eq!(summary.reachable_unrevealed, 0);
        assert_eq!(summary.infection_unknown, 0);
        assert_eq!(summary.propagation_unknown, 0);
        assert_eq!(summary.static_unknown, 0);
    }

    // #11554: raw counts stay per probe; the conflicting-expression count says
    // how many expressions got more than one class from different probes.
    #[test]
    fn conflicting_expressions_count_expressions_with_more_than_one_class() {
        let same_class = [
            finding(ExposureClass::WeaklyExposed),
            finding(ExposureClass::WeaklyExposed),
        ];
        assert_eq!(
            summarize_findings(1, &same_class).conflicting_expressions,
            0
        );

        let mixed = [
            finding(ExposureClass::InfectionUnknown),
            finding(ExposureClass::WeaklyExposed),
            finding(ExposureClass::WeaklyExposed),
        ];
        let summary = summarize_findings(1, &mixed);
        assert_eq!(summary.conflicting_expressions, 1);
        assert_eq!(summary.findings, 3);
        assert_eq!(summary.weakly_exposed, 2);
        assert_eq!(summary.infection_unknown, 1);
    }

    // The count is per expression, not a flag: two conflicting expressions
    // count 2, and a third single-class expression adds nothing.
    #[test]
    fn conflicting_expressions_are_counted_per_expression() {
        let at = |class: ExposureClass, expression: &str| {
            let mut found = finding(class);
            found.probe.expression = expression.to_string();
            found
        };
        let findings = [
            at(ExposureClass::InfectionUnknown, "a > b"),
            at(ExposureClass::WeaklyExposed, "a > b"),
            at(ExposureClass::NoStaticPath, "c < d"),
            at(ExposureClass::Exposed, "c < d"),
            at(ExposureClass::Exposed, "e == f"),
        ];
        assert_eq!(summarize_findings(1, &findings).conflicting_expressions, 2);
        // A policy that suppresses one side of each conflict leaves none.
        let unsuppressed = [&findings[1], &findings[3], &findings[4]];
        assert_eq!(conflicting_expression_count(unsuppressed.into_iter()), 0);
    }
}
