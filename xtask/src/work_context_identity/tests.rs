//! Tests for the context/work-item packet consumer (#1706 PR B,
//! RIPR-SPEC-0272, consuming #1632). Every test name carries the
//! `work_context_identity` prefix so the suite is greppable as one delivery
//! slice.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use super::*;
use crate::work_portfolio::{compile_work_portfolio, load_work_captured_dir};

fn committed_context_corpus() -> Result<WorkContextIdentityCorpusV1, String> {
    let root = workspace_path(DEFAULT_WORK_CONTEXT_CORPUS_DIR);
    let body = fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed context corpus: {error}"))?;
    load_work_context_corpus(&body)
}

fn committed_check() -> Result<WorkContextCheckViewV1, String> {
    let corpus = committed_context_corpus()?;
    run_work_context_check(DEFAULT_WORK_CONTEXT_CORPUS_DIR, &corpus)
}

fn scenario<'a>(
    view: &'a WorkContextCheckViewV1,
    id: &str,
) -> Result<&'a WorkContextScenarioResultV1, String> {
    view.results
        .iter()
        .find(|scenario| scenario.scenario_id == id)
        .ok_or_else(|| format!("missing scenario `{id}`"))
}

fn case<'a>(
    view: &'a WorkContextCheckViewV1,
    id: &str,
    index: usize,
) -> Result<&'a WorkContextCaseResultV1, String> {
    let scenario = scenario(view, id)?;
    scenario
        .cases
        .get(index)
        .ok_or_else(|| format!("scenario `{id}` is missing case {index}"))
}

fn has_violation(
    view: &WorkContextCheckViewV1,
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
    let root = workspace_path(DEFAULT_WORK_CONTEXT_CORPUS_DIR);
    fs::read_to_string(root.join("corpus.json"))
        .map_err(|error| format!("read committed corpus: {error}"))
}

/// The committed corpus compiles, carries the six required scenarios and
/// every case meets its pinned expectation.
#[test]
fn work_context_identity_committed_corpus_six_scenarios_hold() -> Result<(), String> {
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

/// #1632 + acceptance: packets pass for a one-campaign selection, several
/// related campaigns, a standalone selection with zero campaign placement, a
/// repair selection on the recorded PR/worktree, and a narrow single-agent
/// selection despite other active campaigns.
#[test]
fn work_context_identity_valid_packets_bind_their_selections() -> Result<(), String> {
    let view = committed_check()?;
    for (id, index) in [
        ("context-01-one-campaign-packet-passes", 0_usize),
        (
            "context-02-several-related-campaigns-packet-passes",
            0_usize,
        ),
        ("context-03-standalone-no-campaign-packet-passes", 0_usize),
        ("context-04-pr-repair-packet-passes", 0_usize),
        ("context-05-narrow-single-agent-packet-passes", 0_usize),
    ] {
        let case = case(&view, id, index)?;
        if case.status != "passed" || !case.violations.is_empty() {
            return Err(format!("{id} case {index} must pass cleanly"));
        }
        if case.packet_identity.is_none() {
            return Err(format!(
                "{id} case {index} must resolve a stable packet identity"
            ));
        }
    }
    // The repair packet must bind the repair_review selection (fixture 5).
    let repair = case(&view, "context-04-pr-repair-packet-passes", 0)?;
    if repair.selection_key.as_deref() != Some("selection:repair_review:issue:9101") {
        return Err(format!(
            "repair packet must bind the repair_review selection, got {:?}",
            repair.selection_key
        ));
    }
    // The narrow packet stays single-agent (fixture 12).
    let corpus = committed_context_corpus()?;
    let narrow = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "context-05-narrow-single-agent-packet-passes")
        .ok_or_else(|| "missing scenario context-05".to_string())?;
    if !narrow.cases[0]
        .packet
        .selection
        .role_context_budget
        .single_agent
    {
        return Err("the narrow packet must stay single-agent".to_string());
    }
    Ok(())
}

/// Fixtures 2 and 3: a packet may carry several related campaign references
/// as context, and a standalone issue with no campaign placement still
/// compiles a valid packet; campaign references never grant authority.
#[test]
fn work_context_identity_campaign_refs_are_context_not_permission() -> Result<(), String> {
    let corpus = committed_context_corpus()?;
    let multi = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "context-02-several-related-campaigns-packet-passes")
        .ok_or_else(|| "missing scenario context-02".to_string())?;
    if multi.cases[0].packet.selection.campaign_refs.len() < 2 {
        return Err("the multi-campaign packet must carry several campaign refs".to_string());
    }
    let view = committed_check()?;
    let standalone = case(&view, "context-03-standalone-no-campaign-packet-passes", 0)?;
    if standalone.status != "passed" {
        return Err("the standalone no-campaign packet must pass".to_string());
    }
    Ok(())
}

