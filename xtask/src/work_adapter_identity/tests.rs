//! Tests for the Codex adapter consumer (#1706 PR B, RIPR-SPEC-0277,
//! consuming #1637/#1649). Every test name carries the
//! `work_adapter_identity` prefix so the suite is greppable as one delivery
//! slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::load_work_captured_dir;
use crate::work_selection_identity::issue_identity;

fn committed_adapter_corpus() -> Result<WorkAdapterIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_ADAPTER_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed adapter corpus: {error}"))?;
    load_work_adapter_corpus(&body)
}

fn committed_check() -> Result<WorkAdapterCheckViewV1, String> {
    let corpus = committed_adapter_corpus()?;
    run_work_adapter_check(DEFAULT_WORK_ADAPTER_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkAdapterCheckViewV1,
    id: &str,
) -> Result<&'a WorkAdapterScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkAdapterCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkAdapterCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkAdapterCheckViewV1,
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
    let root = workspace_path(DEFAULT_WORK_ADAPTER_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed corpus compiles, carries the five required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_adapter_identity_committed_corpus_five_scenarios_hold() -> Result<(), String> {
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

/// #1637: a read-only scout delegation over one selected issue passes and
/// resolves a stable adapter identity.
#[test]
fn work_adapter_identity_scout_read_only_delegation_passes() -> Result<(), String> {
    let view = committed_check()?;
    let scout = case(&view, "adapter-01-scout-read-only-delegation-passes", 0)?;
    if scout.status != "passed" || !scout.violations.is_empty() {
        return Err("the scout delegation must pass cleanly".to_string());
    }
    if scout.role != WorkAdapterRoleV1::Scout {
        return Err("the delegation must record the scout role".to_string());
    }
    if scout.flow_stage != WorkAdapterFlowStageV1::Delegate {
        return Err("the delegation must record the delegate stage".to_string());
    }
    if scout.adapter_identity.is_none() {
        return Err("the passing delegation must resolve a stable identity".to_string());
    }
    Ok(())
}

/// #1637: a builder delegation passes only because the bound selection
/// carries a prepared claim and an isolated worktree.
#[test]
fn work_adapter_identity_builder_with_prepared_claim_passes() -> Result<(), String> {
    let view = committed_check()?;
    let builder = case(&view, "adapter-02-builder-with-prepared-claim-passes", 0)?;
    if builder.status != "passed" {
        return Err("the prepared-claim builder delegation must pass".to_string());
    }
    let corpus = committed_adapter_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "adapter-02-builder-with-prepared-claim-passes")
        .ok_or_else(|| "missing scenario adapter-02".to_string())?;
    let selection = &scenario.cases[0].adapter.selection;
    if selection.overlaps.claims.is_empty() && selection.overlaps.worktrees.is_empty() {
        return Err(
            "the prepared-claim builder selection must carry a claim or a worktree".to_string(),
        );
    }
    Ok(())
}

/// #1649: a work-planner delegation over an independent multi-campaign
/// selection passes; a reviewer delegation resumed from committed artifacts
/// only passes.
#[test]
fn work_adapter_identity_planner_and_resume_review_pass() -> Result<(), String> {
    let view = committed_check()?;
    let planner = case(&view, "adapter-03-planner-compile-delegation-passes", 0)?;
    if planner.status != "passed" {
        return Err("the work-planner delegation must pass".to_string());
    }
    if planner.role != WorkAdapterRoleV1::WorkPlanner {
        return Err("the delegation must record the work planner role".to_string());
    }
    if planner.flow_stage != WorkAdapterFlowStageV1::Compile {
        return Err("the delegation must record the compile stage".to_string());
    }
    let reviewer = case(&view, "adapter-04-resume-from-artifacts-review-passes", 0)?;
    if reviewer.status != "passed" {
        return Err("the resume-from-artifacts reviewer delegation must pass".to_string());
    }
    let corpus = committed_adapter_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "adapter-04-resume-from-artifacts-review-passes")
        .ok_or_else(|| "missing scenario adapter-04".to_string())?;
    if scenario.cases[0].adapter.derived_from.is_empty() {
        return Err("the resume reviewer must derive from committed artifacts".to_string());
    }
    Ok(())
}

/// #1637: a subagent portfolio re-read, an unprepared builder, a subagent
/// claim-write, an empty derivation, a foreign repository and an unknown
/// legacy goal each fail with their exact law/route pair.
#[test]
fn work_adapter_identity_portfolio_reread_and_wrong_inputs_fail() -> Result<(), String> {
    let view = committed_check()?;
    let expected = [
        (
            0,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            1,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            2,
            WorkSelectionLawV1::SubjectIdentity,
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
            "adapter-05-portfolio-reread-and-wrong-inputs-fail",
            index,
            law,
            route,
        )?;
    }
    Ok(())
}

/// #1637/#1649 schema shape: a delegation binds no singleton fields — no
/// repository-wide campaign, agent assignment, CI wait state or progress
/// percentage — and unknown fields fail closed at parse time.
#[test]
fn work_adapter_identity_adapter_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    if !body.contains("\"adapter_id\"") {
        return Err("committed corpus must contain at least one adapter row".to_string());
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
                "adapter corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// The committed corpus exercises a spread of the closed role and flow
/// stage vocabularies; every recorded value is a known wire name.
#[test]
fn work_adapter_identity_roles_and_stages_stay_on_the_closed_set() -> Result<(), String> {
    let corpus = committed_adapter_corpus()?;
    let known_roles: BTreeSet<WorkAdapterRoleV1> = WorkAdapterRoleV1::all().into_iter().collect();
    let known_stages: BTreeSet<WorkAdapterFlowStageV1> =
        WorkAdapterFlowStageV1::all().into_iter().collect();
    let mut roles = BTreeSet::new();
    let mut stages = BTreeSet::new();
    for scenario in &corpus.scenarios {
        for case in &scenario.cases {
            if !known_roles.contains(&case.adapter.role) {
                return Err(format!(
                    "scenario `{}` carries a role outside the closed set",
                    scenario.id
                ));
            }
            if !known_stages.contains(&case.adapter.flow_stage) {
                return Err(format!(
                    "scenario `{}` carries a flow stage outside the closed set",
                    scenario.id
                ));
            }
            roles.insert(case.adapter.role);
            stages.insert(case.adapter.flow_stage);
        }
    }
    if roles.len() < 4 {
        return Err(format!(
            "the committed corpus should exercise at least four roles, got {roles:?}"
        ));
    }
    if stages.len() < 3 {
        return Err(format!(
            "the committed corpus should exercise at least three flow stages, got {stages:?}"
        ));
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries. Only laws the committed
/// corpus actually pins are required in both projections.
#[test]
fn work_adapter_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_adapter_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_adapter_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionLawV1::SubjectIdentity,
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
    for role in [WorkAdapterRoleV1::Scout, WorkAdapterRoleV1::Builder] {
        let wire = selection_wire_name(&role);
        if !json.contains(&wire) {
            return Err(format!("JSON projection must carry the role `{wire}`"));
        }
        if !markdown.contains(&wire) {
            return Err(format!("Markdown projection must carry the role `{wire}`"));
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
fn work_adapter_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_adapter_check_json(&first)?;
    let second_json = work_adapter_check_json(&second)?;
    if first_json != second_json {
        return Err("adapter check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("adapter check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty adapter identity fields.
#[test]
fn work_adapter_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_adapter_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_adapter_corpus(
        r##"{
            "schema_version": "work_adapter_identity_corpus.v1",
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
        "\"adapter-01-scout-read-only-delegation-passes\"",
        "\"adapter-02-builder-with-prepared-claim-passes\"",
    );
    if load_work_adapter_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_adapter_corpus(&body.replace(
        "work_adapter_identity_corpus.v1",
        "work_adapter_identity_corpus.v2",
    ))
    .is_ok()
    {
        return Err("schema drift must fail closed".to_string());
    }
    if load_work_adapter_corpus(&body.replace("\"scout\"", "\"drifter\"")).is_ok() {
        return Err("a role outside the closed vocabulary must fail closed".to_string());
    }
    Ok(())
}

/// The committed provenance covers every corpus byte: the shared validator
/// must report no violation for the committed corpus.
#[test]
fn work_adapter_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_adapter_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Adapter law detail: every delegation the committed corpus passes
/// delegates an issue recorded in the captured issue source of its
/// scenario, and binds the compiled snapshot identity of that scenario's
/// basis.
#[test]
fn work_adapter_identity_delegations_bind_recorded_issues_and_snapshots() -> Result<(), String> {
    let corpus = committed_adapter_corpus()?;
    let root = corpus_root(DEFAULT_WORK_ADAPTER_CORPUS_DIR);
    for scenario in &corpus.scenarios {
        let dir = resolve_scenario_captured(&root, &scenario.captured)?;
        let captured = load_work_captured_dir(&dir)?;
        let snapshot = compile_work_portfolio(&captured, None)?;
        for adapter_case in &scenario.cases {
            if adapter_case.adapter.portfolio_snapshot != snapshot.portable_identity {
                return Err(format!(
                    "scenario `{}` binds snapshot `{}` but its captured source compiles `{}`",
                    scenario.id,
                    adapter_case.adapter.portfolio_snapshot,
                    snapshot.portable_identity
                ));
            }
            if adapter_case.expect == WorkSelectionExpectV1::Pass {
                let Some(issue) = &adapter_case.adapter.selection.issue else {
                    return Err(format!(
                        "scenario `{}` passing delegation carries no issue identity",
                        scenario.id
                    ));
                };
                let known = captured.issues.as_ref().is_some_and(|issues| {
                    issues.issues.iter().any(|row| row.number == issue.number)
                });
                if !known {
                    return Err(format!(
                        "scenario `{}` delegates issue {} which the captured source does not record",
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
fn work_adapter_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_adapter_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_ADAPTER_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_adapter_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "adapter check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-adapter-check.json",
            "target/ripr/reports/work-adapter-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "adapter check wrote outside the reports directory: {key}"
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
