//! Tests for the plan-disposition consumer (#1706 PR B, RIPR-SPEC-0276,
//! consuming #1646). Every test name carries the `work_plan_identity`
//! prefix so the suite is greppable as one delivery slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::load_work_captured_dir;
use crate::work_selection_identity::issue_identity;

fn committed_plan_corpus() -> Result<WorkPlanIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_PLAN_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed plan corpus: {error}"))?;
    load_work_plan_corpus(&body)
}

fn committed_check() -> Result<WorkPlanCheckViewV1, String> {
    let corpus = committed_plan_corpus()?;
    run_work_plan_check(DEFAULT_WORK_PLAN_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkPlanCheckViewV1,
    id: &str,
) -> Result<&'a WorkPlanScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkPlanCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkPlanCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkPlanCheckViewV1,
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

fn committed_corpus_json() -> Result<String, String> {
    let root = workspace_path(DEFAULT_WORK_PLAN_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed corpus compiles, carries the five required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_plan_identity_committed_corpus_five_scenarios_hold() -> Result<(), String> {
    let view = committed_check()?;
    if view.counts.scenarios != 5 {
        return Err(format!(
            "expected five scenarios, got {}",
            view.counts.scenarios
        ));
    }
    if view.counts.cases != 10 {
        return Err(format!("expected ten cases, got {}", view.counts.cases));
    }
    if view.counts.failed != 0 {
        return Err(format!(
            "expected every committed case to pass, got {:?}",
            view.counts
        ));
    }
    Ok(())
}

/// #1646: a single scoped PR disposition for one selected issue passes and
/// resolves a stable plan identity.
#[test]
fn work_plan_identity_single_scoped_pr_disposition_passes() -> Result<(), String> {
    let view = committed_check()?;
    let scoped = case(&view, "plan-01-single-scoped-pr-disposition-passes", 0)?;
    if scoped.status != "passed" || !scoped.violations.is_empty() {
        return Err("the single scoped PR plan must pass cleanly".to_string());
    }
    if scoped.disposition != WorkPlanDispositionV1::SingleScopedPr {
        return Err("the plan must record the single scoped PR disposition".to_string());
    }
    if scoped.plan_identity.is_none() {
        return Err("the passing plan must resolve a stable identity".to_string());
    }
    Ok(())
}

/// #1646: a standalone issue disposition without campaign placement passes,
/// and an append disposition naming a recorded campaign passes.
#[test]
fn work_plan_identity_standalone_and_append_dispositions_pass() -> Result<(), String> {
    let view = committed_check()?;
    let standalone = case(&view, "plan-02-standalone-issue-disposition-passes", 0)?;
    if standalone.status != "passed" {
        return Err("the standalone issue plan must pass".to_string());
    }
    if standalone.disposition != WorkPlanDispositionV1::StandaloneIssueWork {
        return Err("the plan must record the standalone issue disposition".to_string());
    }
    let append = case(
        &view,
        "plan-03-append-to-named-campaign-disposition-passes",
        0,
    )?;
    if append.status != "passed" {
        return Err("the append-to-named-campaign plan must pass".to_string());
    }
    if append.disposition != WorkPlanDispositionV1::AppendToNamedCampaign {
        return Err("the plan must record the append disposition".to_string());
    }
    Ok(())
}

/// #1646: a blocked disposition naming its blocking contract passes; the
/// corpus binds the blocked plan's blocker.
#[test]
fn work_plan_identity_blocked_disposition_passes() -> Result<(), String> {
    let view = committed_check()?;
    let blocked = case(&view, "plan-04-blocked-by-contract-disposition-passes", 0)?;
    if blocked.status != "passed" {
        return Err("the blocked plan must pass".to_string());
    }
    if blocked.disposition != WorkPlanDispositionV1::BlockedByContractOrDecision {
        return Err("the plan must record the blocked disposition".to_string());
    }
    let corpus = committed_plan_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "plan-04-blocked-by-contract-disposition-passes")
        .ok_or_else(|| "missing scenario plan-04".to_string())?;
    if scenario.cases[0].plan.blocked_by.is_empty() {
        return Err("the blocked plan must name its blocker".to_string());
    }
    Ok(())
}

/// #1646: read-only planning cannot reorder the portfolio; a missing or
/// unknown append target, an unnamed blocker, a foreign repository and an
/// unknown legacy goal each fail with their exact law/route pair.
#[test]
fn work_plan_identity_reorder_and_wrong_inputs_fail() -> Result<(), String> {
    let view = committed_check()?;
    let expected = [
        (
            0,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            1,
            WorkSelectionLawV1::CampaignRefIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            2,
            WorkSelectionLawV1::CampaignRefIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            3,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            4,
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
        (
            5,
            WorkSelectionLawV1::LegacyCompatibility,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
    ];
    for (index, law, route) in expected {
        has_violation(
            &view,
            "plan-05-reorder-and-wrong-inputs-fail",
            index,
            law,
            route,
        )?;
    }
    Ok(())
}

/// #1646 schema shape: a plan binds no singleton fields — no
/// repository-wide campaign, agent assignment, CI wait state or progress
/// percentage — and unknown fields fail closed at parse time.
#[test]
fn work_plan_identity_plan_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    if !body.contains("\"plan_id\"") {
        return Err("committed corpus must contain at least one plan row".to_string());
    }
    let corpus_text = body.to_ascii_lowercase();
    for forbidden in [
        "\"active_goal\"",
        "\"current_campaign\"",
        "\"agent_assignment\"",
        "\"ci_wait\"",
        "\"progress_percent\"",
        "\"progress_percentage\"",
    ] {
        if corpus_text.contains(forbidden) {
            return Err(format!(
                "plan corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// The committed corpus exercises a spread of the closed disposition
/// vocabulary; every recorded disposition is a known wire name.
#[test]
fn work_plan_identity_dispositions_stay_on_the_closed_set() -> Result<(), String> {
    let corpus = committed_plan_corpus()?;
    let known: BTreeSet<WorkPlanDispositionV1> = WorkPlanDispositionV1::all().into_iter().collect();
    let mut seen = BTreeSet::new();
    for scenario in &corpus.scenarios {
        for case in &scenario.cases {
            if !known.contains(&case.plan.disposition) {
                return Err(format!(
                    "scenario `{}` carries a disposition outside the closed set",
                    scenario.id
                ));
            }
            seen.insert(case.plan.disposition);
        }
    }
    if seen.len() < 4 {
        return Err(format!(
            "the committed corpus should exercise at least four dispositions, got {seen:?}"
        ));
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries. Only laws the committed
/// corpus actually pins are required in both projections.
#[test]
fn work_plan_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_plan_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_plan_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionLawV1::SubjectIdentity,
        WorkSelectionLawV1::CampaignRefIdentity,
        WorkSelectionLawV1::LegacyCompatibility,
    ] {
        let wire = selection_wire_name(&law);
        if !json.contains(&wire) {
            return Err(format!("JSON projection must carry the law `{wire}`"));
        }
        if !markdown.contains(&wire) {
            return Err(format!("Markdown projection must carry the law `{wire}`"));
        }
    }
    let counts = parsed
        .get("counts")
        .ok_or_else(|| "view JSON must carry counts".to_string())?;
    if counts.get("failed").and_then(|v| v.as_u64()) != Some(0) {
        return Err("committed corpus must report zero failed cases".to_string());
    }
    Ok(())
}

/// Determinism: fixed committed inputs produce byte-stable JSON and a
/// stable portable identity across runs.
#[test]
fn work_plan_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_plan_check_json(&first)?;
    let second_json = work_plan_check_json(&second)?;
    if first_json != second_json {
        return Err("plan check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("plan check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty plan identity fields.
#[test]
fn work_plan_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_plan_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_plan_corpus(
        r##"{
            "schema_version": "work_plan_identity_corpus.v1",
            "repository": "EffortlessMetrics/ripr-swarm",
            "selection_policy": "explicit-root-selection",
            "selection_policy_version": "v1",
            "scenarios": []
        }"##,
    )
    .is_ok()
    {
        return Err("a corpus with zero scenarios must fail closed".to_string());
    }
    let body = committed_corpus_json()?;
    let mutated = body.replace(
        "\"plan-01-single-scoped-pr-disposition-passes\"",
        "\"plan-02-standalone-issue-disposition-passes\"",
    );
    if load_work_plan_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_plan_corpus(&body.replace(
        "work_plan_identity_corpus.v1",
        "work_plan_identity_corpus.v2",
    ))
    .is_ok()
    {
        return Err("schema drift must fail closed".to_string());
    }
    Ok(())
}

/// The committed provenance covers every corpus byte: the shared validator
/// must report no violation for the committed corpus.
#[test]
fn work_plan_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_plan_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Plan law detail: every plan the committed corpus passes plans for an
/// issue recorded in the captured issue source of its scenario, and binds
/// the compiled snapshot identity of that scenario's basis.
#[test]
fn work_plan_identity_plans_bind_recorded_issues_and_snapshots() -> Result<(), String> {
    let corpus = committed_plan_corpus()?;
    let root = corpus_root(DEFAULT_WORK_PLAN_CORPUS_DIR);
    for scenario in &corpus.scenarios {
        let dir = resolve_scenario_captured(&root, &scenario.captured)?;
        let captured = load_work_captured_dir(&dir)?;
        let snapshot = compile_work_portfolio(&captured, None)?;
        for plan_case in &scenario.cases {
            if plan_case.plan.portfolio_snapshot != snapshot.portable_identity {
                return Err(format!(
                    "scenario `{}` binds snapshot `{}` but its captured source compiles `{}`",
                    scenario.id, plan_case.plan.portfolio_snapshot, snapshot.portable_identity
                ));
            }
            if plan_case.expect == WorkSelectionExpectV1::Pass {
                let Some(issue) = &plan_case.plan.selection.issue else {
                    return Err(format!(
                        "scenario `{}` passing plan carries no issue identity",
                        scenario.id
                    ));
                };
                let known = captured.issues.as_ref().is_some_and(|issues| {
                    issues.issues.iter().any(|row| row.number == issue.number)
                });
                if !known {
                    return Err(format!(
                        "scenario `{}` plans issue {} which the captured source does not record",
                        scenario.id,
                        issue_identity(issue.number)
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_plan_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_plan_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_PLAN_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_plan_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "plan check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-plan-check.json",
            "target/ripr/reports/work-plan-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "plan check wrote outside the reports directory: {key}"
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
