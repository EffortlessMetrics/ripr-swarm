//! Tests for the root synthesis consumer (#1706 PR B, RIPR-SPEC-0273,
//! consuming #1636). Every test name carries the `work_synthesis_identity`
//! prefix so the suite is greppable as one delivery slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::{compile_work_portfolio, load_work_captured_dir};

fn committed_synthesis_corpus() -> Result<WorkSynthesisIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_SYNTHESIS_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed synthesis corpus: {error}"))?;
    load_work_synthesis_corpus(&body)
}

fn committed_check() -> Result<WorkSynthesisCheckViewV1, String> {
    let corpus = committed_synthesis_corpus()?;
    run_work_synthesis_check(DEFAULT_WORK_SYNTHESIS_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkSynthesisCheckViewV1,
    id: &str,
) -> Result<&'a WorkSynthesisScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkSynthesisCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkSynthesisCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkSynthesisCheckViewV1,
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
    let root = workspace_path(DEFAULT_WORK_SYNTHESIS_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed corpus compiles, carries the five required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_synthesis_identity_committed_corpus_five_scenarios_hold() -> Result<(), String> {
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

/// #1636: a synthesis for a covered selected transition passes and binds
/// the transition, the coverage rows, a bounded next action and the
/// derived artifacts.
#[test]
fn work_synthesis_identity_covered_transition_synthesis_passes() -> Result<(), String> {
    let view = committed_check()?;
    let covered = case(&view, "synthesis-01-covered-transition-synthesis-passes", 0)?;
    if covered.status != "passed" || !covered.violations.is_empty() {
        return Err("the covered synthesis must pass cleanly".to_string());
    }
    if covered.synthesis_identity.is_none() {
        return Err("the covered synthesis must resolve a stable identity".to_string());
    }
    if !covered.uncovered_requirements.is_empty() || covered.contradiction_count != 0 {
        return Err("the covered synthesis must record no gaps".to_string());
    }
    Ok(())
}

/// Acceptance 4 + fixture 3: a standalone selection with zero campaign
/// placement still supports an honest synthesis.
#[test]
fn work_synthesis_identity_standalone_synthesis_passes() -> Result<(), String> {
    let view = committed_check()?;
    let standalone = case(&view, "synthesis-02-standalone-synthesis-passes", 0)?;
    if standalone.status != "passed" {
        return Err("the standalone synthesis must pass".to_string());
    }
    let corpus = committed_synthesis_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "synthesis-02-standalone-synthesis-passes")
        .ok_or_else(|| "missing scenario synthesis-02".to_string())?;
    if !scenario.cases[0]
        .synthesis
        .selection
        .campaign_refs
        .is_empty()
    {
        return Err("the standalone synthesis must bind zero campaign refs".to_string());
    }
    Ok(())
}

/// Fixture 10 + acceptance: a synthesis must surface a partial coverage gap
/// as a named contradiction instead of hiding it behind a green row.
#[test]
fn work_synthesis_identity_partial_coverage_contradiction_surfaced() -> Result<(), String> {
    let view = committed_check()?;
    let partial = case(
        &view,
        "synthesis-03-partial-coverage-contradiction-surfaced",
        0,
    )?;
    if partial.status != "passed" {
        return Err("the partial synthesis must pass with its gap surfaced".to_string());
    }
    if partial.uncovered_requirements.is_empty() {
        return Err("the partial synthesis must record uncovered rows".to_string());
    }
    if partial.contradiction_count == 0 {
        return Err("the partial synthesis must record contradictions".to_string());
    }
    Ok(())
}

/// #1636: a synthesis cannot hide an uncovered row, silently switch
/// campaign, report a different transition than the selected one, or derive
/// from nothing — each fails with its exact law/route pair.
#[test]
fn work_synthesis_identity_hidden_gap_silent_switch_and_wrong_inputs_fail() -> Result<(), String> {
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
            WorkSelectionLawV1::ActionIdentity,
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
            "synthesis-04-hidden-gap-silent-switch-and-wrong-inputs-fail",
            index,
            law,
            route,
        )?;
    }
    Ok(())
}

/// Fixture 6 + acceptance: legacy refs on a synthesis resolve read-only
/// with migration posture and grant no authority.
#[test]
fn work_synthesis_identity_legacy_refs_read_only_no_authority() -> Result<(), String> {
    let view = committed_check()?;
    let legacy = case(&view, "synthesis-05-legacy-read-only-migration-posture", 0)?;
    if legacy.status != "passed" || !legacy.violations.is_empty() {
        return Err("the read-only legacy synthesis must pass".to_string());
    }
    let posture = legacy
        .migration_posture
        .iter()
        .find(|line| line.contains("legacy_active_goal_ref"))
        .ok_or_else(|| "the legacy synthesis must emit migration posture".to_string())?;
    if !posture.contains("no write, readiness, merge or closeout authority") {
        return Err(format!(
            "migration posture must record the no-authority grant: {posture}"
        ));
    }
    Ok(())
}

