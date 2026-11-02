//! Contract tests for `ripr check --format agent` (#11580).

use super::agent::{MAX_GAP_LINES, render, render_finding};
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
        kani_source_inventory: None,
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
/// is not an open test-side gap, so `stop=` stays, unless it also carries a
/// `weak_assertion` row (see `reach_limit_with_weak_assertion_row_does_not_stop`).
#[test]
fn static_limited_weakly_exposed_alone_keeps_stop() {
    let mut limited_weak = weak_gap("probe:limited-weak", 4);
    limited_weak.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
    limited_weak.related_tests[0].oracle_strength = OracleStrength::Strong;
    let text = rendered(&output(vec![limited_weak]));
    assert!(text.contains("\nstop="), "{text}");
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(!text.contains("|id=probe:limited-weak"), "{text}");
    assert!(!text.contains("\nnext="), "{text}");
}

/// A weakly exposed finding behind the reach-only limit, shaped like the
/// humantime fortnight commit: the one related test asserts only `is_ok()`.
fn reach_limited_weak(id: &str, line: usize) -> Finding {
    let mut limited = weak_gap(id, line);
    limited.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
    limited
}

/// #11825: every finding is static-limited, so `gaps=0`, but a reach-only
/// limit does not make a smoke-only related test fixable-from-nowhere. The
/// report must not print `stop=`; it prints `open=` and the drill-in.
#[test]
fn reach_limit_with_weak_assertion_row_does_not_stop() {
    let text = rendered(&output(vec![
        reach_limited_weak("probe:fortnight", 264),
        finding("probe:enum", ExposureClass::StaticUnknown, 316),
    ]));
    assert!(!text.contains("stop="), "{text}");
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(
        text.contains("\nopen=a static limit leaves reach unconfirmed"),
        "{text}"
    );
    assert!(
        text.contains(
            "weakly_exposed|src/lib.rs:264|pct >= 50|missing=-|test=t1|id=probe:fortnight"
        ),
        "{text}"
    );
    assert!(text.contains("miss=weak_assertion"), "{text}");
    assert!(
        text.contains("\nlimits=static_unknown:1,weakly_exposed:1\n"),
        "{text}"
    );
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain probe:fortnight"),
        "{text}"
    );
}

/// #11825: more open findings than `MAX_GAP_LINES` print the cap and an
/// exact omitted count; `reachable_unrevealed` opens the same way.
#[test]
fn open_limited_findings_over_the_cap_report_the_omitted_count() {
    let total = MAX_GAP_LINES + 2;
    let mut findings = Vec::new();
    for index in 0..total {
        let mut open = reach_limited_weak(&format!("probe:open{index}"), 10 + index);
        if index == 0 {
            open.class = ExposureClass::ReachableUnrevealed;
        }
        findings.push(open);
    }
    let text = rendered(&output(findings));
    assert!(text.contains("\nopen="), "{text}");
    assert!(!text.contains("stop="), "{text}");
    assert!(
        text.matches("|test=t1|id=probe:open").count() <= MAX_GAP_LINES,
        "{text}"
    );
    assert!(
        text.contains("\n+2 more open findings not shown; `--format json` lists all\n"),
        "{text}"
    );
}

