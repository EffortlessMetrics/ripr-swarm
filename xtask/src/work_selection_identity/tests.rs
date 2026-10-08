//! Tests for the shared selected-work identity schemas (#1706 PR A,
//! RIPR-SPEC-0244). Every test name carries the `work_selection_identity`
//! prefix so the suite is greppable as one delivery slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::{
    compile_work_portfolio, compile_work_portfolio_corpus, load_work_captured_dir,
};

fn committed_check() -> Result<WorkSelectionCheckViewV1, String> {
    let corpus = committed_corpus()?;
    run_work_selection_check(DEFAULT_WORK_SELECTION_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkSelectionCheckViewV1,
    id: &str,
) -> Result<&'a WorkSelectionScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkSelectionCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkSelectionCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkSelectionCheckViewV1,
    id: &str,
    index: usize,
    law: WorkSelectionLawV1,
    route: WorkSelectionRouteV1,
) -> Result<(), String> {
    let case = case(view, id, index)?;
    if case
        .violations
        .iter()
        .any(|violation| violation.law == law && violation.route == route)
    {
        Ok(())
    } else {
        Err(format!(
            "scenario `{id}` case {index} has violations {:?}, expected law {:?} route {:?}",
            case.violations, law, route
        ))
    }
}

/// Fixtures 1-12 + acceptance 1/3/4/9: the committed corpus compiles, all
/// twelve required scenarios are present and every case meets its pinned
/// expectation.
#[test]
fn work_selection_identity_committed_corpus_twelve_scenarios_hold() -> Result<(), String> {
    let view = committed_check()?;
    if view.counts.scenarios != 12 {
        return Err(format!(
            "expected twelve scenarios, got {}",
            view.counts.scenarios
        ));
    }
    if view.counts.cases != 19 || view.counts.passed != 19 || view.counts.failed != 0 {
        return Err(format!(
            "expected 19/19 passed cases, got {:?}",
            view.counts
        ));
    }
    Ok(())
}