/// #1636: the schema has no field that could create another selected issue
/// or switch campaign silently; singleton fields stay absent.
#[test]
fn work_synthesis_identity_schema_creates_no_selection() -> Result<(), String> {
    let body = committed_corpus_json()?;
    if !body.contains("\"synthesis_id\"") {
        return Err("committed corpus must contain at least one synthesis row".to_string());
    }
    let corpus_text = body.to_ascii_lowercase();
    for forbidden in [
        "\"active_goal\"",
        "\"current_campaign\"",
        "\"agent_assignment\"",
        "\"ci_wait\"",
        "\"progress_percent\"",
        "\"progress_percentage\"",
        "\"new_selection\"",
        "\"created_selection\"",
        "\"additional_selection\"",
    ] {
        if corpus_text.contains(forbidden) {
            return Err(format!(
                "synthesis corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// #1636: every synthesis embeds the full selected-work identity, so a
/// synthesis whose embedded selection the RIPR-SPEC-0244 engine rejects
/// fails through the shared laws rather than a forked synthesis-only check.
#[test]
fn work_synthesis_identity_embedded_selection_reuses_shared_laws() -> Result<(), String> {
    let corpus = committed_synthesis_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "synthesis-01-covered-transition-synthesis-passes")
        .ok_or_else(|| "missing scenario synthesis-01".to_string())?;
    let mut tampered = scenario.cases[0].synthesis.clone();
    // Tamper with the embedded selection's basis: the shared BasisIdentity
    // law must fire through the synthesis check.
    tampered.selection.basis_sha = "0".repeat(40);
    tampered.basis_sha = "0".repeat(40);
    let dir = resolve_scenario_captured(
        &corpus_root(DEFAULT_WORK_SYNTHESIS_CORPUS_DIR),
        &scenario.captured,
    )?;
    let captured = load_work_captured_dir(&dir)?;
    let snapshot = compile_work_portfolio(&captured, None)?;
    let basis = build_portfolio_basis(
        &snapshot,
        &corpus.selection_policy,
        &corpus.selection_policy_version,
    );
    let evaluation = check_synthesis(&tampered, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|violation| violation.law == WorkSelectionLawV1::BasisIdentity)
    {
        return Err("tampered embedded selection must fail the shared basis law".to_string());
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries.
#[test]
fn work_synthesis_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_synthesis_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_synthesis_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionLawV1::ActionIdentity,
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
fn work_synthesis_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_synthesis_check_json(&first)?;
    let second_json = work_synthesis_check_json(&second)?;
    if first_json != second_json {
        return Err("synthesis check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("synthesis check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty synthesis identity fields.
#[test]
fn work_synthesis_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_synthesis_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_synthesis_corpus(
        r##"{
            "schema_version": "work_synthesis_identity_corpus.v1",
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
        "\"synthesis-01-covered-transition-synthesis-passes\"",
        "\"synthesis-02-standalone-synthesis-passes\"",
    );
    if load_work_synthesis_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_synthesis_corpus(&body.replace(
        "work_synthesis_identity_corpus.v1",
        "work_synthesis_identity_corpus.v2",
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
fn work_synthesis_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_synthesis_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Contradiction-honesty law detail: a Contradicted row must also be named
/// in the contradictions list, and a synthesis naming a contradiction that
/// matches the row requirement stays honest.
#[test]
fn work_synthesis_identity_contradiction_rows_must_be_named() -> Result<(), String> {
    let corpus = committed_synthesis_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "synthesis-03-partial-coverage-contradiction-surfaced")
        .ok_or_else(|| "missing scenario synthesis-03".to_string())?;
    for row in &scenario.cases[0].synthesis.acceptance_coverage {
        if row.status != WorkAcceptanceStatusV1::Covered
            && !scenario.cases[0]
                .synthesis
                .contradictions
                .iter()
                .any(|contradiction| contradiction.contains(&row.requirement))
        {
            return Err(format!(
                "uncovered row `{}` must be named in the contradictions list",
                row.requirement
            ));
        }
    }
    Ok(())
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_synthesis_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_synthesis_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_SYNTHESIS_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_synthesis_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "synthesis check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-synthesis-check.json",
            "target/ripr/reports/work-synthesis-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "synthesis check wrote outside the reports directory: {key}"
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
