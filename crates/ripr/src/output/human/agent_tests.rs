//! Contract tests for `ripr check --format agent` (#11580).

use super::agent::{MAX_GAP_LINES, MIN_GAP_LINES, gap_line_budget, render, render_finding};
use crate::app::{CheckDiffProvenance, CheckOutput, Mode};
use crate::config::RiprConfig;
use crate::domain::{
    ActivationEvidence, Confidence, DeltaKind, DiscriminatorShape, ExposureClass, Finding,
    MissingDiscriminatorFact, OracleKind, OracleStrength, Probe, ProbeFamily, ProbeId, RelatedTest,
    RelatedTestMiss, RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation,
    StageEvidence, StageState, StaticLimitKind,
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
        shape: DiscriminatorShape::Boundary,
        drafted_boundary: None,
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

/// Summary counts are derived from the supplied findings by the producer's
/// own summarizer, so the full-summary `verify=` counts agree with the
/// header on every fixture unless a test overrides them deliberately.
fn output(findings: Vec<Finding>) -> CheckOutput {
    let summary = crate::analysis::summary::summarize_findings(1, &findings);
    CheckOutput {
        value_origins: Default::default(),
        harness_projections: Vec::new(),
        schema_version: "0.1".to_string(),
        tool: "ripr".to_string(),
        mode: Mode::Draft,
        root: PathBuf::from("repo"),
        base: Some("origin/main".to_string()),
        summary,
        findings,
        preview_language_advisories: Vec::new(),
        language_runs: Vec::new(),
        no_scope_provided: false,
        unanalyzed_working_tree: false,
        untracked_working_tree_source_paths: Vec::new(),
        ignored_working_tree_source_paths: Vec::new(),
        unlinked_python_tests: None,
        owner_opt_outs: Vec::new(),
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

fn agent_dispatch(output: &CheckOutput, config: &RiprConfig) -> Result<String, String> {
    crate::output::render::render_check_with_config(
        output,
        &crate::app::OutputFormat::Agent,
        config,
    )
}

fn outcome_output(
    kind: crate::analysis_outcome::AnalysisOutcomeKind,
    findings: Vec<Finding>,
) -> Result<CheckOutput, String> {
    use crate::analysis_outcome::{
        AnalysisIdentity, AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcome,
        AnalysisOutcomeCounts, AnalysisRecovery, AnalysisRecoveryKind, AnalysisStage,
    };
    let count = u64::try_from(findings.len()).map_err(|err| err.to_string())?;
    let mut output = output(findings);
    let limitations = if kind.is_complete() {
        Vec::new()
    } else {
        vec![AnalysisLimitation::new(
            AnalysisLimitationKind::ProducerFailure,
            AnalysisStage::LanguageAdapter,
            AnalysisRecovery::new(AnalysisRecoveryKind::Retry, "retry the selected input")?,
        )]
    };
    output.analysis_outcome = Some(AnalysisOutcome::new(
        kind,
        AnalysisIdentity::default(),
        AnalysisOutcomeCounts {
            changed_file_count: 1,
            changed_line_count: 1,
            candidate_line_count: 1,
            probe_count: count,
            finding_count: count,
        },
        limitations,
    )?);
    Ok(output)
}

#[test]
fn agent_dispatch_never_completes_incomplete_empty_or_exposed_reports() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::{
        AnalysisFailed, PartialWithLimitations, UnsupportedInput,
    };
    for (kind, token) in [
        (PartialWithLimitations, "partial_with_limitations"),
        (UnsupportedInput, "unsupported_input"),
        (AnalysisFailed, "analysis_failed"),
    ] {
        for findings in [
            Vec::new(),
            vec![finding("probe:ok", ExposureClass::Exposed, 4)],
        ] {
            let output = outcome_output(kind, findings)?;
            let text = agent_dispatch(&output, &RiprConfig::default())?;
            assert!(
                text.contains(&format!("analysis=incomplete|outcome={token}\n")),
                "{text}"
            );
            assert!(text.contains("\nstop=analysis incomplete;"), "{text}");
            assert!(
                text.contains("|recovery=retry the selected input\n"),
                "{text}"
            );
            assert!(!text.contains("\ndone="), "{text}");
        }
    }
    Ok(())
}

/// #13658: an explicit empty/unparseable `--diff` yields a `no_scope`
/// outcome; stdout must not claim `done`. Control: a complete no-findings run
/// still says `done`.
#[test]
fn agent_no_scope_outcome_is_not_done_but_complete_no_findings_is() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::{CompleteNoFindings, NoScope};
    let config = RiprConfig::default();
    let mut scoped = outcome_output(CompleteNoFindings, Vec::new())?;
    scoped.analysis_outcome = Some(crate::analysis_outcome::AnalysisOutcome::new(
        NoScope,
        crate::analysis_outcome::AnalysisIdentity::default(),
        crate::analysis_outcome::AnalysisOutcomeCounts::default(),
        Vec::new(),
    )?);
    let no_scope = agent_dispatch(&scoped, &config)?;
    assert!(
        no_scope.contains("\nstop=nothing was in scope"),
        "{no_scope}"
    );
    assert!(!no_scope.contains("done="), "{no_scope}");
    let clean = agent_dispatch(&outcome_output(CompleteNoFindings, Vec::new())?, &config)?;
    assert!(
        clean.contains("\ndone=no changed behavior found"),
        "{clean}"
    );
    Ok(())
}

#[test]
fn agent_dispatch_keeps_partial_gaps_and_complete_controls() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::{
        CompleteNoFindings, CompleteWithFindings, PartialWithLimitations,
    };
    use crate::analysis_outcome::{
        AnalysisLimitation, AnalysisLimitationKind, AnalysisRecovery, AnalysisRecoveryKind,
        AnalysisStage,
    };
    let config = RiprConfig::default();
    let partial = outcome_output(PartialWithLimitations, vec![weak_gap("probe:gap", 2)])?;
    let text = agent_dispatch(&partial, &config)?;
    assert!(
        text.contains("analysis=incomplete|outcome=partial_with_limitations\n"),
        "{text}"
    );
    assert!(text.contains("|id=probe:gap\n"), "{text}");
    assert!(
        text.contains("next=ripr explain --format agent probe:gap\n"),
        "{text}"
    );
    assert!(
        !text.contains("\ndone=") && !text.contains("\nstop="),
        "{text}"
    );

    for (kind, findings) in [
        (CompleteNoFindings, Vec::new()),
        (
            CompleteWithFindings,
            vec![finding("probe:ok", ExposureClass::Exposed, 4)],
        ),
    ] {
        let text = agent_dispatch(&outcome_output(kind, findings)?, &config)?;
        assert!(text.contains("\ndone="), "{text}");
        assert!(
            !text.contains("analysis=incomplete") && !text.contains("\nstop="),
            "{text}"
        );
    }
    let text = agent_dispatch(
        &outcome_output(CompleteWithFindings, vec![weak_gap("probe:gap", 2)])?,
        &config,
    )?;
    assert!(
        text.contains("next=ripr explain --format agent probe:gap\n"),
        "{text}"
    );
    assert!(
        !text.contains("analysis=incomplete") && !text.contains("\ndone="),
        "{text}"
    );
    // A producer-declared advisory limitation does not make a complete run
    // incomplete. Testing limitations.len() instead of the closed kind is wrong.
    let advisory_limit = AnalysisLimitation::new(
        AnalysisLimitationKind::EolOnlyChurn,
        AnalysisStage::DiffParse,
        AnalysisRecovery::new(AnalysisRecoveryKind::Retry, "review the line-ending change")?,
    );
    let mut advisory = outcome_output(CompleteNoFindings, Vec::new())?;
    advisory
        .analysis_outcome
        .as_mut()
        .ok_or("missing complete control outcome")?
        .limitations
        .push(advisory_limit.clone());
    let text = agent_dispatch(&advisory, &config)?;
    assert!(text.contains("\ndone="), "{text}");
    assert!(!text.contains("analysis=incomplete"), "{text}");
    // An advisory row before the limiting cause must not displace the actual
    // failure's recovery. The omission count includes all undisplayed rows.
    let mut mixed = outcome_output(PartialWithLimitations, Vec::new())?;
    mixed
        .analysis_outcome
        .as_mut()
        .ok_or("missing partial control outcome")?
        .limitations
        .insert(0, advisory_limit);
    let text = agent_dispatch(&mixed, &config)?;
    assert!(text.contains("limitation=producer_failure|"), "{text}");
    assert!(
        text.contains("|recovery=retry the selected input\n"),
        "{text}"
    );
    assert!(text.contains("+1 more analysis limitations;"), "{text}");
    assert!(!text.contains("limitation=eol_only_churn|"), "{text}");
    Ok(())
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
    assert_eq!(lines[3], "next=ripr explain --format agent probe:a");
    // #13516: the verification record is last and repeats the header's full
    // counts (2 findings, 1 exposed); no outcome means refused.
    assert_eq!(
        lines[4],
        "verify=refused|outcome=missing|findings=2|exposed=1|reason=outcome_missing,gaps_remain"
    );
    assert_eq!(lines.len(), 5, "{text}");
}