/// Acceptance 8 + fixture 9: wrong repository, issue, action, basis,
/// worktree, head and selection-id identities each fail visibly with their
/// exact recompile/reconcile route.
#[test]
fn work_selection_identity_wrong_identities_fail_visibly_with_exact_routes() -> Result<(), String> {
    let view = committed_check()?;
    let expected = [
        (
            0,
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
        (
            1,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            2,
            WorkSelectionLawV1::ActionIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            3,
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
        (
            4,
            WorkSelectionLawV1::WorktreeIdentity,
            WorkSelectionRouteV1::ReconcileResources,
        ),
        (
            5,
            WorkSelectionLawV1::HeadIdentity,
            WorkSelectionRouteV1::ReconcileHead,
        ),
        (
            6,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
    ];
    for (index, law, route) in expected {
        has_violation(
            &view,
            "scenario-09-wrong-identities-fail-visibly",
            index,
            law,
            route,
        )?;
        let case = case(&view, "scenario-09-wrong-identities-fail-visibly", index)?;
        if case.violations.is_empty() {
            return Err(format!("wrong-identity case {index} must fail visibly"));
        }
    }
    // Case 6 pins the selection-id law specifically: the id named another
    // issue while every other identity stayed clean.
    let selection_detail = case(&view, "scenario-09-wrong-identities-fail-visibly", 6)?
        .violations
        .iter()
        .find(|violation| violation.law == WorkSelectionLawV1::SubjectIdentity)
        .map(|violation| violation.detail.clone())
        .ok_or_else(|| "missing selection-id violation detail".to_string())?;
    if !selection_detail.contains("selection id") {
        return Err(format!(
            "selection-id violation must name the selection id: {selection_detail}"
        ));
    }
    // A stale basis routes recompilation, never silent release or takeover.
    let detail = case(&view, "scenario-09-wrong-identities-fail-visibly", 3)?
        .violations
        .iter()
        .find(|violation| violation.law == WorkSelectionLawV1::BasisIdentity)
        .map(|violation| violation.detail.clone())
        .ok_or_else(|| "missing basis violation detail".to_string())?;
    if !detail.contains("recompile the basis") {
        return Err(format!(
            "basis violation must name the recompile route: {detail}"
        ));
    }
    Ok(())
}

/// Review hardening: subject-binding laws fail closed. A pull-request
/// subject linked to different work, a bare work-item subject, an invented
/// scope reference and a selection id naming another issue each fail
/// `subject_identity`, and duplicate-family siblings must stay visible.
#[test]
fn work_selection_identity_subject_binding_laws_fail_closed() -> Result<(), String> {
    let captured = load_work_captured_dir(&workspace_path("fixtures/work_portfolio/corpus"))?;
    let snapshot = compile_work_portfolio(&captured, None)?;
    let basis = build_portfolio_basis(&snapshot, "explicit-root-selection", "v1");
    let corpus = committed_corpus()?;
    let clean = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "scenario-05-existing-open-pr-selected-for-repair")
        .and_then(|scenario| scenario.cases.first())
        .and_then(|case| case.packet.clone())
        .ok_or_else(|| "committed corpus must carry the scenario 5 packet".to_string())?;
    if !check_packet(&clean, &basis, &snapshot, &captured)
        .violations
        .is_empty()
    {
        return Err("the clean scenario 5 packet must pass every law".to_string());
    }

    // A PR opened for different work cannot carry the selection.
    let mut wrong_pr = clean.clone();
    wrong_pr.pull_request = Some(8802);
    let evaluation = check_packet(&wrong_pr, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|row| row.law == WorkSelectionLawV1::SubjectIdentity)
    {
        return Err("a PR linked to another issue must fail subject_identity".to_string());
    }

    // A bare work-item subject has no captured source of record.
    let mut bare_work_item = clean.clone();
    bare_work_item.issue = None;
    bare_work_item.pull_request = None;
    bare_work_item.work_item = Some(SelectedWorkItemIdentityV1 {
        id: "durable-1".to_string(),
        identity: work_item_identity("durable-1"),
    });
    let evaluation = check_packet(&bare_work_item, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|row| row.law == WorkSelectionLawV1::SubjectIdentity)
    {
        return Err("a bare work-item subject must fail subject_identity".to_string());
    }

    // Scope references must stay source-linked.
    let mut invented_scope = clean.clone();
    invented_scope.accepted_requirements = vec!["REQ-invented".to_string()];
    let evaluation = check_packet(&invented_scope, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|row| row.law == WorkSelectionLawV1::SubjectIdentity)
    {
        return Err("an invented requirement must fail subject_identity".to_string());
    }

    // A selection id naming another issue contradicts the packet.
    let mut wrong_selection = clean.clone();
    wrong_selection.selection_id = "selection:repair_review:issue:9102".to_string();
    let evaluation = check_packet(&wrong_selection, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|row| row.law == WorkSelectionLawV1::SubjectIdentity)
    {
        return Err("a wrong selection id must fail subject_identity".to_string());
    }

    // Duplicate-family siblings are live overlaps: dropping 9106 from the
    // scenario 5-style overlap set of a 9105 packet must surface
    // overlap_visibility.
    let mut hidden_sibling = clean.clone();
    hidden_sibling.issue = Some(SelectedIssueIdentityV1 {
        number: 9105,
        identity: issue_identity(9105),
    });
    hidden_sibling.candidate_id = candidate_identity(9105);
    hidden_sibling.lifecycle_action = WorkCandidateKindV1::StartBuild;
    hidden_sibling.selection_id = selection_identity(WorkCandidateKindV1::StartBuild, 9105);
    hidden_sibling.pull_request = None;
    hidden_sibling.overlaps.issues = Vec::new();
    let evaluation = check_packet(&hidden_sibling, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|row| row.law == WorkSelectionLawV1::OverlapVisibility)
    {
        return Err("a hidden duplicate-family sibling must fail overlap_visibility".to_string());
    }
    Ok(())
}

/// The human Markdown projection renders the same wire names as the JSON
/// projection, never the Rust `Debug` spellings.
#[test]
fn work_selection_identity_markdown_renders_wire_names() -> Result<(), String> {
    let view = committed_check()?;
    let markdown = work_selection_check_markdown(&view);
    for debug_name in [
        "RepositoryIdentity",
        "SubjectIdentity",
        "ActionIdentity",
        "BasisIdentity",
        "HeadIdentity",
        "WorktreeIdentity",
        "CampaignRefIdentity",
        "LegacyCompatibility",
        "OverlapVisibility",
        "RecompileBasis",
        "ReconcileSelection",
        "ReconcileResources",
        "ReconcileHead",
        "RejectLegacyAuthority",
        "Complete",
        "Partial",
    ] {
        if markdown.contains(debug_name) {
            return Err(format!(
                "markdown must render wire names, found Debug spelling `{debug_name}`"
            ));
        }
    }
    for wire_name in ["overlap_visibility", "reconcile_selection"] {
        if !markdown.contains(wire_name) {
            return Err(format!("markdown must render the `{wire_name}` wire name"));
        }
    }
    Ok(())
}

/// Scenario captured directories resolve corpus-locally first, fall back to
/// the repository fixtures root, and fail closed on traversal, absolute and
/// backslash spellings on every host platform.
#[test]
fn work_selection_identity_scenario_captured_resolution() -> Result<(), String> {
    let corpus_root = workspace_path(DEFAULT_WORK_SELECTION_CORPUS_DIR);
    let local = resolve_scenario_captured(&corpus_root, "multi-campaign")?;
    if !local.is_dir() || !local.ends_with(Path::new("captured").join("multi-campaign")) {
        return Err(format!(
            "corpus-local captured variant must resolve under the corpus: {}",
            local.display()
        ));
    }
    let fallback = resolve_scenario_captured(&corpus_root, "work_portfolio/corpus")?;
    if fallback != workspace_path("fixtures").join("work_portfolio/corpus") {
        return Err(format!(
            "foreign captured paths must fall back to the fixtures root: {}",
            fallback.display()
        ));
    }
    for bad in ["../escape", "a/../../escape", "/abs/corpus", "a\\b", ""] {
        if resolve_scenario_captured(&corpus_root, bad).is_ok() {
            return Err(format!("captured path `{bad}` must fail closed"));
        }
    }
    Ok(())
}

/// Spec-ref wire shape: exactly `RIPR-SPEC-` + four digits, optionally a
/// `-<slug>` suffix; anything else fails closed.
#[test]
fn work_selection_identity_spec_ref_wire_shape() -> Result<(), String> {
    for accepted in ["RIPR-SPEC-0202", "RIPR-SPEC-0244-selected-work-identity"] {
        if !spec_ref_has_canonical_shape(accepted) {
            return Err(format!(
                "`{accepted}` must satisfy the canonical wire shape"
            ));
        }
    }
    for rejected in [
        "RIPR-SPEC-202",
        "RIPR-SPEC-02025",
        "RIPR-SPEC-0202x",
        "SPEC-0202",
        "RIPR-SPEC-0202-",
        "ripr-spec-0202",
    ] {
        if spec_ref_has_canonical_shape(rejected) {
            return Err(format!("`{rejected}` must fail the canonical wire shape"));
        }
    }
    Ok(())
}

/// The committed provenance covers every corpus byte: the shared validator
/// must report no violation for the committed corpus (digests bind, no
/// unlisted fixture file bypasses the gate).
#[test]
fn work_selection_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_selection_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Fixture 7 + acceptance 8: a changed portfolio snapshot keeps a compatible
/// selection while the snapshot identity itself moves.
#[test]
fn work_selection_identity_snapshot_change_keeps_compatible_issue_head() -> Result<(), String> {
    let view = committed_check()?;
    let first = scenario(&view, "scenario-01-selected-issue-one-campaign")?;
    let seventh = scenario(&view, "scenario-07-snapshot-change-issue-head-compatible")?;
    if first.portfolio_identity == seventh.portfolio_identity {
        return Err(
            "the stale-local variant must change the portfolio snapshot identity".to_string(),
        );
    }
    if first.basis_sha != seventh.basis_sha {
        return Err("the compatible snapshot change must keep the default-branch head".to_string());
    }
    if seventh.snapshot_completeness == WorkConfidenceV1::Complete {
        return Err("the stale-local variant must degrade snapshot completeness".to_string());
    }
    if case(
        &view,
        "scenario-07-snapshot-change-issue-head-compatible",
        0,
    )?
    .status
        != "passed"
    {
        return Err("the compatible selection must stay selected".to_string());
    }
    Ok(())
}

/// Fixture 8 + acceptance 5: a portfolio change that introduces an
/// overlapping PR forces reconcile; the hidden overlap is named.
#[test]
fn work_selection_identity_new_overlap_forces_reconcile() -> Result<(), String> {
    let view = committed_check()?;
    let case = case(&view, "scenario-08-overlap-introduced-forces-reconcile", 0)?;
    if case.status != "passed" || case.violations.is_empty() {
        return Err("scenario 8 must meet its pinned failure expectation".to_string());
    }
    let violation = case
        .violations
        .iter()
        .find(|violation| violation.law == WorkSelectionLawV1::OverlapVisibility)
        .ok_or_else(|| "missing overlap violation".to_string())?;
    if !violation.detail.contains("#8899") {
        return Err(format!(
            "overlap violation must name the introduced PR: {}",
            violation.detail
        ));
    }
    Ok(())
}

/// Acceptance 7 + fixture 6: legacy refs resolve read-only with migration
/// posture and grant no authority; a legacy writer-authorization packet
/// fails with the exact rejection route.
#[test]
fn work_selection_identity_legacy_refs_read_only_no_authority() -> Result<(), String> {
    let view = committed_check()?;
    let legacy = case(&view, "scenario-06-legacy-writer-authorization-fails", 0)?;
    if legacy.status != "passed" {
        return Err("scenario 6 must meet its pinned failure expectation".to_string());
    }
    let violation = legacy
        .violations
        .iter()
        .find(|violation| {
            violation.law == WorkSelectionLawV1::LegacyCompatibility
                && violation.route == WorkSelectionRouteV1::RejectLegacyAuthority
        })
        .ok_or_else(|| "legacy writer grant must fail with reject_legacy_authority".to_string())?;
    if !violation.detail.contains("writer") {
        return Err(format!(
            "legacy rejection must name the attempted authority: {}",
            violation.detail
        ));
    }
    // Scenario 11: a modern packet carrying legacy refs stays valid, emits
    // migration posture, and still resolves to a stable selection identity.
    let resumed = case(&view, "scenario-11-fresh-root-resume-from-artifacts", 0)?;
    if resumed.status != "passed" || !resumed.violations.is_empty() {
        return Err("scenario 11 must pass with read-only legacy refs".to_string());
    }
    let posture = resumed
        .migration_posture
        .iter()
        .find(|line| line.contains("legacy_active_goal_ref"))
        .ok_or_else(|| "scenario 11 must emit migration posture".to_string())?;
    if !posture.contains("no write, readiness, merge or closeout authority") {
        return Err(format!(
            "migration posture must record the no-authority grant: {posture}"
        ));
    }
    if resumed.selection_identity.is_none() {
        return Err("scenario 11 must resolve a stable selection identity".to_string());
    }
    Ok(())
}

/// Acceptance 4 + fixtures 2/3/4/12: standalone work with zero campaigns,
/// multi-campaign membership, independent disjoint selections, and the
/// narrow single-agent issue all stay representable without any default
/// campaign.
#[test]
fn work_selection_identity_campaign_representations_without_default() -> Result<(), String> {
    let view = committed_check()?;
    if case(&view, "scenario-03-standalone-no-campaign-placement", 0)?.status != "passed" {
        return Err("standalone work with zero campaign refs must pass".to_string());
    }
    if case(
        &view,
        "scenario-02-selected-issue-several-related-campaigns",
        0,
    )?
    .status
        != "passed"
    {
        return Err("multi-campaign membership must pass".to_string());
    }
    let fourth = scenario(
        &view,
        "scenario-04-two-campaigns-independent-disjoint-resources",
    )?;
    if fourth.cases.len() != 2 || fourth.cases.iter().any(|case| case.status != "passed") {
        return Err("scenario 4 must carry two passing independent selections".to_string());
    }
    let corpus = committed_corpus()?;
    let fourth_entry = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == fourth.scenario_id)
        .ok_or_else(|| "missing scenario 4 corpus entry".to_string())?;
    let mut resources: Vec<BTreeSet<String>> = Vec::new();
    let mut campaigns: Vec<BTreeSet<String>> = Vec::new();
    for case in &fourth_entry.cases {
        let packet = case
            .packet
            .as_ref()
            .ok_or_else(|| "scenario 4 cases must carry packets".to_string())?;
        resources.push(packet.overlaps.resources.iter().cloned().collect());
        campaigns.push(
            packet
                .campaign_refs
                .iter()
                .map(|campaign_ref| campaign_ref.id.clone())
                .collect(),
        );
    }
    let shared_resources: BTreeSet<String> =
        resources[0].intersection(&resources[1]).cloned().collect();
    let shared_campaigns: BTreeSet<String> =
        campaigns[0].intersection(&campaigns[1]).cloned().collect();
    if !shared_resources.is_empty() || !shared_campaigns.is_empty() {
        return Err(format!(
            "scenario 4 selections must be resource/campaign disjoint, got {shared_resources:?} {shared_campaigns:?}"
        ));
    }
    // Fixture 12: the narrow one-file issue stays single-agent while other
    // campaigns stay active in the same snapshot.
    let twelfth = scenario(
        &view,
        "scenario-12-narrow-single-agent-despite-active-campaigns",
    )?;
    let snapshot = compile_work_portfolio_corpus("corpus")?;
    let active_campaigns = snapshot
        .campaigns
        .iter()
        .filter(|campaign| campaign.state == "active")
        .count();
    if active_campaigns < 2 {
        return Err("scenario 12 needs at least two active campaigns".to_string());
    }
    if twelfth.cases.iter().any(|case| case.status != "passed") {
        return Err("scenario 12 must pass".to_string());
    }
    Ok(())
}

/// Acceptance 2/6 (schema level; consumer migration is PR B/C): the DTO has
/// no repository-wide default campaign, no mutable agent assignment, no CI
/// wait state and no progress percentage, and unknown fields fail closed.
#[test]
fn work_selection_identity_no_singleton_or_runtime_state_fields() -> Result<(), String> {
    let corpus = committed_corpus()?;
    let packet = corpus
        .scenarios
        .first()
        .and_then(|scenario| scenario.cases.first())
        .and_then(|case| case.packet.clone())
        .ok_or_else(|| "committed corpus must carry a packet".to_string())?;
    let body = serde_json::to_string(&packet)
        .map_err(|error| format!("serialize packet: {error}"))?
        .to_ascii_lowercase();
    for forbidden in [
        "default_campaign",
        "current_campaign",
        "assigned_agent",
        "agent_assignment",
        "ci_wait",
        "progress_percent",
        "progress_percentage",
    ] {
        if body.contains(forbidden) {
            return Err(format!(
                "SelectedWorkIdentityV1 must not carry `{forbidden}` (got {body})"
            ));
        }
    }
    // deny_unknown_fields rejects a runtime-state field addition.
    let mut value =
        serde_json::to_value(&packet).map_err(|error| format!("serialize packet: {error}"))?;
    value
        .as_object_mut()
        .ok_or_else(|| "packet must serialize to an object".to_string())?
        .insert("progress_percentage".to_string(), serde_json::json!(50));
    if serde_json::from_value::<SelectedWorkIdentityV1>(value).is_ok() {
        return Err("unknown runtime-state fields must fail closed".to_string());
    }
    Ok(())
}

/// Acceptance 9: human and JSON projections derive from one check DTO and
/// agree on scenarios, cases and pass/fail counts.
#[test]
fn work_selection_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json_body = work_selection_check_json(&view)?;
    let parsed: WorkSelectionCheckViewV1 = serde_json::from_str(&json_body)
        .map_err(|error| format!("parse check view JSON: {error}"))?;
    if parsed.counts != view.counts {
        return Err(format!(
            "JSON projection counts {:?} drifted from the DTO {:?}",
            parsed.counts, view.counts
        ));
    }
    let markdown = work_selection_check_markdown(&view);
    let scenario_rows = markdown
        .lines()
        .filter(|line| line.starts_with("| scenario-"))
        .count();
    if scenario_rows != view.counts.scenarios as usize {
        return Err(format!(
            "markdown renders {scenario_rows} scenario rows for {} scenarios",
            view.counts.scenarios
        ));
    }
    let counts_line = format!(
        "- scenarios: {}, cases: {}, passed: {}, failed: {}",
        view.counts.scenarios, view.counts.cases, view.counts.passed, view.counts.failed
    );
    if !markdown.contains(&counts_line) {
        return Err(format!(
            "markdown counts line must agree with the DTO: {counts_line}"
        ));
    }
    Ok(())
}