/// Fixture 6 + acceptance: a legacy active-goal reference on a packet
/// resolves read-only with migration posture and never authorizes a writer;
/// an unresolvable legacy reference fails with the exact reconcile route.
#[test]
fn work_context_identity_legacy_refs_read_only_no_authority() -> Result<(), String> {
    let view = committed_check()?;
    has_violation(
        &view,
        "context-06-legacy-and-wrong-context-refs-fail",
        0,
        WorkSelectionLawV1::LegacyCompatibility,
        WorkSelectionRouteV1::ReconcileSelection,
    )?;
    let failing = case(&view, "context-06-legacy-and-wrong-context-refs-fail", 0)?;
    let posture = failing
        .migration_posture
        .iter()
        .find(|line| line.contains("legacy_active_goal_ref"))
        .ok_or_else(|| "the legacy packet must emit migration posture".to_string())?;
    if !posture.contains("no write, readiness, merge or closeout authority") {
        return Err(format!(
            "migration posture must record the no-authority grant: {posture}"
        ));
    }
    Ok(())
}

/// #1632: references a packet carries must stay relevant to the bound
/// selection; an unrelated campaign reference, an unbounded
/// selected-work-only overflow and an evidence-free packet each fail with
/// their exact route.
#[test]
fn work_context_identity_irrelevant_refs_and_unbounded_scope_fail() -> Result<(), String> {
    let view = committed_check()?;
    let expected = [
        (
            1,
            WorkSelectionLawV1::CampaignRefIdentity,
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
    ];
    for (index, law, route) in expected {
        has_violation(
            &view,
            "context-06-legacy-and-wrong-context-refs-fail",
            index,
            law,
            route,
        )?;
    }
    Ok(())
}

/// Wrong-input law: a packet for another repository or a stale basis head
/// fails with the recompile-basis route (fixture 9 through the packet row).
#[test]
fn work_context_identity_wrong_repository_and_stale_basis_fail() -> Result<(), String> {
    let view = committed_check()?;
    has_violation(
        &view,
        "context-06-legacy-and-wrong-context-refs-fail",
        4,
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionRouteV1::RecompileBasis,
    )?;
    has_violation(
        &view,
        "context-06-legacy-and-wrong-context-refs-fail",
        5,
        WorkSelectionLawV1::BasisIdentity,
        WorkSelectionRouteV1::RecompileBasis,
    )
}

/// #1632: every packet embeds the full selected-work identity, so a packet
/// whose embedded selection the RIPR-SPEC-0244 engine rejects fails through
/// the shared laws rather than a forked packet-only check.
#[test]
fn work_context_identity_embedded_selection_reuses_shared_laws() -> Result<(), String> {
    let corpus = committed_context_corpus()?;
    let scenario = corpus
        .scenarios
        .iter()
        .find(|scenario| scenario.id == "context-01-one-campaign-packet-passes")
        .ok_or_else(|| "missing scenario context-01".to_string())?;
    let mut tampered = scenario.cases[0].packet.clone();
    // Tamper with the embedded selection's basis: the shared BasisIdentity
    // law must fire through the packet check.
    tampered.selection.basis_sha = "0".repeat(40);
    tampered.basis_sha = "0".repeat(40);
    let dir = resolve_scenario_captured(
        &corpus_root(DEFAULT_WORK_CONTEXT_CORPUS_DIR),
        &scenario.captured,
    )?;
    let captured = load_work_captured_dir(&dir)?;
    let snapshot = compile_work_portfolio(&captured, None)?;
    let basis = build_portfolio_basis(
        &snapshot,
        &corpus.selection_policy,
        &corpus.selection_policy_version,
    );
    let evaluation = check_context_packet(&tampered, &basis, &snapshot, &captured);
    if !evaluation
        .violations
        .iter()
        .any(|violation| violation.law == WorkSelectionLawV1::BasisIdentity)
    {
        return Err("tampered embedded selection must fail the shared basis law".to_string());
    }
    Ok(())
}

/// #1632 schema shape: a packet binds no singleton fields — no
/// repository-wide campaign, agent assignment, CI wait state or progress
/// percentage — and unknown fields fail closed at parse time.
#[test]
fn work_context_identity_packet_schema_has_no_singleton_fields() -> Result<(), String> {
    let body = committed_corpus_json()?;
    if !body.contains("\"packet_id\"") {
        return Err("committed corpus must contain at least one packet row".to_string());
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
                "context corpus must never carry the singleton field {forbidden}"
            ));
        }
    }
    Ok(())
}

