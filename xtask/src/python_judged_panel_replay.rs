//! Python judged PR panel replay (RIPR-SPEC-0092, issue #3555 PR B): replays
//! the retained panel inventory against the real `ripr check` binary without
//! ever rewriting an accepted judgment.
//!
//! Replay consumes the one semantic loader (`python_judged_panel::
//! load_validated_inventory`), then per row: materializes a bounded temp
//! workspace from the retained unified diff alone (the proved base tree plus
//! the head tree the diff applies to), invokes the actual debug binary over
//! the head workspace with `--mode fast --json` and an isolated
//! `RIPR_CACHE_DIR`, and retains a typed candidate record outside the accepted
//! panel under `target/ripr/python-judged-panel/replay/`.
//!
//! Honesty boundaries, each load-bearing:
//!
//! - Materialization is offline and diff-proved only: a row is replayable
//!   exactly when its anchor file's base and head content is fully determined
//!   by the retained diff (hunks cover the file from line 1 with no gaps);
//!   anything else replays as a typed `not_run` outcome. No git clone, no
//!   network, no fabricated content: `--network` is declared and refused.
//! - The materialized file set is exactly the diff-proved extent plus the
//!   replay-harness `ripr.toml` language enablement; historical sweep rows
//!   hand-trim their diffs at anchor lines far from line 1, so those rows
//!   stay `not_run` rather than replaying a guessed rendition.
//! - Comparison is advisory candidate data only: seed rows compare against
//!   `expected_classification`, judged rows against the retained
//!   `actual_classification` with a `PriorActualStale` note when the retained
//!   `judged_against` identity does not name the current binary version. No
//!   accepted label, direction, or artifact is modified.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::python_judged_panel::{
    INVENTORY_PATHS, PythonJudgedPanelItem, RowKind, load_validated_inventory, row_kind,
};
use crate::run::{TimedBytesOutput, capture_bytes_in_dir_with_timeout, run_output_owned};

const RERUN_COMMAND: &str = "cargo xtask python-judged-panel replay";
const RECORDS_DIR: &str = "target/ripr/python-judged-panel/replay";
const RECORD_SCHEMA_VERSION: &str = "0.1";
const RECORD_KIND: &str = "python_judged_panel_replay_record";
const SPEC: &str = "RIPR-SPEC-0092";
/// The minimal replay-harness config: language enablement only, everything
/// else ripr defaults. Its digest is bound into every replay record.
const REPLAY_CONFIG_TOML: &str = "[languages]\nenabled = [\"rust\", \"python\"]\n";
const CONFIG_SOURCE: &str = "replay_harness_language_enablement_otherwise_defaults";
/// Per-case deadline for one `ripr check` over a bounded temp workspace.
const REPLAY_CHECK_TIMEOUT: Duration = Duration::from_mins(5);

/// `--network` is declared so the refusal is typed, not silent: ordinary CI
/// validates retained content offline.
const NETWORK_REFUSAL: &str = "network materialization lands with a later #3555 slice; ordinary CI replays retained content offline";

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let mut limit: Option<usize> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--check" => {}
            "--limit" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(format!(
                        "--limit requires a positive integer\nrerun: {RERUN_COMMAND}"
                    ));
                };
                let parsed: usize = value.trim().parse().map_err(|error| {
                    format!("--limit must be a positive integer: {error}\nrerun: {RERUN_COMMAND}")
                })?;
                if parsed == 0 {
                    return Err(format!(
                        "--limit must be a positive integer\nrerun: {RERUN_COMMAND}"
                    ));
                }
                limit = Some(parsed);
                index += 1;
            }
            "--network" => return Err(format!("{NETWORK_REFUSAL}\nrerun: {RERUN_COMMAND}")),
            other => {
                return Err(format!(
                    "unknown python-judged-panel replay argument `{other}`; expected `replay [--check] [--limit <n>] [--network]`\nrerun: {RERUN_COMMAND}"
                ));
            }
        }
        index += 1;
    }
    let root = Path::new(".");
    let records_dir = root.join(RECORDS_DIR);
    let binary = crate::ripr_fixture_binary()?;
    let summary =
        replay_inventory_at(root, &INVENTORY_PATHS, &records_dir, limit, binary.as_str())?;
    print_summary(&summary);
    Ok(())
}

/// Runs the full offline replay over the given validated inventory. `records_dir`
/// receives one JSON record per attempted case; production passes the
/// repo-relative `target/ripr/python-judged-panel/replay` path, tests pass a
/// temp directory. The accepted panel under `fixtures/` is only ever read.
pub(crate) fn replay_inventory_at(
    root: &Path,
    displays: &[&str],
    records_dir: &Path,
    limit: Option<usize>,
    binary: &str,
) -> Result<ReplaySummary, String> {
    // One semantic loader: replay consumes only rows `check` would accept.
    let loaded = load_validated_inventory(root, displays)?;
    let binary = BinaryIdentity::resolve(binary)?;

    // Phase 1: plan every row from retained content only (no binary run).
    let mut plans = Vec::new();
    for file in &loaded {
        let judged_against = file
            .envelope
            .measurement_summary
            .as_ref()
            .and_then(|summary| summary.judged_against.non_blank_value())
            .map(str::to_string);
        for item in &file.envelope.items {
            plans.push(build_case_plan(
                root,
                file.display.as_str(),
                item,
                judged_against.as_deref(),
            ));
        }
    }

    // Phase 2: bound the replayable queue. Bounded-out rows are disclosed in
    // the summary and intentionally get no record this run.
    let replayable_total = plans
        .iter()
        .filter(|case| matches!(case.plan, CasePlanKind::Replay(_)))
        .count();
    let bounded_out = limit
        .map(|bound| replayable_total.saturating_sub(bound))
        .unwrap_or(0);
    let mut replay_budget = replayable_total.saturating_sub(bounded_out);

    // Phase 3: materialize, invoke the real binary, compare, retain.
    fs::create_dir_all(records_dir).map_err(|error| {
        format!(
            "create replay records directory `{}`: {error}",
            records_dir.display()
        )
    })?;
    let mut records = Vec::new();
    for case in &plans {
        match &case.plan {
            CasePlanKind::Replay(subject) if replay_budget > 0 => {
                replay_budget -= 1;
                let record = replay_materialized_case(root, &binary, case, subject);
                write_record(records_dir, &record)?;
                records.push(record);
            }
            CasePlanKind::Replay(_) => {}
            CasePlanKind::NotRun(reason) => {
                let record = not_run_record(case, &binary, reason);
                write_record(records_dir, &record)?;
                records.push(record);
            }
        }
    }
    Ok(summarize(&records, &binary, bounded_out))
}

fn print_summary(summary: &ReplaySummary) {
    println!(
        "Python judged PR panel replay (offline over retained content): replayed={} not_run={} mismatched={} bounded_out={}",
        summary.replayed, summary.not_run, summary.mismatched, summary.bounded_out
    );
    for line in &summary.case_lines {
        println!("{line}");
    }
    println!(
        "binary: {} (sha256 {})",
        summary.binary.version, summary.binary.sha256
    );
    println!("accepted panel untouched: candidate records only, under {RECORDS_DIR}/");
    println!("rerun: {RERUN_COMMAND}");
}

/// One case's retained-content plan: either a materializable subject or a
/// typed reason it cannot be replayed offline.
enum CasePlanKind {
    Replay(SubjectMaterialization),
    NotRun(String),
}

/// A replayable subject: every file the diff proves, plus the anchor identity
/// the candidate classification is read against.
struct SubjectMaterialization {
    anchor_file: String,
    anchor_line: Option<u64>,
    anchor_owner: String,
    files: Vec<ProvedFile>,
}