/// #11825 negative: the same reach limit with no weak-assertion row (the
/// fixed-tree shape: exact-value tests remain, only the limit is left) still
/// stops, and so does a weak row behind a different limit kind.
#[test]
fn reach_limit_without_weak_assertion_row_keeps_stop() {
    let mut fixed = reach_limited_weak("probe:fixed", 264);
    fixed.related_tests[0].miss = Some(RelatedTestMiss::ObservationUnconfirmed);
    fixed.related_tests[0].oracle_strength = OracleStrength::Strong;
    let text = rendered(&output(vec![fixed]));
    assert!(text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
    assert!(!text.contains("\nnext="), "{text}");

    let mut other_kind = weak_gap("probe:macro", 5);
    other_kind.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
    let text = rendered(&output(vec![other_kind]));
    assert!(text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
}

/// #12058 negative: an unrelated weak test that is related only by file or
/// module proximity does not make a reach-limited finding open; it stays
/// `stop=`. The same row bound to the owner (a direct call or an owner-named
/// test) still opens it.
#[test]
fn reach_limit_with_a_proximity_only_weak_row_keeps_stop() {
    use crate::domain::RelationReason;
    for reason in [
        RelationReason::SameTestFile,
        RelationReason::SameModule,
        RelationReason::WeakTokenSubstring,
    ] {
        let mut limited = reach_limited_weak("probe:unrelated", 264);
        limited.related_tests[0].relation_reason = Some(reason);
        let text = rendered(&output(vec![limited]));
        assert!(text.contains("\nstop="), "{reason:?}: {text}");
        assert!(!text.contains("open="), "{reason:?}: {text}");
        assert!(!text.contains("\nnext="), "{reason:?}: {text}");
    }
    for reason in [
        RelationReason::DirectOwnerCall,
        RelationReason::HelperOwnerCall,
        RelationReason::OwnerNamedTest,
    ] {
        let mut limited = reach_limited_weak("probe:bound", 264);
        limited.related_tests[0].relation_reason = Some(reason);
        let text = rendered(&output(vec![limited]));
        assert!(text.contains("\nopen="), "{reason:?}: {text}");
        assert!(!text.contains("stop="), "{reason:?}: {text}");
    }
}

/// #12058: a proximity-only weak row ahead of an owner-bound weak row must not
/// hide the bound one; the bound row opens the finding and is the one shown.
#[test]
fn reach_limit_prefers_a_later_owner_bound_row_over_a_proximity_only_one() {
    use crate::domain::RelationReason;
    let mut limited = reach_limited_weak("probe:mixed", 264);
    let mut proximity = limited.related_tests[0].clone();
    proximity.name = "unrelated_same_file".to_string();
    proximity.relation_reason = Some(RelationReason::SameTestFile);
    limited.related_tests[0].relation_reason = Some(RelationReason::DirectOwnerCall);
    // The proximity row comes first.
    limited.related_tests.insert(0, proximity);
    let text = rendered(&output(vec![limited]));
    assert!(text.contains("\nopen="), "{text}");
    assert!(!text.contains("stop="), "{text}");
    assert!(text.contains("discount_runs"), "{text}");
    assert!(!text.contains("unrelated_same_file"), "{text}");
}

/// #11825 negative: a non-current (base-side) limited weak finding is not
/// open work.
#[test]
fn base_side_reach_limited_weak_row_keeps_stop() {
    let mut old = reach_limited_weak("probe:old", 3);
    old.source_currentness = SourceCurrentness::BaseDeleted;
    let text = rendered(&output(vec![old]));
    assert!(!text.contains("open="), "{text}");
    assert!(text.contains("\nstop="), "{text}");
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

// ---- `ripr explain --format agent` (#12104) ----

fn explained(finding: &Finding) -> String {
    render_finding(
        finding,
        std::path::Path::new("repo"),
        "ripr context --root repo --at probe:a",
        false,
    )
}

#[test]
fn explain_agent_gap_is_one_record_one_test_open_terminal_and_context_next() {
    let text = explained(&weak_gap("probe:a", 2));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        vec![
            "#ripr explain agent static-advisory class=weakly_exposed",
            "weakly_exposed|src/lib.rs:2|pct >= 50|missing=pct == 50|test=t1|id=probe:a",
            "t1=tests/t.rs:3 discount_runs relational_check `assert!(discount(10, 100) > 0);` miss=weak_assertion",
            "open=a test-side gap remains: no related test is shown to notice this change; static advisory, not runtime proof",
            "next=ripr context --root repo --at probe:a",
        ]
    );
}

#[test]
fn explain_agent_exposed_finding_is_done_not_open() {
    let text = explained(&finding("probe:ok", ExposureClass::Exposed, 4));
    assert!(text.lines().any(|line| line.starts_with("done=")), "{text}");
    assert!(!text.contains("open="), "{text}");
    assert!(!text.contains("stop="), "{text}");
    assert!(text.contains("|missing=-|test=-|"), "{text}");
}

#[test]
fn explain_agent_limited_finding_stops_and_names_the_limit_without_a_gap_claim() {
    let mut limited_weak = weak_gap("probe:limited", 4);
    limited_weak.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
    limited_weak.related_tests[0].oracle_strength = OracleStrength::Strong;
    let text = explained(&limited_weak);
    assert!(
        text.lines()
            .any(|line| line == "limit=rust_transitive_reach_unresolved"),
        "{text}"
    );
    assert!(text.lines().any(|line| line.starts_with("stop=")), "{text}");
    // Discriminating negative: a limited finding is not a gap, so the missing
    // value must not be offered as something to assert.
    assert!(!text.contains("|missing=pct == 50|"), "{text}");
    assert!(!text.contains("open="), "{text}");
}

#[test]
fn explain_agent_reach_limit_with_weak_row_is_open_without_a_missing_value() {
    let text = explained(&reach_limited_weak("probe:fortnight", 264));
    assert!(
        text.lines()
            .any(|line| line.starts_with("open=a static limit leaves reach unconfirmed")),
        "{text}"
    );
    assert!(text.contains("|missing=-|test=t1|"), "{text}");
}

#[test]
fn explain_agent_agrees_with_check_agent_on_gap_membership() {
    // One predicate decides "gap" for both surfaces: for every class, explain
    // prints `open=` exactly when check lists the finding in `gaps=`.
    for class in [
        ExposureClass::Exposed,
        ExposureClass::WeaklyExposed,
        ExposureClass::ReachableUnrevealed,
        ExposureClass::NoStaticPath,
        ExposureClass::StaticUnknown,
    ] {
        let f = finding("probe:x", class.clone(), 3);
        let check_has_gap = rendered(&output(vec![f.clone()])).contains(" gaps=1 ");
        let explain_open = explained(&f).contains("\nopen=");
        assert_eq!(check_has_gap, explain_open, "{class:?}");
    }
}

#[test]
fn explain_agent_record_cannot_be_forged_and_stays_static() {
    let mut gap = weak_gap("probe:a", 2);
    gap.probe.expression = "a | b\nc".to_string();
    let text = explained(&gap);
    let line = text.lines().nth(1).unwrap_or_default();
    assert!(line.contains("a \\| b c"), "{line}");
    for parts in [["kil", "led"], ["surv", "ived"], ["pro", "ven"]] {
        let word = parts.concat();
        assert!(!text.contains(&word), "{word} in {text}");
    }
}

#[test]
fn explain_agent_non_current_findings_fail_closed_with_a_currentness_stop() {
    for currentness in [
        SourceCurrentness::BaseDeleted,
        SourceCurrentness::MovedOrRenamed,
        SourceCurrentness::UnresolvedSubject,
    ] {
        for class in [ExposureClass::Exposed, ExposureClass::WeaklyExposed] {
            let mut f = weak_gap("probe:old", 5);
            f.class = class.clone();
            f.source_currentness = currentness;
            let text = explained(&f);
            assert!(
                text.contains("\nstop=this finding is not a current edit target ("),
                "{currentness:?} {class:?}: {text}"
            );
            assert!(text.contains(currentness.as_str()), "{text}");
            assert!(!text.contains("done=") && !text.contains("open="), "{text}");
            assert!(text.contains("|test=-|"), "{text}");
        }
    }
}

#[test]
fn explain_agent_suppressed_gap_is_not_open_but_unsuppressed_is() {
    let gap = weak_gap("probe:a", 2);
    let ctx = "ripr context --at probe:a";
    let root = std::path::Path::new("repo");
    let open = render_finding(&gap, root, ctx, false);
    let suppressed = render_finding(&gap, root, ctx, true);
    assert!(open.contains("\nopen="), "{open}");
    assert!(
        suppressed.contains("\nstop=this finding is suppressed by policy"),
        "{suppressed}"
    );
    assert!(
        !suppressed.contains("open=") && !suppressed.contains("done="),
        "{suppressed}"
    );
    assert!(suppressed.contains("|test=-|"), "{suppressed}");
}