/// Deliverable 4: the closed plan-disposition vocabulary for the PR C
/// planning consumer is frozen at exactly the eight #1646 wire names.
#[test]
fn work_selection_identity_plan_disposition_vocabulary_frozen() -> Result<(), String> {
    let expected = [
        ("single_scoped_pr", WorkPlanDispositionV1::SingleScopedPr),
        ("multi_pr_campaign", WorkPlanDispositionV1::MultiPrCampaign),
        (
            "append_to_named_campaign",
            WorkPlanDispositionV1::AppendToNamedCampaign,
        ),
        (
            "standalone_issue_work",
            WorkPlanDispositionV1::StandaloneIssueWork,
        ),
        (
            "focused_tracker_only",
            WorkPlanDispositionV1::FocusedTrackerOnly,
        ),
        ("already_planned", WorkPlanDispositionV1::AlreadyPlanned),
        (
            "blocked_by_contract_or_decision",
            WorkPlanDispositionV1::BlockedByContractOrDecision,
        ),
        (
            "root_portfolio_decision_required",
            WorkPlanDispositionV1::RootPortfolioDecisionRequired,
        ),
    ];
    if WorkPlanDispositionV1::all().len() != 8 {
        return Err("plan disposition vocabulary must stay at exactly eight values".to_string());
    }
    for (wire, disposition) in expected {
        let rendered = serde_json::to_value(disposition)
            .map_err(|error| format!("serialize disposition: {error}"))?;
        if rendered.as_str() != Some(wire) {
            return Err(format!(
                "disposition wire name drifted: expected `{wire}`, got {rendered}"
            ));
        }
    }
    Ok(())
}