/// One diff-proved file: base and/or head content fully determined by the
/// retained hunks (`None` when that side does not exist or is not proved).
struct ProvedFile {
    path: String,
    base: Option<String>,
    head: Option<String>,
}

/// Everything a replay record needs about one row, captured from the validated
/// loader output before any execution.
struct CasePlan {
    case_id: String,
    source_envelope: String,
    row_kind: RowKind,
    diff_path: String,
    diff_sha256: String,
    expected_direction: String,
    expected_classification: Option<String>,
    actual_classification: Option<String>,
    judged_against: Option<String>,
    plan: CasePlanKind,
    /// Sorted head-relative file names the materialization will write.
    workspace_files: Vec<String>,
}

fn build_case_plan(
    root: &Path,
    display: &str,
    item: &PythonJudgedPanelItem,
    judged_against: Option<&str>,
) -> CasePlan {
    let diff_sha256 = sha256_file_or_blank(&root.join(&item.diff_path));
    let (plan, workspace_files) = plan_case(root, item);
    CasePlan {
        case_id: item.id.clone(),
        source_envelope: display.to_string(),
        row_kind: row_kind(item),
        diff_path: item.diff_path.clone(),
        diff_sha256,
        expected_direction: item.expected_direction.clone(),
        expected_classification: item
            .expected_classification
            .non_blank_value()
            .map(str::to_string),
        actual_classification: item
            .actual_classification
            .non_blank_value()
            .map(str::to_string),
        judged_against: judged_against.map(str::to_string),
        plan,
        workspace_files,
    }
}

/// Plans one row from retained content only: confined diff load, strict parse,
/// anchor-section resolution, and proved-extent reconstruction.
fn plan_case(root: &Path, item: &PythonJudgedPanelItem) -> (CasePlanKind, Vec<String>) {
    // Every insufficiency below is a typed not_run outcome, never an error.
    let not_run = |reason: String| (CasePlanKind::NotRun(reason), Vec::new());
    let mut violations = Vec::new();
    let Some(diff_body) = crate::python_judged_panel::load_diff_body(
        root,
        &item.diff_path,
        &format!("item `{}`", item.id),
        &mut violations,
    ) else {
        return not_run(format!("retained diff unusable: {}", violations.join("; ")));
    };
    let parsed = match crate::python_judged_panel::parse_unified_diff(&diff_body) {
        Ok(parsed) => parsed,
        Err(error) => {
            return not_run(format!(
                "retained diff does not parse as a strict unified diff: {error}"
            ));
        }
    };
    let Some(anchor_file) = item.anchor.file.non_blank_value() else {
        return not_run(
            "row declares no anchor file (carryover/historical row); replaying it would invent a subject"
                .to_string(),
        );
    };
    let touches_anchor = |section: &crate::python_judged_panel::DiffSection| {
        section.old_path.as_deref() == Some(anchor_file)
            || section.new_path.as_deref() == Some(anchor_file)
    };
    let anchor_sections = parsed.sections.iter().filter(|s| touches_anchor(s)).count();
    if anchor_sections == 0 {
        return not_run(format!(
            "retained diff does not touch anchor file `{anchor_file}`"
        ));
    }
    if anchor_sections > 1 {
        return not_run(format!(
            "anchor file `{anchor_file}` occurs in {anchor_sections} file sections"
        ));
    }

    let mut files = Vec::new();
    let mut anchor_head_lines: Option<u64> = None;
    for section in &parsed.sections {
        let base = reconstruct_side(section, Side::Old);
        let head = reconstruct_side(section, Side::New);
        if touches_anchor(section) {
            // The declared base file must be reconstructable, and the head side
            // the candidate is read against must be proved too.
            if section.old_path.as_deref() == Some(anchor_file) && base.is_none() {
                return not_run(format!(
                    "base of anchor file `{anchor_file}` is not reconstructable: retained hunks do not cover it from line 1 without gaps"
                ));
            }
            if section.new_path.as_deref() == Some(anchor_file) && head.is_none() {
                return not_run(format!(
                    "head of anchor file `{anchor_file}` is not reconstructable: retained hunks do not cover it from line 1 without gaps"
                ));
            }
            anchor_head_lines = head.as_ref().map(|body| body.lines().count() as u64);
        }
        let Some(path) = section
            .new_path
            .clone()
            .or_else(|| section.old_path.clone())
        else {
            continue;
        };
        if !is_confined_materialization_path(&path) {
            return not_run(format!(
                "diff declares path `{path}` which would escape the materialized workspace"
            ));
        }
        if base.is_none() && head.is_none() {
            continue;
        }
        files.push(ProvedFile { path, base, head });
    }
    if anchor_head_lines.is_none() && item.anchor.line.value().is_some() {
        return not_run(format!(
            "anchor file `{anchor_file}` proved no head content to read the candidate against"
        ));
    }
    // The anchor line must sit inside the proved head extent, or the candidate
    // would be read against a truncated rendition.
    if let (Some(line), Some(head_lines)) = (item.anchor.line.value(), anchor_head_lines)
        && *line > head_lines
    {
        return not_run(format!(
            "anchor line {line} sits outside the diff-proved head extent ({head_lines} lines)"
        ));
    }
    let mut workspace_files = files
        .iter()
        .filter(|file| file.head.is_some())
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();
    workspace_files.push("ripr.toml".to_string());
    workspace_files.sort();
    (
        CasePlanKind::Replay(SubjectMaterialization {
            anchor_file: anchor_file.to_string(),
            anchor_line: item.anchor.line.value().copied(),
            anchor_owner: item.anchor.owner.clone(),
            files,
        }),
        workspace_files,
    )
}

/// Materialization write paths must stay inside the temp workspace: relative,
/// forward-slash only, no `.`/`..` components.
fn is_confined_materialization_path(path: &str) -> bool {
    let as_path = Path::new(path);
    !path.is_empty()
        && !path.contains('\\')
        && !path.contains(":")
        && !as_path.is_absolute()
        && !as_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
}

#[derive(Clone, Copy)]
enum Side {
    Old,
    New,
}

/// Reconstructs one side's full content when the hunks provably cover the file
/// from line 1 with no gaps; `None` when the side is not fully determined by
/// the retained diff (or the side does not exist, e.g. a new file's old side).
fn reconstruct_side(
    section: &crate::python_judged_panel::DiffSection,
    side: Side,
) -> Option<String> {
    use crate::python_judged_panel::DiffLineKind;
    let exists = match side {
        Side::Old => section.old_path.is_some(),
        Side::New => section.new_path.is_some(),
    };
    if !exists || section.hunks.is_empty() {
        return None;
    }
    let mut expected = 1_u64;
    for hunk in &section.hunks {
        let (start, count) = match side {
            Side::Old => (hunk.old_start, hunk.next_old.saturating_sub(hunk.old_start)),
            Side::New => (hunk.new_start, hunk.next_new.saturating_sub(hunk.new_start)),
        };
        if start != expected {
            return None;
        }
        expected = expected.checked_add(count)?;
    }
    let mut lines = Vec::new();
    for hunk in &section.hunks {
        for line in &hunk.lines {
            let keep = !matches!(
                (side, line.kind),
                (Side::Old, DiffLineKind::Added) | (Side::New, DiffLineKind::Deleted)
            );
            if keep {
                lines.push(line.text.clone());
            }
        }
    }
    let mut content = lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    Some(content)
}

#[derive(Debug, Clone, Serialize)]
struct BinaryIdentity {
    version: String,
    version_token: String,
    path: String,
    sha256: String,
}

