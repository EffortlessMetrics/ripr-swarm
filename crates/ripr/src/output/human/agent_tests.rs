//! Contract tests for `ripr check --format agent` (#11580).

use super::agent::{MAX_GAP_LINES, render};
use crate::app::{CheckDiffProvenance, CheckOutput, Mode};
use crate::config::RiprConfig;
use crate::domain::{
    ActivationEvidence, Confidence, DeltaKind, ExposureClass, Finding, MissingDiscriminatorFact,
    OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest, RelatedTestMiss,
    RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation, StageEvidence, StageState,
    StaticLimitKind, Summary,
};
use std::path::PathBuf;

fn finding(id: &str, class: ExposureClass, line: usize) -> Finding {
    Finding {
        analysis_scope: crate::domain::AnalysisScope::Diff,
        confidence_basis: Vec::new(),
        id: id.to_string(),
        canonical_gap: None,
        probe: Probe {
            id: ProbeId(id.to_string()),
            location: SourceLocation::new("src/lib.rs", line, 1),
            owner: None,
            family: ProbeFamily::Predicate,
            delta: DeltaKind::Control,
            before: None,
            after: None,
            expression: "pct >= 50".to_string(),
            expected_sinks: vec![],
            required_oracles: vec![],
        },
        class,
        ripr: RiprEvidence {
            reach: StageEvidence::new(StageState::Yes, Confidence::High, "reach"),
            infect: StageEvidence::new(StageState::Weak, Confidence::Medium, "infect"),
            propagate: StageEvidence::new(StageState::Yes, Confidence::High, "propagate"),
            reveal: RevealEvidence {
                observe: StageEvidence::new(StageState::Yes, Confidence::High, "observe"),
                discriminate: StageEvidence::new(
                    StageState::Weak,
                    Confidence::Medium,
                    "discriminate",
                ),
            },
        },
        confidence: 0.8,
        evidence: vec![],
        missing: vec![],
        flow_sinks: vec![],
        activation: ActivationEvidence::default(),
        stop_reasons: vec![],
        related_tests_matched_total: None,
        related_tests: vec![],
        recommended_next_step: None,
        language: None,
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

fn weak_gap(id: &str, line: usize) -> Finding {
    let mut gap = finding(id, ExposureClass::WeaklyExposed, line);
    gap.activation.missing_discriminators = vec![MissingDiscriminatorFact {
        value: "pct == 50".to_string(),
        reason: "the boundary is never an input".to_string(),
        flow_sink: None,
    }];
    gap.related_tests = vec![RelatedTest {
        name: "discount_runs".to_string(),
        file: PathBuf::from("tests/t.rs"),
        line: 3,
        oracle: Some("assert!(discount(10, 100) > 0);".to_string()),
        oracle_kind: OracleKind::RelationalCheck,
        oracle_strength: OracleStrength::Weak,
        relation_reason: None,
        relation_confidence: None,
        miss: Some(RelatedTestMiss::WeakAssertion),
        role: Default::default(),
        miss_support: None,
    }];
    gap
}

fn output(findings: Vec<Finding>) -> CheckOutput {
    CheckOutput {
        value_origins: Default::default(),
        harness_projections: Vec::new(),
        schema_version: "0.1".to_string(),
        tool: "ripr".to_string(),
        mode: Mode::Draft,
        root: PathBuf::from("repo"),
        base: Some("origin/main".to_string()),
        summary: Summary::default(),
        findings,
        preview_language_advisories: Vec::new(),
        language_runs: Vec::new(),
        no_scope_provided: false,
        unanalyzed_working_tree: false,
        untracked_working_tree_source_paths: Vec::new(),
        ignored_working_tree_source_paths: Vec::new(),
        unlinked_python_tests: None,
        suppression: None,
        analysis_outcome: None,
        partial_scope: None,
        analyzed_revisions: None,
    }
}

fn rendered(output: &CheckOutput) -> String {
    render(
        output,
        &RiprConfig::default(),
        None,
        CheckDiffProvenance::CommittedHistory,
    )
}

#[test]
fn top_gap_carries_location_missing_value_test_id_and_next_command() {
    let text = rendered(&output(vec![
        weak_gap("probe:a", 2),
        finding("probe:ok", ExposureClass::Exposed, 9),
    ]));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "#ripr agent static-advisory base=origin/main findings=2 gaps=1 exposed=1 weakly_exposed=1"
    );
    assert_eq!(
        lines[1],
        "weakly_exposed|src/lib.rs:2|pct >= 50|missing=pct == 50|test=t1|id=probe:a"
    );
    assert_eq!(
        lines[2],
        "t1=tests/t.rs:3 discount_runs relational_check `assert!(discount(10, 100) > 0);` miss=weak_assertion"
    );
    assert_eq!(lines[3], "next=ripr explain probe:a");
    assert_eq!(lines.len(), 4, "{text}");
}

#[test]
fn exposed_only_run_is_done_with_no_gap_lines_or_next() {
    let text = rendered(&output(vec![finding(
        "probe:ok",
        ExposureClass::Exposed,
        4,
    )]));
    assert!(text.starts_with("#ripr agent static-advisory"), "{text}");
    assert!(
        text.contains("\ndone=no weak or unobserved gap left"),
        "{text}"
    );
    assert!(!text.contains("|src/lib.rs"), "{text}");
    assert!(!text.contains("next="), "{text}");
    assert!(!text.contains("limits="), "{text}");
}

#[test]
fn static_limited_run_stops_and_names_the_limit_instead_of_a_gap() {
    let text = rendered(&output(vec![finding(
        "probe:np",
        ExposureClass::NoStaticPath,
        7,
    )]));
    assert!(
        text.contains("\nstop=ripr cannot tell from reading the code"),
        "{text}"
    );
    assert!(
        text.contains("nothing more is fixable from the test side"),
        "{text}"
    );
    assert!(text.contains("\nlimits=no_static_path:1\n"), "{text}");
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(!text.contains("|missing="), "{text}");
}

/// #11825: triage can select a static-limited finding (higher repair rank)
/// while an omitted weakly_exposed finding is still a test-side gap. The
/// stop projection must follow the remaining gap, not the selected limit.
#[test]
fn open_test_side_gap_suppresses_stop_when_selected_finding_is_limited() {
    let mut limited = finding("probe:limited", ExposureClass::StaticUnknown, 1);
    limited.recommended_next_step = Some("inspect the unresolved owner".to_string());
    let text = rendered(&output(vec![limited, weak_gap("probe:gap", 2)]));
    assert!(!text.contains("\nstop="), "{text}");
    assert!(text.contains(" gaps=1 "), "{text}");
    assert!(
        text.contains(
            "weakly_exposed|src/lib.rs:2|pct >= 50|missing=pct == 50|test=t1|id=probe:gap"
        ),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain probe:gap"),
        "{text}"
    );
    assert!(text.contains("\nlimits=static_unknown:1\n"), "{text}");
}

/// #11825 negative: a weakly_exposed finding that is itself static-limited
/// is not an open test-side gap, so `stop=` stays.
#[test]
fn static_limited_weakly_exposed_alone_keeps_stop() {
    let mut limited_weak = weak_gap("probe:limited-weak", 4);
    limited_weak.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
    let text = rendered(&output(vec![limited_weak]));
    assert!(text.contains("\nstop="), "{text}");
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(!text.contains("|id=probe:limited-weak"), "{text}");
    assert!(!text.contains("\nnext="), "{text}");
}

#[test]
fn empty_scope_and_no_scope_are_distinct_terminal_states() {
    let clean = rendered(&output(Vec::new()));
    assert!(
        clean.contains("\ndone=no changed behavior found in scope"),
        "{clean}"
    );
    let mut unscoped = output(Vec::new());
    unscoped.no_scope_provided = true;
    let stopped = rendered(&unscoped);
    assert!(
        stopped.contains("\nstop=nothing was analyzed (no scope)"),
        "{stopped}"
    );
    assert!(!stopped.contains("done="), "{stopped}");
}

#[test]
fn non_current_findings_are_never_listed_as_gaps() {
    let mut base_side = weak_gap("probe:old", 3);
    base_side.source_currentness = SourceCurrentness::BaseDeleted;
    let text = rendered(&output(vec![base_side]));
    assert!(!text.contains("|id=probe:old"), "{text}");
    assert!(
        text.contains("\nstop=every finding is base-side or unresolved"),
        "{text}"
    );
}

#[test]
fn gap_lines_are_bounded_and_the_rest_is_counted_explicitly() {
    let findings: Vec<Finding> = (0..7)
        .map(|index| weak_gap(&format!("probe:g{index}"), index + 1))
        .collect();
    let text = rendered(&output(findings));
    assert_eq!(
        text.matches("\nweakly_exposed|").count(),
        MAX_GAP_LINES,
        "{text}"
    );
    assert!(text.contains(" gaps=7 "), "{text}");
    assert!(text.contains("\n+2 more gaps not shown;"), "{text}");
    assert_eq!(
        text.matches("\nt1=").count(),
        1,
        "one shared test row: {text}"
    );
}

#[test]
fn rendering_is_deterministic_and_smaller_than_the_full_human_report() {
    let out = output(vec![weak_gap("probe:a", 2), weak_gap("probe:b", 5)]);
    let agent = rendered(&out);
    assert_eq!(agent, rendered(&out));
    let full = super::render_full_with_config(&out, &RiprConfig::default());
    assert!(
        agent.len() * 2 < full.len(),
        "agent {} vs human-full {}",
        agent.len(),
        full.len()
    );
}

#[test]
fn fields_cannot_forge_a_record_boundary() {
    let mut gap = weak_gap("probe:a", 2);
    gap.probe.expression = "a | b\nc".to_string();
    let text = rendered(&output(vec![gap]));
    let line = text.lines().nth(1).unwrap_or_default();
    assert_eq!(
        line.matches("|").count() - line.matches("\\|").count(),
        5,
        "{line}"
    );
    assert!(line.contains("a \\| b c"), "{line}");
}

#[test]
fn vocabulary_stays_static_and_advisory() {
    let text = rendered(&output(vec![weak_gap("probe:a", 2)]));
    // Built from fragments so the repository-wide prose gate does not flag
    // the list itself; the rendered report must contain none of these.
    let banned = [
        ["kil", "led"],
        ["surv", "ived"],
        ["unte", "sted"],
        ["pro", "ven"],
        ["adeq", "uate"],
    ];
    for parts in banned {
        let word = parts.concat();
        assert!(!text.contains(&word), "{word} in {text}");
    }
    assert!(text.contains("static-advisory"), "{text}");
}

#[test]
fn format_name_parses_and_stays_diff_scoped() {
    use crate::app::OutputFormat;
    assert_eq!(
        OutputFormat::parse_cli_name("agent"),
        Some(OutputFormat::Agent)
    );
    assert!(!OutputFormat::Agent.is_repo_scope());
    assert_eq!(OutputFormat::Agent.primary_cli_name(), "agent");
    // Negative: near-miss spellings and the repo packet format do not alias it.
    for other in ["agents", "agent-json", "agent-text", "Agent"] {
        assert_eq!(OutputFormat::parse_cli_name(other), None, "{other}");
    }
}