/// Deliverable 2: stable identity string forms and whole-value equality
/// semantics over the normalized DTOs.
#[test]
fn work_selection_identity_stable_forms_and_equality() -> Result<(), String> {
    if issue_identity(9105) != "issue:9105" {
        return Err("issue identity form drifted".to_string());
    }
    if work_item_identity("slice-9105") != "work-item:slice-9105" {
        return Err("work-item identity form drifted".to_string());
    }
    if candidate_identity(9105) != "candidate:issue:9105" {
        return Err("candidate identity form drifted".to_string());
    }
    if selection_identity(WorkCandidateKindV1::StartBuild, 9105)
        != "selection:start_build:issue:9105"
    {
        return Err("selection identity form drifted".to_string());
    }
    let corpus = committed_corpus()?;
    let packet = corpus
        .scenarios
        .first()
        .and_then(|scenario| scenario.cases.first())
        .and_then(|case| case.packet.clone())
        .ok_or_else(|| "committed corpus must carry a packet".to_string())?;
    let same = packet.clone();
    if packet != same {
        return Err("identical packets must compare equal".to_string());
    }
    let mut changed = packet.clone();
    changed.basis_sha = "0".repeat(64);
    if packet == changed {
        return Err("a changed basis must change packet equality".to_string());
    }
    Ok(())
}

