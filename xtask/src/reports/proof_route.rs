//! `cargo xtask proof route` — the read-only proof-route report from the
//! proof-routing operating model (docs/PROOF_ROUTING.md, slice 3).
//!
//! The command maps the files changed between a base and head revision onto
//! the proof packs in `policy/proof-packs.toml` and reports which CI lanes
//! (from `policy/ci-lane-whitelist.toml`) the change would require, which
//! stay advisory, which would be skipped with a reason, and which are pinned
//! as never routed. Routing state is `advisory-report-only`: this command
//! never executes a proof command and changes no CI behavior.

use super::proof_preflight::{
    PROOF_PREFLIGHT_MODE, PROOF_PREFLIGHT_SCHEMA_VERSION, expected_commands, observe_binding,
};
use super::proof_receipt_binding::PreflightBinding;
use crate::policy::proof_packs::{ProofPack, load_proof_packs};
use crate::run::run_output_owned;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const DEFAULT_BASE: &str = "origin/main";
const DEFAULT_HEAD: &str = "HEAD";
const PROOF_ROUTE_SCHEMA_VERSION: &str = "0.1";
const PROOF_ROUTE_ROUTING_STATE: &str = "advisory-report-only";
const UNKNOWN_SURFACE_REASON: &str = "unknown_surface_full_proof";
const NO_MATCHED_SURFACE_REASON: &str = "no_matched_surface";
const NEVER_ROUTED_REASON: &str = "never_routed";
const PROOF_ROUTE_JSON: &str = "proof-route.json";
const PROOF_ROUTE_MD: &str = "proof-route.md";
const PROOF_USAGE: &str =
    "usage: cargo xtask proof <route|preflight> [--base <rev>] [--head <rev>]";

/// Static lane-cost markers. A lane is `heavy` when any of its commands needs
/// a workspace build or full test run, `medium` when it drives the Node or
/// VSIX toolchain, and `light` otherwise.
const HEAVY_COMMAND_MARKERS: &[&str] = &[
    "cargo check",
    "cargo clippy",
    "cargo test",
    "llvm-cov",
    "nextest",
];
const MEDIUM_COMMAND_MARKERS: &[&str] = &["npm", "vsce"];

pub(crate) fn proof(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{PROOF_USAGE}");
        return Ok(());
    }
    let Some((subcommand, rest)) = args.split_first() else {
        return Err(format!("missing proof subcommand; {PROOF_USAGE}"));
    };
    match subcommand.as_str() {
        "route" => proof_route(rest),
        "preflight" => super::proof_preflight::proof_preflight(rest),
        other => Err(format!("unknown proof subcommand `{other}`; {PROOF_USAGE}")),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProofRouteOptions {
    pub(super) base: String,
    pub(super) head: String,
}

impl Default for ProofRouteOptions {
    fn default() -> Self {
        Self {
            base: DEFAULT_BASE.to_string(),
            head: DEFAULT_HEAD.to_string(),
        }
    }
}

pub(super) fn parse_route_options(args: &[String]) -> Result<ProofRouteOptions, String> {
    let mut options = ProofRouteOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--base" => {
                index += 1;
                options.base = non_empty_arg(args, index, "--base")?.to_string();
            }
            "--head" => {
                index += 1;
                options.head = non_empty_arg(args, index, "--head")?.to_string();
            }
            other => {
                return Err(format!("unknown proof argument `{other}`; {PROOF_USAGE}"));
            }
        }
        index += 1;
    }
    Ok(options)
}

fn non_empty_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let Some(value) = args.get(index) else {
        return Err(format!("missing value for {flag}; {PROOF_USAGE}"));
    };
    if value.trim().is_empty() {
        return Err(format!("proof {flag} requires a non-empty value"));
    }
    Ok(value)
}

fn proof_route(args: &[String]) -> Result<(), String> {
    let options = parse_route_options(args)?;
    let packs = load_proof_packs()?;
    let lanes = load_ci_lanes()?;
    let base_sha = resolve_commit_sha(&options.base)?;
    let head_sha = resolve_commit_sha(&options.head)?;
    let changed_files = changed_files(&options.base, &options.head)?;
    let route = route_proof(&packs, &lanes, &changed_files);

    let json_value = proof_route_json(&options, &base_sha, &head_sha, &changed_files, &route);
    let json_text = serde_json::to_string_pretty(&json_value)
        .map_err(|err| format!("serialize proof route report: {err}"))?;
    crate::write_report(PROOF_ROUTE_JSON, &format!("{json_text}\n"))?;
    crate::write_report(
        PROOF_ROUTE_MD,
        &proof_route_markdown(&options, &base_sha, &head_sha, &changed_files, &route),
    )?;
    println!("Wrote target/ripr/reports/{PROOF_ROUTE_JSON}");
    println!("Wrote target/ripr/reports/{PROOF_ROUTE_MD}");
    Ok(())
}

fn git_output(args: &[&str]) -> Result<String, String> {
    let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    run_output_owned("git", &owned)
}

pub(super) fn resolve_commit_sha(rev: &str) -> Result<String, String> {
    let spec = format!("{rev}^{{commit}}");
    git_output(&["rev-parse", "--verify", spec.as_str()])
        .map(|output| output.trim().to_string())
        .map_err(|err| format!("bad proof route revision {rev:?}: {err}"))
}

pub(super) fn changed_files(base: &str, head: &str) -> Result<Vec<String>, String> {
    changed_files_in(Path::new("."), base, head)
}

fn changed_files_in(repo: &Path, base: &str, head: &str) -> Result<Vec<String>, String> {
    // Raw NUL-delimited inventory (#4006): `-z` output is never C-quoted,
    // so exotic names survive byte-exact; parsing rules come from the
    // shared authority in `decode_changed_files`, not from line splitting
    // here.
    let range = format!("{base}...{head}");
    let owned = ["diff", "--name-only", "-z", range.as_str()]
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = crate::run::capture_process_output_in("git", &owned, Some(repo), &[], &[], &[])
        .map_err(|error| format!("git diff --name-only -z inventory: {}", error.message))?;
    decode_changed_files(&output)
}

/// Decode raw `--name-only -z` bytes through the shared NUL path-record
/// authority (#4006). Strict: non-UTF-8 or empty records fail loudly
/// instead of collapsing through lossy conversion.
fn decode_changed_files(output: &[u8]) -> Result<Vec<String>, String> {
    ripr::analysis::parse_git_path_records(output)
        .map_err(|err| format!("proof-route changed-file inventory: {err}"))
        .and_then(|paths| {
            paths
                .iter()
                .map(|path| {
                    path.to_str().map(str::to_string).ok_or_else(|| {
                        format!(
                            "proof-route changed-file inventory: decoded path {} is not valid UTF-8",
                            path.display()
                        )
                    })
                })
                .collect()
        })
}

/// One `[[lane]]` entry from `policy/ci-lane-whitelist.toml`, reduced to the
/// fields proof routing needs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CiLane {
    id: String,
    commands: Vec<String>,
}