/// #13157: all three compact check drill-ins retain the same source
/// identity and opt into compact explain. Policy-bearing reports keep the
/// old route, including artifact replay, which rejects the agent format.
#[test]
fn agent_explain_navigation_preserves_scope_format_and_policy_compatibility() -> Result<(), String>
{
    use crate::analysis::DiffScopeFilter;
    use crate::app::{CheckInput, FindingDrillIn, finding_navigation_with_worktree};
    use crate::output::suppressions::CheckSuppressionOutcome;
    use std::path::Path;

    let selector = "probe:src/owner's file.rs:predicate:abc";
    let mut input = CheckInput {
        root: PathBuf::from("repo with 'quote'"),
        base: Some("topic/base".to_string()),
        mode: Mode::Ready,
        include_unchanged_tests: false,
        perl_facts_path: Some(PathBuf::from("facts with space.json")),
        scope: DiffScopeFilter::new(
            ["src/owner's file.rs".to_string()],
            ["generated/**".to_string()],
        )
        .map_err(|error| format!("valid navigation scope expected: {error:?}"))?,
        ..CheckInput::default()
    };
    for policy_present in [false, true] {
        input.suppression_policy = policy_present.then(|| PathBuf::from("policy with space.toml"));
        for (case, worktree, diff, artifact) in [
            ("committed", false, None, None),
            ("worktree", true, None, None),
            (
                "diff",
                false,
                Some(Path::new("change with 'quote'.diff")),
                None,
            ),
            (
                "artifact",
                false,
                None,
                Some(Path::new("saved with 'quote'.json")),
            ),
        ] {
            input.diff_file = diff.map(Path::to_path_buf);
            let navigation = finding_navigation_with_worktree(&input, artifact, true, worktree);
            let ordinary = navigation.explain_command(selector);
            assert!(!ordinary.contains("--format"), "{ordinary}");
            let expected = if policy_present {
                format!("next={ordinary}")
            } else {
                format!("next={}", navigation.explain_command_compact(selector))
            };
            let drill_in = FindingDrillIn::Commands(navigation);
            for target in [
                weak_gap(selector, 2),
                reach_limited_weak(selector, 2),
                finding(selector, ExposureClass::NoStaticPath, 2),
            ] {
                let reach_limited = target.static_limit_kind.is_some();
                let stopped = target.class == ExposureClass::NoStaticPath;
                let mut report = output(vec![target]);
                report.root = input.root.clone();
                report.base = input.base.clone();
                // Empty matches are intentional: an explicit policy still
                // has the replay constraint when it suppresses no finding.
                report.suppression = policy_present.then(|| CheckSuppressionOutcome {
                    policy_path: "policy with space.toml".to_string(),
                    suppressed: Vec::new(),
                    warnings: Vec::new(),
                });
                let text = render(
                    &report,
                    &RiprConfig::default(),
                    Some(&drill_in),
                    CheckDiffProvenance::from_effective_source(worktree, diff.is_some()),
                );
                let next: Vec<&str> = text
                    .lines()
                    .filter(|line| line.starts_with("next="))
                    .collect();
                assert_eq!(
                    next,
                    [expected.as_str()],
                    "{case}, policy={policy_present}: {text}"
                );
                assert!(!text.contains("done="), "{case}: {text}");
                assert_eq!(text.contains("\nstop="), stopped, "{case}: {text}");
                assert_eq!(
                    text.contains("\nopen=a static limit leaves reach unconfirmed"),
                    reach_limited,
                    "{case}, policy={policy_present}: {text}"
                );
                assert_eq!(
                    text.matches("--format agent").count(),
                    usize::from(!policy_present),
                    "{case}, policy={policy_present}: {text}"
                );
            }
        }
    }
    Ok(())
}

/// #13157: the agent surface's drill-in carries exactly one `--format agent`,
/// inserted before the (possibly quoted) selector, while the human
/// navigation keeps the flag-free command. The legacy navigation pins the
/// insertion point; scoped drill-ins are pinned end-to-end in cli_smoke.
#[test]
fn agent_drill_in_carries_the_compact_format_before_the_selector() {
    let navigation = crate::app::FindingNavigation::legacy();
    assert_eq!(
        navigation.explain_command_compact("probe:a"),
        "ripr explain --format agent probe:a"
    );
    // Quoting is host-dependent (single quotes off-Windows, double quotes on
    // Windows consoles), so derive the expected rendering from the same
    // `shell_arg` helper the production call site uses.
    let spaced = "probe:my gap";
    assert_eq!(
        navigation.explain_command_compact(spaced),
        format!(
            "ripr explain --format agent {}",
            crate::agent::loop_commands::shell_arg(spaced)
        )
    );
    // Human navigation is unchanged (negative control).
    assert_eq!(
        navigation.explain_command("probe:a"),
        "ripr explain probe:a"
    );
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
    // #13238: the terminal names the documented co-located-test route; it
    // must not claim nothing more is fixable while the human surface
    // recommends adding a test for the same finding.
    assert!(
        !text.contains("nothing more is fixable from the test side"),
        "{text}"
    );
    assert!(text.contains("co-located test"), "{text}");
    assert!(text.contains("\nlimits=no_static_path:1\n"), "{text}");
    assert!(text.contains(" gaps=0 "), "{text}");
    // The all-limited run still carries a bounded route to the evidence: the
    // ripr explain drill-in.
    // #13594: a `missing=-` record names nothing to fix, so it is omitted.
    assert!(!text.contains("no_static_path|src/lib.rs:7"), "{text}");
    assert!(!text.contains("missing=-"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain --format agent probe:np"),
        "{text}"
    );
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
            .any(|line| line == "next=ripr explain --format agent probe:gap"),
        "{text}"
    );
    assert!(text.contains("\nlimits=static_unknown:1\n"), "{text}");
}