/// Fixture 10 (schema/fixture pinning; the burn-down consumer migrates in
/// PR C): the merged work item leaves the issue acceptance-uncovered, and
/// the projection states agree with the captured evidence.
#[test]
fn work_selection_identity_burndown_partial_projection_states() -> Result<(), String> {
    let view = committed_check()?;
    let tenth = scenario(&view, "scenario-10-burndown-partial-acceptance-uncovered")?;
    if tenth.acceptance_state != "acceptance_uncovered" {
        return Err(format!(
            "issue 9109 must stay acceptance-uncovered after the merged PR, got `{}`",
            tenth.acceptance_state
        ));
    }
    if scenario(&view, "scenario-01-selected-issue-one-campaign")?.acceptance_state != "not_started"
    {
        return Err("issue 9105 with no PR must read not_started".to_string());
    }
    if scenario(&view, "scenario-05-existing-open-pr-selected-for-repair")?.acceptance_state
        != "in_flight"
    {
        return Err("issue 9101 with an open PR must read in_flight".to_string());
    }
    Ok(())
}

/// Fixture 11: the committed corpus is the whole resume authority; no chat
/// transcript or singleton goal file is referenced anywhere.
#[test]
fn work_selection_identity_fresh_root_resume_from_artifacts_only() -> Result<(), String> {
    let root = workspace_path(DEFAULT_WORK_SELECTION_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))?
        .to_ascii_lowercase();
    for forbidden in ["active.toml", "chat transcript", "chat_history"] {
        if body.contains(forbidden) {
            return Err(format!(
                "corpus must resume from committed artifacts only, found `{forbidden}`"
            ));
        }
    }
    let view = committed_check()?;
    if case(&view, "scenario-11-fresh-root-resume-from-artifacts", 0)?
        .selection_identity
        .is_none()
    {
        return Err("fresh-root resume must resolve a stable selection identity".to_string());
    }
    Ok(())
}

