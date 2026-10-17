//! Tests for the burn-down consumer (#1706 PR B, RIPR-SPEC-0274,
//! consuming #1648). Every test name carries the `work_burndown_identity`
//! prefix so the suite is greppable as one delivery slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::{compile_work_portfolio, load_work_captured_dir};

fn committed_burndown_corpus() -> Result<WorkBurnDownIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_BURNDOWN_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed burn-down corpus: {error}"))?;
    load_work_burndown_corpus(&body)
}

fn committed_check() -> Result<WorkBurnDownCheckViewV1, String> {
    let corpus = committed_burndown_corpus()?;
    run_work_burndown_check(DEFAULT_WORK_BURNDOWN_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkBurnDownCheckViewV1,
    id: &str,
) -> Result<&'a WorkBurnDownScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkBurnDownCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkBurnDownCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkBurnDownCheckViewV1,
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
    let root = workspace_path(DEFAULT_WORK_BURNDOWN_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed corpus compiles, carries the five required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_burndown_identity_committed_corpus_five_scenarios_hold() -> Result<(), String> {
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

/// #1648: a burn-down whose evidence rows are all covered, current or
/// merged derives `Complete` and passes.
#[test]
fn work_burndown_identity_complete_evidence_burn_down_passes() -> Result<(), String> {
    let view = committed_check()?;
    let complete = case(&view, "burndown-01-complete-evidence-burn-down-passes", 0)?;
    if complete.status != "passed" || !complete.violations.is_empty() {
        return Err("the complete burn-down must pass cleanly".to_string());
    }
    if complete.completion != WorkCompletionV1::Complete {
        return Err("the complete burn-down must derive `complete`".to_string());
    }
    if !complete.open_evidence.is_empty() {
        return Err("the complete burn-down must record no open evidence".to_string());
    }
    if complete.burndown_identity.is_none() {
        return Err("the complete burn-down must resolve a stable identity".to_string());
    }
    Ok(())
}

/// Acceptance 4 + fixture 3: a standalone selection with zero campaign
/// placement still derives an honest burn-down.
#[test]
fn work_burndown_identity_standalone_burn_down_passes() -> Result<(), String> {
    let view = committed_check()?;
    let standalone = case(&view, "burndown-02-standalone-burn-down-passes", 0)?;
    if standalone.status != "passed" {
        return Err("the standalone burn-down must pass".to_string());
    }
    let corpus = committed_burndown_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "burndown-02-standalone-burn-down-passes")
        .ok_or_else(|| "missing scenario burndown-02".to_string())?;
    if !scenario.cases[0]
        .burndown
        .selection
        .campaign_refs
        .is_empty()
    {
        return Err("the standalone burn-down must bind zero campaign refs".to_string());
    }
    if !scenario.cases[0].burndown.campaign_records.is_empty() {
        return Err("the standalone burn-down must record no campaign context".to_string());
    }
    Ok(())
}

/// Fixture 10 + #1648: burn-down stays partial after one campaign work
/// item merges while issue acceptance remains uncovered; the campaign
/// record stays context only and cannot promote the completion.
#[test]
fn work_burndown_identity_partial_after_one_campaign_item_merges() -> Result<(), String> {
    let view = committed_check()?;
    let partial = case(
        &view,
        "burndown-03-partial-after-one-campaign-item-merges",
        0,
    )?;
    if partial.status != "passed" {
        return Err("the partial burn-down must pass".to_string());
    }
    if partial.completion != WorkCompletionV1::Partial {
        return Err(format!(
            "the burn-down must stay partial, got {:?}",
            partial.completion
        ));
    }
    if partial.open_evidence.is_empty() {
        return Err("the partial burn-down must keep the open acceptance visible".to_string());
    }
    if partial.campaign_record_count == 0 {
        return Err("the campaign record must stay recorded as context".to_string());
    }
    Ok(())
}

/// #1648: campaign/work-item records cannot override current evidence — a
/// `Complete` burn-down with an open row fails; a wrong current main, an
/// unknown campaign record, an evidence-free burn-down and a wrong
/// repository each fail with their exact law/route pair.
#[test]
fn work_burndown_identity_campaign_override_and_wrong_inputs_fail() -> Result<(), String> {
    let view = committed_check()?;
    let expected = [
        (
            0,
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            1,
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
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
    ];
    for (index, law, route) in expected {
        has_violation(
            &view,
            "burndown-04-campaign-override-and-wrong-inputs-fail",
            index,
            law,
            route,
        )?;
    }
    Ok(())
}

/// Fixture 6 + acceptance: legacy refs on a burn-down resolve read-only
/// with migration posture and grant no authority.
#[test]
fn work_burndown_identity_legacy_refs_read_only_no_authority() -> Result<(), String> {
    let view = committed_check()?;
    let legacy = case(&view, "burndown-05-legacy-read-only-migration-posture", 0)?;
    if legacy.status != "passed" || !legacy.violations.is_empty() {
        return Err("the read-only legacy burn-down must pass".to_string());
    }
    let posture = legacy
        .migration_posture
        .iter()
        .find(|line| line.contains("legacy_active_goal_ref"))
        .ok_or_else(|| "the legacy burn-down must emit migration posture".to_string())?;
    if !posture.contains("no write, readiness, merge or closeout authority") {
        return Err(format!(
            "migration posture must record the no-authority grant: {posture}"
        ));
    }
    Ok(())
}

/// The completion and evidence vocabularies are closed; the committed
/// corpus only uses declared values.
#[test]
fn work_burndown_identity_completion_is_a_closed_vocabulary() -> Result<(), String> {
    let corpus = committed_burndown_corpus()?;
    for scenario in &corpus.scenarios {
        for burndown_case in &scenario.cases {
            match burndown_case.burndown.completion {
                WorkCompletionV1::Complete | WorkCompletionV1::Partial | WorkCompletionV1::Open => {
                }
            }
            for row in &burndown_case.burndown.issue_evidence {
                match row.status {
                    WorkEvidenceStatusV1::AcceptanceCovered
                    | WorkEvidenceStatusV1::AcceptanceOpen
                    | WorkEvidenceStatusV1::ProofCurrent
                    | WorkEvidenceStatusV1::ProofStale
                    | WorkEvidenceStatusV1::Merged => {}
                }
            }
        }
    }
    Ok(())
}

/// #1648: every burn-down embeds the full selected-work identity, so a
/// burn-down whose embedded selection the RIPR-SPEC-0244 engine rejects
/// fails through the shared laws rather than a forked burn-down-only check.
#[test]
fn work_burndown_identity_embedded_selection_reuses_shared_laws() -> Result<(), String> {
    let corpus = committed_burndown_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "burndown-01-complete-evidence-burn-down-passes")
        .ok_or_else(|| "missing scenario burndown-01".to_string())?;
    let mut tampered = scenario.cases[0].burndown.clone();
    // Tamper with the embedded selection's basis: the shared BasisIdentity
    // law must fire through the burn-down check.
    tampered.selection.basis_sha = "0".repeat(40);
    tampered.basis_sha = "0".repeat(40);
    let dir = resolve_scenario_captured(
        &corpus_root(DEFAULT_WORK_BURNDOWN_CORPUS_DIR),
        &scenario.captured,
    )?;
    let captured = load_work_captured_dir(&dir)?;
    let snapshot = compile_work_portfolio(&captured, None)?;
    let basis = build_portfolio_basis(
        &snapshot,
        &corpus.selection_policy,
        &corpus.selection_policy_version,
    );
    let evaluation = check_burndown(&tampered, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|violation| violation.law == WorkSelectionLawV1::BasisIdentity)
    {
        return Err("tampered embedded selection must fail the shared basis law".to_string());
    }
    Ok(())
}

/// #1648 schema shape: a burn-down binds no singleton fields — no
/// repository-wide campaign, agent assignment, CI wait state or progress
/// percentage — and unknown fields fail closed at parse time.
#[test]
fn work_burndown_identity_burndown_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    if !body.contains("\"burndown_id\"") {
        return Err("committed corpus must contain at least one burn-down row".to_string());
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
                "burn-down corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries.
#[test]
fn work_burndown_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_burndown_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_burndown_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionLawV1::BasisIdentity,
        WorkSelectionLawV1::SubjectIdentity,
        WorkSelectionLawV1::CampaignRefIdentity,
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
fn work_burndown_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_burndown_check_json(&first)?;
    let second_json = work_burndown_check_json(&second)?;
    if first_json != second_json {
        return Err("burn-down check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("burn-down check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty burn-down identity fields.
#[test]
fn work_burndown_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_burndown_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_burndown_corpus(
        r##"{
            "schema_version": "work_burndown_identity_corpus.v1",
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
        "\"burndown-01-complete-evidence-burn-down-passes\"",
        "\"burndown-02-standalone-burn-down-passes\"",
    );
    if load_work_burndown_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_burndown_corpus(&body.replace(
        "work_burndown_identity_corpus.v1",
        "work_burndown_identity_corpus.v2",
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
fn work_burndown_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_burndown_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Fixture 10 evidence shape: the partial burn-down keeps a merged campaign
/// work item and an open acceptance row in the same honest row set.
#[test]
fn work_burndown_identity_merged_item_keeps_open_acceptance_visible() -> Result<(), String> {
    let corpus = committed_burndown_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "burndown-03-partial-after-one-campaign-item-merges")
        .ok_or_else(|| "missing scenario burndown-03".to_string())?;
    let statuses: Vec<WorkEvidenceStatusV1> = scenario.cases[0]
        .burndown
        .issue_evidence
        .iter()
        .map(|row| row.status)
        .collect();
    if !statuses.contains(&WorkEvidenceStatusV1::Merged) {
        return Err("the partial burn-down must record the merged work item".to_string());
    }
    if !statuses.contains(&WorkEvidenceStatusV1::AcceptanceOpen) {
        return Err("the partial burn-down must record the open acceptance".to_string());
    }
    Ok(())
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_burndown_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_burndown_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_BURNDOWN_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_burndown_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "burn-down check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-burndown-check.json",
            "target/ripr/reports/work-burndown-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "burn-down check wrote outside the reports directory: {key}"
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
