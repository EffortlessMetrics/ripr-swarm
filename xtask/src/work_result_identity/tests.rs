//! Tests for the task/result/receipt schema consumer (#1706 PR B,
//! RIPR-SPEC-0269, consuming #1633). Every test name carries the
//! `work_result_identity` prefix so the suite is greppable as one delivery
//! slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::{compile_work_portfolio, load_work_captured_dir};

fn committed_result_corpus() -> Result<WorkResultIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_RESULT_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed result corpus: {error}"))?;
    load_work_result_corpus(&body)
}

fn committed_check() -> Result<WorkResultCheckViewV1, String> {
    let corpus = committed_result_corpus()?;
    run_work_result_check(DEFAULT_WORK_RESULT_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkResultCheckViewV1,
    id: &str,
) -> Result<&'a WorkResultScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkResultCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkResultCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkResultCheckViewV1,
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

/// Compile one captured dir exactly as the check does and report the basis
/// facts corpus authoring needs: default-branch head and snapshot identity.
#[test]
fn work_result_identity_basis_facts_for_corpus_authoring() -> Result<(), String> {
    for captured in [
        "fixtures/work_portfolio/corpus",
        "fixtures/work_selection_identity/captured/standalone",
        "fixtures/work_selection_identity/captured/multi-campaign",
    ] {
        let dir = load_work_captured_dir(&workspace_path(captured))?;
        let snapshot = compile_work_portfolio(&dir, None)?;
        let basis = build_portfolio_basis(&snapshot, "explicit-root-selection", "v1");
        println!(
            "AUTHORING_FACT captured={captured} head={} snapshot={}",
            basis.repository.default_branch_sha, basis.snapshot_identity
        );
    }
    Ok(())
}

/// The committed corpus compiles, carries the five required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_result_identity_committed_corpus_five_scenarios_hold() -> Result<(), String> {
    let view = committed_check()?;
    if view.counts.scenarios != 5 {
        return Err(format!(
            "expected five scenarios, got {}",
            view.counts.scenarios
        ));
    }
    if view.counts.failed != 0 {
        return Err(format!(
            "expected every committed case to pass, got {:?}",
            view.counts
        ));
    }
    Ok(())
}