/// Determinism: fixed committed inputs produce byte-stable JSON and a
/// stable portable identity across runs.
#[test]
fn work_selection_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_selection_check_json(&first)?;
    let second_json = work_selection_check_json(&second)?;
    if first_json != second_json {
        return Err("selection check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("selection check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and unknown fields.
#[test]
fn work_selection_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_selection_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    let base = r##"{
        "schema_version": "work_selection_identity_corpus.v1",
        "repository": "EffortlessMetrics/ripr-swarm",
        "selection_policy": "explicit-root-selection",
        "selection_policy_version": "v1",
        "scenarios": [
            {
                "id": "scenario-x",
                "title": "x",
                "captured": "work_portfolio/corpus",
                "cases": [
                    {
                        "packet": null,
                        "legacy_packet": null,
                        "expect": "fail",
                        "expect_violations": [],
                        "plan_disposition": "single_scoped_pr"
                    }
                ]
            }
        ]
    }"##;
    if load_work_selection_corpus(base).is_ok() {
        return Err("a failure case without pinned law/route pairs must fail closed".to_string());
    }
    let body = committed_corpus_json()?;
    let mutated = body.replace(
        "\"scenario-01-selected-issue-one-campaign\"",
        "\"scenario-02-selected-issue-several-related-campaigns\"",
    );
    if load_work_selection_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_selection_corpus(&body.replace(
        "work_selection_identity_corpus.v1",
        "work_selection_identity_corpus.v2",
    ))
    .is_ok()
    {
        return Err("schema drift must fail closed".to_string());
    }
    Ok(())
}