pub(super) fn load_ci_lanes() -> Result<Vec<CiLane>, String> {
    let mut violations = Vec::new();
    let document =
        crate::read_ci_ledger_document(crate::PROOF_PACK_LANE_WHITELIST_PATH, &mut violations);
    let lanes = document
        .as_ref()
        .map(|document| parse_ci_lanes(document, &mut violations))
        .unwrap_or_default();
    if violations.is_empty() {
        Ok(lanes)
    } else {
        Err(format!(
            "{} is not usable for proof routing:\n{}\nrun `cargo xtask check-ci-lane-whitelist` for the full report",
            crate::PROOF_PACK_LANE_WHITELIST_PATH,
            violations
                .iter()
                .map(|violation| format!("- {violation}"))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }
}

fn parse_ci_lanes(document: &crate::CiLedgerDocument, violations: &mut Vec<String>) -> Vec<CiLane> {
    let path = crate::PROOF_PACK_LANE_WHITELIST_PATH;
    let mut lanes = Vec::new();
    for table in crate::ci_tables(document, "lane") {
        let Some(id) = crate::ci_required_non_empty_table_string(path, table, "id", violations)
        else {
            continue;
        };
        let commands =
            crate::ci_required_table_array(path, table, "commands", violations).unwrap_or_default();
        lanes.push(CiLane { id, commands });
    }
    lanes
}

fn estimated_lane_cost(commands: &[String]) -> &'static str {
    let contains_any = |markers: &[&str]| {
        commands
            .iter()
            .any(|command| markers.iter().any(|marker| command.contains(marker)))
    };
    if contains_any(HEAVY_COMMAND_MARKERS) {
        "heavy"
    } else if contains_any(MEDIUM_COMMAND_MARKERS) {
        "medium"
    } else {
        "light"
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MatchedPack {
    id: String,
    ci_lane: Option<String>,
    matched_files: Vec<String>,
    required_commands: Vec<String>,
    advisory_commands: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LaneRoute {
    id: String,
    estimated_cost: &'static str,
    packs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SkippedLane {
    id: String,
    estimated_cost: &'static str,
    reason: &'static str,
}

/// One pack the change must pay proof for, carried with its command lists so
/// downstream consumers (the preflight executor) never re-derive routing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RoutedPack {
    pub(super) id: String,
    pub(super) required_commands: Vec<String>,
    pub(super) advisory_commands: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ProofRoute {
    matched_packs: Vec<MatchedPack>,
    unmatched_files: Vec<String>,
    pub(super) full_proof: bool,
    pub(super) release_proof_required: bool,
    /// Manifest-ordered packs whose proof this change must pay: the matched
    /// packs, or every pack when an unknown surface routes to the full proof.
    pub(super) routed_packs: Vec<RoutedPack>,
    required_lanes: Vec<LaneRoute>,
    advisory_lanes: Vec<LaneRoute>,
    skipped_lanes: Vec<SkippedLane>,
    never_routed_lanes: Vec<LaneRoute>,
}

/// Pure routing decision: changed files -> matched packs -> lane verdicts.
///
/// - A file matching no pack triggers the full proof
///   (`unknown_surface_policy = "full-proof"`): every pack's lane is required.
/// - A surface in more than one pack runs the union of the matched packs.
/// - The lane of a `never_routed` pack (release proof) is required whenever
///   the pack matches or the full proof triggers, and is otherwise reported
///   as never routed — it is never listed as skipped.
pub(super) fn route_proof(
    packs: &[ProofPack],
    lanes: &[CiLane],
    changed_files: &[String],
) -> ProofRoute {
    let mut matched_files_by_pack: Vec<Vec<String>> = vec![Vec::new(); packs.len()];
    let mut unmatched_files = Vec::new();
    for file in changed_files {
        let mut matched_any = false;
        for (index, pack) in packs.iter().enumerate() {
            if pack
                .paths
                .iter()
                .any(|glob| crate::glob_matches(glob, file))
                && let Some(files) = matched_files_by_pack.get_mut(index)
            {
                files.push(file.clone());
                matched_any = true;
            }
        }
        if !matched_any {
            unmatched_files.push(file.clone());
        }
    }
    let full_proof = !unmatched_files.is_empty();

    let matched_packs: Vec<MatchedPack> = packs
        .iter()
        .zip(&matched_files_by_pack)
        .filter(|(_, files)| !files.is_empty())
        .map(|(pack, files)| MatchedPack {
            id: pack.id.clone(),
            ci_lane: pack.ci_lane.clone(),
            matched_files: files.clone(),
            required_commands: pack.required_commands.clone(),
            advisory_commands: pack.advisory_commands.clone(),
        })
        .collect();

    // Packs whose proof the change must pay: the matched packs, or every pack
    // when an unknown surface routes to the full proof.
    let routed_packs: Vec<&ProofPack> = packs
        .iter()
        .zip(&matched_files_by_pack)
        .filter(|(_, files)| full_proof || !files.is_empty())
        .map(|(pack, _)| pack)
        .collect();

    // Release proof is required whenever a never-routed (release) pack
    // matched or the unknown-surface full proof triggered.
    let release_proof_required = routed_packs.iter().any(|pack| pack.never_routed);

    let mut required: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pack in &routed_packs {
        if let Some(lane) = &pack.ci_lane {
            required
                .entry(lane.clone())
                .or_default()
                .push(pack.id.clone());
        }
    }

    // Advisory lanes: lanes that host a routed pack's advisory commands and
    // are not already required. Advisory stays visible without blocking.
    let mut advisory: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for lane in lanes {
        if required.contains_key(&lane.id) {
            continue;
        }
        let mut pack_ids = Vec::new();
        for pack in &routed_packs {
            if pack.advisory_commands.iter().any(|command| {
                lane.commands
                    .iter()
                    .any(|lane_command| lane_command == command)
            }) {
                pack_ids.push(pack.id.clone());
            }
        }
        if !pack_ids.is_empty() {
            advisory.insert(lane.id.clone(), pack_ids);
        }
    }

    let mut never_routed_packs_by_lane: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for pack in packs.iter().filter(|pack| pack.never_routed) {
        if let Some(lane) = &pack.ci_lane {
            never_routed_packs_by_lane
                .entry(lane.clone())
                .or_default()
                .push(pack.id.clone());
        }
    }

    let mut required_lanes = Vec::new();
    let mut advisory_lanes = Vec::new();
    let mut skipped_lanes = Vec::new();
    let mut never_routed_lanes = Vec::new();
    let mut emitted = BTreeSet::new();
    for lane in lanes {
        emitted.insert(lane.id.clone());
        let estimated_cost = estimated_lane_cost(&lane.commands);
        if let Some(pack_ids) = required.get(&lane.id) {
            required_lanes.push(LaneRoute {
                id: lane.id.clone(),
                estimated_cost,
                packs: pack_ids.clone(),
            });
        } else if let Some(pack_ids) = advisory.get(&lane.id) {
            advisory_lanes.push(LaneRoute {
                id: lane.id.clone(),
                estimated_cost,
                packs: pack_ids.clone(),
            });
        } else if let Some(pack_ids) = never_routed_packs_by_lane.get(&lane.id) {
            never_routed_lanes.push(LaneRoute {
                id: lane.id.clone(),
                estimated_cost,
                packs: pack_ids.clone(),
            });
        } else {
            skipped_lanes.push(SkippedLane {
                id: lane.id.clone(),
                estimated_cost,
                reason: NO_MATCHED_SURFACE_REASON,
            });
        }
    }
    // A pack lane missing from the whitelist stays visible rather than being
    // dropped silently; `check-proof-packs` reports the manifest drift.
    for (lane_id, pack_ids) in &required {
        if !emitted.contains(lane_id) {
            required_lanes.push(LaneRoute {
                id: lane_id.clone(),
                estimated_cost: "light",
                packs: pack_ids.clone(),
            });
        }
    }

    let routed_packs = routed_packs
        .iter()
        .map(|pack| RoutedPack {
            id: pack.id.clone(),
            required_commands: pack.required_commands.clone(),
            advisory_commands: pack.advisory_commands.clone(),
        })
        .collect();

    ProofRoute {
        matched_packs,
        unmatched_files,
        full_proof,
        release_proof_required,
        routed_packs,
        required_lanes,
        advisory_lanes,
        skipped_lanes,
        never_routed_lanes,
    }
}

fn lane_routes_json(lanes: &[LaneRoute]) -> Value {
    Value::Array(
        lanes
            .iter()
            .map(|lane| {
                json!({
                    "id": lane.id,
                    "estimated_cost": lane.estimated_cost,
                    "packs": lane.packs,
                })
            })
            .collect(),
    )
}

fn proof_route_json(
    options: &ProofRouteOptions,
    base_sha: &str,
    head_sha: &str,
    changed_files: &[String],
    route: &ProofRoute,
) -> Value {
    let matched_packs: Vec<Value> = route
        .matched_packs
        .iter()
        .map(|pack| {
            json!({
                "id": pack.id,
                "ci_lane": pack.ci_lane,
                "matched_files": pack.matched_files,
                "required_commands": pack.required_commands,
                "advisory_commands": pack.advisory_commands,
            })
        })
        .collect();
    let skipped_lanes: Vec<Value> = route
        .skipped_lanes
        .iter()
        .map(|lane| {
            json!({
                "id": lane.id,
                "reason": lane.reason,
                "estimated_cost": lane.estimated_cost,
            })
        })
        .collect();
    json!({
        "schema_version": PROOF_ROUTE_SCHEMA_VERSION,
        "routing_state": PROOF_ROUTE_ROUTING_STATE,
        "base": {"rev": options.base, "sha": base_sha},
        "head": {"rev": options.head, "sha": head_sha},
        "changed_files": changed_files,
        "matched_packs": matched_packs,
        "unmatched_files": route.unmatched_files,
        "full_proof": route.full_proof,
        "full_proof_reason": if route.full_proof {
            Value::String(UNKNOWN_SURFACE_REASON.to_string())
        } else {
            Value::Null
        },
        "release_proof_required": route.release_proof_required,
        "required_lanes": lane_routes_json(&route.required_lanes),
        "advisory_lanes": lane_routes_json(&route.advisory_lanes),
        "skipped_lanes": skipped_lanes,
        "never_routed_lanes": lane_routes_json(&route.never_routed_lanes),
    })
}

fn backticked_list(values: &[String]) -> String {
    if values.is_empty() {
        return "(none)".to_string();
    }
    values
        .iter()
        .map(|value| format!("`{value}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn proof_route_markdown(
    options: &ProofRouteOptions,
    base_sha: &str,
    head_sha: &str,
    changed_files: &[String],
    route: &ProofRoute,
) -> String {
    let mut body = String::from("# ripr proof route report\n\n");
    body.push_str("Status: advisory\n");
    body.push_str(&format!("Routing state: {PROOF_ROUTE_ROUTING_STATE}\n"));
    body.push_str(&format!(
        "Release proof required: {}\n\n",
        route.release_proof_required
    ));
    body.push_str(
        "This report maps changed files onto the proof packs in `policy/proof-packs.toml` and \
         the CI lanes in `policy/ci-lane-whitelist.toml`. It is read-only evidence: no proof \
         command is executed and no CI behavior changes because of this report.\n\n",
    );
    body.push_str("Range:\n\n");
    body.push_str(&format!("- base: `{}` (`{base_sha}`)\n", options.base));
    body.push_str(&format!("- head: `{}` (`{head_sha}`)\n\n", options.head));

    body.push_str(&format!("## Changed files ({})\n\n", changed_files.len()));
    if changed_files.is_empty() {
        body.push_str("No files changed between base and head.\n\n");
    } else {
        for file in changed_files {
            body.push_str(&format!("- `{file}`\n"));
        }
        body.push('\n');
    }

    body.push_str("## Matched packs\n\n");
    if route.matched_packs.is_empty() {
        body.push_str("No proof pack matched the changed files.\n\n");
    } else {
        body.push_str(
            "| Pack | CI lane | Matched files | Required commands | Advisory commands |\n",
        );
        body.push_str("| --- | --- | --- | --- | --- |\n");
        for pack in &route.matched_packs {
            body.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                pack.id,
                pack.ci_lane.as_deref().unwrap_or("(missing)"),
                backticked_list(&pack.matched_files),
                backticked_list(&pack.required_commands),
                backticked_list(&pack.advisory_commands),
            ));
        }
        body.push('\n');
    }

    body.push_str("## Unmatched files\n\n");
    if route.unmatched_files.is_empty() {
        body.push_str("All changed files matched at least one proof pack.\n\n");
    } else {
        for file in &route.unmatched_files {
            body.push_str(&format!("- `{file}`\n"));
        }
        body.push_str(&format!(
            "\nUnknown surfaces route to the full proof (`{UNKNOWN_SURFACE_REASON}`): every \
             pack's lane is required for this change.\n\n",
        ));
    }

    body.push_str("## Lane routing\n\n");
    body.push_str(&format!(
        "Routing state is `{PROOF_ROUTE_ROUTING_STATE}`: skipped entries below are recorded \
         reasons only, every CI lane still runs as configured, and no CI behavior is changed \
         by this report.\n\n",
    ));
    body.push_str("| Lane | Decision | Estimated cost | Detail |\n");
    body.push_str("| --- | --- | --- | --- |\n");
    for lane in &route.required_lanes {
        let detail = if route.full_proof {
            format!("{UNKNOWN_SURFACE_REASON}; packs: {}", lane.packs.join(", "))
        } else {
            format!("packs: {}", lane.packs.join(", "))
        };
        body.push_str(&format!(
            "| {} | required | {} | {detail} |\n",
            lane.id, lane.estimated_cost
        ));
    }
    for lane in &route.advisory_lanes {
        body.push_str(&format!(
            "| {} | advisory | {} | packs: {} |\n",
            lane.id,
            lane.estimated_cost,
            lane.packs.join(", ")
        ));
    }
    for lane in &route.never_routed_lanes {
        body.push_str(&format!(
            "| {} | never-routed | {} | {NEVER_ROUTED_REASON}; release proof is never routed away (packs: {}) |\n",
            lane.id,
            lane.estimated_cost,
            lane.packs.join(", ")
        ));
    }
    for lane in &route.skipped_lanes {
        body.push_str(&format!(
            "| {} | skipped | {} | {} |\n",
            lane.id, lane.estimated_cost, lane.reason
        ));
    }
    body.push('\n');

    body.push_str(&format!(
        "Rerun: `cargo xtask proof route --base {} --head {}`\n",
        options.base, options.head
    ));
    body
}

// --- pr-summary integration -------------------------------------------------
//
// `cargo xtask pr-summary` appends an advisory "Proof Route" section rendered
// from the same `route_proof` core as `cargo xtask proof route`. The section
// is failure-isolated: when route computation fails (bad git range, broken
// manifest), pr-summary still succeeds and the section degrades to a one-line
// "unavailable" note.

/// Where the pr-summary section looks for a local preflight receipt.
const PROOF_PREFLIGHT_RECEIPT_PATH: &str = "target/ripr/reports/proof-preflight.json";

/// Compatibility verdict for advisory local evidence. Historical execution
/// identity remains distinct from its current compatible subject.
#[derive(Clone, Debug, Eq, PartialEq)]
enum PreflightReceiptStatus {
    /// Execution SHA and captured source/base/plan/environment match.
    Fresh {
        execution_head_sha: String,
        overall_status: String,
        passed: usize,
        failed: usize,
        not_run: usize,
    },
    /// Compatible source and context at a different commit; retain run identity.
    CarriedForward {
        execution_head_sha: String,
        current_head_sha: String,
        overall_status: String,
        passed: usize,
        failed: usize,
        not_run: usize,
    },
    /// Receipt exists but was produced for a different head commit.
    Stale {
        receipt_head_sha: String,
        head_sha: String,
    },
    /// No receipt file on disk.
    Missing,
    /// Receipt file exists but cannot be read or parsed.
    Unreadable(String),
}

/// Everything the pr-summary section renders, computed once.
#[derive(Clone, Debug, Eq, PartialEq)]
struct ProofRouteSummary {
    base: String,
    head: String,
    changed_file_count: usize,
    route: ProofRoute,
    receipt: PreflightReceiptStatus,
}

/// The "Proof Route" section appended to `cargo xtask pr-summary`. Never
/// fails: a route-computation error renders as an "unavailable" note so
/// pr-summary's existing consumers keep working.
pub(crate) fn pr_summary_proof_route_section() -> String {
    proof_route_section_markdown(&compute_pr_summary_route())
}

/// Compute the route with the `proof route` defaults (`origin/main...HEAD`),
/// routing only the committed range; pr-summary itself also lists
/// uncommitted, staged, and untracked changes, which are not routed.
fn compute_pr_summary_route() -> Result<ProofRouteSummary, String> {
    let options = ProofRouteOptions::default();
    let packs = load_proof_packs()?;
    let lanes = load_ci_lanes()?;
    let base_sha = resolve_commit_sha(&options.base)?;
    let head_sha = resolve_commit_sha(&options.head)?;
    let changed = changed_files(&base_sha, &head_sha)?;
    let route = route_proof(&packs, &lanes, &changed);
    let receipt = preflight_receipt_status(
        Path::new(PROOF_PREFLIGHT_RECEIPT_PATH),
        &base_sha,
        &head_sha,
        &route,
    );
    Ok(ProofRouteSummary {
        base: options.base,
        head: options.head,
        changed_file_count: changed.len(),
        route,
        receipt,
    })
}

fn preflight_receipt_status(
    path: &Path,
    base_sha: &str,
    head_sha: &str,
    route: &ProofRoute,
) -> PreflightReceiptStatus {
    if !path.exists() {
        return PreflightReceiptStatus::Missing;
    }
    match std::fs::read_to_string(path) {
        Ok(text) => match observe_binding(base_sha, head_sha, route) {
            Ok(binding) => {
                parse_preflight_receipt(&text, head_sha, Some(&binding), &expected_commands(route))
            }
            Err(reason) => PreflightReceiptStatus::Unreadable(format!(
                "current reuse context unavailable: {reason}"
            )),
        },
        Err(err) => {
            PreflightReceiptStatus::Unreadable(format!("failed to read {}: {err}", path.display()))
        }
    }
}

/// Classify without rewriting execution identity. Bindings are advisory local
/// observations, not producer authentication or protected qualification.
fn parse_preflight_receipt(
    text: &str,
    head_sha: &str,
    current: Option<&PreflightBinding>,
    expected: &[(String, Vec<String>)],
) -> PreflightReceiptStatus {
    let unreadable = |reason: &str| PreflightReceiptStatus::Unreadable(reason.to_string());
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(err) => return unreadable(&format!("malformed preflight receipt JSON: {err}")),
    };
    let Some(executed) = value
        .pointer("/head/sha")
        .and_then(Value::as_str)
        .filter(|sha| !sha.is_empty())
    else {
        return unreadable("preflight receipt carries no head.sha");
    };
    if value.get("schema_version").and_then(Value::as_str) != Some(PROOF_PREFLIGHT_SCHEMA_VERSION)
        || value.get("mode").and_then(Value::as_str) != Some(PROOF_PREFLIGHT_MODE)
    {
        return if executed != head_sha {
            PreflightReceiptStatus::Stale {
                receipt_head_sha: executed.to_string(),
                head_sha: head_sha.to_string(),
            }
        } else {
            unreadable("preflight receipt has no supported reuse contract")
        };
    }
    if value.get("reuse_unavailable_reason") != Some(&Value::Null) {
        return unreadable("preflight receipt does not declare reuse available");
    }
    let binding: PreflightBinding = match value
        .get("reuse_binding")
        .cloned()
        .map(serde_json::from_value)
    {
        Some(Ok(binding)) => binding,
        _ => return unreadable("preflight receipt has no reusable execution binding"),
    };
    if binding.execution_head_sha != executed
        || value.pointer("/base/sha").and_then(Value::as_str) != Some(binding.base_sha.as_str())
    {
        return unreadable("preflight receipt execution identity contradicts its binding");
    }
    let Some(current) = current else {
        return unreadable("current reuse context is unavailable");
    };
    if current.execution_head_sha != head_sha || !binding.compatible_with(current) {
        return unreadable("preflight source, base, environment or plan changed");
    }
    let Some(commands) = value.get("commands").and_then(Value::as_array) else {
        return unreadable("preflight receipt carries no commands");
    };
    if commands.len() != expected.len() {
        return unreadable("preflight receipt command plan is incomplete");
    }
    let mut failed = false;
    for (command, (name, packs)) in commands.iter().zip(expected) {
        if command.get("command").and_then(Value::as_str) != Some(name.as_str())
            || command.get("packs") != Some(&json!(packs))
        {
            return unreadable("preflight receipt command identities differ from the current plan");
        }
        let exit_code = match command.get("exit_code") {
            Some(Value::Null) => None,
            Some(value) => match value.as_i64().and_then(|code| i32::try_from(code).ok()) {
                Some(code) => Some(code),
                None => return unreadable("preflight receipt has an invalid command exit code"),
            },
            None => return unreadable("preflight receipt carries no command exit code"),
        };
        let status = command.get("status").and_then(Value::as_str);
        if (status == Some("pass") && exit_code != Some(0))
            || (status == Some("not_run") && exit_code.is_some())
        {
            return unreadable("preflight receipt command status contradicts its exit code");
        }
        // A timeout may fail with code 0, and launch/signal failures have no code.
        match status {
            Some("pass") if !failed => {}
            Some("fail") if !failed => failed = true,
            Some("not_run") if failed => {}
            _ => return unreadable("preflight receipt has an invalid command outcome sequence"),
        }
    }
    let overall = if failed { "fail" } else { "pass" };
    if value.get("overall_status").and_then(Value::as_str) != Some(overall) {
        return unreadable("preflight receipt overall status contradicts command outcomes");
    }
    let count = |status: &str| {
        commands
            .iter()
            .filter(|command| command.get("status").and_then(Value::as_str) == Some(status))
            .count()
    };
    if executed == head_sha {
        PreflightReceiptStatus::Fresh {
            execution_head_sha: executed.to_string(),
            overall_status: overall.to_string(),
            passed: count("pass"),
            failed: count("fail"),
            not_run: count("not_run"),
        }
    } else {
        PreflightReceiptStatus::CarriedForward {
            execution_head_sha: executed.to_string(),
            current_head_sha: head_sha.to_string(),
            overall_status: overall.to_string(),
            passed: count("pass"),
            failed: count("fail"),
            not_run: count("not_run"),
        }
    }
}

/// First line of a (possibly multi-line) error, marked when truncated.
fn error_one_liner(err: &str) -> String {
    let mut lines = err.lines();
    let first = lines.next().unwrap_or("").trim().to_string();
    if lines.next().is_some() {
        format!("{first} (truncated; see `cargo xtask proof route` for the full error)")
    } else {
        first
    }
}

fn lane_summary_inline(lanes: &[LaneRoute]) -> String {
    if lanes.is_empty() {
        return "(none)".to_string();
    }
    lanes
        .iter()
        .map(|lane| format!("`{}` ({})", lane.id, lane.estimated_cost))
        .collect::<Vec<_>>()
        .join(", ")
}

fn skipped_reason_summary(lanes: &[SkippedLane]) -> String {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for lane in lanes {
        *counts.entry(lane.reason).or_insert(0) += 1;
    }
    counts
        .iter()
        .map(|(reason, count)| format!("{count}x `{reason}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn preflight_receipt_line(receipt: &PreflightReceiptStatus) -> String {
    match receipt {
        PreflightReceiptStatus::Fresh {
            execution_head_sha,
            overall_status,
            passed,
            failed,
            not_run,
        } => format!(
            "Preflight receipt: fresh for current head — overall status `{overall_status}`; \
             commands run: {passed} pass, {failed} fail ({not_run} not run); execution head `{execution_head_sha}`.\n"
        ),
        PreflightReceiptStatus::CarriedForward {
            execution_head_sha,
            current_head_sha,
            overall_status,
            passed,
            failed,
            not_run,
        } => format!(
            "Preflight receipt: carried forward from execution head `{execution_head_sha}` to source-equivalent current head `{current_head_sha}` — original overall status `{overall_status}`; original commands run: {passed} pass, {failed} fail ({not_run} not run); advisory only.\n"
        ),
        PreflightReceiptStatus::Stale {
            receipt_head_sha,
            head_sha,
        } => format!(
            "Preflight receipt: no fresh preflight receipt (receipt head `{receipt_head_sha}` \
             does not match current head `{head_sha}`).\n"
        ),
        PreflightReceiptStatus::Missing => format!(
            "Preflight receipt: no fresh preflight receipt (none at `{PROOF_PREFLIGHT_RECEIPT_PATH}`).\n"
        ),
        PreflightReceiptStatus::Unreadable(err) => format!(
            "Preflight receipt: no fresh preflight receipt ({}).\n",
            error_one_liner(err)
        ),
    }
}

/// Render the pr-summary section from a route outcome. Pure so the section
/// contract (advisory line, skipped-count summary, receipt freshness,
/// failure fallback) is unit-testable.
fn proof_route_section_markdown(summary: &Result<ProofRouteSummary, String>) -> String {
    let mut body = String::from("\n## Proof Route\n\n");
    let summary = match summary {
        Ok(summary) => summary,
        Err(err) => {
            body.push_str(&format!(
                "Proof route unavailable: {}\n\n",
                error_one_liner(err)
            ));
            body.push_str(&format!(
                "Routing state is `{PROOF_ROUTE_ROUTING_STATE}`: this section is advisory, \
                 read-only evidence and CI lanes are unchanged by it.\n"
            ));
            push_reproduce_commands(&mut body, DEFAULT_BASE, DEFAULT_HEAD);
            return body;
        }
    };

    body.push_str(&format!(
        "Range: `{}...{}` — the proof-route defaults: only the committed range is \
         routed. pr-summary itself also lists uncommitted, staged, and untracked \
         changes; those are not part of this route until committed.\n",
        summary.base, summary.head
    ));
    body.push_str(&format!(
        "Routing state is `{PROOF_ROUTE_ROUTING_STATE}`: this section is advisory, \
         read-only evidence and CI lanes are unchanged by it.\n"
    ));
    body.push_str(&format!(
        "Release proof required: {}{}\n\n",
        summary.route.release_proof_required,
        if summary.route.release_proof_required {
            " — full release proof required (release-package pack is never routed away)"
        } else {
            ""
        }
    ));

    body.push_str(&format!(
        "Changed-surface packs ({} changed file(s)):\n\n",
        summary.changed_file_count
    ));
    if summary.route.matched_packs.is_empty() {
        body.push_str("No proof pack matched the changed files between base and head.\n\n");
    } else {
        body.push_str("| Pack | Matched files | CI lane |\n");
        body.push_str("| --- | --- | --- |\n");
        for pack in &summary.route.matched_packs {
            body.push_str(&format!(
                "| {} | {} | {} |\n",
                pack.id,
                pack.matched_files.len(),
                pack.ci_lane.as_deref().unwrap_or("(missing)")
            ));
        }
        body.push('\n');
    }
    if summary.route.full_proof {
        body.push_str(&format!(
            "Full proof: {} unmatched file(s) routed this change to the full proof \
             (`{UNKNOWN_SURFACE_REASON}`); every pack's lane is required.\n\n",
            summary.route.unmatched_files.len()
        ));
    }

    body.push_str("Lanes:\n\n");
    body.push_str(&format!(
        "- Required: {}\n",
        lane_summary_inline(&summary.route.required_lanes)
    ));
    body.push_str(&format!(
        "- Advisory: {}\n",
        lane_summary_inline(&summary.route.advisory_lanes)
    ));
    if summary.route.skipped_lanes.is_empty() {
        body.push_str("- Skipped: 0\n");
    } else {
        body.push_str(&format!(
            "- Skipped: {} ({}) — full lane table: `cargo xtask proof route`\n",
            summary.route.skipped_lanes.len(),
            skipped_reason_summary(&summary.route.skipped_lanes)
        ));
    }
    if !summary.route.never_routed_lanes.is_empty() {
        body.push_str(&format!(
            "- Never-routed: {} — release proof is never routed away\n",
            lane_summary_inline(&summary.route.never_routed_lanes)
        ));
    }
    body.push('\n');

    body.push_str(&preflight_receipt_line(&summary.receipt));
    push_reproduce_commands(&mut body, &summary.base, &summary.head);
    body
}

fn push_reproduce_commands(body: &mut String, base: &str, head: &str) {
    body.push_str("\nReproduce locally:\n\n");
    body.push_str(&format!(
        "- `cargo xtask proof route --base {base} --head {head}`\n"
    ));
    body.push_str(&format!(
        "- `cargo xtask proof preflight --base {base} --head {head}`\n"
    ));
}

/// In-repo manifest and lane-whitelist fixtures shared by the `proof route`
/// and `proof preflight` test modules.
#[cfg(test)]
pub(super) mod test_support {
    use super::{CiLane, ProofRoute, parse_ci_lanes, route_proof};
    use crate::policy::proof_packs::{ProofPack, parse_proof_packs};
    use std::path::{Path, PathBuf};

    fn repo_root() -> Result<PathBuf, String> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.parent().map(Path::to_path_buf).ok_or_else(|| {
            format!(
                "failed to resolve repo root from {}",
                manifest_dir.display()
            )
        })
    }

    fn read_repo_file(relative: &str) -> Result<String, String> {
        let path = repo_root()?.join(relative);
        std::fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))
    }

    pub(in super::super) fn manifest_packs() -> Result<Vec<ProofPack>, String> {
        let text = read_repo_file("policy/proof-packs.toml")?;
        let (document, mut violations) =
            crate::parse_ci_ledger_document(crate::PROOF_PACK_MANIFEST_PATH, &text);
        let packs = parse_proof_packs(&document, &mut violations);
        if violations.is_empty() {
            Ok(packs)
        } else {
            Err(format!(
                "manifest fixture should parse cleanly: {violations:?}"
            ))
        }
    }

    pub(in super::super) fn whitelist_lanes() -> Result<Vec<CiLane>, String> {
        let text = read_repo_file("policy/ci-lane-whitelist.toml")?;
        let (document, mut violations) =
            crate::parse_ci_ledger_document(crate::PROOF_PACK_LANE_WHITELIST_PATH, &text);
        let lanes = parse_ci_lanes(&document, &mut violations);
        if violations.is_empty() {
            Ok(lanes)
        } else {
            Err(format!(
                "lane whitelist fixture should parse cleanly: {violations:?}"
            ))
        }
    }

    pub(in super::super) fn route_for(files: &[&str]) -> Result<ProofRoute, String> {
        let packs = manifest_packs()?;
        let lanes = whitelist_lanes()?;
        let changed: Vec<String> = files.iter().map(|file| (*file).to_string()).collect();
        Ok(route_proof(&packs, &lanes, &changed))
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::route_for;
    use super::*;

    fn test_binding(head: &str) -> PreflightBinding {
        PreflightBinding {
            execution_head_sha: head.to_string(),
            source_tree: "source-tree".to_string(),
            base_sha: "base-sha".to_string(),
            merge_base_sha: "merge-base-sha".to_string(),
            environment_digest: "environment-digest".to_string(),
            plan_digest: "plan-digest".to_string(),
        }
    }

    fn bound_receipt() -> Value {
        json!({
            "schema_version": PROOF_PREFLIGHT_SCHEMA_VERSION,
            "mode": PROOF_PREFLIGHT_MODE,
            "head": {"sha": "executed-sha"},
            "base": {"sha": "base-sha"},
            "reuse_binding": test_binding("executed-sha"),
            "reuse_unavailable_reason": null,
            "overall_status": "fail",
            "commands": [
                {"command": "test a", "packs": ["p"], "status": "pass", "duration_ms": 12, "exit_code": 0},
                {"command": "test b", "packs": ["p"], "status": "fail", "duration_ms": 34, "exit_code": 1},
                {"command": "test c", "packs": ["p"], "status": "not_run", "duration_ms": 0, "exit_code": null}
            ]
        })
    }

    fn receipt_plan() -> Vec<(String, Vec<String>)> {
        ["test a", "test b", "test c"]
            .map(|command| (command.to_string(), vec!["p".to_string()]))
            .to_vec()
    }

    #[test]
    fn carried_receipt_status_transition_matrix_preserves_outcomes_and_refusal_reasons()
    -> Result<(), String> {
        // Enumerate producer-legal sequences explicitly, independently of the
        // parser's state machine. Canonical exits isolate status validation.
        let statuses = [
            Some(json!("pass")),
            Some(json!("fail")),
            Some(json!("not_run")),
            None,
            Some(Value::Null),
            Some(json!("unknown")),
            Some(json!(false)),
        ];
        let mut accepted = 0;
        let mut sequence_refused = 0;
        let mut overall_refused = 0;
        for first in &statuses {
            for second in &statuses {
                for third in &statuses {
                    let sequence = [first, second, third];
                    let names = sequence.map(|status| status.as_ref().and_then(Value::as_str));
                    let legal = match names {
                        [Some("pass"), Some("pass"), Some("pass")] => Some(("pass", 3, 0, 0)),
                        [Some("pass"), Some("pass"), Some("fail")] => Some(("fail", 2, 1, 0)),
                        [Some("pass"), Some("fail"), Some("not_run")] => Some(("fail", 1, 1, 1)),
                        [Some("fail"), Some("not_run"), Some("not_run")] => Some(("fail", 0, 1, 2)),
                        _ => None,
                    };
                    for overall in &statuses {
                        let mut value = bound_receipt();
                        for (index, replacement) in sequence.iter().enumerate() {
                            let command = value["commands"][index]
                                .as_object_mut()
                                .ok_or("fixture command")?;
                            match replacement {
                                Some(status) => {
                                    command.insert("status".to_string(), status.clone());
                                }
                                None => {
                                    command.remove("status");
                                }
                            }
                            command.insert(
                                "exit_code".to_string(),
                                match names[index] {
                                    Some("pass") => json!(0),
                                    Some("fail") => json!(1),
                                    _ => Value::Null,
                                },
                            );
                        }
                        let receipt = value.as_object_mut().ok_or("fixture receipt")?;
                        match overall {
                            Some(status) => {
                                receipt.insert("overall_status".to_string(), status.clone());
                            }
                            None => {
                                receipt.remove("overall_status");
                            }
                        }
                        let expected = match legal {
                            None => {
                                sequence_refused += 1;
                                PreflightReceiptStatus::Unreadable(
                                    "preflight receipt has an invalid command outcome sequence"
                                        .to_string(),
                                )
                            }
                            Some((status, _, _, _))
                                if overall.as_ref().and_then(Value::as_str) != Some(status) =>
                            {
                                overall_refused += 1;
                                PreflightReceiptStatus::Unreadable(
                                    "preflight receipt overall status contradicts command outcomes"
                                        .to_string(),
                                )
                            }
                            Some((status, passed, failed, not_run)) => {
                                accepted += 1;
                                PreflightReceiptStatus::CarriedForward {
                                    execution_head_sha: "executed-sha".to_string(),
                                    current_head_sha: "published-sha".to_string(),
                                    overall_status: status.to_string(),
                                    passed,
                                    failed,
                                    not_run,
                                }
                            }
                        };
                        let text = value.to_string();
                        let actual = parse_preflight_receipt(
                            &text,
                            "published-sha",
                            Some(&test_binding("published-sha")),
                            &receipt_plan(),
                        );
                        assert_eq!(
                            actual, expected,
                            "sequence={sequence:?}, overall={overall:?}"
                        );
                        let rendered = preflight_receipt_line(&actual);
                        match &expected {
                            PreflightReceiptStatus::Unreadable(reason) => assert_eq!(
                                rendered,
                                format!(
                                    "Preflight receipt: no fresh preflight receipt ({reason}).\n"
                                )
                            ),
                            PreflightReceiptStatus::CarriedForward {
                                overall_status,
                                passed,
                                failed,
                                not_run,
                                ..
                            } => {
                                assert_eq!(
                                    rendered,
                                    format!(
                                        "Preflight receipt: carried forward from execution head `executed-sha` to source-equivalent current head `published-sha` — original overall status `{overall_status}`; original commands run: {passed} pass, {failed} fail ({not_run} not run); advisory only.\n"
                                    )
                                );
                            }
                            _ => {
                                return Err(
                                    "matrix expected only carried or refused outcomes".to_string()
                                );
                            }
                        }
                    }
                }
            }
        }
        assert_eq!((accepted, sequence_refused, overall_refused), (4, 2373, 24));
        eprintln!(
            "status matrix: {accepted} carried, {sequence_refused} sequence refusals, {overall_refused} overall refusals"
        );
        Ok(())
    }

    fn assert_receipt_refused(value: &Value, reason: &str) {
        for head in ["executed-sha", "published-sha"] {
            let status = parse_preflight_receipt(
                &value.to_string(),
                head,
                Some(&test_binding(head)),
                &receipt_plan(),
            );
            assert_eq!(
                status,
                PreflightReceiptStatus::Unreadable(reason.to_string())
            );
            assert!(preflight_receipt_line(&status).contains(reason));
        }
    }

    #[test]
    fn contradictory_or_missing_exit_evidence_refuses_receipt_reuse() -> Result<(), String> {
        for index in 0..3 {
            let mut value = bound_receipt();
            value["commands"][index]
                .as_object_mut()
                .ok_or("fixture command")?
                .remove("exit_code");
            assert_receipt_refused(&value, "preflight receipt carries no command exit code");
            for malformed in [json!("0"), json!(false), json!(0.5), json!(i64::MAX)] {
                let mut value = bound_receipt();
                value["commands"][index]["exit_code"] = malformed;
                assert_receipt_refused(
                    &value,
                    "preflight receipt has an invalid command exit code",
                );
            }
        }
        for (index, code) in [(0, json!(23)), (0, Value::Null), (2, json!(0))] {
            let mut value = bound_receipt();
            value["commands"][index]["exit_code"] = code;
            assert_receipt_refused(
                &value,
                "preflight receipt command status contradicts its exit code",
            );
        }
        Ok(())
    }

    #[test]
    fn unavailable_or_missing_reuse_verdict_refuses_receipt_reuse() -> Result<(), String> {
        let mut value = bound_receipt();
        value
            .as_object_mut()
            .ok_or("fixture receipt")?
            .remove("reuse_unavailable_reason");
        assert_receipt_refused(&value, "preflight receipt does not declare reuse available");
        for reason in [
            json!("execution boundary observations changed"),
            json!(""),
            json!(false),
        ] {
            let mut value = bound_receipt();
            value["reuse_unavailable_reason"] = reason;
            assert_receipt_refused(&value, "preflight receipt does not declare reuse available");
        }
        Ok(())
    }

    #[test]
    fn timeout_and_unavailable_exit_codes_preserve_failed_outcomes() {
        for code in [Value::Null, json!(0), json!(1)] {
            let mut value = bound_receipt();
            value["commands"][1]["exit_code"] = code;
            for head in ["executed-sha", "published-sha"] {
                let status = parse_preflight_receipt(
                    &value.to_string(),
                    head,
                    Some(&test_binding(head)),
                    &receipt_plan(),
                );
                assert!(matches!(
                    status,
                    PreflightReceiptStatus::Fresh { ref overall_status, passed: 1, failed: 1, not_run: 1, .. }
                    | PreflightReceiptStatus::CarriedForward { ref overall_status, passed: 1, failed: 1, not_run: 1, .. }
                    if overall_status == "fail"
                ));
                assert!(preflight_receipt_line(&status).contains("status `fail`"));
            }
        }
    }

    #[test]
    fn source_equivalent_receipt_carries_original_execution_and_outcomes() {
        let value = bound_receipt();
        let text = value.to_string();
        let status = parse_preflight_receipt(
            &text,
            "published-sha",
            Some(&test_binding("published-sha")),
            &receipt_plan(),
        );
        assert_eq!(
            status,
            PreflightReceiptStatus::CarriedForward {
                execution_head_sha: "executed-sha".to_string(),
                current_head_sha: "published-sha".to_string(),
                overall_status: "fail".to_string(),
                passed: 1,
                failed: 1,
                not_run: 1,
            }
        );
        let rendered = preflight_receipt_line(&status);
        assert!(rendered.contains("carried forward from execution head `executed-sha`"));
        assert!(rendered.contains("current head `published-sha`"));
        assert!(rendered.contains("original overall status `fail`"));
        assert!(rendered.contains("original commands run: 1 pass, 1 fail (1 not run)"));
        assert!(!rendered.contains("fresh for current head"));
        assert_eq!(text, value.to_string());
    }

    #[test]
    fn same_commit_changed_source_base_environment_or_plan_refuses_reuse() -> Result<(), String> {
        let text = bound_receipt().to_string();
        for field in [
            "source_tree",
            "base_sha",
            "merge_base_sha",
            "environment_digest",
            "plan_digest",
        ] {
            let mut current = serde_json::to_value(test_binding("executed-sha"))
                .map_err(|error| error.to_string())?;
            current[field] = json!("changed");
            let current = serde_json::from_value(current).map_err(|error| error.to_string())?;
            assert!(
                matches!(
                    parse_preflight_receipt(&text, "executed-sha", Some(&current), &receipt_plan()),
                    PreflightReceiptStatus::Unreadable(_)
                ),
                "{field}"
            );
        }
        assert!(matches!(
            parse_preflight_receipt(&text, "executed-sha", None, &receipt_plan()),
            PreflightReceiptStatus::Unreadable(_)
        ));
        Ok(())
    }

    #[test]
    fn altered_or_incomplete_execution_receipts_refuse_reuse() -> Result<(), String> {
        let check = |value: Value| {
            assert!(matches!(
                parse_preflight_receipt(
                    &value.to_string(),
                    "executed-sha",
                    Some(&test_binding("executed-sha")),
                    &receipt_plan()
                ),
                PreflightReceiptStatus::Unreadable(_)
            ));
        };
        for pointer in [
            "/reuse_binding",
            "/commands",
            "/reuse_binding/execution_head_sha",
            "/reuse_binding/source_tree",
            "/reuse_binding/base_sha",
            "/reuse_binding/merge_base_sha",
            "/reuse_binding/environment_digest",
            "/reuse_binding/plan_digest",
        ] {
            let mut value = bound_receipt();
            *value.pointer_mut(pointer).ok_or("fixture pointer")? = Value::Null;
            check(value);
        }
        for (pointer, replacement) in [
            ("/head/sha", json!("wrong-execution")),
            ("/base/sha", json!("wrong-base")),
            ("/schema_version", json!("0.1")),
            ("/mode", json!("qualification")),
            ("/commands/0/command", json!("wrong command")),
            ("/commands/0/packs", json!(["wrong-pack"])),
            ("/commands/0/status", json!("not_run")),
            ("/commands/1/status", json!("unknown")),
            ("/commands/2/status", json!("pass")),
            ("/overall_status", json!("pass")),
        ] {
            let mut value = bound_receipt();
            *value.pointer_mut(pointer).ok_or("fixture pointer")? = replacement;
            // A legacy/different-head receipt may classify stale, still never reusable.
            let status = parse_preflight_receipt(
                &value.to_string(),
                "executed-sha",
                Some(&test_binding("executed-sha")),
                &receipt_plan(),
            );
            assert!(
                matches!(
                    status,
                    PreflightReceiptStatus::Unreadable(_) | PreflightReceiptStatus::Stale { .. }
                ),
                "{pointer}"
            );
        }
        let mut value = bound_receipt();
        value["commands"]
            .as_array_mut()
            .ok_or("fixture commands")?
            .swap(0, 1);
        check(value);
        let mut value = bound_receipt();
        let _ = value["commands"]
            .as_array_mut()
            .ok_or("fixture commands")?
            .pop();
        check(value);
        let mut value = bound_receipt();
        let extra = value["commands"][0].clone();
        value["commands"]
            .as_array_mut()
            .ok_or("fixture commands")?
            .push(extra);
        check(value);
        Ok(())
    }

    fn matched_pack_ids(route: &ProofRoute) -> Vec<&str> {
        route
            .matched_packs
            .iter()
            .map(|pack| pack.id.as_str())
            .collect()
    }

    fn required_lane_ids(route: &ProofRoute) -> Vec<&str> {
        route
            .required_lanes
            .iter()
            .map(|lane| lane.id.as_str())
            .collect()
    }

    fn skipped_lane_ids(route: &ProofRoute) -> Vec<&str> {
        route
            .skipped_lanes
            .iter()
            .map(|lane| lane.id.as_str())
            .collect()
    }

    #[test]
    fn docs_only_diff_routes_to_docs_spec_pack_only() -> Result<(), String> {
        let route = route_for(&["docs/specs/SPEC-0001-example.md"])?;
        assert_eq!(matched_pack_ids(&route), vec!["docs-spec"]);
        assert_eq!(required_lane_ids(&route), vec!["docs"]);
        assert!(!route.full_proof);
        assert!(!route.release_proof_required);
        assert!(route.unmatched_files.is_empty());
        // The release lane is never listed as skipped.
        assert!(!skipped_lane_ids(&route).contains(&"release-readiness-proof"));
        assert!(
            route
                .never_routed_lanes
                .iter()
                .any(|lane| lane.id == "release-readiness-proof")
        );
        Ok(())
    }

    #[test]
    fn output_diff_routes_to_output_contracts_pack() -> Result<(), String> {
        let route = route_for(&["crates/ripr/src/output/human.rs"])?;
        let matched = matched_pack_ids(&route);
        assert!(
            matched.contains(&"output-contracts"),
            "matched: {matched:?}"
        );
        // The manifest places src/output in static-language too; a surface in
        // more than one pack runs the union of the matched packs.
        assert!(matched.contains(&"static-language"), "matched: {matched:?}");
        let required = required_lane_ids(&route);
        assert!(
            required.contains(&"output-contracts"),
            "required: {required:?}"
        );
        assert!(
            required.contains(&"static-language"),
            "required: {required:?}"
        );
        assert!(!route.full_proof);
        Ok(())
    }

    #[test]
    fn mixed_diff_routes_to_the_union_of_matched_packs() -> Result<(), String> {
        let route = route_for(&[
            "docs/specs/SPEC-0001-example.md",
            "crates/ripr/src/output/json.rs",
        ])?;
        let matched = matched_pack_ids(&route);
        for expected in ["docs-spec", "static-language", "output-contracts"] {
            assert!(matched.contains(&expected), "matched: {matched:?}");
        }
        let required = required_lane_ids(&route);
        for expected in ["docs", "static-language", "output-contracts"] {
            assert!(required.contains(&expected), "required: {required:?}");
        }
        assert!(!route.full_proof);
        Ok(())
    }

    #[test]
    fn unknown_surface_routes_to_full_proof() -> Result<(), String> {
        let route = route_for(&["scripts/unknown-surface.bin"])?;
        assert!(route.full_proof);
        assert_eq!(route.unmatched_files, vec!["scripts/unknown-surface.bin"]);
        assert!(route.matched_packs.is_empty());
        // Full proof requires every pack's lane, including release proof.
        let required = required_lane_ids(&route);
        for expected in [
            "docs",
            "static-language",
            "output-contracts",
            "traceability",
            "routed-rust-small",
            "vscode-e2e",
            "release-readiness-proof",
        ] {
            assert!(required.contains(&expected), "required: {required:?}");
        }
        assert!(route.release_proof_required);
        assert!(!skipped_lane_ids(&route).contains(&"release-readiness-proof"));
        Ok(())
    }

    #[test]
    fn xtask_diff_routes_to_xtask_report_pack() -> Result<(), String> {
        let route = route_for(&["xtask/src/reports/proof_route.rs"])?;
        assert_eq!(matched_pack_ids(&route), vec!["xtask-report"]);
        assert_eq!(required_lane_ids(&route), vec!["routed-rust-small"]);
        assert!(!route.full_proof);
        assert!(!route.release_proof_required);
        Ok(())
    }

    #[test]
    fn analysis_diff_routes_to_analysis_fixture_pack() -> Result<(), String> {
        let route = route_for(&["crates/ripr/src/analysis/probes.rs"])?;
        assert_eq!(matched_pack_ids(&route), vec!["analysis-fixture"]);
        assert_eq!(required_lane_ids(&route), vec!["routed-rust-small"]);
        assert!(!route.full_proof);
        assert!(!route.release_proof_required);
        Ok(())
    }

    #[test]
    fn identity_registry_diff_routes_to_the_identity_registry_pack() -> Result<(), String> {
        let route = route_for(&["crates/ripr/src/domain/identity/catalog.rs"])?;
        assert_eq!(matched_pack_ids(&route), vec!["identity-registry"]);
        assert_eq!(required_lane_ids(&route), vec!["routed-rust-small"]);
        assert!(!route.full_proof);
        assert!(!route.release_proof_required);
        Ok(())
    }

    #[test]
    fn release_file_routes_release_pack_and_is_never_skipped() -> Result<(), String> {
        let route = route_for(&["Cargo.toml"])?;
        assert_eq!(matched_pack_ids(&route), vec!["release-package"]);
        assert_eq!(required_lane_ids(&route), vec!["release-readiness-proof"]);
        assert!(!route.full_proof);
        assert!(route.release_proof_required);
        assert!(!skipped_lane_ids(&route).contains(&"release-readiness-proof"));
        assert!(route.never_routed_lanes.is_empty());
        Ok(())
    }

    #[test]
    fn changelog_routes_release_pack_and_is_never_skipped() -> Result<(), String> {
        let route = route_for(&["CHANGELOG.md"])?;
        let matched = matched_pack_ids(&route);
        // CHANGELOG.md is markdown and a release surface: docs-spec and
        // release-package both match and run as a union.
        assert!(matched.contains(&"release-package"), "matched: {matched:?}");
        assert!(matched.contains(&"docs-spec"), "matched: {matched:?}");
        assert!(route.release_proof_required);
        let required = required_lane_ids(&route);
        assert!(
            required.contains(&"release-readiness-proof"),
            "required: {required:?}"
        );
        assert!(!skipped_lane_ids(&route).contains(&"release-readiness-proof"));
        Ok(())
    }

    #[test]
    fn json_and_markdown_agree_on_lane_decisions() -> Result<(), String> {
        let route = route_for(&["docs/specs/SPEC-0001-example.md", "CHANGELOG.md"])?;
        let options = ProofRouteOptions::default();
        let changed = vec![
            "docs/specs/SPEC-0001-example.md".to_string(),
            "CHANGELOG.md".to_string(),
        ];
        let json_value = proof_route_json(&options, "base-sha", "head-sha", &changed, &route);
        let markdown = proof_route_markdown(&options, "base-sha", "head-sha", &changed, &route);

        // Both renderers derive from the same ProofRoute; every lane decision
        // in the JSON must appear with the same decision label in the MD.
        let lane_sets: [(&str, &str); 4] = [
            ("required_lanes", "required"),
            ("advisory_lanes", "advisory"),
            ("never_routed_lanes", "never-routed"),
            ("skipped_lanes", "skipped"),
        ];
        for (key, decision) in lane_sets {
            let lanes = json_value
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("proof-route json should carry `{key}`"))?;
            for lane in lanes {
                let id = lane
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("`{key}` entries should carry an id"))?;
                let row = format!("| {id} | {decision} |");
                if !markdown.contains(&row) {
                    return Err(format!("markdown should contain row `{row}`"));
                }
            }
        }
        assert_eq!(
            json_value
                .get("release_proof_required")
                .and_then(Value::as_bool),
            Some(route.release_proof_required)
        );
        assert!(markdown.contains(&format!(
            "Release proof required: {}",
            route.release_proof_required
        )));
        Ok(())
    }

    #[test]
    fn pack_path_globs_match_expected_shapes() -> Result<(), String> {
        // `**/*.md` matches markdown at the repo root and nested.
        assert!(crate::glob_matches("**/*.md", "README.md"));
        assert!(crate::glob_matches("**/*.md", "docs/learnings/notes.md"));
        assert!(!crate::glob_matches("**/*.md", "docs/notes.txt"));
        // `docs/specs/**` matches nested spec files.
        assert!(crate::glob_matches("docs/specs/**", "docs/specs/a/b.md"));
        assert!(!crate::glob_matches("docs/specs/**", "docs/SPECS.md"));
        // Exact paths match only themselves.
        assert!(crate::glob_matches("Cargo.toml", "Cargo.toml"));
        assert!(!crate::glob_matches("Cargo.toml", "crates/ripr/Cargo.toml"));
        Ok(())
    }

    #[test]
    fn estimated_lane_cost_uses_the_static_command_map() {
        let heavy = vec!["cargo test --workspace".to_string()];
        let heavy_coverage = vec![
            "cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info".to_string(),
        ];
        let medium = vec!["npm run compile".to_string()];
        let medium_vsce = vec!["npx @vscode/vsce package".to_string()];
        let light = vec!["cargo xtask check-doc-index".to_string()];
        let empty: Vec<String> = Vec::new();
        assert_eq!(estimated_lane_cost(&heavy), "heavy");
        assert_eq!(estimated_lane_cost(&heavy_coverage), "heavy");
        assert_eq!(estimated_lane_cost(&medium), "medium");
        assert_eq!(estimated_lane_cost(&medium_vsce), "medium");
        assert_eq!(estimated_lane_cost(&light), "light");
        assert_eq!(estimated_lane_cost(&empty), "light");
    }

    #[test]
    fn report_outputs_carry_the_routing_contract() -> Result<(), String> {
        let route = route_for(&["docs/specs/SPEC-0001-example.md"])?;
        let options = ProofRouteOptions::default();
        let json_value = proof_route_json(
            &options,
            "base-sha",
            "head-sha",
            &["docs/specs/SPEC-0001-example.md".to_string()],
            &route,
        );
        assert_eq!(
            json_value.get("schema_version").and_then(Value::as_str),
            Some(PROOF_ROUTE_SCHEMA_VERSION)
        );
        assert_eq!(
            json_value.get("routing_state").and_then(Value::as_str),
            Some(PROOF_ROUTE_ROUTING_STATE)
        );
        assert_eq!(
            json_value.get("full_proof").and_then(Value::as_bool),
            Some(false)
        );
        let markdown = proof_route_markdown(
            &options,
            "base-sha",
            "head-sha",
            &["docs/specs/SPEC-0001-example.md".to_string()],
            &route,
        );
        assert!(markdown.contains(PROOF_ROUTE_ROUTING_STATE));
        assert!(markdown.contains("| docs | required |"));
        assert!(markdown.contains("never-routed"));
        Ok(())
    }

    fn summary_for(
        files: &[&str],
        receipt: PreflightReceiptStatus,
    ) -> Result<ProofRouteSummary, String> {
        Ok(ProofRouteSummary {
            base: DEFAULT_BASE.to_string(),
            head: DEFAULT_HEAD.to_string(),
            changed_file_count: files.len(),
            route: route_for(files)?,
            receipt,
        })
    }

    #[test]
    fn pr_summary_section_renders_route_without_receipt() -> Result<(), String> {
        let summary = summary_for(
            &["xtask/src/reports/proof_route.rs"],
            PreflightReceiptStatus::Missing,
        )?;
        let section = proof_route_section_markdown(&Ok(summary));
        assert!(section.contains("## Proof Route"));
        assert!(section.contains("Range: `origin/main...HEAD`"));
        assert!(section.contains("advisory-report-only"));
        assert!(section.contains("CI lanes are unchanged"));
        assert!(section.contains("| Pack | Matched files | CI lane |"));
        assert!(section.contains("| xtask-report | 1 | routed-rust-small |"));
        assert!(section.contains("- Required: `routed-rust-small`"));
        assert!(section.contains("Release proof required: false"));
        // Skipped lanes appear as a count with a reason summary, never as a
        // full per-lane table; the full report stays with `proof route`.
        assert!(section.contains("- Skipped: "));
        assert!(section.contains("`no_matched_surface`"));
        assert!(section.contains("full lane table: `cargo xtask proof route`"));
        assert!(!section.contains("| skipped |"));
        assert!(section.contains("no fresh preflight receipt"));
        assert!(section.contains("- `cargo xtask proof route --base origin/main --head HEAD`"));
        assert!(section.contains("- `cargo xtask proof preflight --base origin/main --head HEAD`"));
        Ok(())
    }

    #[test]
    fn pr_summary_section_reports_full_proof_for_unknown_surfaces() -> Result<(), String> {
        let summary = summary_for(
            &["scripts/unknown-surface.bin"],
            PreflightReceiptStatus::Missing,
        )?;
        let section = proof_route_section_markdown(&Ok(summary));
        assert!(section.contains("Full proof: 1 unmatched file(s)"));
        assert!(section.contains(UNKNOWN_SURFACE_REASON));
        assert!(section.contains("Release proof required: true"));
        // When release proof is required the section states it explicitly.
        assert!(section.contains("full release proof required"));
        Ok(())
    }

    #[test]
    fn pr_summary_section_states_full_release_proof_for_release_surface() -> Result<(), String> {
        // A release-surface change (Cargo.toml version bump) routes the
        // release-package pack, requires the release lane, is never skipped,
        // and the section states full release proof explicitly.
        let route = route_for(&["Cargo.toml"])?;
        assert!(route.release_proof_required);
        assert!(
            route
                .required_lanes
                .iter()
                .any(|lane| lane.id == "release-readiness-proof")
        );
        assert!(!skipped_lane_ids(&route).contains(&"release-readiness-proof"));
        let summary = summary_for(&["Cargo.toml"], PreflightReceiptStatus::Missing)?;
        let section = proof_route_section_markdown(&Ok(summary));
        assert!(section.contains("Release proof required: true"));
        assert!(section.contains("full release proof required"));
        Ok(())
    }

    #[test]
    fn pr_summary_section_includes_fresh_receipt_status() -> Result<(), String> {
        let binding = test_binding("head-sha");
        let expected =
            ["a", "b", "c", "d"].map(|command| (command.to_string(), vec!["p".to_string()]));
        let receipt_json = json!({
            "schema_version": PROOF_PREFLIGHT_SCHEMA_VERSION,
            "mode": PROOF_PREFLIGHT_MODE,
            "base": {"sha": "base-sha"},
            "reuse_binding": binding,
            "reuse_unavailable_reason": null,
            "head": {"rev": "HEAD", "sha": "head-sha"},
            "overall_status": "fail",
            "commands": [
                {"command": "a", "packs": ["p"], "status": "pass", "exit_code": 0},
                {"command": "b", "packs": ["p"], "status": "pass", "exit_code": 0},
                {"command": "c", "packs": ["p"], "status": "fail", "exit_code": 1},
                {"command": "d", "packs": ["p"], "status": "not_run", "exit_code": null},
            ],
        });
        let text = serde_json::to_string(&receipt_json)
            .map_err(|err| format!("serialize receipt fixture: {err}"))?;
        let receipt = parse_preflight_receipt(&text, "head-sha", Some(&binding), &expected);
        assert_eq!(
            receipt,
            PreflightReceiptStatus::Fresh {
                execution_head_sha: "head-sha".to_string(),
                overall_status: "fail".to_string(),
                passed: 2,
                failed: 1,
                not_run: 1,
            }
        );
        let summary = summary_for(&["docs/specs/SPEC-0001-example.md"], receipt)?;
        let section = proof_route_section_markdown(&Ok(summary));
        assert!(
            section.contains("Preflight receipt: fresh for current head — overall status `fail`")
        );
        assert!(section.contains("commands run: 2 pass, 1 fail (1 not run)"));
        assert!(!section.contains("no fresh preflight receipt"));
        Ok(())
    }

    #[test]
    fn pr_summary_section_never_presents_a_stale_receipt() -> Result<(), String> {
        let text = r#"{"head": {"sha": "old-sha"}, "overall_status": "pass", "commands": []}"#;
        let receipt = parse_preflight_receipt(text, "new-sha", None, &[]);
        assert_eq!(
            receipt,
            PreflightReceiptStatus::Stale {
                receipt_head_sha: "old-sha".to_string(),
                head_sha: "new-sha".to_string(),
            }
        );
        let summary = summary_for(&["docs/specs/SPEC-0001-example.md"], receipt)?;
        let section = proof_route_section_markdown(&Ok(summary));
        assert!(section.contains("no fresh preflight receipt"));
        assert!(section.contains("receipt head `old-sha`"));
        assert!(section.contains("current head `new-sha`"));
        // A stale receipt's results are never surfaced as current.
        assert!(!section.contains("overall status"));
        assert!(!section.contains("commands run:"));
        Ok(())
    }

    #[test]
    fn pr_summary_section_degrades_when_route_computation_fails() {
        let error = "bad proof route revision \"origin/main\": git exited 128\nextra detail line";
        let section = proof_route_section_markdown(&Err(error.to_string()));
        assert!(section.contains("## Proof Route"));
        assert!(
            section.contains("Proof route unavailable: bad proof route revision \"origin/main\"")
        );
        // Multi-line errors collapse to a marked one-liner.
        assert!(!section.contains("extra detail line"));
        assert!(section.contains("truncated"));
        // The advisory contract and reproduction commands survive the failure.
        assert!(section.contains("advisory-report-only"));
        assert!(section.contains("- `cargo xtask proof route --base origin/main --head HEAD`"));
        assert!(section.contains("- `cargo xtask proof preflight --base origin/main --head HEAD`"));
    }

    #[test]
    fn unbound_same_head_receipt_cannot_be_reused() {
        let text = r#"{"head":{"sha":"head-sha"},"overall_status":"pass","commands":[]}"#;
        assert!(matches!(
            parse_preflight_receipt(text, "head-sha", None, &[]),
            PreflightReceiptStatus::Unreadable(_)
        ));
    }

    #[test]
    fn malformed_or_headless_receipts_are_unreadable_not_fresh() {
        assert!(matches!(
            parse_preflight_receipt("not json", "sha", None, &[]),
            PreflightReceiptStatus::Unreadable(_)
        ));
        assert!(matches!(
            parse_preflight_receipt("{}", "sha", None, &[]),
            PreflightReceiptStatus::Unreadable(_)
        ));
    }

    #[test]
    fn route_options_parse_defaults_and_reject_unknown_args() -> Result<(), String> {
        let parsed = parse_route_options(&[])?;
        assert_eq!(parsed.base, DEFAULT_BASE);
        assert_eq!(parsed.head, DEFAULT_HEAD);
        let parsed = parse_route_options(&[
            "--base".to_string(),
            "HEAD~1".to_string(),
            "--head".to_string(),
            "HEAD".to_string(),
        ])?;
        assert_eq!(parsed.base, "HEAD~1");
        assert_eq!(parsed.head, "HEAD");
        for args in [
            vec!["--what".to_string()],
            vec!["--base".to_string()],
            vec!["--base".to_string(), " ".to_string()],
        ] {
            if parse_route_options(&args).is_ok() {
                return Err(format!("expected parse failure for {args:?}"));
            }
        }
        Ok(())
    }

    #[test]
    fn strict_changed_file_inventory_rejects_non_utf8() -> Result<(), String> {
        // The strict-failure side of the NUL authority at the proof-route
        // decode boundary: non-UTF-8 records fail loudly instead of
        // collapsing through lossy conversion.
        let err = match decode_changed_files(b"ok.txt\0\xffbad\0") {
            Err(err) => err,
            Ok(files) => {
                return Err(format!("non-UTF-8 inventory must fail, decoded {files:?}"));
            }
        };
        if !err.contains("not valid UTF-8") {
            return Err(format!("unexpected strict-decode error: {err}"));
        }
        Ok(())
    }

    #[test]
    fn changed_files_decode_exotic_names_exact() -> Result<(), String> {
        // Discriminates NUL-delimited inventory (#4006): space and
        // non-ASCII names must decode byte-exact; the old line parser kept
        // git's C-quoted octal form. Asserts through the real
        // `changed_files_in` production path in a synthetic repository.
        let repo = temp_repo("ripr-proof-route-names")?;
        run_git(&repo, &["init"])?;
        run_git(
            &repo,
            &["config", "user.email", "ripr-proof@example.invalid"],
        )?;
        run_git(&repo, &["config", "user.name", "RIPR Proof Test"])?;
        write_repo_file(&repo, "base.txt", "base\n")?;
        run_git(&repo, &["add", "."])?;
        run_git(&repo, &["commit", "--no-gpg-sign", "-m", "initial"])?;
        write_repo_file(&repo, "sp ace.txt", "spaces\n")?;
        write_repo_file(&repo, "uni-\u{e9}.txt", "unicode\n")?;
        run_git(&repo, &["add", "-A"])?;
        run_git(&repo, &["commit", "--no-gpg-sign", "-m", "exotic"])?;

        let mut files = changed_files_in(&repo, "HEAD~1", "HEAD")?;
        files.sort();
        let expected = vec!["sp ace.txt".to_string(), "uni-\u{e9}.txt".to_string()];
        std::fs::remove_dir_all(&repo)
            .map_err(|err| format!("cleanup {}: {err}", repo.display()))?;
        if files != expected {
            return Err(format!(
                "exotic proof-route inventory mismatch: got {files:?}, want {expected:?}"
            ));
        }
        Ok(())
    }

    fn temp_repo(name: &str) -> Result<std::path::PathBuf, String> {
        let unique = format!(
            "{}-{}-{}",
            name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|err| format!("system clock before epoch: {err}"))?
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path)
            .map_err(|err| format!("create {}: {err}", path.display()))?;
        Ok(path)
    }

    fn write_repo_file(repo: &Path, relative: &str, text: &str) -> Result<(), String> {
        let path = repo.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("create {}: {err}", parent.display()))?;
        }
        std::fs::write(&path, text).map_err(|err| format!("write {}: {err}", path.display()))
    }

    fn run_git(repo: &Path, args: &[&str]) -> Result<(), String> {
        // Route through the centralized runner: the fixture must not add
        // its own raw spawn site.
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let output =
            crate::run::capture_output_in_dir("git", &owned, repo, "proof route fixture git")?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "git fixture command failed: {args:?}; stderr: {}",
                output.stderr.trim()
            ))
        }
    }
}