/// #11825 negative: a weakly_exposed finding that is itself static-limited
/// is not an open test-side gap, so `stop=` stays, unless it also carries a
/// `weak_assertion` row (see `reach_limit_with_weak_assertion_row_does_not_stop`).
/// #13238: the run still owes the bounded route — the record and the
/// `ripr explain` drill-in appear above the terminal line.
#[test]
fn static_limited_weakly_exposed_alone_keeps_stop() {
    let mut limited_weak = weak_gap("probe:limited-weak", 4);
    limited_weak.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
    limited_weak.related_tests[0].oracle_strength = OracleStrength::Strong;
    let text = rendered(&output(vec![limited_weak]));
    assert!(text.contains("\nstop="), "{text}");
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(!text.contains("|id=probe:limited-weak"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain --format agent probe:limited-weak"),
        "{text}"
    );
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
    // #13803: the listed open row is counted beside `gaps=0`.
    assert!(text.contains(" gaps=0 unconfirmed_gaps=1 "), "{text}");
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
            .any(|line| line == "next=ripr explain --format agent probe:fortnight"),
        "{text}"
    );
}

/// #11825: more open findings than `MAX_GAP_LINES` print the cap and an
/// exact omitted count; `reachable_unrevealed` opens the same way.
#[test]
fn open_limited_findings_over_the_cap_report_the_omitted_count() {
    let total = MIN_GAP_LINES + 2;
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
        text.matches("|test=t1|id=probe:open").count() <= MIN_GAP_LINES,
        "{text}"
    );
    assert!(
        text.contains("\n+2 more open findings not shown; `--format json` lists all\n"),
        "{text}"
    );
}