fn committed_corpus_json() -> Result<String, String> {
    let root = workspace_path(DEFAULT_WORK_SELECTION_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// Provenance fails closed on digest drift and path traversal.
#[test]
fn work_selection_identity_provenance_fails_closed() -> Result<(), String> {
    let sha = "0".repeat(64);
    let template = format!(
        r##"{{
            "schema_version": "work_selection_identity_provenance.v1",
            "repository": "EffortlessMetrics/ripr-swarm",
            "captured_at": "2026-10-05T12:00:00Z",
            "capture_method": "test",
            "files": [
                {{ "path": "@PATH@", "sha256": "{sha}" }}
            ]
        }}"##
    );
    for path in [
        "/abs/corpus.json",
        "../corpus.json",
        "",
        "..\\corpus.json",
        "captured\\multi-campaign\\corpus.json",
        "corpus.json",
    ] {
        let body = template.replace("@PATH@", path);
        let parsed = load_work_selection_provenance(&body);
        if path == "corpus.json" {
            if parsed.is_err() {
                return Err(format!("relative path `{path}` must parse: {parsed:?}"));
            }
        } else if parsed.is_ok() {
            return Err(format!("provenance path `{path}` must fail closed"));
        }
    }
    Ok(())
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_selection_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_selection_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_SELECTION_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_selection_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "selection check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-selection-check.json",
            "target/ripr/reports/work-selection-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "selection check wrote outside the reports directory: {key}"
                ));
            }
        }
        for name in &allowed {
            if !sandbox.join(name).is_file() {
                return Err(format!("missing report {name}"));
            }
        }
        Ok(())
    })();

    let _ = restore();
    let _ = fs::remove_dir_all(&sandbox);
    result
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(src).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn hash_dir(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let bytes =
                    fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
                let relative = path
                    .strip_prefix(root)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative, crate::blind_journey::sha256_hex(&bytes));
            }
        }
    }
    Ok(out)
}

/// The basis builder reflects source freshness honestly instead of
/// presenting partial observation as complete.
#[test]
fn work_selection_identity_basis_completeness_reflects_sources() -> Result<(), String> {
    let view = committed_check()?;
    let first = scenario(&view, "scenario-01-selected-issue-one-campaign")?;
    if first.snapshot_completeness != WorkConfidenceV1::Complete {
        return Err("the canonical corpus basis must be complete".to_string());
    }
    let canonical = load_work_captured_dir(&workspace_path("fixtures/work_portfolio/corpus"))?;
    if canonical.manifest.repository != view.repository {
        return Err("corpus repository must match the captured manifest".to_string());
    }
    Ok(())
}

