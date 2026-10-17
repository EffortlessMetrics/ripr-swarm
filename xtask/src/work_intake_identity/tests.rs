//! Tests for the issue intake consumer (#1706 PR B, RIPR-SPEC-0275,
//! consuming #1644). Every test name carries the `work_intake_identity`
//! prefix so the suite is greppable as one delivery slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::load_work_captured_dir;

fn committed_intake_corpus() -> Result<WorkIntakeIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_INTAKE_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed intake corpus: {error}"))?;
    load_work_intake_corpus(&body)
}

fn committed_check() -> Result<WorkIntakeCheckViewV1, String> {
    let corpus = committed_intake_corpus()?;
    run_work_intake_check(DEFAULT_WORK_INTAKE_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkIntakeCheckViewV1,
    id: &str,
) -> Result<&'a WorkIntakeScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkIntakeCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkIntakeCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkIntakeCheckViewV1,
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
    let root = workspace_path(DEFAULT_WORK_INTAKE_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed corpus compiles, carries the five required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_intake_identity_committed_corpus_five_scenarios_hold() -> Result<(), String> {
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

/// #1644 + fixture 1: an issue recorded in one campaign qualifies with
/// placement, relevant campaigns, accepted contracts and derived sources.
#[test]
fn work_intake_identity_in_campaign_qualified_passes() -> Result<(), String> {
    let view = committed_check()?;
    let intake = case(&view, "intake-01-in-campaign-qualified-passes", 0)?;
    if intake.status != "passed" || !intake.violations.is_empty() {
        return Err("the in-campaign intake must pass cleanly".to_string());
    }
    if intake.qualification != WorkQualificationV1::Qualified {
        return Err("the in-campaign intake must be qualified".to_string());
    }
    if intake.intake_identity.is_none() {
        return Err("the in-campaign intake must resolve a stable identity".to_string());
    }
    Ok(())
}

/// Acceptance + #1644: an issue outside every campaign may still qualify;
/// placement remains a later root/planning decision, never a triage
/// blocker (RIPR-SPEC-0244 acceptance 4).
#[test]
fn work_intake_identity_out_of_campaign_still_qualifies() -> Result<(), String> {
    let view = committed_check()?;
    let standalone = case(&view, "intake-02-out-of-campaign-still-qualifies", 0)?;
    if standalone.status != "passed" {
        return Err("the out-of-campaign intake must pass".to_string());
    }
    let corpus = committed_intake_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "intake-02-out-of-campaign-still-qualifies")
        .ok_or_else(|| "missing scenario intake-02".to_string())?;
    if scenario.cases[0].intake.campaign_placement.is_some() {
        return Err("the standalone intake must carry no campaign placement".to_string());
    }
    if !scenario.cases[0].intake.relevant_campaigns.is_empty() {
        return Err("the standalone intake must carry no relevant campaigns".to_string());
    }
    if scenario.cases[0].intake.qualification != WorkQualificationV1::Qualified {
        return Err("the standalone intake must still qualify".to_string());
    }
    Ok(())
}

/// Fixtures 5 and 12: an intake for review/repair of an existing open PR
/// keeps the overlap visible, and a narrow one-file issue stays
/// single-agent.
#[test]
fn work_intake_identity_overlap_visible_and_single_agent() -> Result<(), String> {
    let view = committed_check()?;
    let repair = case(&view, "intake-03-pr-repair-intake-overlap-visible", 0)?;
    if repair.status != "passed" {
        return Err("the PR-repair intake must pass".to_string());
    }
    if repair.overlap_count == 0 {
        return Err("the PR-repair intake must keep the open overlap visible".to_string());
    }
    let narrow = case(&view, "intake-04-narrow-single-agent-intake-passes", 0)?;
    if narrow.status != "passed" {
        return Err("the narrow intake must pass".to_string());
    }
    let corpus = committed_intake_corpus()?;
    let narrow_scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "intake-04-narrow-single-agent-intake-passes")
        .ok_or_else(|| "missing scenario intake-04".to_string())?;
    if !narrow_scenario.cases[0].intake.single_agent {
        return Err("the narrow intake must stay single-agent".to_string());
    }
    Ok(())
}

/// Fixture 6 + #1644: a legacy active-goal ref that cannot resolve to a
/// recorded campaign fails; an unknown placement, an unknown relevant
/// campaign, an empty contract set, an unknown issue and a wrong
/// repository each fail with their exact law/route pair.
#[test]
fn work_intake_identity_legacy_and_wrong_inputs_fail() -> Result<(), String> {
    let view = committed_check()?;
    let expected = [
        (
            0,
            WorkSelectionLawV1::LegacyCompatibility,
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
            WorkSelectionLawV1::SubjectIdentity,
            WorkSelectionRouteV1::ReconcileSelection,
        ),
        (
            5,
            WorkSelectionLawV1::RepositoryIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
        (
            6,
            WorkSelectionLawV1::BasisIdentity,
            WorkSelectionRouteV1::RecompileBasis,
        ),
    ];
    for (index, law, route) in expected {
        has_violation(
            &view,
            "intake-05-legacy-and-wrong-inputs-fail",
            index,
            law,
            route,
        )?;
    }
    Ok(())
}

/// #1644 schema shape: an intake binds no singleton fields — no
/// repository-wide campaign, agent assignment, CI wait state or progress
/// percentage — and unknown fields fail closed at parse time.
#[test]
fn work_intake_identity_intake_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    if !body.contains("\"intake_id\"") {
        return Err("committed corpus must contain at least one intake row".to_string());
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
                "intake corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries.
#[test]
fn work_intake_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_intake_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_intake_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionLawV1::BasisIdentity,
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
fn work_intake_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_intake_check_json(&first)?;
    let second_json = work_intake_check_json(&second)?;
    if first_json != second_json {
        return Err("intake check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("intake check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty intake identity fields.
#[test]
fn work_intake_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_intake_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_intake_corpus(
        r##"{
            "schema_version": "work_intake_identity_corpus.v1",
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
        "\"intake-01-in-campaign-qualified-passes\"",
        "\"intake-02-out-of-campaign-still-qualifies\"",
    );
    if load_work_intake_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_intake_corpus(&body.replace(
        "work_intake_identity_corpus.v1",
        "work_intake_identity_corpus.v2",
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
fn work_intake_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_intake_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Intake law detail: every issue the committed corpus intakes is recorded
/// in the captured issue source of its scenario.
#[test]
fn work_intake_identity_intakes_bind_recorded_issues() -> Result<(), String> {
    let corpus = committed_intake_corpus()?;
    let root = corpus_root(DEFAULT_WORK_INTAKE_CORPUS_DIR);
    for scenario in &corpus.scenarios {
        let dir = resolve_scenario_captured(&root, &scenario.captured)?;
        let captured = load_work_captured_dir(&dir)?;
        for intake_case in &scenario.cases {
            if intake_case.expect == WorkSelectionExpectV1::Pass
                && !captured_issue_known(&captured, intake_case.intake.issue_number)
            {
                return Err(format!(
                    "scenario `{}` intakes issue {} which the captured source does not record",
                    scenario.id,
                    issue_identity(intake_case.intake.issue_number)
                ));
            }
        }
    }
    Ok(())
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_intake_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_intake_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_INTAKE_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_intake_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "intake check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-intake-check.json",
            "target/ripr/reports/work-intake-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "intake check wrote outside the reports directory: {key}"
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