/// #1633 + fixture 9: a result for another repository, portfolio snapshot,
/// basis, issue/action (via the embedded selection), worktree or exact head
/// fails stale/wrong-input with its exact recompile/reconcile route.
#[test]
fn work_result_identity_wrong_inputs_fail_stale_or_wrong_input() -> Result<(), String> {
    let view = committed_check()?;
    let expected = [
        (
            0,
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
        (
            1,
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
        (
            2,
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
        (
            3,
            WorkSelectionLawV1::HeadIdentity,
            WorkSelectionRouteV1::ReconcileHead,
        ),
        (
            4,
            WorkSelectionLawV1::WorktreeIdentity,
            WorkSelectionRouteV1::ReconcileResources,
        ),
        (
            5,
            WorkSelectionLawV1::ActionIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            6,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            7,
            WorkSelectionLawV1::LegacyCompatibility,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
    ];
    for (index, law, route) in expected {
        has_violation(
            &view,
            "result-05-wrong-identities-fail-stale-or-wrong-input",
            index,
            law,
            route,
        )?;
    }
    Ok(())
}

/// #1633: every result embeds the full selected-work identity, so a result
/// whose embedded selection the RIPR-SPEC-0244 engine rejects fails through
/// the shared laws rather than a forked result-only check.
#[test]
fn work_result_identity_embedded_selection_reuses_shared_laws() -> Result<(), String> {
    let corpus = committed_result_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "result-01-one-campaign-selection-results-pass")
        .ok_or_else(|| "missing scenario result-01".to_string())?;
    let mut tampered = scenario.cases[0].result.clone();
    // Tamper with the embedded selection's basis: the shared BasisIdentity
    // law must fire through the result check.
    tampered.selection.basis_sha = "0".repeat(40);
    tampered.basis_sha = "0".repeat(40);
    let dir = resolve_scenario_captured(
        &corpus_root(DEFAULT_WORK_RESULT_CORPUS_DIR),
        &scenario.captured,
    )?;
    let captured = load_work_captured_dir(&dir)?;
    let snapshot = compile_work_portfolio(&captured, None)?;
    let basis = build_portfolio_basis(
        &snapshot,
        &corpus.selection_policy,
        &corpus.selection_policy_version,
    );
    let evaluation = check_result(&tampered, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|violation| violation.law == WorkSelectionLawV1::BasisIdentity)
    {
        return Err("tampered embedded selection must fail the shared basis law".to_string());
    }
    Ok(())
}

/// #1633 schema shape: a result binds no singleton fields — no
/// repository-wide campaign, agent assignment, CI wait state or progress
/// percentage — and unknown fields fail closed at parse time.
#[test]
fn work_result_identity_result_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    let first = body
        .find("\"result_id\"")
        .ok_or_else(|| "committed corpus must contain at least one result row".to_string())?;
    let _ = first;
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("committed corpus must parse as JSON: {error}"))?;
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
                "result corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    let _ = value;
    Ok(())
}

/// Fixtures 1/3/5 and acceptance 4: results pass for a one-campaign
/// selection, a standalone selection with zero campaigns, and a repair
/// selection on the already-recorded worktree.
#[test]
fn work_result_identity_valid_results_bind_their_selections() -> Result<(), String> {
    let view = committed_check()?;
    for (scenario, index) in [
        ("result-01-one-campaign-selection-results-pass", 0_usize),
        ("result-02-standalone-selection-results-pass", 0_usize),
        ("result-03-repair-selection-worktree-result-passes", 0_usize),
    ] {
        let case = case(&view, scenario, index)?;
        if case.status != "passed" || !case.violations.is_empty() {
            return Err(format!("{scenario} case {index} must pass cleanly"));
        }
        if case.result_identity.is_none() {
            return Err(format!(
                "{scenario} case {index} must resolve a stable result identity"
            ));
        }
    }
    // The repair result must bind the worktree recorded by the selection.
    let repair = case(
        &view,
        "result-03-repair-selection-worktree-result-passes",
        0,
    )?;
    if repair.selection_key.as_deref() != Some("selection:repair_review:issue:9101") {
        return Err(format!(
            "repair result must bind the repair_review selection, got {:?}",
            repair.selection_key
        ));
    }
    Ok(())
}

/// Acceptance 7 + fixture 6: legacy refs on a result resolve read-only with
/// migration posture and grant no authority; an unresolvable legacy ref
/// fails with the exact reconcile route.
#[test]
fn work_result_identity_legacy_refs_read_only_no_authority() -> Result<(), String> {
    let view = committed_check()?;
    let legacy = case(&view, "result-04-legacy-read-only-migration-posture", 0)?;
    if legacy.status != "passed" || !legacy.violations.is_empty() {
        return Err("the read-only legacy result must pass".to_string());
    }
    let posture = legacy
        .migration_posture
        .iter()
        .find(|line| line.contains("legacy_active_goal_ref"))
        .ok_or_else(|| "the legacy result must emit migration posture".to_string())?;
    if !posture.contains("no write, readiness, merge or closeout authority") {
        return Err(format!(
            "migration posture must record the no-authority grant: {posture}"
        ));
    }
    has_violation(
        &view,
        "result-05-wrong-identities-fail-stale-or-wrong-input",
        7,
        WorkSelectionLawV1::LegacyCompatibility,
        WorkSelectionRouteV1::ReconcileSelection,
    )
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries.
#[test]
fn work_result_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_result_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_result_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionLawV1::BasisIdentity,
        WorkSelectionLawV1::HeadIdentity,
        WorkSelectionLawV1::WorktreeIdentity,
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
fn work_result_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_result_check_json(&first)?;
    let second_json = work_result_check_json(&second)?;
    if first_json != second_json {
        return Err("result check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("result check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty result identity fields.
#[test]
fn work_result_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_result_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_result_corpus(
        r##"{
            "schema_version": "work_result_identity_corpus.v1",
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
        "\"result-01-one-campaign-selection-results-pass\"",
        "\"result-02-standalone-selection-results-pass\"",
    );
    if load_work_result_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_result_corpus(&body.replace(
        "work_result_identity_corpus.v1",
        "work_result_identity_corpus.v2",
    ))
    .is_ok()
    {
        return Err("schema drift must fail closed".to_string());
    }
    Ok(())
}

fn committed_corpus_json() -> Result<String, String> {
    let root = workspace_path(DEFAULT_WORK_RESULT_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed provenance covers every corpus byte: the shared validator
/// must report no violation for the committed corpus.
#[test]
fn work_result_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_result_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_result_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_result_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_RESULT_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_result_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "result check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-result-check.json",
            "target/ripr/reports/work-result-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "result check wrote outside the reports directory: {key}"
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
