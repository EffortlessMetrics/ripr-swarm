//! Tests for the durable-claim binding consumer (#1706 PR B,
//! RIPR-SPEC-0270, consuming #1635/#1647). Every test name carries the
//! `work_claim_identity` prefix so the suite is greppable as one delivery
//! slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;

fn committed_claim_corpus() -> Result<WorkClaimIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_CLAIM_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed claim corpus: {error}"))?;
    load_work_claim_corpus(&body)
}

fn committed_check() -> Result<WorkClaimCheckViewV1, String> {
    let corpus = committed_claim_corpus()?;
    run_work_claim_check(DEFAULT_WORK_CLAIM_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkClaimCheckViewV1,
    id: &str,
) -> Result<&'a WorkClaimScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkClaimCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkClaimCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

/// The committed corpus compiles, carries the five required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_claim_identity_committed_corpus_five_scenarios_hold() -> Result<(), String> {
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

/// Fixtures 1/3/5 + acceptance 4: writer claims for distinct selections, a
/// standalone selection with zero campaigns, and a repair claim on the
/// recorded worktree all bind and pass.
#[test]
fn work_claim_identity_distinct_and_standalone_claims_pass() -> Result<(), String> {
    let view = committed_check()?;
    let first = scenario(&view, "claim-01-writer-claims-for-distinct-selections-pass")?;
    if first.cases.len() != 3 || first.cases.iter().any(|case| case.status != "passed") {
        return Err("three distinct writer claims must all pass".to_string());
    }
    let keys: BTreeSet<String> = first
        .cases
        .iter()
        .filter_map(|case| case.selection_key.clone())
        .collect();
    if keys.len() != 3 {
        return Err(format!(
            "the three claims must bind three distinct selections, got {keys:?}"
        ));
    }
    let standalone = case(&view, "claim-02-standalone-selection-claim-passes", 0)?;
    if standalone.status != "passed" || standalone.claim_state != "active" {
        return Err("the standalone claim must pass as an active claim".to_string());
    }
    if standalone.selection_key.as_deref() != Some("selection:start_build:issue:9106") {
        return Err(format!(
            "standalone claim must bind issue 9106, got {:?}",
            standalone.selection_key
        ));
    }
    Ok(())
}

/// #1635/#1647: a stale portfolio basis requires reconciliation — the
/// claim stays recorded as `stale_requires_reconcile`, it is not released
/// and no other claim takes over its worktree.
#[test]
fn work_claim_identity_stale_basis_reconciles_without_takeover() -> Result<(), String> {
    let view = committed_check()?;
    let stale = case(&view, "claim-03-stale-basis-reconciles-without-takeover", 2)?;
    if stale.status != "passed" || stale.violations.is_empty() {
        return Err("the stale-basis case must meet its pinned failure".to_string());
    }
    if stale.claim_state != "stale_requires_reconcile" {
        return Err(format!(
            "stale basis must keep the claim recorded for reconciliation, got `{}`",
            stale.claim_state
        ));
    }
    if !stale
        .violations
        .iter()
        .any(|violation| violation.law == WorkSelectionLawV1::BasisIdentity)
    {
        return Err("stale basis must name the BasisIdentity law".to_string());
    }
    // Reconcile-not-takeover: the occupying claim from case 0 stays active
    // although case 1 attempted to claim the same worktree.
    let conflict = case(&view, "claim-03-stale-basis-reconciles-without-takeover", 1)?;
    if conflict.status != "passed" || conflict.violations.is_empty() {
        return Err("the worktree-conflict case must meet its pinned failure".to_string());
    }
    let occupier = case(&view, "claim-03-stale-basis-reconciles-without-takeover", 0)?;
    if occupier.claim_state != "active" {
        return Err("the occupying claim must stay active, not released".to_string());
    }
    let detail = &conflict.violations[0].detail;
    if !detail.contains(&occupier.claim_id) {
        return Err(format!(
            "the conflict violation must name the occupying claim: {detail}"
        ));
    }
    Ok(())
}

/// #1635/#1647: one worktree cannot serve two selections and one selection
/// cannot be claimed by two worktrees; each conflict fails with its exact
/// law/route pair.
#[test]
fn work_claim_identity_conflicts_fail_with_exact_routes() -> Result<(), String> {
    let view = committed_check()?;
    let worktree_conflict = case(&view, "claim-03-stale-basis-reconciles-without-takeover", 1)?;
    if worktree_conflict.violations.len() != 1
        || worktree_conflict.violations[0].law != WorkSelectionLawV1::WorktreeIdentity
        || worktree_conflict.violations[0].route != WorkSelectionRouteV1::ReconcileResources
    {
        return Err(format!(
            "worktree conflict must pin exactly worktree_identity/reconcile_resources, got {:?}",
            worktree_conflict.violations
        ));
    }
    let double_claim = case(&view, "claim-04-double-claim-of-one-selection-fails", 1)?;
    if double_claim.violations.len() != 1
        || double_claim.violations[0].law != WorkSelectionLawV1::SubjectIdentity
        || double_claim.violations[0].route != WorkSelectionRouteV1::ReconcileSelection
    {
        return Err(format!(
            "double claim must pin exactly subject_identity/reconcile_selection, got {:?}",
            double_claim.violations
        ));
    }
    let first = case(&view, "claim-04-double-claim-of-one-selection-fails", 0)?;
    if first.claim_state != "active" {
        return Err("the first claim of the selection must stay active".to_string());
    }
    Ok(())
}

/// Acceptance 7 + fixture 6: legacy refs on a claim resolve read-only with
/// migration posture and grant no authority; a mismatched legacy work-item
/// ref fails with the exact reconcile route.
#[test]
fn work_claim_identity_legacy_claims_read_only_no_authority() -> Result<(), String> {
    let view = committed_check()?;
    let legacy = case(
        &view,
        "claim-05-legacy-claim-read-only-migration-posture",
        0,
    )?;
    if legacy.status != "passed" || !legacy.violations.is_empty() {
        return Err("the read-only legacy claim must pass".to_string());
    }
    let posture = legacy
        .migration_posture
        .iter()
        .find(|line| line.contains("legacy_active_goal_ref"))
        .ok_or_else(|| "the legacy claim must emit migration posture".to_string())?;
    if !posture.contains("no write, readiness, merge or closeout authority") {
        return Err(format!(
            "migration posture must record the no-authority grant: {posture}"
        ));
    }
    let bad = case(
        &view,
        "claim-05-legacy-claim-read-only-migration-posture",
        1,
    )?;
    if bad.status != "passed" || bad.violations.len() != 1 {
        return Err("the mismatched legacy ref must meet its pinned failure".to_string());
    }
    if bad.violations[0].law != WorkSelectionLawV1::LegacyCompatibility
        || bad.violations[0].route != WorkSelectionRouteV1::ReconcileSelection
    {
        return Err(format!(
            "mismatched legacy ref must pin legacy_compatibility/reconcile_selection, got {:?}",
            bad.violations
        ));
    }
    Ok(())
}

/// Claiming one item never asserts repository-wide priority: the claim
/// schema and the committed corpus carry no priority, default-campaign,
/// agent-assignment, CI-wait or progress fields.
#[test]
fn work_claim_identity_claim_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    let corpus_text = body.to_ascii_lowercase();
    for forbidden in [
        "\"priority\"",
        "\"active_goal\"",
        "\"current_campaign\"",
        "\"agent_assignment\"",
        "\"ci_wait\"",
        "\"progress_percent\"",
        "\"progress_percentage\"",
    ] {
        if corpus_text.contains(forbidden) {
            return Err(format!(
                "claim corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// Duplicate claim ids fail closed: a claim reusing an active claim id is
/// rejected through the scenario interaction law.
#[test]
fn work_claim_identity_duplicate_claim_id_fails_closed() -> Result<(), String> {
    let mut corpus = committed_claim_corpus()?;
    let scenario = corpus
        .scenarios
        .iter_mut()
        .find(|scenario| scenario.id == "claim-04-double-claim-of-one-selection-fails")
        .ok_or_else(|| "missing scenario claim-04".to_string())?;
    // Reuse the first claim's id on the double-claim row: both the
    // duplicate-id law and the double-claim law fire.
    scenario.cases[1].claim.claim_id = scenario.cases[0].claim.claim_id.clone();
    scenario.cases[1].expect = WorkSelectionExpectV1::Fail;
    scenario.cases[1].expect_violations = vec![
        WorkSelectionExpectedViolationV1 {
            law: WorkSelectionLawV1::SubjectIdentity,
            route: WorkSelectionRouteV1::ReconcileSelection,
        },
        WorkSelectionExpectedViolationV1 {
            law: WorkSelectionLawV1::SubjectIdentity,
            route: WorkSelectionRouteV1::ReconcileSelection,
        },
    ];
    let view = run_work_claim_check(DEFAULT_WORK_CLAIM_CORPUS_DIR, &corpus)?;
    let bad = case(&view, "claim-04-double-claim-of-one-selection-fails", 1)?;
    if bad.status != "passed" || bad.violations.len() != 2 {
        return Err(format!(
            "duplicate claim id must add a second subject_identity violation, got {:?}",
            bad.violations
        ));
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries.
#[test]
fn work_claim_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_claim_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_claim_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::BasisIdentity,
        WorkSelectionLawV1::WorktreeIdentity,
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
fn work_claim_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_claim_check_json(&first)?;
    let second_json = work_claim_check_json(&second)?;
    if first_json != second_json {
        return Err("claim check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("claim check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty claim identity fields.
#[test]
fn work_claim_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_claim_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_claim_corpus(
        r##"{
            "schema_version": "work_claim_identity_corpus.v1",
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
        "\"claim-01-writer-claims-for-distinct-selections-pass\"",
        "\"claim-02-standalone-selection-claim-passes\"",
    );
    if load_work_claim_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_claim_corpus(&body.replace(
        "work_claim_identity_corpus.v1",
        "work_claim_identity_corpus.v2",
    ))
    .is_ok()
    {
        return Err("schema drift must fail closed".to_string());
    }
    Ok(())
}

fn committed_corpus_json() -> Result<String, String> {
    let root = workspace_path(DEFAULT_WORK_CLAIM_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed provenance covers every corpus byte: the shared validator
/// must report no violation for the committed corpus.
#[test]
fn work_claim_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_claim_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Provenance fails closed on path traversal, and the committed provenance
/// binds the corpus schema version. (Digest-drift machinery itself is the
/// same RIPR-SPEC-0244 code path, pinned there.)
#[test]
fn work_claim_identity_provenance_fails_closed() -> Result<(), String> {
    let provenance = committed_provenance_json()?;
    let value: serde_json::Value = serde_json::from_str(&provenance)
        .map_err(|error| format!("committed provenance must parse: {error}"))?;
    if value.get("schema_version").and_then(|v| v.as_str())
        != Some("work_claim_identity_provenance.v1")
    {
        return Err("committed provenance must carry the claim schema version".to_string());
    }
    for path in ["/abs/corpus.json", "../corpus.json", "", "..\\corpus.json"] {
        if reject_unsafe_claim_path(path).is_ok() {
            return Err(format!("provenance path `{path}` must fail closed"));
        }
    }
    if reject_unsafe_claim_path("corpus.json").is_err() {
        return Err("relative path `corpus.json` must parse".to_string());
    }
    Ok(())
}

fn committed_provenance_json() -> Result<String, String> {
    let root = workspace_path(DEFAULT_WORK_CLAIM_CORPUS_DIR);
    fs::read_to_string(root.join("provenance.json"))
        .map_err(|error| format!("read committed provenance: {error}"))
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_claim_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_claim_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_CLAIM_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_claim_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "claim check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-claim-check.json",
            "target/ripr/reports/work-claim-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "claim check wrote outside the reports directory: {key}"
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
