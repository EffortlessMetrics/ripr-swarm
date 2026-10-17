//! Tests for the execution-wave planning consumer (#1706 PR B,
//! RIPR-SPEC-0271, consuming #1634). Every test name carries the
//! `work_wave_identity` prefix so the suite is greppable as one delivery
//! slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn committed_wave_corpus() -> Result<WorkWaveIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_WAVE_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed wave corpus: {error}"))?;
    load_work_wave_corpus(&body)
}

fn committed_check() -> Result<WorkWaveCheckViewV1, String> {
    let corpus = committed_wave_corpus()?;
    run_work_wave_check(DEFAULT_WORK_WAVE_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkWaveCheckViewV1,
    id: &str,
) -> Result<&'a WorkWaveScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkWaveCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkWaveCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkWaveCheckViewV1,
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

/// The committed corpus compiles, carries the six required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_wave_identity_committed_corpus_six_scenarios_hold() -> Result<(), String> {
    let view = committed_check()?;
    if view.counts.scenarios != 6 {
        return Err(format!(
            "expected six scenarios, got {}",
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

/// Fixture 4 + acceptance 6: two campaigns with independent selected
/// candidates and disjoint exclusive resources are admitted as independent
/// concurrent waves; the read-only `portfolio-schema` and `spec-ledger`
/// references are shareable and never serialize the waves.
#[test]
fn work_wave_identity_disjoint_cross_campaign_waves_pass() -> Result<(), String> {
    let view = committed_check()?;
    let admitted = case(&view, "wave-01-disjoint-cross-campaign-waves-pass", 0)?;
    if admitted.status != "passed" || admitted.plan_state != "admitted" {
        return Err("disjoint cross-campaign waves must be admitted".to_string());
    }
    if admitted.plan_identity.is_none() {
        return Err("an admitted plan must resolve a stable plan identity".to_string());
    }
    let single = case(
        &view,
        "wave-02-single-agent-wave-passes-despite-active-campaigns",
        0,
    )?;
    if single.status != "passed" || single.plan_state != "admitted" {
        return Err("the single-agent wave must be admitted despite active campaigns".to_string());
    }
    Ok(())
}

/// #1634: the retired generic `active-goal` resource fails closed, and so
/// does every non-explicit resource shape.
#[test]
fn work_wave_identity_retired_active_goal_resource_fails_closed() -> Result<(), String> {
    let view = committed_check()?;
    let retired = case(
        &view,
        "wave-04-retired-active-goal-resource-fails-closed",
        0,
    )?;
    if retired.status != "passed" || retired.violations.len() != 2 {
        return Err(format!(
            "scenario 4 must pin exactly two resource violations, got {:?}",
            retired.violations
        ));
    }
    for violation in &retired.violations {
        if violation.law != WorkSelectionLawV1::CampaignRefIdentity
            || violation.route != WorkSelectionRouteV1::ReconcileResources
        {
            return Err(format!(
                "resource violations must pin campaign_ref_identity/reconcile_resources, got {:?}",
                violation
            ));
        }
    }
    let goal_detail = retired
        .violations
        .iter()
        .find(|violation| violation.detail.contains("active-goal"))
        .ok_or_else(|| "the retired resource violation must name active-goal".to_string())?;
    if !goal_detail.detail.contains("explicit resources") {
        return Err(format!(
            "the retired-resource route must name the explicit vocabulary: {}",
            goal_detail.detail
        ));
    }
    Ok(())
}

/// #1634: a stale basis routes recompilation, and the plan must consume
/// the complete selected candidate set — an unassigned candidate fails
/// visibly.
#[test]
fn work_wave_identity_stale_basis_and_unassigned_candidate_fail() -> Result<(), String> {
    let view = committed_check()?;
    has_violation(
        &view,
        "wave-05-stale-basis-and-unassigned-candidate-fail",
        0,
        WorkSelectionLawV1::BasisIdentity,
        WorkSelectionRouteV1::RecompileBasis,
    )?;
    has_violation(
        &view,
        "wave-05-stale-basis-and-unassigned-candidate-fail",
        1,
        WorkSelectionLawV1::SubjectIdentity,
        WorkSelectionRouteV1::ReconcileSelection,
    )?;
    let unassigned = case(
        &view,
        "wave-05-stale-basis-and-unassigned-candidate-fail",
        1,
    )?;
    if !unassigned.violations[0]
        .detail
        .contains("not assigned to any wave")
    {
        return Err(format!(
            "the coverage violation must name the unassigned candidate: {}",
            unassigned.violations[0].detail
        ));
    }
    Ok(())
}

/// #1634: exclusive resources shared between two waves fail with the
/// exact overlap route, and a wave above the declared writer capacity
/// fails with the exact resource route.
#[test]
fn work_wave_identity_resource_conflicts_fail_with_exact_routes() -> Result<(), String> {
    let view = committed_check()?;
    has_violation(
        &view,
        "wave-03-overlapping-exclusive-resources-fail",
        0,
        WorkSelectionLawV1::OverlapVisibility,
        WorkSelectionRouteV1::ReconcileResources,
    )?;
    let overlap = case(&view, "wave-03-overlapping-exclusive-resources-fail", 0)?;
    if overlap.violations.len() != 1 {
        return Err(format!(
            "scenario 3 must pin exactly one shared-resource violation, got {:?}",
            overlap.violations
        ));
    }
    if !overlap.violations[0]
        .detail
        .contains("output-contract:preview-panel")
    {
        return Err(format!(
            "the overlap violation must name the shared resource: {}",
            overlap.violations[0].detail
        ));
    }
    has_violation(
        &view,
        "wave-06-capacity-exceeded-fails",
        0,
        WorkSelectionLawV1::WorktreeIdentity,
        WorkSelectionRouteV1::ReconcileResources,
    )?;
    Ok(())
}

/// The plan schema has no singleton planning state: the committed corpus
/// carries no default-campaign, priority, agent-assignment, CI-wait or
/// progress fields, and the retired `active-goal` spelling appears only
/// inside the pinned negative scenario.
#[test]
fn work_wave_identity_plan_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    let corpus_text = body.to_ascii_lowercase();
    for forbidden in [
        "\"current_campaign\"",
        "\"default_campaign\"",
        "\"priority\"",
        "\"agent_assignment\"",
        "\"ci_wait\"",
        "\"progress_percent\"",
        "\"progress_percentage\"",
    ] {
        if corpus_text.contains(forbidden) {
            return Err(format!(
                "wave corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// Embedded selections must satisfy the shared identity engine: a plan
/// whose embedded selection carries a wrong action fails through the
/// shared ActionIdentity law, not a forked wave-only check.
#[test]
fn work_wave_identity_embedded_selection_reuses_shared_laws() -> Result<(), String> {
    let mut corpus = committed_wave_corpus()?;
    let scenario = corpus
        .scenarios
        .iter_mut()
        .find(|scenario| scenario.id == "wave-01-disjoint-cross-campaign-waves-pass")
        .ok_or_else(|| "missing scenario wave-01".to_string())?;
    scenario.cases[0].plan.selections[0].lifecycle_action =
        crate::work_portfolio::WorkCandidateKindV1::RepairReview;
    scenario.cases[0].plan.selections[0].selection_id =
        "selection:repair_review:issue:1693".to_string();
    scenario.cases[0].plan.waves[0].selections =
        vec!["selection:repair_review:issue:1693".to_string()];
    scenario.cases[0].expect = WorkSelectionExpectV1::Fail;
    scenario.cases[0].expect_violations = vec![WorkSelectionExpectedViolationV1 {
        law: WorkSelectionLawV1::ActionIdentity,
        route: WorkSelectionRouteV1::ReconcileSelection,
    }];
    let view = run_work_wave_check(DEFAULT_WORK_WAVE_CORPUS_DIR, &corpus)?;
    let bad = case(&view, "wave-01-disjoint-cross-campaign-waves-pass", 0)?;
    if bad.status != "passed" || bad.violations.len() != 1 {
        return Err(format!(
            "a wrong embedded action must fail exactly the shared action law, got {:?}",
            bad.violations
        ));
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries.
#[test]
fn work_wave_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_wave_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_wave_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::CampaignRefIdentity,
        WorkSelectionLawV1::BasisIdentity,
        WorkSelectionLawV1::SubjectIdentity,
        WorkSelectionLawV1::OverlapVisibility,
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
fn work_wave_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_wave_check_json(&first)?;
    let second_json = work_wave_check_json(&second)?;
    if first_json != second_json {
        return Err("wave check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("wave check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty plan identity fields.
#[test]
fn work_wave_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_wave_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_wave_corpus(
        r##"{
            "schema_version": "work_wave_identity_corpus.v1",
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
        "\"wave-01-disjoint-cross-campaign-waves-pass\"",
        "\"wave-02-single-agent-wave-passes-despite-active-campaigns\"",
    );
    if load_work_wave_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_wave_corpus(&body.replace(
        "work_wave_identity_corpus.v1",
        "work_wave_identity_corpus.v2",
    ))
    .is_ok()
    {
        return Err("schema drift must fail closed".to_string());
    }
    Ok(())
}

fn committed_corpus_json() -> Result<String, String> {
    let root = workspace_path(DEFAULT_WORK_WAVE_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed provenance covers every corpus byte: the shared validator
/// must report no violation for the committed corpus.
#[test]
fn work_wave_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_wave_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Provenance paths fail closed on traversal spellings.
#[test]
fn work_wave_identity_provenance_fails_closed() -> Result<(), String> {
    let provenance = committed_provenance_json()?;
    let value: serde_json::Value = serde_json::from_str(&provenance)
        .map_err(|error| format!("committed provenance must parse: {error}"))?;
    if value.get("schema_version").and_then(|v| v.as_str())
        != Some("work_wave_identity_provenance.v1")
    {
        return Err("committed provenance must carry the wave schema version".to_string());
    }
    for path in ["/abs/corpus.json", "../corpus.json", "", "..\\corpus.json"] {
        if reject_unsafe_wave_path(path).is_ok() {
            return Err(format!("provenance path `{path}` must fail closed"));
        }
    }
    if reject_unsafe_wave_path("corpus.json").is_err() {
        return Err("relative path `corpus.json` must parse".to_string());
    }
    Ok(())
}

fn committed_provenance_json() -> Result<String, String> {
    let root = workspace_path(DEFAULT_WORK_WAVE_CORPUS_DIR);
    fs::read_to_string(root.join("provenance.json"))
        .map_err(|error| format!("read committed provenance: {error}"))
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_wave_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_wave_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_WAVE_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_wave_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "wave check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-wave-check.json",
            "target/ripr/reports/work-wave-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "wave check wrote outside the reports directory: {key}"
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
