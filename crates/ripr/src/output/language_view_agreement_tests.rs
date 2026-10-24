//! Cross-surface agreement on mixed-language fixtures (#6475).
//!
//! One sealed aggregate per fixture feeds the shared view; the same facts
//! then reach the human renderer, the JSON renderer and the gate input
//! predicates. Each surface must disclose exactly what the view says, valid
//! sibling findings must stay visible, and the gate must accept only an input
//! the aggregate calls complete. The `AnalysisOutcome` kind and limitations a
//! pipeline would pair with each fixture are stated in the fixture, not
//! derived from the code under test.

use crate::analysis::LanguageRunStatus;
use crate::analysis::language_outcome::{
    DiffCompleteness, FixtureDiffResult, FixtureRepoResult, LanguageAggregate,
    LanguageAnalysisOutcomeV1, LanguageCompletenessView, LanguageDisclosure, LanguageOutcomeStatus,
    OutcomeMode, reconcile,
};
use crate::analysis_outcome::{
    AnalysisIdentity, AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcome,
    AnalysisOutcomeCounts, AnalysisOutcomeKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage,
};
use crate::app::{CheckOutput, Mode};
use crate::domain::{Finding, LanguageId};
use crate::output::gate::{discloses_incomplete_analysis_outcome, discloses_limited_partial_scope};
use std::collections::BTreeSet;
use std::path::PathBuf;

// Of the two blocking perl rows the aggregate keeps the not-selected one (digest
// order), which owns no public run; the literal below states that.
const PERL_DUAL_RUNS: &[(&str, &str)] = &[];

struct Fixture {
    name: &'static str,
    repo: bool,
    /// The language_runs rows the public surface must carry.
    runs: &'static [(&'static str, &'static str)],
    children: Vec<LanguageAnalysisOutcomeV1>,
    selected: Vec<LanguageId>,
    kind: AnalysisOutcomeKind,
    limitations: Vec<AnalysisLimitationKind>,
}

fn rust_finding() -> Result<Finding, String> {
    let mut finding = crate::mcp::gaps::test_finding()?;
    finding.language = Some(LanguageId::Rust);
    Ok(finding)
}

fn diff_children(
    language: LanguageId,
    findings: Vec<Finding>,
    kept: &[String],
) -> Vec<LanguageAnalysisOutcomeV1> {
    let result = FixtureDiffResult {
        findings,
        changed_files: 1,
        ..FixtureDiffResult::default()
    };
    let touched: BTreeSet<String> = BTreeSet::from([language.as_str().to_string()]);
    LanguageAnalysisOutcomeV1::from_diff_result(language, &result, kept, &touched, "full")
}

fn err_child(language: LanguageId, status: LanguageRunStatus) -> LanguageAnalysisOutcomeV1 {
    LanguageAnalysisOutcomeV1::from_diff_err(language, "boom".to_string(), status, "full")
}

fn rust_complete() -> Result<Vec<LanguageAnalysisOutcomeV1>, String> {
    Ok(diff_children(LanguageId::Rust, vec![rust_finding()?], &[]))
}