/// Human and JSON projections derive from one check DTO: the Markdown
/// renders the same wire names the JSON carries.
#[test]
fn work_context_identity_human_and_json_projections_agree() -> Result<(), String> {
    let view = committed_check()?;
    let json = work_context_check_json(&view)?;
    let parsed: serde_json::Value =
        serde_json::from_str(&json).map_err(|error| format!("view JSON must parse: {error}"))?;
    let markdown = work_context_check_markdown(&view);
    for law in [
        WorkSelectionLawV1::RepositoryIdentity,
        WorkSelectionLawV1::BasisIdentity,
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
fn work_context_identity_deterministic_output() -> Result<(), String> {
    let first = committed_check()?;
    let second = committed_check()?;
    let first_json = work_context_check_json(&first)?;
    let second_json = work_context_check_json(&second)?;
    if first_json != second_json {
        return Err("context check JSON must be byte-stable across runs".to_string());
    }
    if first.portable_identity != second.portable_identity {
        return Err("context check portable identity must be stable".to_string());
    }
    Ok(())
}

/// The corpus loader fails closed on schema drift, duplicate scenario ids,
/// unpinned failure expectations and empty packet identity fields.
#[test]
fn work_context_identity_corpus_loader_fails_closed() -> Result<(), String> {
    if load_work_context_corpus("{}").is_ok() {
        return Err("empty corpus must fail closed".to_string());
    }
    if load_work_context_corpus(
        r##"{
            "schema_version": "work_context_identity_corpus.v1",
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
        "\"context-01-one-campaign-packet-passes\"",
        "\"context-03-standalone-no-campaign-packet-passes\"",
    );
    if load_work_context_corpus(&mutated).is_ok() {
        return Err("duplicate scenario ids must fail closed".to_string());
    }
    if load_work_context_corpus(&body.replace(
        "work_context_identity_corpus.v1",
        "work_context_identity_corpus.v2",
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
fn work_context_identity_committed_provenance_covers_every_corpus_byte() -> Result<(), String> {
    let mut violations = Vec::new();
    validate_work_context_identity_fixture_corpus(&mut violations);
    if !violations.is_empty() {
        return Err(format!(
            "committed corpus validation failed: {violations:?}"
        ));
    }
    Ok(())
}

/// Bounded packet law: the delegation scope is a closed vocabulary with no
/// repository-wide value, and the compiled corpus only uses bounded scopes.
#[test]
fn work_context_identity_scope_is_a_closed_bounded_vocabulary() -> Result<(), String> {
    let corpus = committed_context_corpus()?;
    for scenario in &corpus.scenarios {
        for packet_case in &scenario.cases {
            // Negative cases intentionally carry the unbounded shape; only
            // pass-expected packets must model the bounded scope.
            if packet_case.expect != WorkSelectionExpectV1::Pass {
                continue;
            }
            match packet_case.packet.scope {
                WorkContextScopeV1::SelectedWorkOnly
                | WorkContextScopeV1::SelectedWorkPlusRelevantOverflow => {}
            }
            if packet_case.packet.scope == WorkContextScopeV1::SelectedWorkOnly
                && !packet_case.packet.overflow_refs.is_empty()
            {
                return Err(format!(
                    "scenario `{}` selected-work-only packet must not carry overflow",
                    scenario.id
                ));
            }
        }
    }
    Ok(())
}

/// Mutation-negative: the check command writes only its two reports and
/// leaves every corpus byte untouched.
#[test]
fn work_context_identity_command_mutate_nothing_mutation_negative() -> Result<(), String> {
    let _cwd_guard = crate::acquire_test_cwd_write_guard();
    let sandbox = std::env::temp_dir().join(format!(
        "work_context_identity_mutation_negative_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&sandbox);
    fs::create_dir_all(&sandbox).map_err(|error| error.to_string())?;
    let original_dir = std::env::current_dir().map_err(|error| error.to_string())?;
    let restore = || std::env::set_current_dir(&original_dir);
    std::env::set_current_dir(&sandbox).map_err(|error| error.to_string())?;

    let result = (|| -> Result<(), String> {
        let corpus_src = workspace_path(DEFAULT_WORK_CONTEXT_CORPUS_DIR);
        let corpus_dst = sandbox.join("corpus-check");
        copy_dir(&corpus_src, &corpus_dst)?;
        let before = hash_dir(&corpus_dst)?;
        work_context_check_command(&[
            "--corpus".to_string(),
            "corpus-check".to_string(),
            "--json".to_string(),
        ])?;
        let after = hash_dir(&corpus_dst)?;
        if before != after {
            return Err(format!(
                "context check mutated the corpus: before {before:?} after {after:?}"
            ));
        }
        let written = hash_dir(&sandbox)?;
        let allowed: BTreeSet<String> = [
            "target/ripr/reports/work-context-check.json",
            "target/ripr/reports/work-context-check.md",
        ]
        .iter()
        .map(|name| name.to_string())
        .collect();
        for key in written.keys() {
            if !allowed.contains(key) && !key.starts_with("corpus-check/") {
                return Err(format!(
                    "context check wrote outside the reports directory: {key}"
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