impl BinaryIdentity {
    fn resolve(binary: &str) -> Result<Self, String> {
        let version = run_output_owned(binary, &["--version".to_string()])?
            .trim()
            .to_string();
        if version.is_empty() {
            return Err(format!("`{binary} --version` produced no output"));
        }
        let version_token = version
            .split_whitespace()
            .last()
            .unwrap_or(version.as_str())
            .to_string();
        let sha256 = sha256_file_or_blank(Path::new(binary));
        if sha256.is_empty() {
            return Err(format!(
                "read replay binary `{binary}` for identity binding"
            ));
        }
        Ok(Self {
            version,
            version_token,
            path: binary.to_string(),
            sha256,
        })
    }
}

/// Materializes the bounded temp workspace and runs the real binary over it.
fn replay_materialized_case(
    root: &Path,
    binary: &BinaryIdentity,
    case: &CasePlan,
    subject: &SubjectMaterialization,
) -> ReplayRecord {
    let workspace = match MaterializedWorkspace::create(subject) {
        Ok(workspace) => workspace,
        Err(error) => {
            return not_run_record(case, binary, &format!("materialization failed: {error}"));
        }
    };
    let outcome = match std::path::absolute(root.join(&case.diff_path)) {
        Err(error) => not_run_record(
            case,
            binary,
            &format!("resolve retained diff path: {error}"),
        ),
        Ok(diff_absolute) => {
            let cache_dir = workspace.cache.display().to_string();
            let args: Vec<String> = [
                "check",
                "--root",
                workspace.head.display().to_string().as_str(),
                "--diff",
                diff_absolute.display().to_string().as_str(),
                "--mode",
                "fast",
                "--json",
            ]
            .iter()
            .map(|part| (*part).to_string())
            .collect();

            let mut command = vec![binary.path.clone()];
            command.extend(args.iter().cloned());
            match capture_bytes_in_dir_with_timeout(
                Path::new(&binary.path),
                &args,
                &workspace.root,
                &[("RIPR_CACHE_DIR", cache_dir.as_str())],
                &[],
                REPLAY_CHECK_TIMEOUT,
                &format!("ripr check replay for case `{}`", case.case_id),
            ) {
                Err(error) => {
                    not_run_record(case, binary, &format!("binary invocation failed: {error}"))
                }
                Ok(timed) => build_run_record(case, binary, subject, &workspace, &command, timed),
            }
        }
    };
    workspace.discard();
    outcome
}

fn build_run_record(
    case: &CasePlan,
    binary: &BinaryIdentity,
    subject: &SubjectMaterialization,
    workspace: &MaterializedWorkspace,
    command: &[String],
    output: TimedBytesOutput,
) -> ReplayRecord {
    let exit_code = output.status.and_then(|status| status.code());
    let outcome = if output.timed_out {
        ReplayOutcome::TimedOut {
            timeout_secs: REPLAY_CHECK_TIMEOUT.as_secs(),
        }
    } else if exit_code != Some(0) {
        let stderr = String::from_utf8_lossy(&output.stderr);
        ReplayOutcome::Failed {
            exit_code,
            detail: first_lines(&stderr, 3),
        }
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        match serde_json::from_str::<serde_json::Value>(&stdout) {
            Err(error) => ReplayOutcome::ParseFailed {
                detail: format!("check stdout is not JSON: {error}"),
            },
            Ok(check) => outcome_from_check(&check, subject),
        }
    };
    let candidate = candidate_of(&outcome);
    let comparison = compare_candidate(case, candidate, binary);
    ReplayRecord {
        schema_version: RECORD_SCHEMA_VERSION,
        kind: RECORD_KIND,
        spec: SPEC,
        case_id: case.case_id.clone(),
        source_envelope: case.source_envelope.clone(),
        row_kind: row_kind_name(case.row_kind),
        expected_direction: case.expected_direction.clone(),
        binary: binary.clone(),
        diff: DiffIdentity {
            path: case.diff_path.clone(),
            sha256: case.diff_sha256.clone(),
        },
        config: config_identity(),
        workspace: Some(WorkspaceIdentity {
            root_digest: workspace.head_digest.clone(),
            scope: "diff_proved_lines_plus_replay_config",
            materialized_files: case.workspace_files.clone(),
        }),
        command: Some(command.to_vec()),
        outcome,
        comparison,
        authority_boundary: "review_advisory_only",
        accepted_judgment_modified: false,
    }
}

/// Maps one successful `ripr check --json` run into a typed outcome and
/// extracts the candidate classification at the anchor.
fn analysis_child<'a>(check: &'a serde_json::Value, field: &str) -> Option<&'a serde_json::Value> {
    check.get("analysis_outcome")?.get("outcome")?.get(field)
}

fn outcome_from_check(
    check: &serde_json::Value,
    subject: &SubjectMaterialization,
) -> ReplayOutcome {
    let findings = check
        .get("findings")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let findings_total = findings.len();
    let analysis_kind = analysis_child(check, "kind")
        .and_then(|value| value.as_str())
        .unwrap_or("unreported")
        .to_string();
    let complete = check
        .get("analysis_outcome")
        .and_then(|outcome| outcome.get("analysis_complete"))
        .and_then(|value| value.as_bool());
    let (candidate_classification, anchor_finding_id) = candidate_at_anchor(&findings, subject)
        .map_or((None, None), |(classification, id)| {
            (Some(classification), id)
        });
    let limitations = analysis_child(check, "limitations")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    item.get("kind")
                        .and_then(|value| value.as_str())
                        .unwrap_or("unreported_limitation")
                        .to_string()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    match complete {
        Some(true) => ReplayOutcome::Complete {
            analysis_kind,
            findings_total,
            candidate_classification,
            anchor_finding_id,
        },
        _ => ReplayOutcome::Partial {
            analysis_kind,
            limitations,
            findings_total,
            candidate_classification,
            anchor_finding_id,
        },
    }
}

/// Conservative anchor binding: findings in the anchor file, preferring the
/// anchor's exact owner, then the nearest probe line to the anchor line.
/// Token proximity never upgrades to identity; a miss stays quiet.
/// Returns (classification, finding id).
fn candidate_at_anchor(
    findings: &[serde_json::Value],
    subject: &SubjectMaterialization,
) -> Option<(String, Option<String>)> {
    let in_file = findings.iter().filter(|finding| {
        finding_field(finding, "file").as_deref() == Some(subject.anchor_file.as_str())
    });
    let best = in_file.min_by_key(|finding| {
        let owner_mismatch =
            finding_field(finding, "owner").as_deref() != Some(subject.anchor_owner.as_str());
        let line_distance = finding
            .get("probe")
            .and_then(|probe| probe.get("line"))
            .and_then(|value| value.as_u64())
            .map(|line| match subject.anchor_line {
                Some(anchor) => line.abs_diff(anchor),
                None => line,
            })
            .unwrap_or(u64::MAX);
        (owner_mismatch, line_distance)
    })?;
    Some((
        best.get("classification")?.as_str()?.to_string(),
        best.get("id")
            .and_then(|value| value.as_str())
            .map(String::from),
    ))
}

/// Conservative identity read: the canonical gap field, falling back to the
/// probe field. Never a token heuristic.
fn finding_field(finding: &serde_json::Value, field: &str) -> Option<String> {
    finding
        .get("canonical_gap")
        .and_then(|gap| gap.get(field))
        .and_then(|value| value.as_str())
        .or_else(|| {
            finding
                .get("probe")
                .and_then(|probe| probe.get(field))
                .and_then(|value| value.as_str())
        })
        .map(String::from)
}

fn candidate_of(outcome: &ReplayOutcome) -> Option<&str> {
    match outcome {
        ReplayOutcome::Complete {
            candidate_classification,
            ..
        }
        | ReplayOutcome::Partial {
            candidate_classification,
            ..
        } => candidate_classification.as_deref(),
        _ => None,
    }
}