fn fixtures() -> Result<Vec<Fixture>, String> {
    let both = vec![LanguageId::Rust, LanguageId::Python];
    let with_rust = |extra: Vec<LanguageAnalysisOutcomeV1>| -> Result<Vec<_>, String> {
        let mut children = rust_complete()?;
        children.extend(extra);
        Ok(children)
    };
    Ok(vec![
        Fixture {
            name: "complete",
            repo: false,
            runs: &[],
            children: with_rust(diff_children(LanguageId::Python, Vec::new(), &[]))?,
            selected: both.clone(),
            kind: AnalysisOutcomeKind::CompleteWithFindings,
            limitations: Vec::new(),
        },
        Fixture {
            name: "complete_zero",
            repo: false,
            runs: &[],
            children: diff_children(LanguageId::Rust, Vec::new(), &[]),
            selected: vec![LanguageId::Rust],
            kind: AnalysisOutcomeKind::CompleteNoFindings,
            limitations: Vec::new(),
        },
        Fixture {
            name: "partial",
            repo: false,
            runs: &[],
            children: with_rust(diff_children(
                LanguageId::Python,
                Vec::new(),
                &["packet rejected".to_string()],
            ))?,
            selected: both.clone(),
            kind: AnalysisOutcomeKind::PartialWithLimitations,
            limitations: vec![AnalysisLimitationKind::ProducerFailure],
        },
        Fixture {
            name: "unavailable",
            repo: false,
            runs: &[("python", "unavailable")],
            children: with_rust(vec![err_child(
                LanguageId::Python,
                LanguageRunStatus::Unavailable,
            )])?,
            selected: both.clone(),
            kind: AnalysisOutcomeKind::PartialWithLimitations,
            limitations: vec![AnalysisLimitationKind::LanguageAdapterUnavailable],
        },
        Fixture {
            name: "invalid",
            repo: false,
            runs: &[("python", "invalid")],
            children: with_rust(vec![err_child(
                LanguageId::Python,
                LanguageRunStatus::Invalid,
            )])?,
            selected: both.clone(),
            kind: AnalysisOutcomeKind::PartialWithLimitations,
            limitations: vec![AnalysisLimitationKind::LanguageAdapterUnavailable],
        },
        Fixture {
            name: "failed",
            repo: false,
            runs: &[("python", "failed")],
            children: with_rust(vec![err_child(
                LanguageId::Python,
                LanguageRunStatus::Failed,
            )])?,
            selected: both.clone(),
            kind: AnalysisOutcomeKind::PartialWithLimitations,
            limitations: vec![AnalysisLimitationKind::ProducerFailure],
        },
        Fixture {
            name: "unsupported",
            repo: false,
            runs: &[],
            children: with_rust(vec![LanguageAnalysisOutcomeV1::unsupported(
                "Go", 2, "full",
            )])?,
            selected: vec![LanguageId::Rust],
            kind: AnalysisOutcomeKind::UnsupportedInput,
            limitations: vec![AnalysisLimitationKind::LanguageScopeUnsupported],
        },
        Fixture {
            name: "repo_partial_with_not_selected",
            repo: true,
            runs: &[("python", "partial")],
            children: vec![
                LanguageAnalysisOutcomeV1::from_repo_result(
                    LanguageId::Rust,
                    &FixtureRepoResult {
                        findings: vec![rust_finding()?],
                        production_files: 1,
                        ..FixtureRepoResult::default()
                    },
                    None,
                ),
                LanguageAnalysisOutcomeV1::from_repo_result(
                    LanguageId::Python,
                    &FixtureRepoResult {
                        production_files: 1,
                        partial_reason: Some("file cap reached".to_string()),
                        ..FixtureRepoResult::default()
                    },
                    None,
                ),
                LanguageAnalysisOutcomeV1::repo_not_selected(
                    "typescript",
                    2,
                    "language not in the effective [languages].enabled set",
                ),
            ],
            selected: both.clone(),
            kind: AnalysisOutcomeKind::PartialWithLimitations,
            limitations: vec![AnalysisLimitationKind::LanguageAdapterUnavailable],
        },
        Fixture {
            name: "configured_uncompiled_perl_dual_row",
            repo: false,
            runs: PERL_DUAL_RUNS,
            children: with_rust(vec![
                err_child(LanguageId::Perl, LanguageRunStatus::Unavailable),
                LanguageAnalysisOutcomeV1::not_selected(
                    "perl",
                    1,
                    "adapter not compiled into this ripr binary",
                    "full",
                ),
            ])?,
            selected: vec![LanguageId::Rust],
            kind: AnalysisOutcomeKind::PartialWithLimitations,
            limitations: vec![AnalysisLimitationKind::LanguageAdapterUnavailable],
        },
    ])
}

fn aggregate_for(fixture: &Fixture) -> LanguageAggregate {
    let mut aggregate = if fixture.repo {
        LanguageAggregate::new_repo()
    } else {
        LanguageAggregate::new("full".to_string())
    };
    for child in &fixture.children {
        aggregate.record(child.clone());
    }
    aggregate.finish()
}

fn aggregate_of(children: &[LanguageAnalysisOutcomeV1]) -> LanguageAggregate {
    let mut aggregate = LanguageAggregate::new("full".to_string());
    for child in children {
        aggregate.record(child.clone());
    }
    aggregate.finish()
}

