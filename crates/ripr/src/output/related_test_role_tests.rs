//! Controls for RIPR-SPEC-0224 / #5563: a related row's decision role is its
//! explicit `role`, never its explanatory `miss`.

use crate::domain::{
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

/// Calls whose closure argument decides which rows a path keeps or counts.
const ELIGIBILITY_CALLS: [&str; 11] = [
    ".filter(",
    ".partition(",
    ".retain(",
    ".any(",
    ".all(",
    ".find(",
    ".position(",
    ".filter_map(",
    ".take_while(",
    ".skip_while(",
    ".is_some_and(",
];

/// Production text of a source file: everything before its first test module.
fn production_text(source: &str) -> &str {
    source.split("\n#[cfg(test)]").next().unwrap_or(source)
}

fn reads_miss(text: &str) -> bool {
    if text.contains("is_unmatched") || text.contains("RelatedTestMiss") {
        return true;
    }
    text.match_indices(".miss").any(|(at, _)| {
        !text[at + ".miss".len()..]
            .chars()
            .next()
            .is_some_and(|next| next.is_alphanumeric() || next == '_')
    })
}

/// Every closure argument (possibly spanning lines) of an eligibility call
/// that reads the explanatory miss, as `line: snippet`.
fn miss_reading_closures(source: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for call in ELIGIBILITY_CALLS {
        for (at, _) in source.match_indices(call) {
            let start = at + call.len();
            let mut depth = 1usize;
            let mut end = start;
            for (offset, ch) in source[start..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = start + offset;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let argument = &source[start..end];
            if reads_miss(argument) {
                let line = source[..at].matches('\n').count() + 1;
                let snippet = argument.split_whitespace().collect::<Vec<_>>().join(" ");
                hits.push(format!("{line}: {call}{snippet})"));
            }
        }
    }
    hits
}

/// Documented explanation-only readers and producers allowed to name the miss
/// inside such a closure: `(path suffix, snippet fragment)`.
const MISS_CLOSURE_ALLOWLIST: [(&str, &str); 3] = [
    // Tracked RIPR-SPEC-0224 gap: the exact-oracle next step still excludes
    // `missing_exact_assertion` rows after the role check; eligibility itself
    // is the role.
    (
        "analysis/classify/decision.rs",
        "test.miss != Some(RelatedTestMiss::MissingExactAssertion)",
    ),
    // Producer: the annotation pass selects not-yet-annotated rows to write
    // their miss; it never decides decision eligibility.
    (
        "analysis/classifier/finding.rs",
        ".filter(|test| test.miss.is_none() && test.oracle.is_some())",
    ),
    (
        "analysis/classifier/finding.rs",
        ".filter(|test| test.miss.is_none())",
    ),
];

fn production_sources(
    dir: &std::path::Path,
    out: &mut Vec<(String, String)>,
) -> Result<(), String> {
    for entry in std::fs::read_dir(dir).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "tests" {
                production_sources(&path, out)?;
            }
        } else if name.ends_with(".rs") && !name.contains("tests") {
            let text = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            out.push((
                path.display().to_string(),
                production_text(&text).to_string(),
            ));
        }
    }
    Ok(())
}

/// Control 7: the eligibility helpers never name the miss, and no production
/// closure that decides row eligibility anywhere in the crate reads it.
#[test]
fn decision_helpers_never_branch_on_the_miss() -> Result<(), String> {
    let probe = include_str!("../domain/probe.rs");
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
        assert!(
            !reads_miss(body) && !body.contains("miss:"),
            "helper `{name}` reads the miss"
        );
    }
    let mut sources = Vec::new();
    production_sources(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut sources,
    )?;
    assert!(
        sources.len() > 100,
        "scan found only {} files",
        sources.len()
    );
    let mut violations = Vec::new();
    for (path, source) in &sources {
        assert!(
            !source.contains("is_unmatched"),
            "{path} uses the retired is_unmatched"
        );
        for hit in miss_reading_closures(source) {
            let allowed = MISS_CLOSURE_ALLOWLIST
                .iter()
                .any(|(suffix, fragment)| path.ends_with(suffix) && hit.contains(fragment));
            if !allowed {
                violations.push(format!("{path}:{hit}"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "closures read the miss:\n{}",
        violations.join("\n")
    );
    Ok(())
}

#[test]
fn the_miss_closure_scan_catches_multi_line_and_helper_shapes() {
    let multi_line = "fn eligible(f: &Finding) -> usize {\n    f.related_tests\n        .iter()\n        .filter(|test| {\n            let reaches = test.line > 0;\n            reaches\n                && test.miss\n                    != Some(RelatedTestMiss::AssertionNotObserving)\n        })\n        .count()\n}\n";
    assert_eq!(miss_reading_closures(multi_line).len(), 1);
    let legacy = "rows.iter().any(|t| t.is_unmatched())";
    assert_eq!(miss_reading_closures(legacy).len(), 1);
    let clean = "rows.iter().filter(|t| t.is_contributing() && t.miss_support.is_none())";
    assert!(miss_reading_closures(clean).is_empty());
    let test_tail = "fn a() {}\n#[cfg(test)]\nmod tests { fn b(r: &[RelatedTest]) { r.iter().any(|t| t.miss.is_some()); } }";
    assert!(miss_reading_closures(production_text(test_tail)).is_empty());
}
