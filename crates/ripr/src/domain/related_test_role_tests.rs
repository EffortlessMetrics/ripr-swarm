//! Controls for RIPR-SPEC-0224 / #5563: a related row's decision role is its
//! explicit `role`, never its explanatory `miss`.

use super::{
    ActivationEvidence, Confidence, DeltaKind, DiagnosticWitness, ExposureClass, Finding,
    OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest, RelatedTestMiss,
    RelatedTestRole, RelationReason, RevealEvidence, RiprEvidence, SourceLocation, StageEvidence,
    StageState,
};
use std::path::PathBuf;

fn row(name: &str, reason: RelationReason) -> RelatedTest {
    RelatedTest {
        name: name.to_string(),
        file: PathBuf::from("tests/effects.rs"),
        line: 4,
        oracle: Some(format!("assert_eq!({name}(), 1)")),
        oracle_kind: OracleKind::ExactValue,
        oracle_strength: OracleStrength::Strong,
        relation_reason: Some(reason),
        relation_confidence: None,
        miss: None,
        miss_support: None,
        role: RelatedTestRole::Contributing,
    }
}

fn examined_only(name: &str) -> RelatedTest {
    RelatedTest {
        oracle_kind: OracleKind::Unknown,
        oracle_strength: OracleStrength::None,
        miss: Some(RelatedTestMiss::AssertionNotObserving),
        role: RelatedTestRole::ExaminedOnly,
        ..row(name, RelationReason::SameTestFile)
    }
}

fn finding(related: Vec<RelatedTest>) -> Finding {
    Finding {
        analysis_scope: crate::domain::AnalysisScope::Diff,
        id: "probe:src_lib_rs:1:predicate".to_string(),
        canonical_gap: None,
        probe: Probe {
            id: ProbeId("probe:src_lib_rs:1:predicate".to_string()),
            family: ProbeFamily::Predicate,
            location: SourceLocation::new("src/lib.rs", 1, 1),
            owner: None,
            delta: DeltaKind::Control,
            before: None,
            after: None,
            expression: "expr".to_string(),
            expected_sinks: Vec::new(),
            required_oracles: Vec::new(),
        },
        class: ExposureClass::WeaklyExposed,
        ripr: RiprEvidence {
            reach: StageEvidence::new(StageState::Yes, Confidence::Medium, "reached"),
            infect: StageEvidence::new(StageState::Weak, Confidence::Low, "infected"),
            propagate: StageEvidence::new(StageState::No, Confidence::Medium, "not propagated"),
            reveal: RevealEvidence {
                observe: StageEvidence::new(StageState::Weak, Confidence::Low, "observed"),
                discriminate: StageEvidence::new(
                    StageState::No,
                    Confidence::Medium,
                    "no discriminator",
                ),
            },
        },
        confidence: 0.5,
        evidence: Vec::new(),
        missing: Vec::new(),
        flow_sinks: Vec::new(),
        activation: ActivationEvidence::default(),
        stop_reasons: Vec::new(),
        related_tests_matched_total: None,
        related_tests: related,
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

fn eligible_names(finding: &Finding) -> Vec<String> {
    finding
        .oracle_related_tests()
        .map(|test| test.name.clone())
        .collect()
}

const ALL_MISSES: [Option<RelatedTestMiss>; 9] = [
    None,
    Some(RelatedTestMiss::NoCallPath),
    Some(RelatedTestMiss::NoAssertion),
    Some(RelatedTestMiss::AssertionNotObserving),
    Some(RelatedTestMiss::AssertionNotCredited),
    Some(RelatedTestMiss::WeakAssertion),
    Some(RelatedTestMiss::MissingInput),
    Some(RelatedTestMiss::MissingExactAssertion),
    Some(RelatedTestMiss::ObservationUnconfirmed),
];

/// Everything a pre-existing decision path reads, as one comparable value.
fn decision_surface(finding: &Finding) -> Result<serde_json::Value, String> {
    let witness = DiagnosticWitness::from_finding(finding)
        .map(|witness| serde_json::to_value(&witness))
        .transpose()
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "class": finding.class.as_str(),
        "stages": [
            finding.ripr.reach.state.as_str(),
            finding.ripr.infect.state.as_str(),
            finding.ripr.propagate.state.as_str(),
            finding.ripr.reveal.observe.state.as_str(),
            finding.ripr.reveal.discriminate.state.as_str(),
        ],
        "eligible": eligible_names(finding),
        "eligible_count": finding.oracle_related_tests().count(),
        "witness": witness,
    }))
}

#[test]
fn miss_mutation_never_changes_a_contributing_rows_eligibility() {
    // Control 1: None -> assertion_not_observing -> any other value.
    let mut finding = finding(vec![row("existing", RelationReason::DirectOwnerCall)]);
    for miss in ALL_MISSES {
        finding.related_tests[0].miss = miss;
        assert_eq!(eligible_names(&finding), vec!["existing".to_string()]);
        assert!(finding.related_tests[0].is_contributing());
        assert!(!finding.related_tests[0].is_examined_only());
    }
}