fn analysis_outcome(fixture: &Fixture, findings: u64) -> Result<AnalysisOutcome, String> {
    let limitations = fixture
        .limitations
        .iter()
        .map(|kind| {
            Ok(AnalysisLimitation::new(
                *kind,
                AnalysisStage::LanguageAdapter,
                AnalysisRecovery::new(AnalysisRecoveryKind::Retry, "Re-run the analysis.")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    AnalysisOutcome::new(
        fixture.kind,
        AnalysisIdentity::default(),
        AnalysisOutcomeCounts {
            changed_file_count: 1,
            changed_line_count: 1,
            candidate_line_count: 1,
            probe_count: findings,
            finding_count: findings,
        },
        limitations,
    )
}

fn check_output(
    findings: Vec<Finding>,
    runs: Vec<crate::analysis::LanguageRun>,
    outcome: AnalysisOutcome,
) -> CheckOutput {
    CheckOutput {
        harness_projections: Vec::new(),
        schema_version: crate::app::CHECK_OUTPUT_SCHEMA_VERSION.to_string(),
        tool: "ripr".to_string(),
        mode: Mode::Draft,
        root: PathBuf::from("."),
        base: Some("origin/main".to_string()),
        summary: crate::domain::Summary::default(),
        findings,
        preview_language_advisories: Vec::new(),
        language_runs: runs,
        no_scope_provided: false,
        unanalyzed_working_tree: false,
        untracked_working_tree_source_paths: Vec::new(),
        ignored_working_tree_source_paths: Vec::new(),
        unlinked_python_tests: None,
        suppression: None,
        analysis_outcome: Some(outcome),
        partial_scope: None,
        analyzed_revisions: None,
    }
}

#[test]
fn every_surface_discloses_what_the_view_says() -> Result<(), String> {
    for fixture in fixtures()? {
        let name = fixture.name;
        let aggregate = aggregate_for(&fixture);
        let produced = aggregate
            .outcomes
            .values()
            .map(LanguageAnalysisOutcomeV1::finding_count)
            .sum::<usize>();
        let findings = (0..produced)
            .map(|_| rust_finding())
            .collect::<Result<Vec<_>, _>>()?;
        let outcome = analysis_outcome(&fixture, findings.len() as u64)?;
        // The fixture itself is a coherent, reconciling run.
        reconcile(&aggregate, &fixture.selected, &findings, Some(&outcome))
            .map_err(|error| format!("{name}: {error}"))?;
        let view = LanguageCompletenessView::from_aggregate(&aggregate);
        view.reconcile_public_runs(&view.public_runs)
            .map_err(|error| format!("{name}: {error}"))?;

        let expected: Vec<(String, String)> = fixture
            .runs
            .iter()
            .map(|(language, status)| (language.to_string(), status.to_string()))
            .collect();
        let actual: Vec<(String, String)> = view
            .public_runs
            .iter()
            .map(|run| (run.language.clone(), run.status.as_str().to_string()))
            .collect();
        assert_eq!(actual, expected, "{name}");

        let output = check_output(findings, view.public_runs.clone(), outcome);
        let human = crate::output::human::render(&output);
        let json = crate::output::json::render(&output);
        let value: serde_json::Value =
            serde_json::from_str(&json).map_err(|error| format!("{name}: {error}"))?;

        // Human: one status note per public run, naming language and status.
        assert_eq!(
            human.matches("(status: ").count(),
            view.public_runs.len(),
            "{name}: {human}"
        );
        for run in &view.public_runs {
            let display = crate::domain::LanguageId::display_name_for_wire(&run.language)
                .ok_or("unknown language")?;
            let status = format!("(status: {})", run.status.as_str());
            assert!(
                human
                    .lines()
                    .any(|line| line.contains(display) && line.contains(&status)),
                "{name}: {human}"
            );
        }

        // JSON: `language_runs` is the view's projection, absent when empty.
        let json_runs: Vec<(String, String)> = value
            .get("language_runs")
            .and_then(serde_json::Value::as_array)
            .map(|runs| {
                runs.iter()
                    .map(|run| {
                        (
                            run["language"].as_str().unwrap_or_default().to_string(),
                            run["status"].as_str().unwrap_or_default().to_string(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        let view_runs: Vec<(String, String)> = view
            .public_runs
            .iter()
            .map(|run| (run.language.clone(), run.status.as_str().to_string()))
            .collect();
        assert_eq!(json_runs, view_runs, "{name}");

        // Unsupported scope cannot disappear: the typed limitation reaches
        // the JSON outcome whenever the view says it must.
        let wants_unsupported = view
            .rows
            .iter()
            .any(|row| row.status == LanguageOutcomeStatus::Unsupported);
        assert_eq!(
            json.contains("language_scope_unsupported"),
            wants_unsupported,
            "{name}"
        );

        // A valid sibling's findings stay visible beside any failed adapter.
        assert_eq!(
            value["findings"].as_array().map(Vec::len),
            Some(view.total_findings),
            "{name}"
        );

        // The gate accepts an input only when the aggregate calls it complete.
        let gate_refuses = discloses_limited_partial_scope(&value)
            || discloses_incomplete_analysis_outcome(&value);
        assert_eq!(
            gate_refuses,
            view.completeness == DiffCompleteness::Incomplete,
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn every_selected_language_has_a_row_and_rows_mirror_the_aggregate() -> Result<(), String> {
    for fixture in fixtures()? {
        let aggregate = aggregate_for(&fixture);
        let view = LanguageCompletenessView::from_aggregate(&aggregate);
        for language in &fixture.selected {
            assert!(
                view.rows.iter().any(|row| row.label == language.as_str()),
                "{}: {} has no row",
                fixture.name,
                language.as_str()
            );
        }
        // One row per aggregate child, with the child's own status and
        // finding count; the view never adds, drops or strengthens one.
        let rows: Vec<(&str, LanguageOutcomeStatus, usize)> = view
            .rows
            .iter()
            .map(|row| (row.label.as_str(), row.status, row.findings))
            .collect();
        let children: Vec<(&str, LanguageOutcomeStatus, usize)> = aggregate
            .outcomes
            .iter()
            .map(|(label, child)| (label.as_str(), child.status, child.finding_count()))
            .collect();
        assert_eq!(rows, children, "{}", fixture.name);
        assert_eq!(
            view.total_findings,
            rows.iter().map(|row| row.2).sum::<usize>(),
            "{}",
            fixture.name
        );
    }
    Ok(())
}

#[test]
fn a_hidden_degradation_or_phantom_run_is_rejected() -> Result<(), String> {
    let fixtures = fixtures()?;
    let unavailable = fixtures
        .iter()
        .find(|fixture| fixture.name == "unavailable")
        .ok_or("missing fixture")?;
    let view = LanguageCompletenessView::from_aggregate(&aggregate_of(&unavailable.children));
    // The degraded adapter's public row was dropped by a surface.
    let hidden = view.reconcile_public_runs(&[]).err().ok_or("accepted")?;
    assert!(hidden.contains("python is unavailable"), "{hidden}");
    // A surface invented a run for a language the aggregate never recorded.
    let mut invented = view.public_runs.clone();
    invented.push(crate::analysis::LanguageRun {
        language: "perl".to_string(),
        status: LanguageRunStatus::Failed,
        reason: None,
    });
    let phantom = view
        .reconcile_public_runs(&invented)
        .err()
        .ok_or("accepted")?;
    assert!(phantom.contains("perl"), "{phantom}");
    // A row reported with a different status is a hidden degradation too.
    let weaker = vec![crate::analysis::LanguageRun {
        language: "python".to_string(),
        status: LanguageRunStatus::Partial,
        reason: None,
    }];
    assert!(view.reconcile_public_runs(&weaker).is_err());
    Ok(())
}

#[test]
fn partial_is_a_run_row_in_repo_mode_and_a_typed_limitation_in_diff_mode() -> Result<(), String> {
    assert_eq!(
        LanguageDisclosure::of(OutcomeMode::Repo, LanguageOutcomeStatus::Partial),
        LanguageDisclosure::PublicRun("partial")
    );
    assert_eq!(
        LanguageDisclosure::of(OutcomeMode::Diff, LanguageOutcomeStatus::Partial),
        LanguageDisclosure::TypedLimitation
    );
    // The diff fixture's partial child therefore owns no public run row, and
    // the view says so instead of inventing one.
    let fixtures = fixtures()?;
    let partial = fixtures
        .iter()
        .find(|fixture| fixture.name == "partial")
        .ok_or("missing fixture")?;
    let view = LanguageCompletenessView::from_aggregate(&aggregate_of(&partial.children));
    assert!(view.public_runs.is_empty());
    assert_eq!(view.completeness, DiffCompleteness::Incomplete);
    Ok(())
}

#[test]
fn an_extra_run_for_a_known_label_is_tolerated() -> Result<(), String> {
    // An out-of-scope optional adapter failure may keep a public run over a
    // row the view discloses silently; only an unknown label is a phantom.
    let fixtures = fixtures()?;
    let complete = fixtures
        .iter()
        .find(|fixture| fixture.name == "complete")
        .ok_or("missing fixture")?;
    let view = LanguageCompletenessView::from_aggregate(&aggregate_for(complete));
    assert!(view.public_runs.is_empty());
    let extra = vec![crate::analysis::LanguageRun {
        language: "python".to_string(),
        status: LanguageRunStatus::Failed,
        reason: None,
    }];
    view.reconcile_public_runs(&extra)?;
    Ok(())
}

#[test]
fn disclosure_is_a_fixed_table_over_every_mode_and_status() {
    use LanguageDisclosure::{AdvisoryOnly, PublicRun, Silent, TypedLimitation};
    use LanguageOutcomeStatus as S;
    use OutcomeMode::{Diff, Repo};
    let table = [
        (Diff, S::Complete, Silent),
        (Diff, S::CompleteZero, Silent),
        (Diff, S::Partial, TypedLimitation),
        (Diff, S::Unavailable, PublicRun("unavailable")),
        (Diff, S::Invalid, PublicRun("invalid")),
        (Diff, S::Failed, PublicRun("failed")),
        (Diff, S::Unsupported, TypedLimitation),
        (Diff, S::NotSelected, AdvisoryOnly),
        (Diff, S::NoCurrentScope, Silent),
        (Repo, S::Complete, Silent),
        (Repo, S::CompleteZero, Silent),
        (Repo, S::Partial, PublicRun("partial")),
        (Repo, S::Unavailable, PublicRun("unavailable")),
        (Repo, S::Invalid, PublicRun("invalid")),
        (Repo, S::Failed, PublicRun("failed")),
        (Repo, S::Unsupported, TypedLimitation),
        (Repo, S::NotSelected, AdvisoryOnly),
        (Repo, S::NoCurrentScope, Silent),
    ];
    for (mode, status, expected) in table {
        assert_eq!(
            LanguageDisclosure::of(mode, status),
            expected,
            "{mode:?} {status:?}"
        );
    }
}

#[test]
fn a_duplicate_perl_label_is_order_independent_and_never_hides_a_blocker() {
    let unavailable = err_child(LanguageId::Perl, LanguageRunStatus::Unavailable);
    let not_selected = LanguageAnalysisOutcomeV1::not_selected(
        "perl",
        1,
        "adapter not compiled into this ripr binary",
        "full",
    );
    let forward = aggregate_of(&[unavailable.clone(), not_selected.clone()]);
    let backward = aggregate_of(&[not_selected, unavailable]);
    assert_eq!(forward.semantic_digest, backward.semantic_digest);
    let view = LanguageCompletenessView::from_aggregate(&forward);
    assert_eq!(view.rows.len(), 1);
    // The deterministic digest order keeps the not-selected row, so the view
    // owns no public run for perl. The pipeline still reports the adapter's
    // own unavailable run, which the view tolerates as an extra run of a
    // known label (it cannot represent both rows; recorded on #6475).
    assert_eq!(view.rows[0].status, LanguageOutcomeStatus::NotSelected);
    assert!(view.public_runs.is_empty());
    let pipeline_run = crate::analysis::LanguageRun {
        language: "perl".to_string(),
        status: LanguageRunStatus::Unavailable,
        reason: None,
    };
    assert_eq!(view.reconcile_public_runs(&[pipeline_run]), Ok(()));
    assert_eq!(view.completeness, DiffCompleteness::Incomplete);
}