#[test]
fn work_selection_identity_standalone_overlaps_use_compiler_effective_root() -> Result<(), String> {
    // Manifest root and local-state root diverge: standalone overlap
    // worktrees must use the same effective-root spelling the compiler
    // renders, or a packet using the compiler spelling fails visibility.
    let root = std::env::temp_dir().join(format!(
        "ripr-selection-effective-root-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    ));
    let source = workspace_path("fixtures/work_selection_identity/captured/standalone");
    let mut copied = 0;
    for entry in
        fs::read_dir(&source).map_err(|err| format!("read standalone captured dir: {err}"))?
    {
        let entry = entry.map_err(|err| format!("read standalone entry: {err}"))?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.ends_with(".json") {
            continue;
        }
        let body = fs::read_to_string(entry.path()).map_err(|err| format!("read {name}: {err}"))?;
        fs::create_dir_all(&root).map_err(|err| format!("create temp root: {err}"))?;
        fs::write(root.join(name), body).map_err(|err| format!("write {name}: {err}"))?;
        copied += 1;
    }
    if copied == 0 {
        return Err("standalone captured dir contributed no JSON inputs".to_string());
    }
    // Diverge the local root and hang the open PR worktree under it. The
    // diverged paths are built from the temp dir at runtime so no absolute
    // path literal is committed (check-local-context).
    let diverged = root.join("diverged-local");
    let diverged_worktree = diverged.join("wt-9102");
    let local_text = fs::read_to_string(root.join("local_state.json"))
        .map_err(|err| format!("read temp local_state.json: {err}"))?;
    let mut local: serde_json::Value = serde_json::from_str(&local_text)
        .map_err(|err| format!("parse temp local_state: {err}"))?;
    local["root"] = serde_json::Value::String(diverged.to_string_lossy().into_owned());
    fs::write(root.join("local_state.json"), local.to_string())
        .map_err(|err| format!("write diverged local_state: {err}"))?;
    let prs_text = fs::read_to_string(root.join("pull_requests.json"))
        .map_err(|err| format!("read temp pull_requests.json: {err}"))?;
    let mut prs: serde_json::Value = serde_json::from_str(&prs_text)
        .map_err(|err| format!("parse temp pull_requests: {err}"))?;
    let Some(pr) = prs["pull_requests"].as_array_mut().and_then(|prs| {
        prs.iter_mut()
            .find(|pr| pr["number"].as_u64() == Some(8802))
    }) else {
        return Err("standalone PR 8802 must exist for the root test".to_string());
    };
    pr["worktree_path"] =
        serde_json::Value::String(diverged_worktree.to_string_lossy().into_owned());
    fs::write(root.join("pull_requests.json"), prs.to_string())
        .map_err(|err| format!("write diverged pull_requests: {err}"))?;
    let claims_text = fs::read_to_string(root.join("claims.json"))
        .map_err(|err| format!("read temp claims.json: {err}"))?;
    let mut claims: serde_json::Value =
        serde_json::from_str(&claims_text).map_err(|err| format!("parse temp claims: {err}"))?;
    let Some(claim) = claims["claims"].as_array_mut().and_then(|claims| {
        claims
            .iter_mut()
            .find(|claim| claim["issue"].as_u64() == Some(9102))
    }) else {
        return Err("standalone claim on 9102 must exist for the root test".to_string());
    };
    claim["worktree"] = serde_json::Value::String(diverged_worktree.to_string_lossy().into_owned());
    fs::write(root.join("claims.json"), claims.to_string())
        .map_err(|err| format!("write diverged claims: {err}"))?;

    let captured = load_work_captured_dir(&root)?;
    let snapshot = compile_work_portfolio(&captured, None)?;
    let live = live_overlaps_captured(&snapshot, &captured, 9102);
    if !live.pull_requests.contains(&8802) {
        return Err(format!(
            "PR 8802 must overlap issue 9102: {:?}",
            live.pull_requests
        ));
    }
    let Some(compiled) = snapshot
        .pull_requests
        .iter()
        .find(|pr| pr.number == 8802)
        .and_then(|pr| pr.worktree.clone())
    else {
        return Err("compiler must render PR 8802 worktree".to_string());
    };
    if live.worktrees != vec![compiled.clone()] {
        return Err(format!(
            "standalone overlap worktrees must match the compiler spelling `{compiled}`: {:?}",
            live.worktrees
        ));
    }
    if live.worktrees != vec!["<root>/wt-9102".to_string()] {
        return Err(format!(
            "diverged-local worktree must relativize: {:?}",
            live.worktrees
        ));
    }
    let _ = fs::remove_dir_all(&root);
    Ok(())
}