/// Advisory comparison against exactly what the retained row claims. Judged
/// rows compare candidate-vs-prior-actual and are stale-noted when the
/// retained `judged_against` identity does not name the replayed version.
/// The typed divergence between what the row claims and what the replayed
/// binary produced; `None` when the candidate agrees with the claim.
fn mismatch_for(basis: &str, claimed: &str, candidate: Option<&str>) -> Option<ReplayMismatch> {
    match candidate {
        Some(found) if found != claimed => Some(if basis == "expected_classification" {
            ReplayMismatch::ClassificationMismatch {
                expected: claimed.to_string(),
                candidate: found.to_string(),
            }
        } else {
            ReplayMismatch::PriorActualMismatch {
                prior_actual: claimed.to_string(),
                candidate: found.to_string(),
            }
        }),
        Some(_) => None,
        None => Some(if basis == "expected_classification" {
            ReplayMismatch::ExpectedButQuiet {
                expected: claimed.to_string(),
            }
        } else {
            ReplayMismatch::PriorActualQuiet {
                prior_actual: claimed.to_string(),
            }
        }),
    }
}

fn compare_candidate(
    case: &CasePlan,
    candidate: Option<&str>,
    binary: &BinaryIdentity,
) -> ComparisonRecord {
    let mut comparison = ComparisonRecord {
        basis: None,
        expected: None,
        candidate: candidate.map(str::to_string),
        mismatches: Vec::new(),
        stale: None,
    };
    let (basis, claimed) = match case.row_kind {
        RowKind::Seed => (
            "expected_classification",
            case.expected_classification.as_deref(),
        ),
        RowKind::Judged => (
            "prior_actual_classification",
            case.actual_classification.as_deref(),
        ),
        RowKind::Carryover => return comparison,
    };
    comparison.basis = Some(basis);
    comparison.expected = claimed.map(str::to_string);
    if let Some(claimed) = claimed
        && let Some(mismatch) = mismatch_for(basis, claimed, candidate)
    {
        comparison.mismatches.push(mismatch);
    }
    if case.row_kind == RowKind::Judged {
        let stale = match case.judged_against.as_deref() {
            Some(text) => !text.contains(&binary.version_token),
            None => true,
        };
        if stale {
            comparison.stale = Some(PriorActualStale {
                judged_against: case.judged_against.clone(),
                current_binary_version: binary.version.clone(),
            });
        }
    }
    comparison
}

#[derive(Debug, Serialize)]
struct ReplayRecord {
    schema_version: &'static str,
    kind: &'static str,
    spec: &'static str,
    case_id: String,
    source_envelope: String,
    row_kind: &'static str,
    expected_direction: String,
    binary: BinaryIdentity,
    diff: DiffIdentity,
    config: ConfigIdentity,
    workspace: Option<WorkspaceIdentity>,
    command: Option<Vec<String>>,
    outcome: ReplayOutcome,
    comparison: ComparisonRecord,
    authority_boundary: &'static str,
    accepted_judgment_modified: bool,
}

#[derive(Debug, Serialize)]
struct DiffIdentity {
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct ConfigIdentity {
    source: &'static str,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct WorkspaceIdentity {
    root_digest: String,
    scope: &'static str,
    materialized_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReplayOutcome {
    Complete {
        analysis_kind: String,
        findings_total: usize,
        candidate_classification: Option<String>,
        anchor_finding_id: Option<String>,
    },
    Partial {
        analysis_kind: String,
        limitations: Vec<String>,
        findings_total: usize,
        candidate_classification: Option<String>,
        anchor_finding_id: Option<String>,
    },
    Failed {
        exit_code: Option<i32>,
        detail: String,
    },
    ParseFailed {
        detail: String,
    },
    TimedOut {
        timeout_secs: u64,
    },
    NotRun {
        reason: String,
    },
}

#[derive(Debug, Serialize)]
struct ComparisonRecord {
    /// `expected_classification` for seed rows, `prior_actual_classification`
    /// for judged rows, absent for carryover rows.
    basis: Option<&'static str>,
    expected: Option<String>,
    candidate: Option<String>,
    mismatches: Vec<ReplayMismatch>,
    stale: Option<PriorActualStale>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ReplayMismatch {
    ClassificationMismatch {
        expected: String,
        candidate: String,
    },
    ExpectedButQuiet {
        expected: String,
    },
    PriorActualMismatch {
        prior_actual: String,
        candidate: String,
    },
    PriorActualQuiet {
        prior_actual: String,
    },
}

/// A judged-row currentness note: the retained judgment was made against an
/// identity that does not name the replayed binary version, so divergence is
/// expected and non-conclusive.
#[derive(Debug, Serialize)]
struct PriorActualStale {
    judged_against: Option<String>,
    current_binary_version: String,
}

fn not_run_record(case: &CasePlan, binary: &BinaryIdentity, reason: &str) -> ReplayRecord {
    ReplayRecord {
        schema_version: RECORD_SCHEMA_VERSION,
        kind: RECORD_KIND,
        spec: SPEC,
        case_id: case.case_id.clone(),
        source_envelope: case.source_envelope.clone(),
        row_kind: row_kind_name(case.row_kind),
        expected_direction: case.expected_direction.clone(),
        binary: binary.clone(),
        diff: DiffIdentity {
            path: case.diff_path.clone(),
            sha256: case.diff_sha256.clone(),
        },
        config: config_identity(),
        workspace: None,
        command: None,
        outcome: ReplayOutcome::NotRun {
            reason: reason.to_string(),
        },
        comparison: ComparisonRecord {
            basis: None,
            expected: None,
            candidate: None,
            mismatches: Vec::new(),
            stale: None,
        },
        authority_boundary: "review_advisory_only",
        accepted_judgment_modified: false,
    }
}

fn config_identity() -> ConfigIdentity {
    ConfigIdentity {
        source: CONFIG_SOURCE,
        sha256: sha256_hex(REPLAY_CONFIG_TOML.as_bytes()),
    }
}

fn row_kind_name(kind: RowKind) -> &'static str {
    match kind {
        RowKind::Seed => "seed",
        RowKind::Judged => "judged",
        RowKind::Carryover => "carryover",
    }
}

fn write_record(records_dir: &Path, record: &ReplayRecord) -> Result<(), String> {
    let path = records_dir.join(format!("{}.json", stable_case_slug(&record.case_id)));
    let body = serde_json::to_string_pretty(record)
        .map_err(|error| format!("serialize replay record for `{}`: {error}", record.case_id))?;
    fs::write(&path, body)
        .map_err(|error| format!("write replay record `{}`: {error}", path.display()))
}

fn stable_case_slug(case_id: &str) -> String {
    case_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

struct MaterializedWorkspace {
    root: PathBuf,
    head: PathBuf,
    cache: PathBuf,
    head_digest: String,
}

impl MaterializedWorkspace {
    fn create(subject: &SubjectMaterialization) -> Result<Self, String> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "ripr-python-judged-panel-replay-{}-{}",
            std::process::id(),
            unique
        ));
        let base = root.join("base");
        let head = root.join("head");
        let cache = root.join("cache");
        fs::create_dir_all(&base).map_err(|error| format!("create base tree: {error}"))?;
        fs::create_dir_all(&head).map_err(|error| format!("create head tree: {error}"))?;
        fs::create_dir_all(&cache).map_err(|error| format!("create cache dir: {error}"))?;
        let mut head_entries = Vec::new();
        for file in &subject.files {
            if let Some(base_body) = &file.base {
                write_workspace_file(&base, &file.path, base_body)?;
            }
            if let Some(head_body) = &file.head {
                write_workspace_file(&head, &file.path, head_body)?;
                head_entries.push((file.path.clone(), sha256_hex(head_body.as_bytes())));
            }
        }
        write_workspace_file(&head, "ripr.toml", REPLAY_CONFIG_TOML)?;
        head_entries.push((
            "ripr.toml".to_string(),
            sha256_hex(REPLAY_CONFIG_TOML.as_bytes()),
        ));
        head_entries.sort();
        let mut digest = Sha256::new();
        for (path, sha) in &head_entries {
            digest.update(path.as_bytes());
            digest.update([0_u8]);
            digest.update(sha.as_bytes());
            digest.update(b"\n");
        }
        Ok(Self {
            root,
            head,
            cache,
            head_digest: to_hex(&digest.finalize()),
        })
    }