/// #11825 negative: the same reach limit with no weak-assertion row (the
/// fixed-tree shape: exact-value tests remain, only the limit is left) still
/// stops, and so does a weak row behind a different limit kind. #13238: the
/// stop is no longer a dead end — bounded records and the drill-in appear.
#[test]
fn reach_limit_without_weak_assertion_row_keeps_stop() {
    let mut fixed = reach_limited_weak("probe:fixed", 264);
    fixed.related_tests[0].miss = Some(RelatedTestMiss::ObservationUnconfirmed);
    fixed.related_tests[0].oracle_strength = OracleStrength::Strong;
    let text = rendered(&output(vec![fixed]));
    assert!(text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
    assert!(!text.contains("unconfirmed_gaps="), "{text}");
    assert!(!text.contains("|id=probe:fixed"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain --format agent probe:fixed"),
        "{text}"
    );

    let mut other_kind = weak_gap("probe:macro", 5);
    other_kind.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
    let text = rendered(&output(vec![other_kind]));
    assert!(text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
    assert!(!text.contains("|id=probe:macro"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain --format agent probe:macro"),
        "{text}"
    );
}

/// #13238: an all-limited run with more findings than the record floor caps
/// the records and counts the rest explicitly, the same bounded shape the
/// `open=` path uses.
#[test]
fn all_limited_records_cap_over_the_floor_and_count_the_rest() {
    let total = MIN_GAP_LINES + 2;
    let findings: Vec<Finding> = (0..total)
        .map(|index| {
            finding(
                &format!("probe:lim{index}"),
                ExposureClass::NoStaticPath,
                10 + index,
            )
        })
        .collect();
    let text = rendered(&output(findings));
    assert!(text.contains("\nstop="), "{text}");
    assert!(text.contains(" gaps=0 "), "{text}");
    // #13594: none of these carries a concrete next step, so no records and
    // no "not shown" count; `limits=` and the drill-in carry the route.
    assert!(!text.contains("no_static_path|src/lib.rs:"), "{text}");
    assert!(!text.contains("more limited findings not shown"), "{text}");
    assert!(text.contains("\nlimits=no_static_path:7\n"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain --format agent probe:lim0"),
        "{text}"
    );
}

/// #13238 measured repro: two no_static_path findings and no gaps. The agent
/// surface must not collapse to a bare stop line while the human surface for
/// the same run names a file, a gap id, and the co-located-test next step.
#[test]
fn all_no_static_path_run_matches_the_human_surface_route() {
    let text = rendered(&output(vec![
        finding(
            "gap:rust:src/lib.rs:levy:predicate_boundary:predicate:vip",
            ExposureClass::NoStaticPath,
            15,
        ),
        finding(
            "gap:rust:src/lib.rs:fee:predicate_boundary:predicate:std",
            ExposureClass::NoStaticPath,
            40,
        ),
    ]));
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(text.contains("no_static_path=2"), "{text}");
    assert!(!text.contains("nothing more is fixable"), "{text}");
    assert!(text.contains("co-located test"), "{text}");
    assert!(!text.contains("missing=-"), "{text}");
    assert!(
        text.contains(
            "next=ripr explain --format agent gap:rust:src/lib.rs:levy:predicate_boundary:predicate:vip\n"
        ),
        "{text}"
    );
}

/// #12058 negative: an unrelated weak test that is related only by file or
/// module proximity does not make a reach-limited finding open; it stays
/// `stop=`. The same row bound to the owner (a direct call or an owner-named
/// test) still opens it. #13238: the stopped proximity case still gets the
/// bounded record and drill-in; only `open=` is withheld.
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
        assert!(!text.contains("|id=probe:unrelated"), "{reason:?}: {text}");
        assert!(
            text.lines()
                .any(|line| line == "next=ripr explain --format agent probe:unrelated"),
            "{reason:?}: {text}"
        );
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

/// #13658: a no-scope or no-analyzable-source diff examined nothing, so the
/// agent format stops with a refused `verify=` instead of `done=`; a diff that
/// changed analyzable Rust source and found nothing keeps `done=`.
#[test]
fn nothing_analyzed_stops_and_refuses_verify_while_a_valid_diff_stays_done() -> Result<(), String> {
    use crate::analysis_outcome::{
        AnalysisIdentity, AnalysisOutcome, AnalysisOutcomeCounts, AnalysisOutcomeKind,
    };
    let outcome = |kind, changed_file_count| {
        AnalysisOutcome::new(
            kind,
            AnalysisIdentity::default(),
            AnalysisOutcomeCounts {
                changed_file_count,
                changed_line_count: changed_file_count,
                ..AnalysisOutcomeCounts::default()
            },
            Vec::new(),
        )
    };
    let mut empty = output(Vec::new());
    empty.analysis_outcome = Some(outcome(AnalysisOutcomeKind::NoScope, 0)?);
    let stopped = rendered(&empty);
    assert!(
        stopped.contains("\nstop=nothing was in scope;"),
        "{stopped}"
    );
    assert!(!stopped.contains("done="), "{stopped}");
    // The completion record must not contradict the stop: nothing analyzed is
    // a refused verification (producer-owned `no_behavior_in_scope`).
    assert!(
        stopped.contains("\nverify=refused|outcome=no_scope|"),
        "{stopped}"
    );
    assert!(!stopped.contains("verify=complete"), "{stopped}");

    // A docs-only diff changed files but none ripr analyzes: also nothing
    // examined, matching the human "Nothing to check" state.
    let mut docs_only = output(Vec::new());
    docs_only.summary.changed_rust_files = 0;
    docs_only.analysis_outcome = Some(outcome(AnalysisOutcomeKind::NoBehavioralCandidates, 1)?);
    assert_eq!(docs_only.summary.changed_rust_files, 0);
    let docs_text = rendered(&docs_only);
    assert!(
        docs_text.contains("\nstop=nothing was analyzed"),
        "{docs_text}"
    );
    assert!(!docs_text.contains("done="), "{docs_text}");
    assert!(docs_text.contains("\nverify=refused|"), "{docs_text}");
    assert!(!docs_text.contains("verify=complete"), "{docs_text}");

    let mut healthy = output(Vec::new());
    healthy.summary.changed_rust_files = 1;
    healthy.analysis_outcome = Some(outcome(AnalysisOutcomeKind::NoBehavioralCandidates, 1)?);
    let done = rendered(&healthy);
    assert!(
        done.contains("\ndone=no changed behavior found in scope"),
        "{done}"
    );
    assert!(!done.contains("stop="), "{done}");
    Ok(())
}

#[test]
fn empty_report_over_unanalyzed_files_stops_instead_of_done() {
    let mut untracked = output(Vec::new());
    untracked.unanalyzed_working_tree = true;
    untracked.untracked_working_tree_source_paths = vec!["src/new.rs".to_string()];
    let text = rendered(&untracked);
    assert!(
        text.contains("\nunanalyzed=uncommitted_edits|fix="),
        "{text}"
    );
    assert!(
        text.contains("\nunanalyzed=untracked|files=src/new.rs|fix=git add -N PATH"),
        "{text}"
    );
    assert!(text.contains("\nstop=changes were not analyzed"), "{text}");
    assert!(!text.contains("done="), "{text}");

    let mut ignored = output(Vec::new());
    ignored.ignored_working_tree_source_paths = vec!["gen/a.rs".to_string()];
    let text = rendered(&ignored);
    assert!(
        text.contains("\nunanalyzed=gitignored|files=gen/a.rs|fix=git add -f -N PATH"),
        "{text}"
    );
    assert!(!text.contains("done="), "{text}");

    // Negative: untracked paths without an unanalyzed or working-tree read
    // are not this run's concern, so a clean report stays `done=`.
    let mut quiet = output(Vec::new());
    quiet.untracked_working_tree_source_paths = vec!["src/new.rs".to_string()];
    let text = rendered(&quiet);
    assert!(!text.contains("unanalyzed="), "{text}");
    assert!(text.contains("\ndone=no changed behavior"), "{text}");
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
        MIN_GAP_LINES,
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

/// #13277: a preview-language finding with a producer-recorded missing
/// discriminator and a weak related test must not be told "nothing more is
/// fixable"; the discriminator and test stay visible, advisory only.
fn preview_weak_gap(id: &str, line: usize) -> Finding {
    let mut f = weak_gap(id, line);
    f.language_status = Some(crate::domain::LanguageStatus::Preview);
    f
}

#[test]
fn explain_agent_preview_weak_gap_keeps_the_discriminator_and_test_open() {
    let text = explained(&preview_weak_gap("probe:a", 2));
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[1],
        "weakly_exposed|src/lib.rs:2|pct >= 50|missing=pct == 50|test=t1|id=probe:a"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("open=preview-language evidence is advisory")),
        "{text}"
    );
    assert!(!text.contains("nothing more is fixable"), "{text}");
    assert!(!text.contains("\nstop="), "{text}");
    assert!(
        text.contains("static advisory, not runtime proof"),
        "{text}"
    );
    assert_eq!(
        lines.last().copied(),
        Some("next=ripr context --root repo --at probe:a")
    );
}

#[test]
fn explain_agent_preview_controls_stay_closed_without_established_evidence() {
    // No producer-recorded discriminator: still the honest stop.
    let mut bare = preview_weak_gap("probe:a", 2);
    bare.activation.missing_discriminators.clear();
    let text = explained(&bare);
    assert!(text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
    // No weak related test: nothing to point at.
    let mut no_test = preview_weak_gap("probe:a", 2);
    no_test.related_tests.clear();
    let text = explained(&no_test);
    assert!(text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
    // Exposed preview finding is not a gap.
    let mut exposed = preview_weak_gap("probe:a", 2);
    exposed.class = ExposureClass::Exposed;
    assert!(!explained(&exposed).contains("open="));
    // Static limit and non-current source keep their stops.
    let mut limited = preview_weak_gap("probe:a", 2);
    limited.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
    assert!(!explained(&limited).contains("open="));
    let mut moved = preview_weak_gap("probe:a", 2);
    moved.source_currentness = SourceCurrentness::MovedOrRenamed;
    assert!(!explained(&moved).contains("open="));
    // Suppressed stays suppressed.
    let suppressed = render_finding(
        &preview_weak_gap("probe:a", 2),
        std::path::Path::new("repo"),
        "ripr context --at probe:a",
        true,
    );
    assert!(!suppressed.contains("open="), "{suppressed}");
    // An opaque preview finding (unsupported evidence class) is not a gap:
    // it keeps its stop and never projects an open= test-side action.
    let mut opaque = preview_weak_gap("probe:a", 2);
    opaque.class = ExposureClass::StaticUnknown;
    let text = explained(&opaque);
    assert!(text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
}

/// #13277 check surface: a run whose only finding is a preview weak gap still
/// stops (no test-side gap is offered), but the bounded record must project
/// the producer-owned discriminator and the related test, and the terminal
/// must not claim nothing more is fixable while the guided repair route is
/// unavailable (`agent card` refuses these seams with `seam_not_found`).
#[test]
fn check_agent_preview_only_run_projects_the_discriminator_and_stops_advisory() {
    let text = rendered(&output(vec![preview_weak_gap("probe:a", 2)]));
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(
        text.contains("weakly_exposed|src/lib.rs:2|pct >= 50|missing=pct == 50|test=t1|id=probe:a"),
        "{text}"
    );
    assert!(text.contains("t1=tests/t.rs:3 discount_runs"), "{text}");
    assert!(
        text.contains(
            "stop=preview-language evidence is advisory only and offers no guided repair route"
        ),
        "{text}"
    );
    assert!(
        text.contains("an independent test-side improvement may still be possible"),
        "{text}"
    );
    assert!(
        text.contains("static advisory, not runtime proof"),
        "{text}"
    );
    // Negative assertions: the old misleading terminal and the hidden
    // discriminator are both rejected, not just a new phrase searched for.
    assert!(
        !text.contains("nothing more is fixable from the test side"),
        "{text}"
    );
    assert!(!text.contains("nothing more is fixable"), "{text}");
    assert!(!text.contains("|missing=-|"), "{text}");
    assert!(text.contains("limits=weakly_exposed:1"), "{text}");
    assert!(
        text.lines()
            .any(|line| line == "next=ripr explain --format agent probe:a"),
        "{text}"
    );
    // The explain drill-in for the same finding keeps the open= advisory.
    let drill = explained(&preview_weak_gap("probe:a", 2));
    assert!(
        drill.contains("open=preview-language evidence is advisory"),
        "{drill}"
    );
    assert!(!drill.contains("nothing more is fixable"), "{drill}");
}

/// A preview run whose only finding already reads exposed (the agent's new
/// test closed it) ends with `done=`, not the advisory `stop=` that says a
/// test-side improvement "may still be possible". Negative control: the weak
/// preview gap above keeps its `stop=`.
#[test]
fn check_agent_exposed_preview_finding_reads_done_not_stop() {
    let mut exposed = preview_weak_gap("probe:a", 2);
    exposed.class = ExposureClass::Exposed;
    exposed.activation.missing_discriminators.clear();
    let text = rendered(&output(vec![exposed]));
    assert!(text.contains(" exposed=1"), "{text}");
    assert!(
        text.contains(
            "\ndone=preview-language evidence is advisory only; a related test appears to observe this change, so there is no test-side gap to fix"
        ),
        "{text}"
    );
    assert!(!text.contains("\nstop="), "{text}");
    assert!(!text.contains("open="), "{text}");
    assert!(!text.contains("may still be possible"), "{text}");
    let weak = rendered(&output(vec![preview_weak_gap("probe:a", 2)]));
    assert!(weak.contains("\nstop=preview-language"), "{weak}");
    assert!(!weak.contains("\ndone="), "{weak}");
    // The explain drill-in for the same exposed preview finding agrees.
    let mut exposed = preview_weak_gap("probe:a", 2);
    exposed.class = ExposureClass::Exposed;
    exposed.activation.missing_discriminators.clear();
    let drill = explained(&exposed);
    assert!(
        drill.contains("\ndone=preview-language evidence is advisory only; a related test appears to notice this change"),
        "{drill}"
    );
    assert!(!drill.contains("\nstop="), "{drill}");
    assert!(!drill.contains("co-located test"), "{drill}");
    // A static-limited exposed preview finding fails closed on both surfaces.
    let mut limited = preview_weak_gap("probe:a", 2);
    limited.class = ExposureClass::Exposed;
    limited.activation.missing_discriminators.clear();
    limited.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
    let limited_check = rendered(&output(vec![limited.clone()]));
    assert!(!limited_check.contains("\ndone="), "{limited_check}");
    let limited_drill = explained(&limited);
    assert!(!limited_drill.contains("\ndone="), "{limited_drill}");
}

/// A preview finding without a producer-established discriminator has nothing
/// to project: no record is printed (#13594) and the terminal stays the honest
/// advisory stop.
#[test]
fn check_agent_preview_finding_without_a_discriminator_prints_no_record() {
    let mut bare = preview_weak_gap("probe:a", 2);
    bare.activation.missing_discriminators.clear();
    let text = rendered(&output(vec![bare]));
    // #13594: no discriminator means no record at all, not a `missing=-` one.
    assert!(!text.contains("weakly_exposed|src/lib.rs:2"), "{text}");
    assert!(!text.contains("missing=-"), "{text}");
    assert!(
        text.contains("stop=preview-language evidence is advisory only"),
        "{text}"
    );
    assert!(!text.contains("nothing more is fixable"), "{text}");
}

/// #13594 positive cap: preview findings that carry a discriminator keep their
/// records, capped at the floor with the rest counted.
#[test]
fn preview_limited_records_with_discriminators_cap_and_count_the_rest() {
    let total = MIN_GAP_LINES + 2;
    let findings: Vec<Finding> = (0..total)
        .map(|index| preview_weak_gap(&format!("probe:pv{index}"), 10 + index))
        .collect();
    let text = rendered(&output(findings));
    assert!(text.contains(" gaps=0 "), "{text}");
    assert!(
        text.matches("|missing=pct == 50|").count() == MIN_GAP_LINES,
        "{text}"
    );
    assert!(
        text.contains("\n+2 more limited findings not shown; `--format json` lists all\n"),
        "{text}"
    );
}

/// #13594 review: with a static-limited finding beside a preview finding that
/// carries a discriminator, `next=` drills into the printed record, not the
/// unprinted static-limited one.
#[test]
fn mixed_limited_run_drills_into_the_printed_record() {
    for order in [false, true] {
        let mut findings = vec![
            finding("probe:np", ExposureClass::NoStaticPath, 1),
            preview_weak_gap("probe:pv", 2),
        ];
        if order {
            findings.reverse();
        }
        let text = rendered(&output(findings));
        assert!(text.contains(" gaps=0 "), "{text}");
        assert!(text.contains("|missing=pct == 50|"), "{text}");
        assert!(text.contains("|id=probe:pv"), "{text}");
        assert!(!text.contains("|id=probe:np"), "{text}");
        assert!(
            text.lines()
                .any(|line| line == "next=ripr explain --format agent probe:pv"),
            "{text}"
        );
    }
}

/// #13339 review (thread 1): a preview finding can also be static-limited —
/// an opaque class or a named limitation — while still carrying a producer
/// discriminator. The explain surface keeps `missing=-` for those (the
/// `with_missing` projection only follows `open=` terminals), so the check
/// surface's limited records must not project the discriminator either;
/// otherwise the two compact surfaces would disagree about the same fact and
/// a limitation would read as an actionable value.
#[test]
fn check_agent_static_limited_preview_finding_prints_no_record() {
    let mut opaque = preview_weak_gap("probe:a", 2);
    opaque.class = ExposureClass::StaticUnknown;
    let mut named = preview_weak_gap("probe:a", 2);
    named.static_limit_kind = Some(StaticLimitKind::RustTransitiveReachUnresolved);
    for variant in [opaque, named] {
        // Both variants are static-limited while preview and still carrying
        // the producer discriminator.
        assert!(!variant.activation.missing_discriminators.is_empty());
        let text = rendered(&output(vec![variant.clone()]));
        // #13594: the static limit withholds the discriminator, and a record
        // that would read `missing=-` is omitted rather than printed.
        assert!(
            !text.contains("|missing="),
            "a static-limited preview finding must not print a record:\n{text}"
        );
        assert!(!text.contains("missing=pct == 50"), "{text}");
        // The explain drill-in for the same finding agrees: missing stays
        // hidden behind the static limitation, on a stop terminal.
        let drill = explained(&variant);
        assert!(drill.contains("|missing=-|"), "{drill}");
        assert!(drill.contains("\nstop="), "{drill}");
        assert!(!drill.contains("open="), "{drill}");
    }
}

/// A Python preview weak gap with the producer's repair-card evidence lines,
/// so `python_repair_card` accepts it and the shared route decision can
/// recommend a guided repair start for it.
fn python_card_gap(id: &str) -> Finding {
    let mut gap = preview_weak_gap(id, 2);
    gap.language = Some(crate::domain::LanguageId::Python);
    gap.canonical_gap = Some(crate::domain::FindingCanonicalGap {
        id: "gap:python:src/lib.rs:pricing:predicate_boundary:predicate:pct==50".to_string(),
        language: "python".to_string(),
        file: "src/lib.rs".to_string(),
        owner: "pricing".to_string(),
        behavior_kind: "predicate_boundary".to_string(),
        probe_kind: "predicate".to_string(),
        normalized_discriminator: "pct==50".to_string(),
    });
    gap.evidence = vec![
        "suggested_test_file: tests/t.rs".to_string(),
        "suggested_test_name: discount_runs".to_string(),
        "suggested_test_node_id: tests/t.rs::discount_runs".to_string(),
        "suggested_verify_command: python -m pytest tests/t.rs::discount_runs".to_string(),
        "suggested_verify_command_confidence: high".to_string(),
    ];
    gap
}

/// The drill-in navigation carrying the pipeline's validated gap-bound repair
/// route for `id`, the input `recommends_gap_repair_start` reads.
fn gap_route_drill_in(id: &str) -> crate::app::FindingDrillIn {
    crate::app::FindingDrillIn::Commands(
        crate::app::FindingNavigation::legacy().with_repair_route_outcome(
            crate::app::CheckRepairRoute::GapRoute(crate::app::GapRepairRoute {
                finding_id: id.to_string(),
                gap_id: "gap:python:src/lib.rs:pricing:predicate_boundary:predicate:pct==50"
                    .to_string(),
                base: "origin/main".to_string(),
                uncommitted_files: Vec::new(),
            }),
        ),
    )
}

/// #13339 review (thread 2): when the same shared decision that recommends
/// the repair start in the human surface says a guided route exists, the
/// compact terminal must not categorically deny one; without that route the
/// denial stays (fail-closed), and the old "nothing more is fixable" text is
/// rejected in both arms.
#[test]
fn check_agent_preview_terminal_derives_the_route_claim_from_the_shared_decision() {
    let routed = render(
        &output(vec![python_card_gap("probe:a")]),
        &RiprConfig::default(),
        Some(&gap_route_drill_in("probe:a")),
        CheckDiffProvenance::CommittedHistory,
    );
    assert!(
        routed.contains(
            "open=preview-language evidence is advisory only, not a confirmed gap; ripr agent repair can start the repair for this gap"
        ),
        "{routed}"
    );
    assert!(
        routed
            .lines()
            .next()
            .is_some_and(|h| h.contains(" repairable=1")),
        "{routed}"
    );
    assert!(
        routed.contains("next=ripr agent repair --gap-id "),
        "{routed}"
    );
    assert!(!routed.contains("stop="), "{routed}");
    assert!(
        !routed.contains("offers no guided repair route"),
        "{routed}"
    );
    assert!(!routed.contains("nothing more is fixable"), "{routed}");
    assert!(
        routed.contains("static advisory, not runtime proof"),
        "{routed}"
    );

    let unrouted = render(
        &output(vec![python_card_gap("probe:a")]),
        &RiprConfig::default(),
        None,
        CheckDiffProvenance::CommittedHistory,
    );
    assert!(
        unrouted.contains("offers no guided repair route"),
        "{unrouted}"
    );
    assert!(!unrouted.contains("nothing more is fixable"), "{unrouted}");
}

/// #13599 fail-closed arm: the validated start exists but tracked production
/// files differ from HEAD, so the start would refuse. The agent text says to
/// commit first and prints no runnable start, and the JSON canonical action
/// carries no `repair_route`.
#[test]
fn check_agent_blocked_gap_start_says_commit_first_and_names_no_command() -> Result<(), String> {
    let drill_in = crate::app::FindingDrillIn::Commands(
        crate::app::FindingNavigation::legacy().with_repair_route_outcome(
            crate::app::CheckRepairRoute::GapRoute(crate::app::GapRepairRoute {
                finding_id: "probe:a".to_string(),
                gap_id: "gap:python:src/lib.rs:pricing:predicate_boundary:predicate:pct==50"
                    .to_string(),
                base: "origin/main".to_string(),
                uncommitted_files: vec!["src/lib.rs".to_string()],
            }),
        ),
    );
    let check = output(vec![python_card_gap("probe:a")]);
    let text = render(
        &check,
        &RiprConfig::default(),
        Some(&drill_in),
        CheckDiffProvenance::CommittedHistory,
    );
    assert!(
        text.contains("once your uncommitted production change is committed"),
        "{text}"
    );
    assert!(text.contains("\nopen="), "{text}");
    assert!(!text.contains("stop="), "{text}");
    assert!(!text.contains("next=ripr agent repair"), "{text}");
    assert!(!text.contains("repairable="), "{text}");

    let triage = super::triage::select_human_triage(&check, &RiprConfig::default());
    let action = super::triage::canonical_next_action_for_triage(
        &triage,
        &check,
        Some(&drill_in),
        CheckDiffProvenance::CommittedHistory,
    )?;
    let json = serde_json::to_value(&action).map_err(|error| error.to_string())?;
    assert!(json["stop"].get("repair_route").is_none(), "{json}");
    Ok(())
}

/// The header token follows the body: an incomplete analysis prints the
/// incomplete terminal, so it must not claim a startable repair.
#[test]
fn check_agent_incomplete_run_does_not_claim_repairable() -> Result<(), String> {
    let check = outcome_output(
        crate::analysis_outcome::AnalysisOutcomeKind::PartialWithLimitations,
        vec![python_card_gap("probe:a")],
    )?;
    let text = render(
        &check,
        &RiprConfig::default(),
        Some(&gap_route_drill_in("probe:a")),
        CheckDiffProvenance::CommittedHistory,
    );
    assert!(text.contains("analysis=incomplete"), "{text}");
    assert!(!text.contains("repairable="), "{text}");
    assert!(!text.contains("next=ripr agent repair"), "{text}");
    Ok(())
}

/// #13803: a preview gap is never a counted `gaps=` entry, but the header
/// must not read `gaps=0` alone beside the listed gap record and its `open=`
/// line. `unconfirmed_gaps=` counts it in every route state (startable,
/// blocked, no route), and stays absent when nothing unconfirmed is listed.
#[test]
fn check_agent_header_counts_a_listed_preview_gap_as_unconfirmed() -> Result<(), String> {
    let header = |text: &str| text.lines().next().unwrap_or_default().to_string();
    let config = RiprConfig::default();
    let committed = CheckDiffProvenance::CommittedHistory;

    let routed = render(
        &output(vec![python_card_gap("probe:a")]),
        &config,
        Some(&gap_route_drill_in("probe:a")),
        committed,
    );
    assert_eq!(
        header(&routed),
        "#ripr agent static-advisory base=origin/main findings=1 gaps=0 unconfirmed_gaps=1 repairable=1 weakly_exposed=1",
        "{routed}"
    );
    assert!(routed.contains("|missing=pct == 50|"), "{routed}");
    assert!(routed.contains("\nopen=preview-language"), "{routed}");

    let blocked_route = crate::app::FindingDrillIn::Commands(
        crate::app::FindingNavigation::legacy().with_repair_route_outcome(
            crate::app::CheckRepairRoute::GapRoute(crate::app::GapRepairRoute {
                finding_id: "probe:a".to_string(),
                gap_id: "gap:python:src/lib.rs:pricing:predicate_boundary:predicate:pct==50"
                    .to_string(),
                base: "origin/main".to_string(),
                uncommitted_files: vec!["src/lib.rs".to_string()],
            }),
        ),
    );
    let blocked = render(
        &output(vec![python_card_gap("probe:a")]),
        &config,
        Some(&blocked_route),
        committed,
    );
    assert_eq!(
        header(&blocked),
        "#ripr agent static-advisory base=origin/main findings=1 gaps=0 unconfirmed_gaps=1 weakly_exposed=1",
        "{blocked}"
    );

    let unrouted = render(
        &output(vec![preview_weak_gap("probe:a", 2)]),
        &config,
        None,
        committed,
    );
    assert!(
        header(&unrouted).contains(" gaps=0 unconfirmed_gaps=1 "),
        "{unrouted}"
    );

    // A confirmed gap beside the preview one: the preview record is not
    // listed (only `limits=` counts it), so the header counts nothing
    // unconfirmed.
    let mixed = rendered(&output(vec![
        weak_gap("probe:rust", 3),
        preview_weak_gap("probe:py", 4),
    ]));
    assert!(header(&mixed).contains(" gaps=1 "), "{mixed}");
    assert!(!mixed.contains("unconfirmed_gaps="), "{mixed}");

    // An exposed preview finding, a static-limited preview gap and a preview
    // finding of a non-gap class are not unconfirmed gaps.
    let mut exposed = preview_weak_gap("probe:e", 2);
    exposed.class = ExposureClass::Exposed;
    let mut limited = preview_weak_gap("probe:l", 2);
    limited.static_limit_kind = Some(StaticLimitKind::RustMacroWrappedAssertionUnresolved);
    let mut unknown = preview_weak_gap("probe:u", 2);
    unknown.class = ExposureClass::StaticUnknown;
    for negative in [exposed, limited, unknown] {
        let text = rendered(&output(vec![negative]));
        assert!(!text.contains("unconfirmed_gaps="), "{text}");
    }

    // An incomplete analysis lists no limited records, so it counts no
    // preview gaps.
    let incomplete = render(
        &outcome_output(
            crate::analysis_outcome::AnalysisOutcomeKind::PartialWithLimitations,
            vec![python_card_gap("probe:a")],
        )?,
        &config,
        Some(&gap_route_drill_in("probe:a")),
        committed,
    );
    assert!(!incomplete.contains("unconfirmed_gaps="), "{incomplete}");
    Ok(())
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

#[test]
fn the_gap_budget_grows_with_the_gap_count_and_stops_at_the_ceiling() {
    assert_eq!(gap_line_budget(0), MIN_GAP_LINES);
    assert_eq!(gap_line_budget(7), MIN_GAP_LINES);
    assert_eq!(gap_line_budget(8), MIN_GAP_LINES + 1);
    assert_eq!(gap_line_budget(39), 9);
    assert_eq!(gap_line_budget(40), MAX_GAP_LINES);
    assert_eq!(gap_line_budget(116), MAX_GAP_LINES);
    assert_eq!(gap_line_budget(100_000), MAX_GAP_LINES);
}

#[test]
fn a_larger_diff_prints_more_gap_lines_and_counts_the_rest() {
    let findings: Vec<Finding> = (0..40)
        .map(|index| weak_gap(&format!("probe:g{index}"), index + 1))
        .collect();
    let text = rendered(&output(findings));
    let budget = gap_line_budget(40);
    assert_eq!(budget, MAX_GAP_LINES);
    assert_eq!(text.matches("\nweakly_exposed|").count(), budget, "{text}");
    assert!(text.contains(" gaps=40 "), "{text}");
    assert!(text.contains("\n+30 more gaps not shown;"), "{text}");
}

#[test]
fn the_maximum_report_with_long_ids_fits_the_pr_comment_ceiling() {
    let findings: Vec<Finding> = (0..200)
        .map(|index| {
            // Long ids stand in for real `probe:<path>:<line>:<kind>` ids.
            weak_gap(&format!("probe:{}:g{index}", "p".repeat(110)), index + 1)
        })
        .collect();
    let text = rendered(&output(findings));
    assert_eq!(
        text.matches("\nweakly_exposed|").count(),
        MAX_GAP_LINES,
        "{text}"
    );
    assert!(text.len() < 6000, "{} bytes", text.len());
}

#[test]
fn the_open_path_stays_at_the_floor_however_many_findings() {
    let findings: Vec<Finding> = (0..12)
        .map(|index| reach_limited_weak(&format!("probe:open{index}"), 10 + index))
        .collect();
    let text = rendered(&output(findings));
    assert!(text.contains("\nopen="), "{text}");
    assert_eq!(
        text.matches("|id=probe:open").count(),
        MIN_GAP_LINES,
        "{text}"
    );
    assert!(
        text.contains("\n+7 more open findings not shown;"),
        "{text}"
    );
}

/// #13603: the named test is the one that calls the changed function, not a
/// same-file neighbor that sits first and holds a stronger oracle.
#[test]
fn a_direct_owner_caller_is_named_ahead_of_a_same_file_neighbor() {
    use crate::domain::{RelationConfidence, RelationReason};
    let mut gap = weak_gap("probe:caller", 2);
    let mut neighbor = gap.related_tests[0].clone();
    neighbor.name = "shipping_free_for_big_order".to_string();
    neighbor.line = 17;
    neighbor.relation_reason = Some(RelationReason::SameTestFile);
    neighbor.relation_confidence = Some(RelationConfidence::Medium);
    // As in the repro, the neighbor holds the stronger oracle (an exact pin on
    // another function); strength alone would have named it.
    neighbor.oracle_strength = OracleStrength::Strong;
    gap.related_tests[0].name = "find_exists".to_string();
    gap.related_tests[0].line = 24;
    gap.related_tests[0].relation_reason = Some(RelationReason::DirectOwnerCall);
    gap.related_tests[0].relation_confidence = Some(RelationConfidence::High);
    // The neighbor is listed first, as in the issue's repro.
    gap.related_tests.insert(0, neighbor);
    let text = rendered(&output(vec![gap]));
    assert!(text.contains("find_exists"), "{text}");
    assert!(!text.contains("shipping_free_for_big_order"), "{text}");
}

/// #13603 negative: with no recorded relation on either row, the stronger
/// oracle still wins, as before.
#[test]
fn rows_without_a_recorded_relation_still_pick_the_stronger_oracle() {
    let mut gap = weak_gap("probe:strength", 2);
    let mut stronger = gap.related_tests[0].clone();
    stronger.name = "exact_pin".to_string();
    stronger.oracle_strength = OracleStrength::Strong;
    gap.related_tests.push(stronger);
    let text = rendered(&output(vec![gap]));
    assert!(text.contains("exact_pin"), "{text}");
    assert!(!text.contains("discount_runs"), "{text}");
}

/// #13603 review: a `done=` line names the test that appears to notice the
/// change, so a stronger pin outranks a direct caller holding a smoke check.
#[test]
fn a_done_line_names_the_pinning_test_over_a_smoke_only_direct_caller() {
    use crate::domain::{RelationConfidence, RelationReason};
    let mut done = weak_gap("probe:done", 4);
    done.class = ExposureClass::Exposed;
    done.activation.missing_discriminators.clear();
    let mut caller = done.related_tests[0].clone();
    caller.name = "caller_smoke".to_string();
    caller.oracle_strength = OracleStrength::Smoke;
    caller.miss = None;
    caller.relation_reason = Some(RelationReason::DirectOwnerCall);
    caller.relation_confidence = Some(RelationConfidence::High);
    done.related_tests[0].name = "exact_pin".to_string();
    done.related_tests[0].oracle_strength = OracleStrength::Strong;
    done.related_tests[0].miss = None;
    done.related_tests[0].relation_reason = Some(RelationReason::SameTestFile);
    done.related_tests[0].relation_confidence = Some(RelationConfidence::Medium);
    done.related_tests.insert(0, caller);
    let text = explained(&done);
    assert!(text.lines().any(|line| line.starts_with("done=")), "{text}");
    assert!(text.contains("exact_pin"), "{text}");
    assert!(!text.contains("caller_smoke"), "{text}");
}

fn verify_line(output: &CheckOutput) -> Result<String, String> {
    let text = agent_dispatch(output, &RiprConfig::default())?;
    let last = text.lines().last().ok_or("empty report")?.to_string();
    assert_eq!(
        text.matches("verify=").count(),
        1,
        "exactly one verification record, and it is last: {text}"
    );
    assert!(last.starts_with("verify="), "{text}");
    assert!(text.ends_with('\n'), "{text}");
    Ok(last)
}

fn with_counts(mut output: CheckOutput, findings: usize, exposed: usize) -> CheckOutput {
    output.summary.findings = findings;
    output.summary.exposed = exposed;
    output
}

#[test]
fn verification_record_completes_only_for_closed_outcomes_with_equal_full_counts()
-> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::{CompleteNoFindings, CompleteWithFindings};
    let none = outcome_output(CompleteNoFindings, Vec::new())?;
    assert_eq!(
        verify_line(&none)?,
        "verify=complete|outcome=complete_no_findings|findings=0|exposed=0"
    );
    // The summary is derived from the supplied findings by the producer's
    // summarizer (two findings, both exposed); no count is overridden.
    let exposed = outcome_output(
        CompleteWithFindings,
        vec![
            finding("probe:ok1", ExposureClass::Exposed, 4),
            finding("probe:ok2", ExposureClass::Exposed, 9),
        ],
    )?;
    assert_eq!((exposed.summary.findings, exposed.summary.exposed), (2, 2));
    assert_eq!(
        verify_line(&exposed)?,
        "verify=complete|outcome=complete_with_findings|findings=2|exposed=2"
    );
    Ok(())
}

#[test]
fn verification_record_counts_the_full_summary_not_the_displayed_subset() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::CompleteWithFindings;
    // Only exposed findings are displayed (so `done=` prints), but the full
    // summary still holds an unresolved gap hidden by triage or budgets.
    let hidden = with_counts(
        outcome_output(
            CompleteWithFindings,
            vec![finding("probe:ok", ExposureClass::Exposed, 4)],
        )?,
        5,
        4,
    );
    let text = agent_dispatch(&hidden, &RiprConfig::default())?;
    assert!(text.contains("\ndone="), "{text}");
    assert_eq!(
        verify_line(&hidden)?,
        "verify=refused|outcome=complete_with_findings|findings=5|exposed=4|reason=gaps_remain"
    );
    // A displayed gap list capped by the budget never changes the full counts.
    let many: Vec<Finding> = (0..30)
        .map(|index| weak_gap(&format!("probe:gap{index}"), index + 1))
        .collect();
    let capped = with_counts(outcome_output(CompleteWithFindings, many)?, 30, 0);
    let text = agent_dispatch(&capped, &RiprConfig::default())?;
    assert!(text.contains("more gaps not shown"), "{text}");
    assert_eq!(
        verify_line(&capped)?,
        "verify=refused|outcome=complete_with_findings|findings=30|exposed=0|reason=gaps_remain"
    );
    Ok(())
}

#[test]
fn verification_record_refuses_missing_empty_scope_and_incomplete_outcomes() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::{
        AnalysisFailed, NoBehavioralCandidates, NoChangedLines, NoScope, PartialWithLimitations,
        UnsupportedInput,
    };
    let missing = output(Vec::new());
    assert_eq!(
        verify_line(&missing)?,
        "verify=refused|outcome=missing|findings=0|exposed=0|reason=outcome_missing"
    );
    for kind in [NoScope, NoChangedLines, NoBehavioralCandidates] {
        if kind == NoScope {
            // A NoScope outcome that also disclosed `no_scope_provided`
            // names both reasons.
            let mut unscoped = outcome_output(
                crate::analysis_outcome::AnalysisOutcomeKind::CompleteNoFindings,
                Vec::new(),
            )?;
            unscoped
                .analysis_outcome
                .as_mut()
                .ok_or("missing outcome")?
                .kind = NoScope;
            unscoped.no_scope_provided = true;
            assert_eq!(
                verify_line(&unscoped)?,
                "verify=refused|outcome=no_scope|findings=0|exposed=0|reason=no_behavior_in_scope,no_scope"
            );
        }
        // The per-kind count invariants are the producer's; this test only
        // varies the closed kind of an otherwise valid complete outcome.
        let mut empty = outcome_output(
            crate::analysis_outcome::AnalysisOutcomeKind::CompleteNoFindings,
            Vec::new(),
        )?;
        empty
            .analysis_outcome
            .as_mut()
            .ok_or("missing outcome")?
            .kind = kind;
        let line = verify_line(&empty)?;
        assert_eq!(
            line,
            format!(
                "verify=refused|outcome={}|findings=0|exposed=0|reason=no_behavior_in_scope",
                kind.as_str()
            )
        );
    }
    for kind in [PartialWithLimitations, UnsupportedInput, AnalysisFailed] {
        let line = verify_line(&outcome_output(kind, Vec::new())?)?;
        assert_eq!(
            line,
            format!(
                "verify=refused|outcome={}|findings=0|exposed=0|reason=analysis_incomplete",
                kind.as_str()
            )
        );
    }
    Ok(())
}

#[test]
fn verification_record_refuses_stale_worktree_and_no_scope() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::CompleteNoFindings;
    let mut stale = outcome_output(CompleteNoFindings, Vec::new())?;
    stale.unanalyzed_working_tree = true;
    assert_eq!(
        verify_line(&stale)?,
        "verify=refused|outcome=complete_no_findings|findings=0|exposed=0|reason=unanalyzed_working_tree"
    );
    let mut no_scope = outcome_output(CompleteNoFindings, Vec::new())?;
    no_scope.no_scope_provided = true;
    assert_eq!(
        verify_line(&no_scope)?,
        "verify=refused|outcome=complete_no_findings|findings=0|exposed=0|reason=no_scope"
    );
    let mut both = outcome_output(CompleteNoFindings, Vec::new())?;
    both.unanalyzed_working_tree = true;
    both.no_scope_provided = true;
    assert!(
        verify_line(&both)?.ends_with("reason=unanalyzed_working_tree,no_scope"),
        "every unmet condition is named"
    );
    Ok(())
}

#[test]
fn verification_record_refuses_untracked_but_not_ignored_excluded_source() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::CompleteNoFindings;
    // A working-tree read leaves `unanalyzed_working_tree` false while routed
    // source files outside the diff are listed; an untracked file in an
    // enabled language must still refuse.
    let clean = outcome_output(CompleteNoFindings, Vec::new())?;
    assert_eq!(
        verify_line(&clean)?,
        "verify=complete|outcome=complete_no_findings|findings=0|exposed=0"
    );
    let mut untracked = outcome_output(CompleteNoFindings, Vec::new())?;
    untracked.untracked_working_tree_source_paths = vec!["src/new.rs".to_string()];
    assert!(!untracked.unanalyzed_working_tree);
    assert_eq!(
        verify_line(&untracked)?,
        "verify=refused|outcome=complete_no_findings|findings=0|exposed=0|reason=untracked_source_excluded"
    );
    // Gitignored source is disclosed by the notes but never refuses.
    let mut ignored = outcome_output(CompleteNoFindings, Vec::new())?;
    ignored.ignored_working_tree_source_paths = vec![
        "src/gen.rs".to_string(),
        "node_modules/pkg/index.js".to_string(),
    ];
    assert!(!ignored.unanalyzed_working_tree);
    assert_eq!(
        verify_line(&ignored)?,
        "verify=complete|outcome=complete_no_findings|findings=0|exposed=0"
    );
    let mut both = outcome_output(CompleteNoFindings, Vec::new())?;
    both.untracked_working_tree_source_paths = vec!["src/new.rs".to_string()];
    both.ignored_working_tree_source_paths = vec!["src/gen.rs".to_string()];
    assert!(
        verify_line(&both)?.ends_with("reason=untracked_source_excluded"),
        "only the untracked condition is named"
    );
    Ok(())
}

#[test]
fn verification_record_refuses_a_partial_scope_run() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::CompleteNoFindings;
    let mut partial = outcome_output(CompleteNoFindings, Vec::new())?;
    partial.partial_scope = Some(crate::analysis::PartialDiffScope {
        run_status: crate::analysis::PartialDiffScope::RUN_STATUS.to_string(),
        diff_identity: "sha256:diff".to_string(),
        file_budget: 1,
        line_budget: 1,
        budget_disclosures: Vec::new(),
        selected_files: vec!["src/a.rs".to_string()],
        unselected_files: vec!["src/b.rs".to_string()],
        selected_changed_lines: 1,
        uninspected_files_lower_bound: 1,
        uninspected_changed_lines_lower_bound: 1,
        stop_reason: crate::analysis::PartialDiffStopReason::FileBudget,
        next_file_changed_lines: Some(1),
        whole_diff_budgets: None,
        partition_identity: "sha256:partition".to_string(),
    });
    assert_eq!(
        verify_line(&partial)?,
        "verify=refused|outcome=complete_no_findings|findings=0|exposed=0|reason=partial_scope"
    );
    Ok(())
}

#[test]
fn verification_record_follows_every_terminal_and_is_the_last_line() -> Result<(), String> {
    use crate::analysis_outcome::AnalysisOutcomeKind::{
        CompleteWithFindings, PartialWithLimitations,
    };
    for output in [
        outcome_output(PartialWithLimitations, Vec::new())?,
        outcome_output(CompleteWithFindings, vec![weak_gap("probe:gap", 2)])?,
        outcome_output(
            CompleteWithFindings,
            vec![finding("probe:lim", ExposureClass::NoStaticPath, 3)],
        )?,
        output(Vec::new()),
    ] {
        let text = agent_dispatch(&output, &RiprConfig::default())?;
        let last = text.lines().last().ok_or("empty report")?;
        assert!(last.starts_with("verify="), "{text}");
        assert_eq!(text.matches("verify=").count(), 1, "{text}");
    }
    // The explain drill-in is not a completion surface and carries no record.
    assert!(!explained(&weak_gap("probe:gap", 2)).contains("verify="));
    Ok(())
}