#[test]
fn examined_only_rows_are_excluded_whatever_miss_they_carry() {
    // Control 2.
    let mut finding = finding(vec![
        row("existing", RelationReason::DirectOwnerCall),
        examined_only("examined"),
    ]);
    for miss in ALL_MISSES {
        finding.related_tests[1].miss = miss;
        assert_eq!(eligible_names(&finding), vec!["existing".to_string()]);
        assert!(finding.related_tests[1].is_examined_only());
    }
}

#[test]
fn a_pre_field_artifact_keeps_every_row_eligible() -> Result<(), String> {
    // Control 3: the old wire shape has no `role`, whatever `miss` says.
    let mut legacy = serde_json::to_value(row("legacy", RelationReason::SameTestFile))
        .map_err(|error| error.to_string())?;
    let object = legacy.as_object_mut().ok_or("row is an object")?;
    assert!(
        !object.contains_key("role"),
        "default role is not serialized"
    );
    object.insert("miss".to_string(), "assertion_not_observing".into());
    let parsed: RelatedTest = serde_json::from_value(legacy).map_err(|error| error.to_string())?;
    assert_eq!(parsed.role, RelatedTestRole::Contributing);
    assert_eq!(parsed.miss, Some(RelatedTestMiss::AssertionNotObserving));
    assert_eq!(
        eligible_names(&finding(vec![parsed])),
        vec!["legacy".to_string()]
    );
    Ok(())
}

#[test]
fn the_examined_only_role_round_trips_on_the_wire() -> Result<(), String> {
    let value =
        serde_json::to_value(examined_only("examined")).map_err(|error| error.to_string())?;
    assert_eq!(value["role"], RelatedTestRole::ExaminedOnly.as_str());
    let parsed: RelatedTest = serde_json::from_value(value).map_err(|error| error.to_string())?;
    assert!(parsed.is_examined_only());
    Ok(())
}

#[test]
fn annotating_an_existing_row_changes_no_decision() -> Result<(), String> {
    // Controls 5 and 6: a Python-style sibling-sink row and a TypeScript-style
    // wrong-receiver row, annotated with `assertion_not_observing` and other
    // reasons, leave class, stages, eligibility and the witness unchanged.
    for (label, reason) in [
        (
            "python sibling sink",
            RelationReason::AssertionTargetAffinity,
        ),
        ("typescript wrong receiver", RelationReason::SameTestFile),
    ] {
        let mut finding = finding(vec![row("existing", reason), examined_only("examined")]);
        let baseline = decision_surface(&finding)?;
        for miss in ALL_MISSES {
            finding.related_tests[0].miss = miss;
            assert_eq!(decision_surface(&finding)?, baseline, "{label}: {miss:?}");
        }
    }
    Ok(())
}

/// Control 7: a source-text guard. Eligibility helpers and the lines that
/// consume them must not name the explanatory miss.
#[test]
fn decision_helpers_never_branch_on_the_miss() -> Result<(), String> {
    let probe = include_str!("probe.rs");
    for name in [
        "pub fn is_contributing(&self) -> bool {\n        self.role",
        "pub fn is_examined_only(&self) -> bool {",
        "pub fn oracle_related_tests(&self)",
    ] {
        let start = probe
            .find(name)
            .ok_or_else(|| format!("missing helper {name}"))?;
        let body = &probe[start..];
        let end = body.find("\n    }\n").ok_or("unterminated helper")?;
        let body = &body[..end];
        for needle in [".miss", "RelatedTestMiss", "miss:"] {
            assert!(!body.contains(needle), "helper `{name}` reads {needle}");
        }
    }
    let consumers = [
        (
            "domain/diagnostic_witness.rs",
            include_str!("diagnostic_witness.rs"),
        ),
        (
            "analysis/classify/decision.rs",
            include_str!("../analysis/classify/decision.rs"),
        ),
        (
            "analysis/classify/reveal.rs",
            include_str!("../analysis/classify/reveal.rs"),
        ),
        (
            "analysis/classifier/finding.rs",
            include_str!("../analysis/classifier/finding.rs"),
        ),
        (
            "analysis/language/rust/oracles.rs",
            include_str!("../analysis/language/rust/oracles.rs"),
        ),
        (
            "analysis/language/rust/probes.rs",
            include_str!("../analysis/language/rust/probes.rs"),
        ),
        (
            "analysis/language/rust/mod.rs",
            include_str!("../analysis/language/rust/mod.rs"),
        ),
        (
            "analysis/language/rust/lexical_test_grip.rs",
            include_str!("../analysis/language/rust/lexical_test_grip.rs"),
        ),
        (
            "output/human/triage.rs",
            include_str!("../output/human/triage.rs"),
        ),
    ];
    for (path, source) in consumers {
        assert!(
            !source.contains("is_unmatched"),
            "{path} uses the retired is_unmatched"
        );
        // Test modules legitimately build rows; scan production text only.
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for (index, line) in production.lines().enumerate() {
            let eligibility = line.contains("is_examined_only()")
                || line.contains("is_contributing()")
                || line.contains("oracle_related_tests()");
            let reads_miss = line.contains("RelatedTestMiss") || line.contains(".miss");
            assert!(
                !(eligibility && reads_miss),
                "{path}:{} mixes row eligibility with the miss: {line}",
                index + 1
            );
        }
    }
    Ok(())
}