    /// The workspace is disposable; removal failure (e.g. a Windows file lock)
    /// is never escalated — the temp tree is outside the repository.
    fn discard(&self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_workspace_file(tree: &Path, relative: &str, body: &str) -> Result<(), String> {
    let target = tree.join(relative);
    let parent = target
        .parent()
        .ok_or_else(|| format!("no parent for `{}`", target.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("create `{}`: {error}", parent.display()))?;
    fs::write(&target, body).map_err(|error| format!("write `{}`: {error}", target.display()))
}

fn sha256_file_or_blank(path: &Path) -> String {
    fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    to_hex(&digest.finalize())
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn first_lines(text: &str, max: usize) -> String {
    let collected: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(max)
        .collect();
    if collected.is_empty() {
        "no stderr output".to_string()
    } else {
        collected.join(" | ")
    }
}

pub(crate) struct ReplaySummary {
    replayed: usize,
    not_run: usize,
    mismatched: usize,
    bounded_out: usize,
    binary: BinaryIdentity,
    case_lines: Vec<String>,
}

fn summarize(
    records: &[ReplayRecord],
    binary: &BinaryIdentity,
    bounded_out: usize,
) -> ReplaySummary {
    let mut replayed = 0_usize;
    let mut not_run = 0_usize;
    let mut mismatched = 0_usize;
    let mut case_lines = Vec::new();
    for record in records {
        match &record.outcome {
            ReplayOutcome::NotRun { reason } => {
                not_run += 1;
                case_lines.push(format!(
                    "- {} outcome=not_run reason={reason}",
                    record.case_id
                ));
            }
            outcome => {
                replayed += 1;
                let mismatches = describe_mismatches(&record.comparison.mismatches);
                if !mismatches.is_empty() {
                    mismatched += 1;
                }
                let candidate = record
                    .comparison
                    .candidate
                    .clone()
                    .unwrap_or_else(|| "quiet".to_string());
                case_lines.push(format!(
                    "- {} outcome={} analysis={} candidate={candidate}{}",
                    record.case_id,
                    outcome_kind(outcome),
                    analysis_kind(outcome),
                    if mismatches.is_empty() {
                        String::new()
                    } else {
                        format!(" mismatches=[{mismatches}]")
                    }
                ));
            }
        }
    }
    case_lines.sort();
    ReplaySummary {
        replayed,
        not_run,
        mismatched,
        bounded_out,
        binary: binary.clone(),
        case_lines,
    }
}

fn outcome_kind(outcome: &ReplayOutcome) -> &'static str {
    match outcome {
        ReplayOutcome::Complete { .. } => "complete",
        ReplayOutcome::Partial { .. } => "partial",
        ReplayOutcome::Failed { .. } => "failed",
        ReplayOutcome::ParseFailed { .. } => "parse_failed",
        ReplayOutcome::TimedOut { .. } => "timed_out",
        ReplayOutcome::NotRun { .. } => "not_run",
    }
}

fn analysis_kind(outcome: &ReplayOutcome) -> &str {
    match outcome {
        ReplayOutcome::Complete { analysis_kind, .. }
        | ReplayOutcome::Partial { analysis_kind, .. } => analysis_kind,
        _ => "none",
    }
}

fn describe_mismatches(mismatches: &[ReplayMismatch]) -> String {
    serde_json::to_string(mismatches).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    use super::{replay_inventory_at, run, stable_case_slug};

    const PANEL_DIR: &str = "fixtures/python-judged-pr-panel";

    // Fully-covered synthetic diffs (hunks start at line 1), plus a
    // tenacity-style partial diff whose hunks start at line 88.
    const BOUNDARY_FLIP_BODY: &str = "--- a/pricing.py\n+++ b/pricing.py\n@@ -1,4 +1,4 @@\n def apply_discount(amount, threshold):\n-    if amount >= threshold:\n+    if amount > threshold:\n         return amount * 0.9\n     return amount\n";
    const DIRECT_ASSERTION_BODY: &str = "--- a/pricing.py\n+++ b/pricing.py\n@@ -1,4 +1,4 @@\n def apply_discount(amount, threshold):\n     if amount >= threshold:\n-        return amount * 0.9\n+        return amount * 0.85\n     return amount\n";
    const DECORATOR_BODY: &str = "--- a/routes.py\n+++ b/routes.py\n@@ -1,3 +1,3 @@\n @app.route(\"/checkout\", methods=[\"POST\"])\n def checkout(order):\n-    return {\"total\": order.subtotal}\n+    return {\"total\": order.subtotal, \"tax\": order.subtotal * 0.2}\n";
    const PARTIAL_BODY: &str = "--- a/tenacity/stop.py\n+++ b/tenacity/stop.py\n@@ -88,4 +88,4 @@ class stop_after_attempt(stop_base):\n         self.max_attempt_number = max_attempt_number\n\n     def __call__(self, retry_state: \"RetryCallState\") -> bool:\n-        return retry_state.attempt_number >= self.max_attempt_number\n+        return retry_state.attempt_number > self.max_attempt_number\n";
    const JUDGED_BODY: &str = "--- a/judged.py\n+++ b/judged.py\n@@ -1,3 +1,3 @@\n def judged_owner(value):\n-    if value >= 10:\n+    if value > 10:\n         return \"high\"\n";

    struct TempFixture {
        root: PathBuf,
    }

    impl TempFixture {
        fn new(name: &str) -> Result<Self, String> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "ripr-py-panel-replay-test-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(root.join(format!("{PANEL_DIR}/diffs")))
                .map_err(|error| format!("create test fixture: {error}"))?;
            Ok(Self { root })
        }

        fn write_diff(&self, name: &str, body: &str) -> Result<String, String> {
            let relative = format!("{PANEL_DIR}/diffs/{name}.diff");
            fs::write(self.root.join(&relative), body)
                .map_err(|error| format!("write test diff: {error}"))?;
            Ok(relative)
        }

        fn write_envelope(&self, name: &str, value: &Value) -> Result<String, String> {
            let relative = format!("{PANEL_DIR}/{name}");
            let body = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
            fs::write(self.root.join(&relative), body).map_err(|error| error.to_string())?;
            Ok(relative)
        }

        fn panel_file(&self, name: &str) -> PathBuf {
            self.root.join(format!("{PANEL_DIR}/{name}"))
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// The retained row identity a synthetic replay row is built from.
    struct RowSpec<'a> {
        id: &'a str,
        repo: &'a str,
        direction: &'a str,
        diff_path: &'a str,
        target: &'a str,
        anchor_line: u64,
        owner: &'a str,
        expected: &'a str,
    }

    fn seed_item(spec: &RowSpec) -> Value {
        json!({
            "id": spec.id,
            "repo": spec.repo,
            "diff_path": spec.diff_path,
            "shape": ["pytest_library"],
            "expected_direction": spec.direction,
            "anchor": {
                "file": spec.target,
                "line": spec.anchor_line,
                "owner": spec.owner,
                "boundary": "predicate equality boundary"
            },
            "expected_classification": spec.expected,
            "expected_static_limit_kind": if spec.direction == "should_limit" {
                Value::String("decorator_indirection".to_string())
            } else {
                Value::Null
            },
            "labels": {
                "top_card_useful": null,
                "false_actionable": null,
                "false_exposed": null,
                "verify_command_valid": null,
                "suggested_location_valid": null,
                "packet_boundaries_safe": null,
                "limitation_quality": null
            },
            "authority_boundary": "review_advisory_only",
            "repair_packet_ready": false,
            "must_not_claim": ["Do not treat a null label as a passing judgment."],
            "reason": "synthetic replay selection reason"
        })
    }

    fn coverage_rows(fixture: &TempFixture) -> Result<Vec<Value>, String> {
        let gap = fixture.write_diff("replay-gap", BOUNDARY_FLIP_BODY)?;
        let quiet = fixture.write_diff("replay-quiet", DIRECT_ASSERTION_BODY)?;
        let limit = fixture.write_diff("replay-limit", DECORATOR_BODY)?;
        Ok(vec![
            seed_item(&RowSpec {
                id: "replay-gap-row",
                repo: "replay-gap-repo",
                direction: "should_gap",
                diff_path: &gap,
                target: "pricing.py",
                anchor_line: 2,
                owner: "apply_discount",
                expected: "weakly_exposed",
            }),
            seed_item(&RowSpec {
                id: "replay-quiet-row",
                repo: "replay-quiet-repo",
                direction: "should_stay_quiet",
                diff_path: &quiet,
                target: "pricing.py",
                anchor_line: 3,
                owner: "apply_discount",
                expected: "exposed",
            }),
            seed_item(&RowSpec {
                id: "replay-limit-row",
                repo: "replay-limit-repo",
                direction: "should_limit",
                diff_path: &limit,
                target: "routes.py",
                anchor_line: 3,
                owner: "checkout",
                expected: "static_unknown",
            }),
        ])
    }

    fn seed_envelope(description: &str, items: Vec<Value>) -> Value {
        json!({
            "schema_version": "0.1",
            "kind": "python_judged_pr_panel_manifest",
            "spec": "RIPR-SPEC-0092",
            "tier": "B",
            "description": description,
            "limits": ["synthetic replay inventory remains advisory only"],
            "items": items
        })
    }

    /// Seed rows over all three directions plus one tenacity-style row whose
    /// hunks start at line 88 (not offline-reconstructable).
    fn mixed_inventory(fixture: &TempFixture) -> Result<Vec<(String, Value)>, String> {
        let mut items = coverage_rows(fixture)?;
        let partial = fixture.write_diff("replay-partial", PARTIAL_BODY)?;
        let mut partial_row = seed_item(&RowSpec {
            id: "replay-partial-row",
            repo: "replay-partial-repo",
            direction: "should_gap",
            diff_path: &partial,
            target: "tenacity/stop.py",
            anchor_line: 91,
            owner: "stop_after_attempt.__call__",
            expected: "weakly_exposed",
        });
        partial_row["anchor"]["boundary"] = json!("attempt_number >= max guard");
        items.push(partial_row);
        Ok(vec![(
            "replay-seed.json".to_string(),
            seed_envelope(
                "Synthetic replay inventory: three directions plus an unprovable partial row.",
                items,
            ),
        )])
    }

    /// Two coverage seed rows plus one judged row bound to a judged_against
    /// identity, exercising the prior-actual comparison path.
    fn judged_inventory(
        fixture: &TempFixture,
        judged_against: Option<&str>,
    ) -> Result<Vec<(String, Value)>, String> {
        let judged = fixture.write_diff("replay-judged", JUDGED_BODY)?;
        let mut item = seed_item(&RowSpec {
            id: "replay-judged-row",
            repo: "replay-judged-repo",
            direction: "should_limit",
            diff_path: &judged,
            target: "judged.py",
            anchor_line: 2,
            owner: "judged_owner",
            expected: "static_unknown",
        });
        item["labels"]["false_actionable"] = json!(false);
        item["labels"]["false_exposed"] = json!(false);
        item["labels"]["packet_boundaries_safe"] = json!(true);
        let object = item.as_object_mut().ok_or("item must be an object")?;
        object.remove("must_not_claim");
        object.insert("actual_classification".to_string(), json!("static_unknown"));
        object.insert("actual_oracle_alignment".to_string(), json!("unknown"));
        object.insert("judgment_source".to_string(), json!("manual_review"));
        object.insert("judged_at".to_string(), json!("2026-06-13"));
        object.insert("judged_by".to_string(), json!("campaign"));
        let mut summary = json!({
            "items_judged": 1,
            "false_exposed_count": 0,
            "false_actionable_count": 0,
            "note": "synthetic judged measurement note"
        });
        if let Some(identity) = judged_against {
            summary["judged_against"] = json!(identity);
        }
        let mut items = coverage_rows(fixture)?;
        items.push(item);
        let mut envelope = seed_envelope("Synthetic judged replay inventory.", items);
        envelope["measurement_summary"] = summary;
        Ok(vec![("replay-judged.json".to_string(), envelope)])
    }

    /// Writes the inventory and replays it with records kept in a temp
    /// directory outside the panel.
    /// Pre-writes the envelopes so panel snapshots taken around a replay run
    /// cover the same on-disk inventory.
    fn write_envelopes(
        fixture: &TempFixture,
        envelopes: &[(String, Value)],
    ) -> Result<Vec<String>, String> {
        let mut displays = Vec::new();
        for (name, value) in envelopes {
            displays.push(fixture.write_envelope(name, value)?);
        }
        Ok(displays)
    }

    fn replay(
        fixture: &TempFixture,
        envelopes: &[(String, Value)],
        limit: Option<usize>,
    ) -> Result<(super::ReplaySummary, PathBuf), String> {
        let displays = write_envelopes(fixture, envelopes)?;
        let refs = displays.iter().map(String::as_str).collect::<Vec<_>>();
        let records_dir = fixture.root.join("replay-records");
        let summary = replay_inventory_at(
            &fixture.root,
            &refs,
            &records_dir,
            limit,
            worktree_binary()?.as_str(),
        )?;
        Ok((summary, records_dir))
    }

    /// The absolute worktree debug binary: tests run with cwd set to the
    /// package directory, so the cwd-relative fixture resolution would
    /// escape to the wrong target tree (see AGENTS.md verification bias).
    fn worktree_binary() -> Result<String, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("xtask manifest has no repository parent")?;
        let binary = root
            .join("target")
            .join("debug")
            .join(format!("ripr{}", std::env::consts::EXE_SUFFIX));
        if !binary.is_file() {
            crate::run::run_in_dir(Path::new("cargo"), &["build", "-p", "ripr"], root)?;
        }
        std::path::absolute(&binary)
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("resolve worktree ripr binary: {error}"))
    }

    fn read_record(records_dir: &Path, case_id: &str) -> Result<Value, String> {
        let path = records_dir.join(format!("{}.json", stable_case_slug(case_id)));
        let body = fs::read_to_string(&path)
            .map_err(|error| format!("read record `{}`: {error}", path.display()))?;
        serde_json::from_str(&body).map_err(|error| error.to_string())
    }

    /// sha256 over every panel file (path + content): the immutability oracle.
    fn panel_digest(root: &Path) -> Result<String, String> {
        let panel = root.join(PANEL_DIR);
        let mut entries = Vec::new();
        collect_files(&panel, &panel, &mut entries)?;
        entries.sort();
        let mut digest = Sha256::new();
        for (relative, bytes) in entries {
            digest.update(relative.as_bytes());
            digest.update([0_u8]);
            digest.update(&bytes);
            digest.update(b"\n");
        }
        Ok(digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    fn collect_files(
        base: &Path,
        dir: &Path,
        entries: &mut Vec<(String, Vec<u8>)>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                collect_files(base, &path, entries)?;
            } else {
                let relative = path
                    .strip_prefix(base)
                    .map_err(|error| error.to_string())?
                    .to_string_lossy()
                    .replace('\\', "/");
                let bytes = fs::read(&path).map_err(|error| error.to_string())?;
                entries.push((relative, bytes));
            }
        }
        Ok(())
    }

    fn expect_identity_fields(record: &Value) -> Result<(), String> {
        let binary = record
            .get("binary")
            .ok_or("record is missing binary identity")?;
        if !binary["version"]
            .as_str()
            .is_some_and(|version| version.starts_with("ripr "))
        {
            return Err(format!("binary version not bound: {binary}"));
        }
        let binary_sha = binary["sha256"].as_str().ok_or("binary sha256 missing")?;
        if binary_sha.len() != 64 || !binary_sha.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("binary sha256 is not sha256-shaped: {binary_sha}"));
        }
        if record["diff"]["sha256"].as_str().map(str::len) != Some(64)
            || record["config"]["sha256"].as_str().map(str::len) != Some(64)
        {
            return Err("diff/config sha256 not bound".to_string());
        }
        if record["schema_version"] != json!("0.1")
            || record["kind"] != json!("python_judged_panel_replay_record")
            || record["spec"] != json!("RIPR-SPEC-0092")
            || record["accepted_judgment_modified"] != json!(false)
        {
            return Err("record envelope identity drifted".to_string());
        }
        Ok(())
    }

    /// The comparison section must agree with the row's claim and the observed
    /// candidate: this invariant is under test, not one pinned verdict.
    fn expect_comparison_consistent(record: &Value, basis: &str) -> Result<(), String> {
        let comparison = record
            .get("comparison")
            .ok_or("record is missing comparison")?;
        if comparison["basis"] != json!(basis) {
            return Err(format!(
                "comparison basis should be {basis}: {}",
                comparison["basis"]
            ));
        }
        let candidate = comparison["candidate"].as_str();
        let expected = comparison["expected"].as_str();
        let mismatches = comparison["mismatches"]
            .as_array()
            .ok_or("mismatches must be an array")?;
        let mismatch_kind = || -> Result<&str, String> {
            mismatches
                .first()
                .and_then(|mismatch| mismatch["kind"].as_str())
                .ok_or_else(|| "divergence must record a typed mismatch".to_string())
        };
        match (expected, candidate) {
            (Some(expected), Some(candidate)) if expected != candidate => {
                let wanted = if basis == "expected_classification" {
                    "classification_mismatch"
                } else {
                    "prior_actual_mismatch"
                };
                if mismatch_kind()? != wanted {
                    return Err(format!(
                        "expected mismatch `{wanted}`, found `{}`",
                        mismatch_kind()?
                    ));
                }
            }
            (Some(expected), None) => {
                let wanted = if basis == "expected_classification" {
                    "expected_but_quiet"
                } else {
                    "prior_actual_quiet"
                };
                if mismatch_kind()? != wanted {
                    return Err(format!(
                        "expected mismatch `{wanted}`, found `{}`",
                        mismatch_kind()?
                    ));
                }
                if mismatches[0]["expected"] != json!(expected)
                    && mismatches[0]["prior_actual"] != json!(expected)
                {
                    return Err("mismatch lost the claimed classification".to_string());
                }
            }
            _ => {
                if !mismatches.is_empty() {
                    return Err("matching candidate must not record a mismatch".to_string());
                }
            }
        }
        Ok(())
    }

    /// End-to-end over a temp-materialized synthetic row: base+diff
    /// materialized, real check invocation, candidate classification
    /// extracted, record kept outside the panel, panel bytes untouched.
    #[test]
    fn replay_materializes_row_and_runs_real_check_end_to_end() -> Result<(), String> {
        let fixture = TempFixture::new("end-to-end")?;
        let envelopes = mixed_inventory(&fixture)?;
        write_envelopes(&fixture, &envelopes)?;
        let before = panel_digest(&fixture.root)?;
        let (summary, records_dir) = replay(&fixture, &envelopes, None)?;
        let after = panel_digest(&fixture.root)?;
        if before != after {
            return Err("replay modified the accepted panel".to_string());
        }
        if records_dir.starts_with(fixture.root.join(PANEL_DIR)) {
            return Err("records must live outside the accepted panel directory".to_string());
        }

        // The gap row: fully proved, real binary run, candidate extracted.
        let record = read_record(&records_dir, "replay-gap-row")?;
        expect_identity_fields(&record)?;
        let workspace = record
            .get("workspace")
            .ok_or("replayed record must bind a workspace")?;
        if workspace["root_digest"].as_str().map(str::len) != Some(64) {
            return Err("workspace root digest not bound".to_string());
        }
        let files = workspace["materialized_files"]
            .as_array()
            .ok_or("materialized files missing")?;
        if !files.contains(&json!("pricing.py")) || !files.contains(&json!("ripr.toml")) {
            return Err(format!("workspace materialized the wrong files: {files:?}"));
        }
        let outcome_kind = record["outcome"]["kind"]
            .as_str()
            .ok_or("replayed record must carry an outcome kind")?;
        if outcome_kind != "complete" && outcome_kind != "partial" {
            return Err(format!("expected a run outcome, found {outcome_kind}"));
        }
        let findings_total = record["outcome"]["findings_total"]
            .as_u64()
            .ok_or("findings_total missing")?;
        let candidate = record["comparison"]["candidate"].as_str();
        match (findings_total, candidate) {
            (0, None) => {}
            (1.., Some(_)) => {}
            other => {
                return Err(format!(
                    "candidate disagrees with findings count: {other:?}"
                ));
            }
        }
        let command = record["command"]
            .as_array()
            .ok_or("replay must record the real check command")?;
        if !command.contains(&json!("--mode")) || !command.contains(&json!("fast")) {
            return Err(format!("recorded command lost mode binding: {command:?}"));
        }
        expect_comparison_consistent(&record, "expected_classification")?;

        if summary.replayed != 3 || summary.not_run != 1 || summary.bounded_out != 0 {
            return Err(format!(
                "unexpected summary counts: replayed={} not_run={} bounded_out={}",
                summary.replayed, summary.not_run, summary.bounded_out
            ));
        }
        if summary.case_lines.len() != 4 || !summary.case_lines.iter().is_sorted() {
            return Err("summary case lines must cover every case in sorted order".to_string());
        }
        Ok(())
    }

    /// The quiet row claims `exposed`; whatever the real binary answers, the
    /// divergence (or agreement) must be typed against the accepted claim and
    /// the accepted envelope bytes must stay identical.
    #[test]
    fn replay_records_typed_mismatch_without_touching_accepted_judgment() -> Result<(), String> {
        let fixture = TempFixture::new("mismatch")?;
        let envelopes = mixed_inventory(&fixture)?;
        write_envelopes(&fixture, &envelopes)?;
        let seed_bytes =
            fs::read(fixture.panel_file("replay-seed.json")).map_err(|error| error.to_string())?;
        let (summary, records_dir) = replay(&fixture, &envelopes, None)?;
        let after_bytes =
            fs::read(fixture.panel_file("replay-seed.json")).map_err(|error| error.to_string())?;
        if seed_bytes != after_bytes {
            return Err("replay rewrote an accepted envelope".to_string());
        }
        if summary.mismatched == 0 {
            return Err("expected at least one typed mismatch".to_string());
        }
        let record = read_record(&records_dir, "replay-quiet-row")?;
        if record["row_kind"] != json!("seed") {
            return Err("quiet row must replay as a seed row".to_string());
        }
        expect_comparison_consistent(&record, "expected_classification")?;
        if record["comparison"]["expected"] != json!("exposed") {
            return Err(format!(
                "comparison lost the accepted expectation: {record}"
            ));
        }
        Ok(())
    }

    /// A tenacity-style retained row whose hunks start at line 88 cannot be
    /// reconstructed offline; it must replay as typed not_run with a reason,
    /// no workspace, no command, and no fabricated comparison.
    #[test]
    fn replay_types_insufficient_identity_as_not_run() -> Result<(), String> {
        let fixture = TempFixture::new("not-run")?;
        let envelopes = mixed_inventory(&fixture)?;
        let (_, records_dir) = replay(&fixture, &envelopes, None)?;
        let record = read_record(&records_dir, "replay-partial-row")?;
        if record["outcome"]["kind"] != json!("not_run") {
            return Err(format!("expected not_run, found {record}"));
        }
        let reason = record["outcome"]["reason"]
            .as_str()
            .ok_or("not_run must carry a reason")?;
        if !reason.contains("not reconstructable") || !reason.contains("tenacity/stop.py") {
            return Err(format!(
                "not_run reason lost the identity problem: {reason}"
            ));
        }
        if !record["workspace"].is_null() || !record["command"].is_null() {
            return Err("not_run records must not fabricate workspace or command".to_string());
        }
        expect_identity_fields(&record)?;
        if record["comparison"]["basis"] != Value::Null {
            return Err("not_run records must not claim a comparison".to_string());
        }
        Ok(())
    }

    /// Judged rows compare candidate-vs-prior-actual and are stale-noted when
    /// the retained judged_against identity does not name the current binary;
    /// an identity that names the current version is not stale-noted, so the
    /// note keys on the identity, not the row kind.
    #[test]
    fn replay_notes_prior_actual_stale_for_judged_rows() -> Result<(), String> {
        let stale_identity = "ripr at main as of 2026-06-13 (post #1206 scale)";
        let fixture = TempFixture::new("judged-stale")?;
        let envelopes = judged_inventory(&fixture, Some(stale_identity))?;
        let (_, records_dir) = replay(&fixture, &envelopes, None)?;
        let record = read_record(&records_dir, "replay-judged-row")?;
        if record["row_kind"] != json!("judged") {
            return Err("judged row must retain its kind".to_string());
        }
        expect_comparison_consistent(&record, "prior_actual_classification")?;
        let stale = &record["comparison"]["stale"];
        if stale["judged_against"] != json!(stale_identity)
            || stale["current_binary_version"].as_str().is_none()
        {
            return Err(format!("prior-actual stale note missing: {stale}"));
        }

        let fixture = TempFixture::new("judged-current")?;
        let binary = super::BinaryIdentity::resolve(worktree_binary()?.as_str())?;
        let judged_against = format!("ripr {}, retained judge panel", binary.version_token);
        let envelopes = judged_inventory(&fixture, Some(judged_against.as_str()))?;
        let (_, records_dir) = replay(&fixture, &envelopes, None)?;
        let record = read_record(&records_dir, "replay-judged-row")?;
        if !record["comparison"]["stale"].is_null() {
            return Err(format!(
                "current judged_against must not be stale-noted: {record}"
            ));
        }
        Ok(())
    }

    /// --limit bounds the replayable queue; bounded-out rows are disclosed in
    /// the summary and leave no record this run.
    #[test]
    fn replay_honors_limit_and_discloses_bounded_out() -> Result<(), String> {
        let fixture = TempFixture::new("limit")?;
        let envelopes = mixed_inventory(&fixture)?;
        let (summary, records_dir) = replay(&fixture, &envelopes, Some(2))?;
        if summary.replayed != 2 || summary.bounded_out != 1 || summary.not_run != 1 {
            return Err(format!(
                "limit accounting drifted: replayed={} bounded_out={} not_run={}",
                summary.replayed, summary.bounded_out, summary.not_run
            ));
        }
        let record_count = fs::read_dir(&records_dir)
            .map_err(|error| error.to_string())?
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.path().extension().and_then(|value| value.to_str()) == Some("json")
            })
            .count();
        if record_count != 3 {
            return Err(format!(
                "bounded-out rows must not leave records this run: found {record_count}"
            ));
        }
        Ok(())
    }

    /// `--network` is declared but must fail closed until a later slice, and
    /// --limit argument rot must be rejected with the rerun command.
    #[test]
    fn replay_network_flag_fails_closed() -> Result<(), String> {
        let error = run(&["--network".to_string()])
            .err()
            .ok_or("--network must be refused")?;
        if !error.contains("network materialization lands with a later #3555 slice") {
            return Err(format!("network refusal lost its typed message: {error}"));
        }
        if !error.contains("cargo xtask python-judged-panel replay") {
            return Err(format!("network refusal lost the rerun command: {error}"));
        }
        let error = run(&["--limit".to_string()])
            .err()
            .ok_or("--limit without a value must be rejected")?;
        if !error.contains("--limit requires a positive integer") {
            return Err(format!("unexpected --limit error: {error}"));
        }
        let error = run(&["--limit".to_string(), "0".to_string()])
            .err()
            .ok_or("--limit 0 must be rejected")?;
        if !error.contains("--limit must be a positive integer") {
            return Err(format!("unexpected --limit 0 error: {error}"));
        }
        Ok(())
    }

    /// Base reconstruction reverses exactly the added lines; an unproved side
    /// (hunks starting after line 1, or a gap between hunks) yields None.
    #[test]
    fn reconstruction_reverses_added_lines_and_skips_unproved_sides() -> Result<(), String> {
        use super::{Side, reconstruct_side};
        use crate::python_judged_panel::parse_unified_diff;

        let proved = parse_unified_diff(concat!(
            "--- a/pkg/multi.py\n",
            "+++ b/pkg/multi.py\n",
            "@@ -1,2 +1,2 @@\n",
            " ctx one\n",
            "-old_two()\n",
            "+new_two()\n",
            "@@ -3,2 +3,2 @@\n",
            " ctx three\n",
            "-old_four()\n",
            "+new_four()\n",
        ))
        .map_err(|error| error.to_string())?;
        let section = proved.sections.first().ok_or("section must exist")?;
        if reconstruct_side(section, Side::Old).as_deref()
            != Some("ctx one\nold_two()\nctx three\nold_four()\n")
        {
            return Err(format!("base reconstruction drifted: {section:?}"));
        }
        if reconstruct_side(section, Side::New).as_deref()
            != Some("ctx one\nnew_two()\nctx three\nnew_four()\n")
        {
            return Err("head reconstruction drifted".to_string());
        }

        let gapped = parse_unified_diff(concat!(
            "--- a/pkg/multi.py\n",
            "+++ b/pkg/multi.py\n",
            "@@ -1,2 +1,2 @@\n",
            " ctx one\n",
            "-old_two()\n",
            "+new_two()\n",
            "@@ -4,2 +4,2 @@\n",
            " ctx four\n",
            "-old_five()\n",
            "+new_five()\n",
        ))
        .map_err(|error| error.to_string())?;
        let section = gapped.sections.first().ok_or("section must exist")?;
        if reconstruct_side(section, Side::Old).is_some()
            || reconstruct_side(section, Side::New).is_some()
        {
            return Err("a gapped diff must not reconstruct either side".to_string());
        }

        let new_file = parse_unified_diff(concat!(
            "--- /dev/null\n",
            "+++ b/pkg/fresh.py\n",
            "@@ -0,0 +1,2 @@\n",
            "+first()\n",
            "+second()\n",
        ))
        .map_err(|error| error.to_string())?;
        let section = new_file.sections.first().ok_or("section must exist")?;
        if reconstruct_side(section, Side::Old).is_some() {
            return Err("a new file must not claim base content".to_string());
        }
        if reconstruct_side(section, Side::New).as_deref() != Some("first()\nsecond()\n") {
            return Err("new-file head content drifted".to_string());
        }
        Ok(())
    }
}
