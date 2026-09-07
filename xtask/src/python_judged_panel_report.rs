//! Python judged PR panel report and adjudication (RIPR-SPEC-0092, #3555 PR C).
//!
//! `adjudicate` records a current independent judgment for one case under
//! `target/ripr/python-judged-panel/adjudications/<case_id>.json` without
//! touching the accepted panel: reviewer role and identity are required (flag
//! or `RIPR_PANEL_ADJUDICATOR`), at least one own evidence citation is
//! required, `must_not_claim` is echoed from the validated row, and RIPR's
//! replay candidate classification is stored only as a separately named
//! advisory reference — never as the adjudicator's ground truth. A case
//! counts as adjudicated only with judgments from at least two distinct
//! recorded roles and identities (independence is self-claimed, not
//! mechanically verified); one role stays
//! `pending_second_role`, role disagreement is `disputed`, and an agreeing
//! judgment with no direction-admitted error axis decided is `inconclusive`
//! — never a pass.
//!
//! `report` derives deterministic JSON + Markdown from the validated
//! inventory, the replay records, and the adjudication records. Every rate
//! carries its exact numerator, denominator, coverage boundary, denominator
//! case ids, and the as-of identity bound by those cases' own replay
//! records; no denominator means no
//! rate (the rate key is omitted, never a fake zero). The two-error lattice
//! stays separate — no combined quality score exists anywhere. Replay
//! mismatches are advisory divergence data and never enter a rate or a
//! threshold. Threshold evaluation runs only when an explicit
//! `--threshold-policy <file>` is supplied, emits
//! `pass`/`fail`/`not_evaluable` per threshold, echoes the policy's own
//! rationale and authority, and is non-authoritative by construction: the
//! report never selects a threshold from observed results, never promotes
//! support, and writes no tier claim. Both renderings come from one derived
//! Value; volatile record fields (recorded command line, temp workspace
//! paths, stderr detail) and adjudication timestamps are never echoed, so
//! independent replay runs over identical inputs render identical bytes
//! (`report_bytes_are_stable_across_independent_runs`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::branch_inventory::{parse_rfc3339_epoch_seconds, rfc3339_from_epoch_seconds};
use crate::python_judged_panel::{
    INVENTORY_PATHS, KNOWN_CLASSIFICATIONS, KNOWN_LIMITATION_QUALITIES, PythonJudgedPanelItem,
    RowKind, direction_admits_error, load_validated_inventory, parse_json_without_duplicate_keys,
    row_kind,
};
use crate::python_judged_panel_replay::{RECORDS_DIR, sha256_hex, stable_case_slug};

const SPEC: &str = "RIPR-SPEC-0092";
const REPORT_SCHEMA_VERSION: &str = "0.1";
const REPORT_KIND: &str = "python_judged_panel_report";
const ADJUDICATION_SCHEMA_VERSION: &str = "0.1";
const ADJUDICATION_KIND: &str = "python_judged_panel_adjudication_record";
const POLICY_KIND: &str = "python_judged_panel_threshold_policy";
const AUTHORITY_BOUNDARY: &str = "review_advisory_only";
const REPORT_OUT_DIR: &str = "target/ripr/python-judged-panel";
const ADJUDICATIONS_DIR: &str = "target/ripr/python-judged-panel/adjudications";
/// Reviewer identity env fallback (identity is required, flag or env).
const REVIEWER_ENV: &str = "RIPR_PANEL_ADJUDICATOR";
const REPORT_RERUN: &str = "cargo xtask python-judged-panel report";
const ADJUDICATE_RERUN: &str = "cargo xtask python-judged-panel adjudicate";
const RECORD_KIND: &str = "python_judged_panel_replay_record";
const RECORD_SCHEMA_VERSION: &str = "0.1";
const OUTCOME_VOCABULARY: [&str; 6] = [
    "complete",
    "partial",
    "failed",
    "parse_failed",
    "timed_out",
    "not_run",
];
const MISMATCH_VOCABULARY: [&str; 4] = [
    "classification_mismatch",
    "expected_but_quiet",
    "prior_actual_mismatch",
    "prior_actual_quiet",
];

const NOTE_REPLAY_ADVISORY: &str = "Replay comparisons are advisory divergence data (RIPR-SPEC-0092 PR B); mismatch and comparison-unavailable counts are disclosed for context and never enter a rate, a denominator, or a threshold evaluation.";
const NOTE_NO_INHERITED_DENOMINATOR: &str = "The historical combined denominator n=7 is not inherited; every rate here states its actual achieved denominator.";
const NOTE_NO_COMBINED_SCORE: &str = "No single quality score is computed: false_actionable and false_exposed keep separate numerators, denominators, and rates.";
const NOTE_RELATION_BASIS: &str = "Coverage by relation basis is disclosed unavailable: the retained panel schema carries no typed relation-basis field, and none is invented here.";
const NOTE_STALE_DEFINITION: &str = "`stale` counts replay records whose evidence identity no longer binds the current inputs: a prior-actual stale note, a diff digest that no longer matches the retained fixture, or a row kind that changed since the record was written. `not_run` includes selected rows that have no replay record in the read directory.";
const THRESHOLD_AUTHORITY_NOTE: &str = "Non-authoritative by construction: the threshold candidate and its rationale come entirely from the supplied policy file; this report never selects a threshold from observed results, never promotes support, and writes no operator tier ruling.";
const FALSE_ACTIONABLE_BOUNDARY: &str = "adjudicated rows whose expected_direction admits false_actionable (should_stay_quiet, should_limit) with a decided label";
const FALSE_EXPOSED_BOUNDARY: &str = "adjudicated rows whose expected_direction admits false_exposed (should_gap, should_limit) with a decided label";

// ---------------------------------------------------------------------------
// CLI entry points
// ---------------------------------------------------------------------------

pub(crate) fn run_report(args: &[String]) -> Result<(), String> {
    let mut records = RECORDS_DIR.to_string();
    let mut adjudications = ADJUDICATIONS_DIR.to_string();
    let mut out_dir = REPORT_OUT_DIR.to_string();
    let mut policy: Option<String> = None;
    let mut check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--records" => records = take_value(args, &mut index, "--records <dir>", REPORT_RERUN)?,
            "--adjudications" => {
                adjudications = take_value(args, &mut index, "--adjudications <dir>", REPORT_RERUN)?
            }
            "--threshold-policy" => {
                policy = Some(take_value(
                    args,
                    &mut index,
                    "--threshold-policy <path>",
                    REPORT_RERUN,
                )?)
            }
            "--out" => out_dir = take_value(args, &mut index, "--out <dir>", REPORT_RERUN)?,
            "--check" => check = true,
            other => {
                return Err(format!(
                    "unknown python-judged-panel report argument `{other}`; expected `report [--records <dir>] [--adjudications <dir>] [--threshold-policy <path>] [--out <dir>] [--check]`\nrerun: {REPORT_RERUN}"
                ));
            }
        }
        index += 1;
    }
    let report = build_report_at(
        Path::new("."),
        &INVENTORY_PATHS,
        Path::new(&records),
        &records,
        Path::new(&adjudications),
        &adjudications,
        policy.as_deref(),
    )?;
    let json_path = Path::new(&out_dir).join("report.json");
    let markdown_path = Path::new(&out_dir).join("report.md");
    if check {
        verify_stored_bytes(&json_path, &report.json)?;
        verify_stored_bytes(&markdown_path, &report.markdown)?;
        println!("verified against fresh render: {}", json_path.display());
        println!("verified against fresh render: {}", markdown_path.display());
    } else {
        write_report_generation(&json_path, &markdown_path, &report.json, &report.markdown)?;
        println!("wrote: {}", json_path.display());
        println!("wrote: {}", markdown_path.display());
    }
    print_report_summary(&report);
    println!("rerun: {REPORT_RERUN}");
    Ok(())
}

pub(crate) fn run_adjudicate(args: &[String]) -> Result<(), String> {
    let mut request = AdjudicationRequest {
        case_id: String::new(),
        verdict: String::new(),
        role: String::new(),
        identity: String::new(),
        evidence: Vec::new(),
        false_actionable: None,
        false_exposed: None,
        wrong_target: None,
        invalid_command: None,
        limitation_quality: None,
        notes: None,
        recorded_at: utc_now_rfc3339()?,
    };
    let mut adjudications = ADJUDICATIONS_DIR.to_string();
    let mut records = RECORDS_DIR.to_string();
    let mut identity_from_env = false;
    let mut index = 0;
    while index < args.len() {
        // The four assessment axes share one parse shape: a tri-state bool
        // whose undecided token is `undecided` for the error lattice (null is
        // an explicit undecided there) and `not-assessed` for the quality
        // axes.
        let flag = args[index].as_str();
        match flag {
            "--case" => {
                request.case_id = take_value(args, &mut index, "--case <id>", ADJUDICATE_RERUN)?
            }
            "--verdict" => {
                request.verdict = take_value(
                    args,
                    &mut index,
                    "--verdict <classification>",
                    ADJUDICATE_RERUN,
                )?
            }
            "--role" => {
                request.role = take_value(args, &mut index, "--role <role>", ADJUDICATE_RERUN)?
            }
            "--reviewer" => {
                request.identity =
                    take_value(args, &mut index, "--reviewer <identity>", ADJUDICATE_RERUN)?
            }
            "--evidence" => request.evidence.push(take_value(
                args,
                &mut index,
                "--evidence <ref>",
                ADJUDICATE_RERUN,
            )?),
            "--false-actionable" | "--false-exposed" | "--wrong-target" | "--invalid-command" => {
                let undecided = if flag.starts_with("--false") {
                    "undecided"
                } else {
                    "not-assessed"
                };
                let value = parse_bool_flag(
                    &take_value(
                        args,
                        &mut index,
                        &format!("{flag} true|false|{undecided}"),
                        ADJUDICATE_RERUN,
                    )?,
                    flag,
                    undecided,
                )?;
                match flag {
                    "--false-actionable" => request.false_actionable = value,
                    "--false-exposed" => request.false_exposed = value,
                    "--wrong-target" => request.wrong_target = value,
                    _ => request.invalid_command = value,
                }
            }
            "--limitation-quality" => {
                let value = take_value(
                    args,
                    &mut index,
                    "--limitation-quality precise|imprecise|wrong_kind|over_limited|not-assessed",
                    ADJUDICATE_RERUN,
                )?;
                if value != "not-assessed" {
                    request.limitation_quality = Some(value);
                }
            }
            "--notes" => {
                request.notes = Some(take_value(
                    args,
                    &mut index,
                    "--notes <text>",
                    ADJUDICATE_RERUN,
                )?)
            }
            "--adjudications" => {
                adjudications =
                    take_value(args, &mut index, "--adjudications <dir>", ADJUDICATE_RERUN)?
            }
            "--records" => {
                records = take_value(args, &mut index, "--records <dir>", ADJUDICATE_RERUN)?
            }
            other => {
                return Err(format!(
                    "unknown python-judged-panel adjudicate argument `{other}`; expected `adjudicate --case <id> --verdict <classification> --role <role> (--reviewer <identity> | env {REVIEWER_ENV}) --evidence <ref>`\nrerun: {ADJUDICATE_RERUN}"
                ));
            }
        }
        index += 1;
    }
    if request.identity.is_empty()
        && let Ok(from_env) = std::env::var(REVIEWER_ENV)
    {
        request.identity = from_env;
        identity_from_env = true;
    }
    let outcome = adjudicate_case_at(
        Path::new("."),
        &INVENTORY_PATHS,
        &adjudications,
        &records,
        &request,
    )?;
    println!(
        "adjudication recorded: case `{}` role `{}`{} — {}",
        request.case_id,
        request.role,
        if identity_from_env {
            format!(" (identity from {REVIEWER_ENV})")
        } else {
            String::new()
        },
        outcome
    );
    Ok(())
}

fn take_value(
    args: &[String],
    index: &mut usize,
    what: &str,
    rerun: &str,
) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{what} requires a value\nrerun: {rerun}"))?;
    if value.starts_with("--") {
        return Err(format!(
            "{what} requires a value, found flag `{value}`\nrerun: {rerun}"
        ));
    }
    *index += 1;
    Ok(value.trim().to_string())
}

fn parse_bool_flag(value: &str, flag: &str, undecided: &str) -> Result<Option<bool>, String> {
    match value {
        "true" => Ok(Some(true)),
        "false" => Ok(Some(false)),
        token if token == undecided => Ok(None),
        other => Err(format!(
            "{flag} must be `true`, `false`, or `{undecided}`, found `{other}`\nrerun: {ADJUDICATE_RERUN}"
        )),
    }
}

fn utc_now_rfc3339() -> Result<String, String> {
    // FIX fqP (devin round 2): a pre-epoch system clock is a broken
    // environment. Fabricating `1970-01-01T00:00:00Z` would stamp valid-looking
    // provenance over an environment the run cannot trust, so the error is
    // named and the command fails closed instead.
    let epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            format!(
                "system clock is before the Unix epoch ({error}); refusing to stamp adjudication provenance with a fabricated timestamp"
            )
        })?
        .as_secs() as i64;
    Ok(rfc3339_from_epoch_seconds(epoch))
}

// ---------------------------------------------------------------------------
// Adjudication: the typed current-judgment record
// ---------------------------------------------------------------------------

/// One adjudication request: the reviewer's own current judgment for a case.
/// Tests inject `recorded_at`; the CLI stamps UTC now.
struct AdjudicationRequest {
    case_id: String,
    verdict: String,
    role: String,
    identity: String,
    evidence: Vec<String>,
    false_actionable: Option<bool>,
    false_exposed: Option<bool>,
    wrong_target: Option<bool>,
    invalid_command: Option<bool>,
    limitation_quality: Option<String>,
    notes: Option<String>,
    recorded_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdjudicationRecord {
    schema_version: String,
    kind: String,
    spec: String,
    case_id: String,
    source_envelope: String,
    expected_direction: String,
    must_not_claim: Vec<String>,
    /// RIPR's own replay output (`{file, binary_version, binary_sha256,
    /// diff_sha256, candidate_classification}`), cited as advisory reference
    /// only and kept under a dedicated name so the adjudicator's `verdict`
    /// can never be confused with the candidate classification.
    #[serde(default)]
    cited_replay_record: Option<Value>,
    judgments: Vec<AdjudicationJudgment>,
    /// FIX f2XZN: sha256 over the complete validated row (every field,
    /// serialized) plus the referenced diff content digest at adjudication
    /// time. A stored digest that no longer matches the current row marks the
    /// record `stale_row` at report time.
    #[serde(default)]
    row_revision_sha256: String,
    authority_boundary: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdjudicationJudgment {
    reviewer_role: String,
    reviewer_identity: String,
    recorded_at: String,
    /// The adjudicator's own classification in the conservative static
    /// vocabulary, backed by the judgment's own evidence citations — never by
    /// copying the cited candidate classification.
    verdict: String,
    #[serde(default)]
    false_actionable: Option<bool>,
    #[serde(default)]
    false_exposed: Option<bool>,
    #[serde(default)]
    wrong_target: Option<bool>,
    #[serde(default)]
    invalid_command: Option<bool>,
    #[serde(default)]
    limitation_quality: Option<String>,
    evidence_references: Vec<String>,
    #[serde(default)]
    notes: Option<String>,
}

/// Records the judgment and returns a one-line disposition summary.
/// FIX fqlm (devin round 2): adjudicating one case is a read-modify-write
/// cycle on its record file, so two concurrent reviewers could each publish a
/// replacement and silently discard the other's judgment. FIX fqNv (devin
/// round 4): concurrent report commands can likewise interleave their json
/// and markdown renames into a mixed generation. Both are serialized by one
/// exclusive sibling lock file per contended path; a second concurrent run
/// gets a named error instead. The lock releases on drop, covering every
/// error path after acquisition, and a crash-left stale lock is named in the
/// error so it can be removed deliberately.
struct PathLockGuard {
    lock_path: PathBuf,
}

impl Drop for PathLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.lock_path);
    }
}

fn acquire_path_lock(
    lock_path: PathBuf,
    what: &str,
    already_exists: impl FnOnce(&Path) -> String,
) -> Result<PathLockGuard, String> {
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(_file) => Ok(PathLockGuard { lock_path }),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(already_exists(&lock_path))
        }
        Err(error) => Err(format!(
            "write {what}: create lock `{}`: {error}",
            lock_path.display()
        )),
    }
}

fn acquire_record_lock(record_path: &Path) -> Result<PathLockGuard, String> {
    let file_name = record_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("record file name is not UTF-8: `{}`", record_path.display()))?;
    let parent = record_path
        .parent()
        .ok_or_else(|| format!("no parent directory for `{}`", record_path.display()))?;
    let lock_path = parent.join(format!(".{file_name}.lock"));
    acquire_path_lock(lock_path, "adjudication record", |lock_path| {
        format!(
            "adjudication record `{}` is locked (`{}` exists): another adjudication for this case may be in progress; if none is running, remove the stale lock\nrerun: {ADJUDICATE_RERUN}",
            record_path.display(),
            lock_path.display()
        )
    })
}

fn adjudicate_case_at(
    root: &Path,
    displays: &[&str],
    adjudications_dir: &str,
    records_dir: &str,
    request: &AdjudicationRequest,
) -> Result<String, String> {
    let loaded = load_validated_inventory(root, displays)?;
    // Case identity and slug uniqueness, checked against the full inventory
    // so an ambiguous record file name fails before any judgment is stored.
    let mut slugs = BTreeMap::<String, String>::new();
    let mut row: Option<(&str, &PythonJudgedPanelItem)> = None;
    for file in &loaded {
        for item in &file.envelope.items {
            let slug = stable_case_slug(&item.id);
            if let Some(previous) = slugs.insert(slug.clone(), item.id.clone()) {
                return Err(format!(
                    "case id slug collision: `{previous}` and `{}` both normalize to `{slug}`; rename one case id so every adjudication record file name is unique\nrerun: {ADJUDICATE_RERUN}",
                    item.id
                ));
            }
            if item.id == request.case_id {
                row = Some((file.display.as_str(), item));
            }
        }
    }
    let (source_envelope, item) = row.ok_or_else(|| {
        format!(
            "unknown case id `{}` in the validated inventory\nrerun: {ADJUDICATE_RERUN}",
            request.case_id
        )
    })?;
    validate_request(request, item)?;

    let judgment = AdjudicationJudgment {
        reviewer_role: request.role.clone(),
        reviewer_identity: request.identity.clone(),
        recorded_at: request.recorded_at.clone(),
        verdict: request.verdict.clone(),
        false_actionable: request.false_actionable,
        false_exposed: request.false_exposed,
        wrong_target: request.wrong_target,
        invalid_command: request.invalid_command,
        limitation_quality: request.limitation_quality.clone(),
        evidence_references: request.evidence.clone(),
        notes: request.notes.clone(),
    };

    let record_path = PathBuf::from(adjudications_dir)
        .join(format!("{}.json", stable_case_slug(&request.case_id)));
    // The adjudications directory must exist before the lock file can, so
    // this runs ahead of lock acquisition (FIX fqlm).
    if let Some(parent) = record_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "create adjudications directory `{}`: {error}\nrerun: {ADJUDICATE_RERUN}",
                parent.display()
            )
        })?;
    }
    let _record_lock = acquire_record_lock(&record_path)?;
    // FIX f2XZZ: only a missing record initializes a new one — every other
    // read failure returns a named error so an unreadable record is never
    // truncated or overwritten by a re-adjudication.
    let existing_body = match fs::read_to_string(&record_path) {
        Ok(body) => Some(body),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "read existing adjudication record `{}`: {error}; refusing to overwrite an unreadable record\nrerun: {ADJUDICATE_RERUN}",
                record_path.display()
            ));
        }
    };
    let row_revision = row_revision_sha256(root, item)?;
    let mut record = match existing_body {
        Some(body) => {
            let existing = parse_adjudication_record(&body, &record_path.display().to_string())?;
            if existing.case_id != request.case_id {
                return Err(format!(
                    "adjudication record `{}` declares case `{}` but was addressed for case `{}`",
                    record_path.display(),
                    existing.case_id,
                    request.case_id
                ));
            }
            // FIX f2XZN: the record is bound to the full row revision (every
            // row field plus the referenced diff content); a bound digest that
            // no longer matches means the row or its diff changed after
            // adjudication, and appending to it would mix revisions.
            if existing.row_revision_sha256 != row_revision {
                return Err(format!(
                    "adjudication record `{}` was recorded against a different revision of case `{}` (bound row revision `{}` does not match the current `{row_revision}`); the row or its diff changed after adjudication — re-record every role's judgment against the current row\nrerun: {ADJUDICATE_RERUN}",
                    record_path.display(),
                    request.case_id,
                    existing.row_revision_sha256
                ));
            }
            // FIX fqNa (devin round 4): the stored provenance echoes must
            // match the row this request was validated against, so a drifted
            // record never accumulates judgments under contradictory
            // provenance.
            if existing.source_envelope != source_envelope
                || existing.expected_direction != item.expected_direction
                || existing.must_not_claim != must_not_claim_echo(item)
            {
                return Err(format!(
                    "adjudication record `{}` carries provenance that contradicts the current validated row (envelope `{}` vs `{}`, direction `{}` vs `{}`); re-record every role's judgment against the current row\nrerun: {ADJUDICATE_RERUN}",
                    record_path.display(),
                    existing.source_envelope,
                    source_envelope,
                    existing.expected_direction,
                    item.expected_direction
                ));
            }
            existing
        }
        None => AdjudicationRecord {
            schema_version: ADJUDICATION_SCHEMA_VERSION.to_string(),
            kind: ADJUDICATION_KIND.to_string(),
            spec: SPEC.to_string(),
            case_id: request.case_id.clone(),
            source_envelope: source_envelope.to_string(),
            expected_direction: item.expected_direction.clone(),
            must_not_claim: must_not_claim_echo(item),
            cited_replay_record: None,
            judgments: Vec::new(),
            row_revision_sha256: row_revision.clone(),
            authority_boundary: AUTHORITY_BOUNDARY.to_string(),
        },
    };
    // A re-adjudication by the same (role, identity) replaces that role's
    // prior judgment so the record always carries each role's current view.
    record.judgments.retain(|existing| {
        existing.reviewer_role != request.role || existing.reviewer_identity != request.identity
    });
    record.judgments.push(judgment);
    record.cited_replay_record = cite_replay_record(records_dir, &request.case_id);

    let body = serde_json::to_string_pretty(&record)
        .map_err(|error| format!("serialize adjudication record: {error}"))?;
    write_adjudication_record_atomic(&record_path, &body)?;

    let view = derive_adjudication_view(&record, item, &row_revision);
    Ok(match view.state {
        AdjudicationState::Adjudicated => format!(
            "adjudicated by {} independent roles (verdict `{}`)",
            view.roles.len(),
            view.verdict_agreed.as_deref().unwrap_or("disputed")
        ),
        AdjudicationState::Inconclusive => format!(
            "recorded but inconclusive: {} independent role(s) agree no direction-admitted error axis is decided; inconclusive is a real state, not a pass",
            view.roles.len()
        ),
        AdjudicationState::Disputed => format!(
            "disputed: {} independent role(s) disagree on the terminal judgment; a recorded disposition is required",
            view.roles.len()
        ),
        _ => format!(
            "pending_second_role: {} role(s) recorded so far; a second independent role is required before this counts as adjudicated",
            view.roles.len()
        ),
    })
}

/// The semantic contract one judgment must satisfy. Enforced on the CLI
/// request (`validate_request`) and re-checked per stored judgment at report
/// time, so a stored record can never carry what the CLI would reject
/// (FIX f2XZU) and carryover rows (null expected_classification,
/// robustness-only) can never be adjudicated at all (FIX f2TIz).
struct JudgmentSemantics<'a> {
    role: &'a str,
    identity: &'a str,
    verdict: &'a str,
    false_actionable: Option<bool>,
    false_exposed: Option<bool>,
    evidence_references: &'a [String],
    limitation_quality: Option<&'a str>,
    direction: &'a str,
    carryover: bool,
}

fn validate_judgment_semantics(judgment: JudgmentSemantics<'_>) -> Result<(), String> {
    if judgment.carryover {
        return Err("carryover rows (null expected_classification, robustness-only) cannot be adjudicated; select a current case instead".to_string());
    }
    if !KNOWN_CLASSIFICATIONS.contains(&judgment.verdict) {
        return Err(format!(
            "verdict `{}` is not in the conservative static vocabulary (one of {})",
            judgment.verdict,
            KNOWN_CLASSIFICATIONS.join(", ")
        ));
    }
    if judgment.role.trim().is_empty() {
        return Err(
            "role is required and must not be blank; the two-distinct-recorded-roles rule keys on it"
                .to_string(),
        );
    }
    if judgment.identity.trim().is_empty() {
        return Err(format!(
            "reviewer identity is required: pass --reviewer <identity> or set env {REVIEWER_ENV}"
        ));
    }
    if judgment.evidence_references.is_empty()
        || judgment
            .evidence_references
            .iter()
            .any(|reference| reference.trim().is_empty())
    {
        return Err("evidence is required at least once with a non-blank reference; a judgment without the adjudicator's own evidence citations is not an independent judgment (do not copy the cited RIPR candidate classification as ground truth)".to_string());
    }
    if matches!(judgment.false_actionable, Some(true))
        && matches!(judgment.false_exposed, Some(true))
    {
        return Err(
            "false_actionable and false_exposed cannot both be true for one terminal adjudication"
                .to_string(),
        );
    }
    for (label, value) in [
        ("false_actionable", judgment.false_actionable),
        ("false_exposed", judgment.false_exposed),
    ] {
        if matches!(value, Some(true)) && !direction_admits_error(judgment.direction, label) {
            return Err(format!(
                "{label} true is not admitted by direction `{}` per the SPEC-0092 outcome table",
                judgment.direction
            ));
        }
    }
    // FIX f2TIA: verdict-to-error coherence mirrors the retained-panel
    // validator's outcome table: crediting `exposed` on a row whose direction
    // expects a gap or a fail-closed limitation is an over-credit by
    // definition and must be recorded as false_exposed true.
    if judgment.direction != "should_stay_quiet"
        && judgment.verdict == "exposed"
        && judgment.false_exposed != Some(true)
    {
        return Err(format!(
            "verdict `exposed` on a `{}` row is an over-credit and requires false_exposed true",
            judgment.direction
        ));
    }
    if let Some(quality) = judgment.limitation_quality {
        if judgment.direction != "should_limit" {
            return Err(format!(
                "limitation-quality applies only to `should_limit` rows, found `{}`",
                judgment.direction
            ));
        }
        if !KNOWN_LIMITATION_QUALITIES.contains(&quality) {
            return Err(format!(
                "limitation-quality `{quality}` is not one of {}",
                KNOWN_LIMITATION_QUALITIES.join(", ")
            ));
        }
    }
    Ok(())
}

fn validate_request(
    request: &AdjudicationRequest,
    item: &PythonJudgedPanelItem,
) -> Result<(), String> {
    validate_judgment_semantics(JudgmentSemantics {
        role: &request.role,
        identity: &request.identity,
        verdict: &request.verdict,
        false_actionable: request.false_actionable,
        false_exposed: request.false_exposed,
        evidence_references: &request.evidence,
        limitation_quality: request.limitation_quality.as_deref(),
        direction: &item.expected_direction,
        carryover: row_kind(item) == RowKind::Carryover,
    })
}

fn must_not_claim_echo(item: &PythonJudgedPanelItem) -> Vec<String> {
    item.must_not_claim.value().cloned().unwrap_or_default()
}

/// Cites the case's replay record when one exists, so the adjudication binds
/// the advisory candidate data it had available. Absence is honest: the
/// citation stays null and the judgment stands on its own evidence.
fn cite_replay_record(records_dir: &str, case_id: &str) -> Option<Value> {
    let path = Path::new(records_dir).join(format!("{}.json", stable_case_slug(case_id)));
    let record = parse_replay_record_bytes(
        &fs::read_to_string(&path).ok()?,
        &path.display().to_string(),
    )
    .ok()?;
    // FIX fqNz (devin round 4): a file at the expected name is cited only
    // when it declares the requested case; foreign content under the right
    // name is never attributed as this case's advisory evidence.
    if record.case_id != case_id {
        return None;
    }
    Some(json!({
        "file": format!("{}.json", stable_case_slug(case_id)),
        "binary_version": text(record.binary.as_ref()?, "version")?,
        "binary_sha256": text(record.binary.as_ref()?, "sha256")?,
        "diff_sha256": record.diff.as_ref().and_then(|diff| text(diff, "sha256"))?,
        "candidate_classification": record
            .outcome
            .as_ref()
            .and_then(|outcome| text(outcome, "candidate_classification")),
    }))
}

fn parse_adjudication_record(body: &str, display: &str) -> Result<AdjudicationRecord, String> {
    let value = parse_json_without_duplicate_keys(body)
        .map_err(|error| format!("parse adjudication record `{display}`: {error}"))?;
    let record: AdjudicationRecord = serde_json::from_value(value)
        .map_err(|error| format!("parse adjudication record `{display}`: {error}"))?;
    if record.schema_version != ADJUDICATION_SCHEMA_VERSION
        || record.kind != ADJUDICATION_KIND
        || record.spec != SPEC
    {
        return Err(format!(
            "adjudication record `{display}` carries unknown identity (schema `{}`, kind `{}`, spec `{}`); expected {ADJUDICATION_SCHEMA_VERSION}/{ADJUDICATION_KIND}/{SPEC}",
            record.schema_version, record.kind, record.spec
        ));
    }
    if record.authority_boundary != AUTHORITY_BOUNDARY {
        return Err(format!(
            "adjudication record `{display}` carries authority boundary `{}`; expected `{AUTHORITY_BOUNDARY}`",
            record.authority_boundary
        ));
    }
    Ok(record)
}

// ---------------------------------------------------------------------------
// Replay record reader (PR B output, read back typed)
// ---------------------------------------------------------------------------

/// The subset of a replay record the report consumes. The `kind` strings are
/// checked against PR B's closed vocabularies; unknown fields (volatile
/// command line, temp workspace paths, stderr detail) are ignored on purpose
/// so they can never leak into report bytes.
#[derive(Debug, Deserialize)]
struct ReplayRecordInput {
    schema_version: String,
    kind: String,
    spec: String,
    case_id: String,
    #[serde(default)]
    row_kind: String,
    #[serde(default)]
    binary: Option<Value>,
    #[serde(default)]
    diff: Option<Value>,
    #[serde(default)]
    outcome: Option<Value>,
    #[serde(default)]
    comparison: Option<Value>,
}

/// One replay record projected onto the fields the report is allowed to echo.
struct RecordView {
    file_name: String,
    outcome_kind: String,
    candidate: Option<String>,
    mismatch_kinds: Vec<String>,
    comparison_unavailable: bool,
    prior_actual_stale: bool,
    binary_version: String,
    binary_sha256: String,
    diff_sha256: String,
    row_kind: String,
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn parse_replay_record_bytes(body: &str, display: &str) -> Result<ReplayRecordInput, String> {
    let value = parse_json_without_duplicate_keys(body)
        .map_err(|error| format!("parse replay record `{display}`: {error}"))?;
    let record: ReplayRecordInput = serde_json::from_value(value)
        .map_err(|error| format!("parse replay record `{display}`: {error}"))?;
    if record.schema_version != RECORD_SCHEMA_VERSION
        || record.kind != RECORD_KIND
        || record.spec != SPEC
    {
        return Err(format!(
            "replay record `{display}` carries unknown identity (schema `{}`, kind `{}`, spec `{}`); re-run `cargo xtask python-judged-panel replay`",
            record.schema_version, record.kind, record.spec
        ));
    }
    Ok(record)
}

/// The replay record set plus the one shared as-of binary identity.
type ReplayRecordSet = (BTreeMap<String, RecordView>, Option<(String, String)>);

/// Reads every `*.json` replay record under `records_dir` (a missing
/// directory means no replay data). All records must share one binary
/// identity: the report discloses a single as-of, and a mixed set cannot be
/// attributed to it.
fn read_replay_records(records_dir: &Path) -> Result<ReplayRecordSet, String> {
    let mut views = BTreeMap::new();
    let mut binary_identity: Option<(String, String)> = None;
    if !records_dir.is_dir() {
        return Ok((views, binary_identity));
    }
    let mut entries = list_json_files(records_dir)?;
    entries.sort();
    for file_name in entries {
        let path = records_dir.join(&file_name);
        let body = fs::read_to_string(&path)
            .map_err(|error| format!("read replay record `{}`: {error}", path.display()))?;
        let record = parse_replay_record_bytes(&body, &path.display().to_string())?;
        if record.case_id.trim().is_empty() {
            return Err(format!(
                "replay record `{}` declares a blank case id",
                path.display()
            ));
        }
        // FIX fqoP (devin round 2): a record file is addressable only by its
        // case's stable slug — the same contract the adjudication reader
        // enforces — and a second record for one case id is a contradiction,
        // not a silent overwrite: filename order must never select which
        // evidence the report counts.
        let expected_file = format!("{}.json", stable_case_slug(&record.case_id));
        if file_name != expected_file {
            return Err(format!(
                "replay record `{}` declares case `{}` but is not named `{expected_file}`; rename it or re-run `cargo xtask python-judged-panel replay`",
                path.display(),
                record.case_id
            ));
        }
        if views.contains_key(&record.case_id) {
            return Err(format!(
                "replay record `{}` re-declares case `{}`, which already has a record in this set; keep one record per case and re-run `cargo xtask python-judged-panel replay`",
                path.display(),
                record.case_id
            ));
        }
        let version = record
            .binary
            .as_ref()
            .and_then(|binary| text(binary, "version"))
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "replay record `{}` carries no binary identity; re-run `cargo xtask python-judged-panel replay`",
                    path.display()
                )
            })?;
        let sha256 = record
            .binary
            .as_ref()
            .and_then(|binary| text(binary, "sha256"))
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "replay record `{}` carries no binary sha256; re-run `cargo xtask python-judged-panel replay`",
                    path.display()
                )
            })?;
        let identity = (version.clone(), sha256.clone());
        match &binary_identity {
            Some(existing) if *existing != identity => {
                return Err(format!(
                    "replay records under `{}` carry mixed binary identity (`{}` vs `{}`); re-run `cargo xtask python-judged-panel replay` so the record set has one as-of identity",
                    records_dir.display(),
                    existing.0,
                    identity.0
                ));
            }
            Some(_) => {}
            None => binary_identity = Some(identity.clone()),
        }
        let outcome = record.outcome.as_ref().ok_or_else(|| {
            format!(
                "replay record `{}` carries no typed outcome; re-run `cargo xtask python-judged-panel replay`",
                path.display()
            )
        })?;
        let outcome_kind = text(outcome, "kind")
            .filter(|kind| OUTCOME_VOCABULARY.contains(kind))
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "replay record `{}` carries unknown outcome kind; re-run `cargo xtask python-judged-panel replay`",
                    path.display()
                )
            })?;
        let candidate = text(outcome, "candidate_classification").map(str::to_string);
        let comparison = record.comparison.as_ref().ok_or_else(|| {
            format!(
                "replay record `{}` carries no typed comparison; re-run `cargo xtask python-judged-panel replay`",
                path.display()
            )
        })?;
        let comparison_kind = text(comparison, "kind").unwrap_or_default();
        if comparison_kind != "available" && comparison_kind != "comparison_unavailable" {
            return Err(format!(
                "replay record `{}` carries unknown comparison kind; re-run `cargo xtask python-judged-panel replay`",
                path.display()
            ));
        }
        let mut mismatch_kinds = Vec::new();
        for mismatch in comparison
            .get("mismatches")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let kind = text(mismatch, "kind").unwrap_or_default();
            if !MISMATCH_VOCABULARY.contains(&kind) {
                return Err(format!(
                    "replay record `{}` carries unknown mismatch kind; re-run `cargo xtask python-judged-panel replay`",
                    path.display()
                ));
            }
            mismatch_kinds.push(kind.to_string());
        }
        let view = RecordView {
            file_name,
            outcome_kind,
            candidate,
            mismatch_kinds,
            comparison_unavailable: comparison_kind == "comparison_unavailable",
            prior_actual_stale: comparison
                .get("stale")
                .is_some_and(|stale| !stale.is_null()),
            binary_version: identity.0.clone(),
            binary_sha256: identity.1.clone(),
            diff_sha256: record
                .diff
                .as_ref()
                .and_then(|diff| text(diff, "sha256"))
                .unwrap_or_default()
                .to_string(),
            row_kind: record.row_kind,
        };
        views.insert(record.case_id, view);
    }
    Ok((views, binary_identity))
}

fn list_json_files(dir: &Path) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for entry in
        fs::read_dir(dir).map_err(|error| format!("read directory `{}`: {error}", dir.display()))?
    {
        let path = entry
            .map_err(|error| format!("read directory entry in `{}`: {error}", dir.display()))?
            .path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("json")
        {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("record file name is not UTF-8: `{}`", path.display()))?;
            names.push(name.to_string());
        }
    }
    Ok(names)
}

// ---------------------------------------------------------------------------
// Adjudication views: the two-independent-roles derivation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum AdjudicationState {
    /// Fewer than two independent roles have recorded a judgment.
    PendingSecondRole,
    /// Two or more independent roles disagree on the terminal judgment.
    Disputed,
    /// Independent roles agree, but no direction-admitted error axis is
    /// decided — a real recorded state, never a pass.
    Inconclusive,
    /// Two or more independent roles agree with at least one admitted error
    /// axis decided.
    Adjudicated,
    /// The record's echoed row identity no longer matches the validated row;
    /// excluded from every adjudicated count and disclosed per case.
    StaleRow,
}

impl AdjudicationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::PendingSecondRole => "pending_second_role",
            Self::Disputed => "disputed",
            Self::Inconclusive => "inconclusive",
            Self::Adjudicated => "adjudicated",
            Self::StaleRow => "stale_row",
        }
    }
}

/// One judgment axis reduced across the record's roles.
enum AxisValue<T> {
    /// Fewer than two independent roles — no axis decision exists yet.
    NotIndependent,
    /// The independent roles disagree on this axis.
    Disputed,
    /// The independent roles agree; `None` is an explicit undecided.
    Agreed(Option<T>),
}

impl<T: PartialEq> AxisValue<T> {
    fn from_judgments(
        judgments: &[AdjudicationJudgment],
        read: impl Fn(&AdjudicationJudgment) -> Option<T>,
    ) -> Self {
        if judgments.len() < 2 {
            return Self::NotIndependent;
        }
        let mut agreed: Option<Option<T>> = None;
        for judgment in judgments {
            let value = read(judgment);
            match &agreed {
                None => agreed = Some(value),
                Some(previous) if *previous != value => return Self::Disputed,
                Some(_) => {}
            }
        }
        match agreed {
            Some(agreed) => Self::Agreed(agreed),
            None => Self::NotIndependent,
        }
    }

    fn agreed_decided(&self) -> Option<T>
    where
        T: Clone,
    {
        match self {
            Self::Agreed(Some(value)) => Some(value.clone()),
            _ => None,
        }
    }
}

fn bool_token(axis: &AxisValue<bool>) -> &'static str {
    match axis {
        AxisValue::Agreed(Some(true)) => "true",
        AxisValue::Agreed(Some(false)) => "false",
        AxisValue::Agreed(None) => "undecided",
        AxisValue::Disputed => "disputed",
        AxisValue::NotIndependent => "not_independent",
    }
}

struct AdjudicationView {
    state: AdjudicationState,
    roles: Vec<String>,
    verdict_agreed: Option<String>,
    false_actionable: AxisValue<bool>,
    false_exposed: AxisValue<bool>,
    wrong_target: AxisValue<bool>,
    invalid_command: AxisValue<bool>,
    limitation_quality: AxisValue<String>,
    /// FIX f2XZN: the row revision the record was bound to at adjudication
    /// time, disclosed against the current revision on drift.
    row_revision_stored: String,
    row_revision_current: String,
}

/// Projects one adjudication record onto the validated row: the
/// two-distinct-recorded-roles rule, the terminal lattice agreement, and the
/// stale-row drift check against the current row revision. Independence is
/// approximated by distinct recorded roles/identities — it is self-claimed,
/// not mechanically verified. The terminal SPEC-0092 lattice is disputed
/// exactly when the verdict or either error axis disagrees across roles.
fn derive_adjudication_view(
    record: &AdjudicationRecord,
    item: &PythonJudgedPanelItem,
    row_revision: &str,
) -> AdjudicationView {
    // FIX f2XZN: the bound row-revision digest subsumes the earlier
    // must_not_claim/envelope/direction echo checks — any row or diff change
    // makes the stored digest stale.
    let stale_row = record.row_revision_sha256 != row_revision;
    let judgments = &record.judgments;
    let roles = judgments
        .iter()
        .map(|judgment| judgment.reviewer_role.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let identities = judgments
        .iter()
        .map(|judgment| judgment.reviewer_identity.clone())
        .collect::<BTreeSet<_>>();
    let independent = roles.len() >= 2 && identities.len() >= 2;
    let verdict = AxisValue::from_judgments(judgments, |judgment| Some(judgment.verdict.clone()));
    let false_actionable =
        AxisValue::from_judgments(judgments, |judgment| judgment.false_actionable);
    let false_exposed = AxisValue::from_judgments(judgments, |judgment| judgment.false_exposed);
    let disputed = matches!(
        (&verdict, &false_actionable, &false_exposed),
        (_, AxisValue::Disputed, _) | (AxisValue::Disputed, ..) | (_, _, AxisValue::Disputed)
    );
    let lattice_agrees = independent && !disputed;

    let state = if stale_row {
        AdjudicationState::StaleRow
    } else if !independent {
        AdjudicationState::PendingSecondRole
    } else if disputed {
        AdjudicationState::Disputed
    } else {
        let decided = usize::from(
            direction_admits_error(&item.expected_direction, "false_actionable")
                && false_actionable.agreed_decided().is_some(),
        ) + usize::from(
            direction_admits_error(&item.expected_direction, "false_exposed")
                && false_exposed.agreed_decided().is_some(),
        );
        if decided == 0 {
            AdjudicationState::Inconclusive
        } else {
            AdjudicationState::Adjudicated
        }
    };

    AdjudicationView {
        state,
        verdict_agreed: if lattice_agrees {
            verdict.agreed_decided()
        } else {
            None
        },
        roles,
        false_actionable,
        false_exposed,
        wrong_target: AxisValue::from_judgments(judgments, |judgment| judgment.wrong_target),
        invalid_command: AxisValue::from_judgments(judgments, |judgment| judgment.invalid_command),
        limitation_quality: AxisValue::from_judgments(judgments, |judgment| {
            judgment.limitation_quality.clone()
        }),
        row_revision_stored: record.row_revision_sha256.clone(),
        row_revision_current: row_revision.to_string(),
    }
}

fn read_adjudication_records(
    adjudications_dir: &Path,
) -> Result<BTreeMap<String, AdjudicationRecord>, String> {
    let mut records = BTreeMap::new();
    if !adjudications_dir.is_dir() {
        return Ok(records);
    }
    let mut entries = list_json_files(adjudications_dir)?;
    entries.sort();
    for file_name in entries {
        let path = adjudications_dir.join(&file_name);
        let body = fs::read_to_string(&path)
            .map_err(|error| format!("read adjudication record `{}`: {error}", path.display()))?;
        let record = parse_adjudication_record(&body, &path.display().to_string())?;
        if record.case_id.trim().is_empty() {
            return Err(format!(
                "adjudication record `{}` declares a blank case id",
                path.display()
            ));
        }
        let expected_file = format!("{}.json", stable_case_slug(&record.case_id));
        if expected_file != file_name {
            return Err(format!(
                "adjudication record `{file_name}` declares case `{}` but its record file name must be `{expected_file}`",
                record.case_id
            ));
        }
        records.insert(record.case_id.clone(), record);
    }
    Ok(records)
}

// ---------------------------------------------------------------------------
// Derivation: one typed pass over inventory + records + adjudications
// ---------------------------------------------------------------------------

/// The complete rendered report: `json` and `markdown` render from the same
/// derived Value, so the two surfaces can never disagree.
struct RenderedReport {
    json: String,
    markdown: String,
}

/// One error axis's rate with its full provenance. `rate` is omitted (never a
/// fake zero) when the denominator is zero.
#[derive(Default)]
struct ErrorRate {
    numerator: usize,
    denominator: usize,
    undecided: usize,
    coverage_boundary: String,
    denominator_case_ids: Vec<String>,
    binary_version: Option<String>,
    binary_sha256: Option<String>,
    /// FIX f2TMb: where the as-of identity came from — the denominator
    /// cases' own replay records, a disclosure that no common identity
    /// exists across them, or no denominator at all.
    as_of_basis: String,
    rate: Option<f64>,
}

/// One quality axis's flagged/assessed accounting over adjudicated rows.
#[derive(Serialize, Default)]
struct AxisCounts {
    flagged: usize,
    assessed: usize,
    unassessed: usize,
    disputed_axis: usize,
}

/// `records_display` / `adjudications_display` are the path strings echoed in
/// the report (the CLI passes the same string it resolves); tests pass a
/// stable display while pointing at different physical directories so the
/// determinism proof can compare two independent replay runs.
fn build_report_at(
    root: &Path,
    displays: &[&str],
    records_dir: &Path,
    records_display: &str,
    adjudications_dir: &Path,
    adjudications_display: &str,
    policy_path: Option<&str>,
) -> Result<RenderedReport, String> {
    let loaded = load_validated_inventory(root, displays)?;

    let mut inventory_identity = Vec::new();
    for display in displays {
        let bytes = fs::read(root.join(display))
            .map_err(|error| format!("read panel envelope `{display}` for identity: {error}"))?;
        inventory_identity.push(json!({"path": display, "sha256": sha256_hex(&bytes)}));
    }
    inventory_identity.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    let panel_digest = {
        let mut digest_source = String::new();
        for identity in &inventory_identity {
            digest_source.push_str(identity["path"].as_str().unwrap_or_default());
            digest_source.push('\u{0}');
            digest_source.push_str(identity["sha256"].as_str().unwrap_or_default());
            digest_source.push('\n');
        }
        sha256_hex(digest_source.as_bytes())
    };

    let (records, binary_identity) = read_replay_records(records_dir)?;
    let adjudications = read_adjudication_records(adjudications_dir)?;

    // Per-case views, in stable case-id order.
    let mut rows: Vec<(&str, &PythonJudgedPanelItem)> = Vec::new();
    for file in &loaded {
        for item in &file.envelope.items {
            rows.push((file.display.as_str(), item));
        }
    }
    rows.sort_by(|left, right| left.1.id.cmp(&right.1.id));

    // A record naming a case outside the validated inventory is stale data,
    // not report input: fail closed so the operator re-runs replay.
    for (case_id, view) in &records {
        if !rows.iter().any(|(_, item)| &item.id == case_id) {
            return Err(format!(
                "replay record `{}` names case `{case_id}` which is not in the validated inventory; re-run `cargo xtask python-judged-panel replay` to refresh the record set",
                view.file_name
            ));
        }
    }
    for case_id in adjudications.keys() {
        if !rows.iter().any(|(_, item)| item.id == *case_id) {
            return Err(format!(
                "adjudication record names case `{case_id}` which is not in the validated inventory; re-record or remove it"
            ));
        }
    }

    let mut counts = BTreeMap::from([
        ("selected", rows.len()),
        ("replayed", 0),
        ("not_run", 0),
        ("adjudicated", 0),
        ("pending_second_role", 0),
        ("disputed", 0),
        ("inconclusive", 0),
        ("stale_row", 0),
        ("stale", 0),
        ("mismatched", 0),
        ("comparison_unavailable", 0),
        ("no_replay_record", 0),
    ]);
    let cover =
        |map: &mut BTreeMap<String, [usize; 3]>, key: &str, replayed: bool, adjudicated: bool| {
            let cell = map.entry(key.to_string()).or_insert([0, 0, 0]);
            cell[0] += 1;
            cell[1] += usize::from(replayed);
            cell[2] += usize::from(adjudicated);
        };
    let (mut by_direction, mut by_repository, mut by_family) =
        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let (mut by_oracle, mut by_limit) = (BTreeMap::new(), BTreeMap::new());
    let mut cases = Vec::new();
    let mut false_actionable = ErrorRate {
        coverage_boundary: FALSE_ACTIONABLE_BOUNDARY.to_string(),
        ..ErrorRate::default()
    };
    let mut false_exposed = ErrorRate {
        coverage_boundary: FALSE_EXPOSED_BOUNDARY.to_string(),
        ..ErrorRate::default()
    };
    let mut wrong_target = AxisCounts::default();
    let mut invalid_command = AxisCounts::default();
    let mut limitation_correctness: BTreeMap<String, usize> = BTreeMap::new();
    // FIX fqNy (devin round 4): per case, the rate as-of identity this run
    // may cite — `None` when the case has no replay record or its record is
    // not current against the present diff/row kind, so a stale replay can
    // never label current adjudications with a binary that never replayed
    // this revision.
    let mut rate_identity: BTreeMap<String, Option<(String, String)>> = BTreeMap::new();
    for key in KNOWN_LIMITATION_QUALITIES
        .iter()
        .map(|quality| quality.to_string())
        .chain([
            "undecided".to_string(),
            "disputed_axis".to_string(),
            "not_adjudicated".to_string(),
        ])
    {
        limitation_correctness.insert(key, 0);
    }

    for (source_envelope, item) in &rows {
        let case_id = &item.id;
        let row_revision = row_revision_sha256(root, item)?;
        let replay_view = records.get(case_id);
        // FIX f2TIz: carryover rows (null expected_classification,
        // robustness-only) are excluded defensively — no stored adjudication
        // can enter their counts, denominators, quality tallies, or
        // thresholds, and `adjudicate` refuses to create one.
        let is_carryover = row_kind(item) == RowKind::Carryover;
        let adjudication_record = if is_carryover {
            None
        } else {
            adjudications.get(case_id)
        };
        let adjudication_view = adjudication_record.map(|record| {
            // FIX f2XZU: every stored judgment must satisfy the same
            // semantic rules the CLI enforces; a violation fails the report
            // named per case and judgment, it is never silently excluded.
            let file_name = format!("{}.json", stable_case_slug(case_id));
            // FIX fqNa (devin round 4): the stored provenance echoes must
            // match the current validated row — a hand-edited or drifted
            // echo is a contradiction, never preservable report input.
            if record.source_envelope != *source_envelope
                || record.expected_direction != item.expected_direction
                || record.must_not_claim != must_not_claim_echo(item)
            {
                return Err(format!(
                    "adjudication record `{file_name}` case `{case_id}`: stored provenance contradicts the current validated row (envelope `{}` vs `{}`, direction `{}` vs `{}`); re-record the adjudication against the current row",
                    record.source_envelope, source_envelope,
                    record.expected_direction, item.expected_direction
                ));
            }
            for judgment in &record.judgments {
                // FIX fqNa: stored provenance must carry a real instant.
                if parse_rfc3339_epoch_seconds(&judgment.recorded_at).is_err() {
                    return Err(format!(
                        "adjudication record `{file_name}` case `{case_id}` judgment by role `{}`: recorded_at `{}` is not a parseable RFC 3339 timestamp",
                        judgment.reviewer_role, judgment.recorded_at
                    ));
                }
                if let Err(violation) = validate_judgment_semantics(JudgmentSemantics {
                    role: &judgment.reviewer_role,
                    identity: &judgment.reviewer_identity,
                    verdict: &judgment.verdict,
                    false_actionable: judgment.false_actionable,
                    false_exposed: judgment.false_exposed,
                    evidence_references: &judgment.evidence_references,
                    limitation_quality: judgment.limitation_quality.as_deref(),
                    direction: &item.expected_direction,
                    carryover: false,
                }) {
                    return Err(format!(
                        "adjudication record `{file_name}` case `{case_id}` judgment by role `{}`: {violation}",
                        judgment.reviewer_role
                    ));
                }
            }
            let mut seen_pairs = BTreeSet::new();
            for judgment in &record.judgments {
                if !seen_pairs.insert((
                    judgment.reviewer_role.clone(),
                    judgment.reviewer_identity.clone(),
                )) {
                    return Err(format!(
                        "adjudication record `{file_name}` case `{case_id}`: duplicate judgment for role `{}` identity `{}`; each recorded role/identity pair may appear once",
                        judgment.reviewer_role, judgment.reviewer_identity
                    ));
                }
            }
            Ok(derive_adjudication_view(record, item, &row_revision))
        });
        let adjudication_view = adjudication_view.transpose()?;

        // Replay side. Mismatch and comparison-unavailable counts follow the
        // replay summary's semantics: attempted runs only — a not_run record
        // has no comparison by definition.
        let diff_matches = replay_view
            .map(|view| view.diff_sha256 == sha256_file_or_blank(&root.join(&item.diff_path)))
            .unwrap_or(false);
        let identity_current = replay_view
            .map(|view| {
                diff_matches
                    && !view.prior_actual_stale
                    && view.row_kind == row_kind_name(row_kind(item))
            })
            .unwrap_or(false);
        let replayed = replay_view.is_some_and(|view| view.outcome_kind != "not_run");
        if replayed {
            *counts.entry("replayed").or_insert(0) += 1;
            if replay_view.is_some_and(|view| !view.mismatch_kinds.is_empty()) {
                *counts.entry("mismatched").or_insert(0) += 1;
            }
            if replay_view.is_some_and(|view| view.comparison_unavailable) {
                *counts.entry("comparison_unavailable").or_insert(0) += 1;
            }
        } else {
            *counts.entry("not_run").or_insert(0) += 1;
            if replay_view.is_none() {
                *counts.entry("no_replay_record").or_insert(0) += 1;
            }
        }
        if replay_view.is_some() && !identity_current {
            *counts.entry("stale").or_insert(0) += 1;
        }
        rate_identity.insert(
            case_id.clone(),
            replay_view
                .filter(|_| identity_current)
                .map(|view| (view.binary_version.clone(), view.binary_sha256.clone())),
        );

        // Adjudication side.
        let is_adjudicated = adjudication_view
            .as_ref()
            .is_some_and(|view| view.state == AdjudicationState::Adjudicated);
        if let Some(view) = &adjudication_view {
            *counts.entry(view.state.as_str()).or_insert(0) += 1;
        }

        // Coverage cells. A row counts under each of its behavior-family
        // shapes, so the family table may sum above `selected`; the table
        // title discloses that.
        cover(
            &mut by_direction,
            &item.expected_direction,
            replayed,
            is_adjudicated,
        );
        cover(&mut by_repository, &item.repo, replayed, is_adjudicated);
        for shape in &item.shape {
            cover(&mut by_family, shape, replayed, is_adjudicated);
        }
        cover(
            &mut by_oracle,
            item.actual_oracle_alignment
                .non_blank_value()
                .unwrap_or("unrecorded"),
            replayed,
            is_adjudicated,
        );
        cover(
            &mut by_limit,
            item.expected_static_limit_kind
                .non_blank_value()
                .unwrap_or("none"),
            replayed,
            is_adjudicated,
        );

        // Rates: separate error denominators over adjudicated rows only.
        // Replay mismatch data is never consulted here.
        if let (Some(view), true) = (&adjudication_view, is_adjudicated) {
            for (admitted, axis, rate) in [
                (
                    direction_admits_error(&item.expected_direction, "false_actionable"),
                    &view.false_actionable,
                    &mut false_actionable,
                ),
                (
                    direction_admits_error(&item.expected_direction, "false_exposed"),
                    &view.false_exposed,
                    &mut false_exposed,
                ),
            ] {
                if !admitted {
                    continue;
                }
                match axis.agreed_decided() {
                    Some(decided) => {
                        rate.denominator += 1;
                        rate.denominator_case_ids.push(case_id.clone());
                        if decided {
                            rate.numerator += 1;
                        }
                    }
                    None => rate.undecided += 1,
                }
            }
            for (axis, target) in [
                (&view.wrong_target, &mut wrong_target),
                (&view.invalid_command, &mut invalid_command),
            ] {
                match axis {
                    AxisValue::Agreed(Some(flagged)) => {
                        target.flagged += usize::from(*flagged);
                        target.assessed += 1;
                    }
                    AxisValue::Agreed(None) => target.unassessed += 1,
                    AxisValue::Disputed => target.disputed_axis += 1,
                    AxisValue::NotIndependent => {}
                }
            }
        }
        if item.expected_direction == "should_limit" {
            let key = match (&adjudication_view, is_adjudicated) {
                (Some(view), true) => match &view.limitation_quality {
                    AxisValue::Agreed(Some(quality)) => quality.clone(),
                    AxisValue::Agreed(None) => "undecided".to_string(),
                    AxisValue::Disputed => "disputed_axis".to_string(),
                    AxisValue::NotIndependent => "not_adjudicated".to_string(),
                },
                _ => "not_adjudicated".to_string(),
            };
            *limitation_correctness.entry(key).or_insert(0) += 1;
        }

        cases.push(json!({
            "case_id": case_id,
            "source_envelope": source_envelope,
            "repo": item.repo.clone(),
            "expected_direction": item.expected_direction.clone(),
            "row_kind": row_kind_name(row_kind(item)),
            "behavior_family": item.shape.clone(),
            "must_not_claim": must_not_claim_echo(item),
            "replay": replay_view.map(|view| json!({
                "record": view.file_name.clone(),
                "outcome": view.outcome_kind.clone(),
                "candidate_classification": view.candidate.clone(),
                "diff_sha256": view.diff_sha256.clone(),
                "identity_current": identity_current,
                "mismatched": !view.mismatch_kinds.is_empty(),
                "mismatch_kinds": view.mismatch_kinds.clone(),
                "comparison_unavailable": view.comparison_unavailable,
            })),
            "adjudication": adjudication_view.map(|view| json!({
                "record": format!("{}.json", stable_case_slug(case_id)),
                "state": view.state.as_str(),
                "roles": view.roles.clone(),
                "verdict": view.verdict_agreed.clone(),
                "false_actionable": bool_token(&view.false_actionable),
                "false_exposed": bool_token(&view.false_exposed),
                "wrong_target": bool_token(&view.wrong_target),
                "invalid_command": bool_token(&view.invalid_command),
                "limitation_quality": match &view.limitation_quality {
                    AxisValue::Agreed(Some(quality)) => Value::String(quality.clone()),
                    AxisValue::Agreed(None) => json!("undecided"),
                    AxisValue::Disputed => json!("disputed"),
                    AxisValue::NotIndependent => json!("not_independent"),
                },
                "row_revision": {
                    "stored": view.row_revision_stored,
                    "current": view.row_revision_current,
                },
            })),
        }));
    }

    counts.insert(
        "unjudged",
        rows.len() - counts["adjudicated"] - counts["inconclusive"],
    );
    for rate in [&mut false_actionable, &mut false_exposed] {
        rate.denominator_case_ids.sort();
        if rate.denominator > 0 {
            rate.rate = Some(rate.numerator as f64 / rate.denominator as f64);
        }
        // FIX f2TMb + FIX fqNy: the as-of identity comes only from the
        // denominator cases' own *current* replay records — every denominator
        // case must bind a record with one shared binary identity, else the
        // rate discloses `no_common_binary_identity` instead of citing the
        // directory-wide identity.
        let mut shared: Option<(String, String)> = None;
        let mut common = rate.denominator > 0;
        for id in &rate.denominator_case_ids {
            match rate_identity.get(id).and_then(|bound| bound.clone()) {
                Some(identity) => match &shared {
                    None => shared = Some(identity),
                    Some(previous) if *previous != identity => common = false,
                    Some(_) => {}
                },
                None => common = false,
            }
        }
        match (rate.denominator > 0, common, shared) {
            (true, true, Some((version, sha256))) => {
                rate.binary_version = Some(version);
                rate.binary_sha256 = Some(sha256);
                rate.as_of_basis = "denominator_case_records".to_string();
            }
            (true, _, _) => {
                rate.as_of_basis = "no_common_binary_identity".to_string();
            }
            (false, _, _) => {
                rate.as_of_basis = "no_denominator".to_string();
            }
        }
    }

    let thresholds = policy_path
        .map(|path| {
            evaluate_threshold_policy(
                root,
                path,
                rows.len(),
                counts["adjudicated"],
                &false_actionable,
                &false_exposed,
            )
        })
        .transpose()?;

    let mut report = json!({
        "schema_version": REPORT_SCHEMA_VERSION,
        "kind": REPORT_KIND,
        "spec": SPEC,
        "authority_boundary": AUTHORITY_BOUNDARY,
        "inputs": {
            "inventory": inventory_identity,
            "records_dir": records_display,
            "adjudications_dir": adjudications_display,
            "threshold_policy": policy_path,
        },
        "as_of": {
            "panel_digest": panel_digest,
            "replay_binary": binary_identity
                .as_ref()
                .map(|(version, sha256)| json!({"version": version, "sha256": sha256})),
            "replay_record_schema_version": if records.is_empty() {
                Value::Null
            } else {
                json!(RECORD_SCHEMA_VERSION)
            },
        },
        "counts": counts,
        "coverage": {
            "by_direction": by_direction,
            "by_repository": by_repository,
            "by_behavior_family": by_family,
            "by_oracle_alignment": by_oracle,
            "by_limitation_kind": by_limit,
            "relation_basis": {
                "available": false,
                "reason": "the retained panel schema carries no typed relation-basis field; coverage by relation basis is disclosed unavailable rather than invented",
            },
        },
        "rates": {
            "false_actionable": rate_value(&false_actionable),
            "false_exposed": rate_value(&false_exposed),
            "wrong_target": wrong_target,
            "invalid_command": invalid_command,
            "limitation_correctness": limitation_correctness,
        },
        "thresholds": thresholds.clone().unwrap_or(Value::Null),
        "cases": cases,
        "notes": [
            NOTE_REPLAY_ADVISORY,
            NOTE_NO_INHERITED_DENOMINATOR,
            NOTE_NO_COMBINED_SCORE,
            NOTE_RELATION_BASIS,
            NOTE_STALE_DEFINITION,
        ],
    });
    if thresholds.is_none() {
        report
            .as_object_mut()
            .ok_or("panel report must be a JSON object")?
            .remove("thresholds");
    }
    let json = format!(
        "{}\n",
        serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize panel report: {error}"))?
    );
    let markdown = render_markdown(&report);
    Ok(RenderedReport { json, markdown })
}

fn rate_value(rate: &ErrorRate) -> Value {
    let mut value = json!({
        "numerator": rate.numerator,
        "denominator": rate.denominator,
        "undecided": rate.undecided,
        "coverage_boundary": rate.coverage_boundary,
        "denominator_case_ids": rate.denominator_case_ids,
        "as_of_basis": rate.as_of_basis,
        "as_of": {
            "binary_version": rate.binary_version,
            "binary_sha256": rate.binary_sha256,
        },
    });
    if let Some(measured) = rate.rate {
        value["rate"] = json!(measured);
    }
    value
}

fn row_kind_name(kind: RowKind) -> &'static str {
    match kind {
        RowKind::Seed => "seed",
        RowKind::Judged => "judged",
        RowKind::Carryover => "carryover",
    }
}

fn sha256_file_or_blank(path: &Path) -> String {
    fs::read(path)
        .map(|bytes| sha256_hex(&bytes))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Threshold policy: explicit input, explicit evaluation, non-authoritative
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThresholdPolicyFile {
    schema_version: String,
    kind: String,
    spec: String,
    rationale: String,
    #[serde(default)]
    authority: Option<String>,
    thresholds: Vec<ThresholdSpec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThresholdSpec {
    metric: String,
    operator: String,
    value: f64,
}

const METRIC_FALSE_ACTIONABLE_RATE: &str = "false_actionable_rate";
const METRIC_FALSE_EXPOSED_RATE: &str = "false_exposed_rate";
const METRIC_ADJUDICATED_COUNT: &str = "adjudicated_count";

/// Evaluates the supplied policy as-is. The policy path is resolved against
/// the inventory root (the repository root in production) but echoed exactly
/// as given.
fn evaluate_threshold_policy(
    root: &Path,
    policy_path: &str,
    selected: usize,
    adjudicated: usize,
    false_actionable: &ErrorRate,
    false_exposed: &ErrorRate,
) -> Result<Value, String> {
    let body = fs::read_to_string(root.join(policy_path)).map_err(|error| {
        format!("read threshold policy `{policy_path}`: {error}\nrerun: {REPORT_RERUN}")
    })?;
    let value = parse_json_without_duplicate_keys(&body)
        .map_err(|error| format!("parse threshold policy `{policy_path}`: {error}"))?;
    let policy: ThresholdPolicyFile = serde_json::from_value(value)
        .map_err(|error| format!("parse threshold policy `{policy_path}`: {error}"))?;
    if policy.schema_version != REPORT_SCHEMA_VERSION
        || policy.kind != POLICY_KIND
        || policy.spec != SPEC
    {
        return Err(format!(
            "threshold policy `{policy_path}` carries unknown identity (schema `{}`, kind `{}`, spec `{}`); expected {REPORT_SCHEMA_VERSION}/{POLICY_KIND}/{SPEC}",
            policy.schema_version, policy.kind, policy.spec
        ));
    }
    if policy.rationale.trim().is_empty() {
        return Err(format!(
            "threshold policy `{policy_path}` must carry a non-blank rationale; an unexplained threshold candidate cannot be evaluated"
        ));
    }
    if policy.thresholds.is_empty() {
        return Err(format!(
            "threshold policy `{policy_path}` must declare at least one threshold"
        ));
    }
    let mut seen = BTreeSet::new();
    let mut evaluations = Vec::new();
    for threshold in &policy.thresholds {
        if !seen.insert((threshold.metric.clone(), threshold.operator.clone())) {
            return Err(format!(
                "threshold policy `{policy_path}` declares duplicate metric `{}` with operator `{}`",
                threshold.metric, threshold.operator
            ));
        }
        let evaluation = match threshold.metric.as_str() {
            METRIC_FALSE_ACTIONABLE_RATE | METRIC_FALSE_EXPOSED_RATE => {
                if threshold.operator != "max" {
                    return Err(format!(
                        "threshold policy `{policy_path}`: metric `{}` requires operator `max`, found `{}`",
                        threshold.metric, threshold.operator
                    ));
                }
                if !(0.0..=1.0).contains(&threshold.value) {
                    return Err(format!(
                        "threshold policy `{policy_path}`: metric `{}` requires a value in [0, 1], found {}",
                        threshold.metric, threshold.value
                    ));
                }
                let (rate, axis) = if threshold.metric == METRIC_FALSE_ACTIONABLE_RATE {
                    (false_actionable, "false_actionable")
                } else {
                    (false_exposed, "false_exposed")
                };
                match rate.rate {
                    Some(measured) => json!({
                        "metric": threshold.metric,
                        "operator": "max",
                        "threshold_value": threshold.value,
                        "measured": measured,
                        "result": if measured <= threshold.value { "pass" } else { "fail" },
                        "reason": format!(
                            "{axis} numerator {} / denominator {} over the adjudicated rows named in the rate's denominator_case_ids",
                            rate.numerator, rate.denominator
                        ),
                    }),
                    None => json!({
                        "metric": threshold.metric,
                        "operator": "max",
                        "threshold_value": threshold.value,
                        "measured": Value::Null,
                        "result": "not_evaluable",
                        "reason": format!(
                            "no {axis} denominator: {} adjudicated row(s) with a decided {axis} label; no denominator means no rate",
                            rate.denominator
                        ),
                    }),
                }
            }
            METRIC_ADJUDICATED_COUNT => {
                if threshold.operator != "min" {
                    return Err(format!(
                        "threshold policy `{policy_path}`: metric `{METRIC_ADJUDICATED_COUNT}` requires operator `min`, found `{}`",
                        threshold.operator
                    ));
                }
                if threshold.value < 0.0 {
                    return Err(format!(
                        "threshold policy `{policy_path}`: metric `{METRIC_ADJUDICATED_COUNT}` requires a non-negative value, found {}",
                        threshold.value
                    ));
                }
                json!({
                    "metric": threshold.metric,
                    "operator": "min",
                    "threshold_value": threshold.value,
                    "measured": adjudicated,
                    "result": if adjudicated as f64 >= threshold.value { "pass" } else { "fail" },
                    "reason": format!(
                        "{adjudicated} of {selected} selected rows carry a complete two-role adjudication; disputed, inconclusive, pending, and stale rows never count"
                    ),
                })
            }
            other => {
                return Err(format!(
                    "threshold policy `{policy_path}` declares unknown metric `{other}`; expected {METRIC_FALSE_ACTIONABLE_RATE}, {METRIC_FALSE_EXPOSED_RATE}, or {METRIC_ADJUDICATED_COUNT}"
                ));
            }
        };
        evaluations.push(evaluation);
    }
    Ok(json!({
        "policy": {
            "path": policy_path,
            "rationale": policy.rationale,
            "authority": policy.authority.filter(|authority| !authority.trim().is_empty()),
        },
        "evaluations": evaluations,
        "authority_note": THRESHOLD_AUTHORITY_NOTE,
    }))
}

// ---------------------------------------------------------------------------
// Rendering (JSON from serde; Markdown from the same Value)
// ---------------------------------------------------------------------------

fn verify_stored_bytes(path: &Path, expected: &str) -> Result<(), String> {
    let stored = fs::read_to_string(path).map_err(|error| {
        format!(
            "report file `{}` is missing or unreadable; run `{REPORT_RERUN}` first: {error}",
            path.display()
        )
    })?;
    if stored != expected {
        return Err(format!(
            "report file `{}` drifted from a fresh render; run `{REPORT_RERUN}` to refresh",
            path.display()
        ));
    }
    Ok(())
}

/// FIX f2TMb helper: one staged temp sibling, flushed to disk before the
/// caller renames it into place.
fn stage_temp_sibling(path: &Path, body: &str) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("no parent directory for `{}`", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "create report output directory `{}`: {error}",
            parent.display()
        )
    })?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("report");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temp = parent.join(format!(".{file_name}.tmp-{}-{nanos}", std::process::id()));
    let mut file =
        fs::File::create(&temp).map_err(|error| format!("stage `{}`: {error}", temp.display()))?;
    file.write_all(body.as_bytes())
        .map_err(|error| format!("stage `{}`: {error}", temp.display()))?;
    file.sync_all()
        .map_err(|error| format!("flush `{}`: {error}", temp.display()))?;
    Ok(temp)
}

/// FIX f2TNz: report.json and report.md publish as one generation — both
/// files are staged completely as temp siblings first, then renamed into
/// place. If the second rename fails, the first publication is rolled back
/// from the prior bytes held in memory, so no half-updated pair survives.
fn write_report_generation(
    json_path: &Path,
    markdown_path: &Path,
    json: &str,
    markdown: &str,
) -> Result<(), String> {
    // FIX fqNv (devin round 4): two concurrent report commands could
    // interleave their separate renames and publish a json from one
    // generation with markdown from another; one exclusive lock per output
    // directory serializes the pair.
    let out_parent = json_path
        .parent()
        .ok_or_else(|| format!("no parent directory for `{}`", json_path.display()))?;
    fs::create_dir_all(out_parent).map_err(|error| {
        format!(
            "create report output directory `{}`: {error}",
            out_parent.display()
        )
    })?;
    let _generation_lock = acquire_path_lock(
        out_parent.join(".report-generation.lock"),
        "report generation",
        |lock_path| {
            format!(
                "another report generation is publishing to this output directory (`{}` exists); if none is running, remove the stale lock",
                lock_path.display()
            )
        },
    )?;
    let json_temp = stage_temp_sibling(json_path, json)?;
    let markdown_temp = match stage_temp_sibling(markdown_path, markdown) {
        Ok(temp) => temp,
        Err(error) => {
            let _ = fs::remove_file(&json_temp);
            return Err(error);
        }
    };
    let prior_json = fs::read(json_path).ok();
    if let Err(error) = fs::rename(&json_temp, json_path) {
        let _ = fs::remove_file(&json_temp);
        let _ = fs::remove_file(&markdown_temp);
        return Err(format!("publish `{}`: {error}", json_path.display()));
    }
    if let Err(error) = fs::rename(&markdown_temp, markdown_path) {
        // Roll the first publication back so the pair stays one generation.
        // FIX fTNz (devin round 1): a failed restoration must be named, not
        // claimed — "restored" in the message would otherwise be a false
        // confidence surface when the restore write itself failed.
        let restored = match &prior_json {
            Some(bytes) => fs::write(json_path, bytes).is_ok(),
            None => match fs::remove_file(json_path) {
                Ok(()) => true,
                Err(remove_error) if remove_error.kind() == std::io::ErrorKind::NotFound => true,
                Err(_) => false,
            },
        };
        let _ = fs::remove_file(&markdown_temp);
        let restoration_note = if restored {
            format!(
                "the prior report.json generation was restored; retry or inspect `{}`",
                json_path.display()
            )
        } else {
            "RESTORATION FAILED: report.json may hold the new generation without its markdown pair — delete the mismatched pair and re-run".to_string()
        };
        return Err(format!(
            "publish `{}` failed after `{}` was replaced ({error}); {restoration_note}",
            markdown_path.display(),
            json_path.display()
        ));
    }
    Ok(())
}

/// FIX f2XZZ: the adjudication record replacement is atomic — serialize,
/// stage a unique temp sibling in the same directory, flush, then rename over
/// the destination. `std::fs::rename` replaces an existing destination on
/// Windows (MoveFileEx with MOVEFILE_REPLACE_EXISTING). FIX fqm (devin round
/// 2): there is deliberately no copy fallback — `fs::copy` truncates the
/// destination before writing, so a mid-copy failure would destroy the prior
/// record. A rename failure therefore fails the command and leaves the prior
/// record byte-identical. Any failure before the rename removes the staged
/// temp.
fn write_adjudication_record_atomic(path: &Path, body: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("no parent directory for `{}`", path.display()))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("record file name is not UTF-8: `{}`", path.display()))?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temp = parent.join(format!(".{file_name}.tmp-{}-{nanos}", std::process::id()));
    let staged = (|| -> Result<(), String> {
        let mut file = fs::File::create(&temp)
            .map_err(|error| format!("stage `{}`: {error}", temp.display()))?;
        file.write_all(body.as_bytes())
            .map_err(|error| format!("stage `{}`: {error}", temp.display()))?;
        file.sync_all()
            .map_err(|error| format!("flush `{}`: {error}", temp.display()))?;
        Ok(())
    })();
    if let Err(error) = staged {
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "write adjudication record `{}`: {error}
rerun: {ADJUDICATE_RERUN}",
            path.display()
        ));
    }
    if let Err(rename_error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!(
            "publish adjudication record `{}`: rename failed ({rename_error}); the prior record is unchanged\nrerun: {ADJUDICATE_RERUN}",
            path.display()
        ));
    }
    Ok(())
}

/// FIX f2XZN: the row revision an adjudication binds — sha256 over the
/// complete validated row (every field, serialized deterministically) plus
/// the referenced diff content digest. `Nullable::Missing` and explicit null
/// both serialize as null: they carry the same row content here.
fn row_revision_sha256(root: &Path, item: &PythonJudgedPanelItem) -> Result<String, String> {
    let mut serialized = serde_json::to_vec(item)
        .map_err(|error| format!("serialize case `{}` for revision binding: {error}", item.id))?;
    serialized.push(0);
    serialized.extend_from_slice(sha256_file_or_blank(&root.join(&item.diff_path)).as_bytes());
    Ok(sha256_hex(&serialized))
}

fn print_report_summary(report: &RenderedReport) {
    let value = serde_json::from_str::<Value>(&report.json).unwrap_or(Value::Null);
    let count = |key: &str| value["counts"][key].as_u64().unwrap_or(0);
    println!(
        "Python judged PR panel report: selected={} replayed={} not_run={} adjudicated={} inconclusive={} disputed={} pending_second_role={} stale_row={} stale={} mismatched={} comparison_unavailable={}",
        count("selected"),
        count("replayed"),
        count("not_run"),
        count("adjudicated"),
        count("inconclusive"),
        count("disputed"),
        count("pending_second_role"),
        count("stale_row"),
        count("stale"),
        count("mismatched"),
        count("comparison_unavailable"),
    );
    for key in ["false_actionable", "false_exposed"] {
        let rate = &value["rates"][key];
        let rate_text = rate["rate"]
            .as_f64()
            .map(|measured| format!("{measured:.3}"))
            .unwrap_or_else(|| "none: no denominator".to_string());
        println!(
            "{key}: {}/{} (rate {rate_text}, undecided {})",
            rate["numerator"].as_u64().unwrap_or(0),
            rate["denominator"].as_u64().unwrap_or(0),
            rate["undecided"].as_u64().unwrap_or(0),
        );
    }
}

/// Both renderings walk the same derived Value: the two surfaces can never
/// disagree.
fn render_markdown(report: &Value) -> String {
    let count = |key: &str| report["counts"][key].as_u64().unwrap_or(0);
    let mut out = String::new();
    out.push_str("# Python Judged PR Panel Report (RIPR-SPEC-0092)\n\nschema ");
    out.push_str(report["schema_version"].as_str().unwrap_or("?"));
    out.push_str(" — authority boundary: ");
    out.push_str(report["authority_boundary"].as_str().unwrap_or("?"));
    out.push_str("\n\n## As-of identity\n\n");
    out.push_str(&format!(
        "- panel digest: `{}` ({} envelope file(s))\n",
        report["as_of"]["panel_digest"].as_str().unwrap_or("?"),
        report["inputs"]["inventory"]
            .as_array()
            .map(Vec::len)
            .unwrap_or(0),
    ));
    for identity in report["inputs"]["inventory"]
        .as_array()
        .into_iter()
        .flatten()
    {
        out.push_str(&format!(
            "  - `{}` sha256 `{}`\n",
            identity["path"].as_str().unwrap_or("?"),
            identity["sha256"].as_str().unwrap_or("?"),
        ));
    }
    match report["as_of"]["replay_binary"]["version"].as_str() {
        Some(version) => out.push_str(&format!(
            "- replay records: `{}` — binary `{version}` sha256 `{}` (record schema {})\n",
            report["inputs"]["records_dir"].as_str().unwrap_or("?"),
            report["as_of"]["replay_binary"]["sha256"]
                .as_str()
                .unwrap_or("?"),
            report["as_of"]["replay_record_schema_version"]
                .as_str()
                .unwrap_or("unknown"),
        )),
        None => out.push_str(&format!(
            "- replay records: `{}` — no records read\n",
            report["inputs"]["records_dir"].as_str().unwrap_or("?"),
        )),
    }
    out.push_str(&format!(
        "- adjudications: `{}`\n",
        report["inputs"]["adjudications_dir"]
            .as_str()
            .unwrap_or("?"),
    ));

    out.push_str("\n## Counts\n\n");
    for key in [
        "selected",
        "replayed",
        "not_run",
        "adjudicated",
        "pending_second_role",
        "disputed",
        "inconclusive",
        "stale_row",
        "stale",
        "mismatched",
        "comparison_unavailable",
        "unjudged",
        "no_replay_record",
    ] {
        out.push_str(&format!("- {key}: {}\n", count(key)));
    }

    out.push_str("\n## Coverage\n");
    for (title, key) in [
        ("By direction", "by_direction"),
        ("By repository", "by_repository"),
        (
            "By behavior family (a row counts under each of its shapes)",
            "by_behavior_family",
        ),
        (
            "By oracle alignment (`unrecorded` for rows without an observed alignment)",
            "by_oracle_alignment",
        ),
        (
            "By limitation kind (`none` for rows that name no static limit)",
            "by_limitation_kind",
        ),
    ] {
        out.push_str(&format!(
            "\n### {title}\n\n| value | selected | replayed | adjudicated |\n| --- | --- | --- | --- |\n"
        ));
        for (value, cell) in report["coverage"][key].as_object().into_iter().flatten() {
            out.push_str(&format!(
                "| {value} | {} | {} | {} |\n",
                cell[0].as_u64().unwrap_or(0),
                cell[1].as_u64().unwrap_or(0),
                cell[2].as_u64().unwrap_or(0),
            ));
        }
    }
    out.push_str(&format!(
        "- relation basis: unavailable — {}\n",
        report["coverage"]["relation_basis"]["reason"]
            .as_str()
            .unwrap_or("?"),
    ));

    out.push_str("\n## Separate error rates (two-error lattice; no combined score)\n\n");
    for key in ["false_actionable", "false_exposed"] {
        let rate = &report["rates"][key];
        out.push_str(&format!(
            "- {key}: numerator {} / denominator {}",
            rate["numerator"].as_u64().unwrap_or(0),
            rate["denominator"].as_u64().unwrap_or(0),
        ));
        match rate["rate"].as_f64() {
            Some(measured) => out.push_str(&format!(" — rate {measured:.3}")),
            None => out.push_str(" — rate not disclosed: no denominator"),
        }
        let cases = rate["denominator_case_ids"]
            .as_array()
            .map(|ids| {
                ids.iter()
                    .map(|id| id.as_str().unwrap_or("?"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        out.push_str(&format!(
            " (undecided {})\n  - coverage boundary: {}\n  - denominator cases: {}\n",
            rate["undecided"].as_u64().unwrap_or(0),
            rate["coverage_boundary"].as_str().unwrap_or("?"),
            if cases.is_empty() {
                "none"
            } else {
                cases.as_str()
            },
        ));
        match rate["as_of_basis"].as_str() {
            // FIX f2TMb: the cited identity is the denominator cases' own;
            // without one common identity the rate discloses instead of
            // fabricating the directory-wide identity.
            Some("denominator_case_records") => match rate["as_of"]["binary_version"].as_str() {
                Some(version) => out.push_str(&format!(
                    "  - as-of: binary `{version}` sha256 `{}` (bound by the denominator cases' own replay records)\n",
                    rate["as_of"]["binary_sha256"].as_str().unwrap_or("?"),
                )),
                None => out.push_str("  - as-of: no replay binary identity bound\n"),
            },
            Some("no_common_binary_identity") => out.push_str(
                "  - as-of: not disclosed — no common binary identity across the denominator cases' own replay records\n",
            ),
            _ => out.push_str("  - as-of: no denominator\n"),
        }
    }
    for key in ["wrong_target", "invalid_command"] {
        let axis = &report["rates"][key];
        out.push_str(&format!(
            "- {key}: flagged {} of assessed {} (unassessed {}, disputed_axis {})\n",
            axis["flagged"].as_u64().unwrap_or(0),
            axis["assessed"].as_u64().unwrap_or(0),
            axis["unassessed"].as_u64().unwrap_or(0),
            axis["disputed_axis"].as_u64().unwrap_or(0),
        ));
    }
    let limitation = &report["rates"]["limitation_correctness"];
    let count_at = |key: &str| limitation[key].as_u64().unwrap_or(0);
    out.push_str(&format!(
        "- limitation_correctness (adjudicated `should_limit` rows): precise {}, imprecise {}, wrong_kind {}, over_limited {}, undecided {}, disputed_axis {}, not_adjudicated {}\n",
        count_at("precise"),
        count_at("imprecise"),
        count_at("wrong_kind"),
        count_at("over_limited"),
        count_at("undecided"),
        count_at("disputed_axis"),
        count_at("not_adjudicated"),
    ));

    if let Some(thresholds) = report.get("thresholds") {
        out.push_str("\n## Threshold evaluation (explicit, non-authoritative)\n\n- policy: `");
        out.push_str(thresholds["policy"]["path"].as_str().unwrap_or("?"));
        out.push_str("`\n- rationale (echoed from the policy file): ");
        out.push_str(thresholds["policy"]["rationale"].as_str().unwrap_or("?"));
        out.push('\n');
        if let Some(authority) = thresholds["policy"]["authority"].as_str() {
            out.push_str(&format!("- authority (echoed): {authority}\n"));
        }
        out.push_str(
            "\n| metric | operator | threshold | measured | result | reason |\n| --- | --- | --- | --- | --- | --- |\n",
        );
        for evaluation in thresholds["evaluations"].as_array().into_iter().flatten() {
            let measured = match evaluation["measured"].as_f64() {
                Some(measured) => format!("{measured:.3}"),
                None => "n/a".to_string(),
            };
            let threshold = evaluation["threshold_value"].as_f64().unwrap_or(0.0);
            out.push_str(&format!(
                "| {} | {} | {} | {measured} | {} | {} |\n",
                evaluation["metric"].as_str().unwrap_or("?"),
                evaluation["operator"].as_str().unwrap_or("?"),
                if threshold == 0.0 {
                    "0".to_string()
                } else {
                    format!("{threshold}")
                },
                evaluation["result"].as_str().unwrap_or("?"),
                evaluation["reason"].as_str().unwrap_or("?"),
            ));
        }
        out.push_str(&format!(
            "\n{}\n",
            thresholds["authority_note"].as_str().unwrap_or("?")
        ));
    }

    out.push_str(
        "\n## Cases\n\n| case | direction | row kind | replay outcome | candidate | mismatches | adjudication | roles |\n| --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for case in report["cases"].as_array().into_iter().flatten() {
        let replay = &case["replay"];
        let (outcome, candidate, mismatches) = match replay.as_object() {
            // A candidate classification exists only where a comparison was
            // actually available; not_run and unavailable outcomes must not
            // read as quiet.
            Some(replay) if replay["comparison_unavailable"].as_bool() != Some(true) => (
                replay["outcome"].as_str().unwrap_or("?").to_string(),
                replay["candidate_classification"]
                    .as_str()
                    .unwrap_or("quiet")
                    .to_string(),
                match replay["mismatch_kinds"].as_array() {
                    Some(kinds) if !kinds.is_empty() => kinds
                        .iter()
                        .map(|kind| kind.as_str().unwrap_or("?"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => "none".to_string(),
                },
            ),
            Some(replay) => (
                replay["outcome"].as_str().unwrap_or("?").to_string(),
                "n/a".to_string(),
                "n/a".to_string(),
            ),
            None => (
                "no_record".to_string(),
                "n/a".to_string(),
                "n/a".to_string(),
            ),
        };
        let adjudication = &case["adjudication"];
        let (state, roles) = match adjudication.as_object() {
            Some(adjudication) => (
                adjudication["state"].as_str().unwrap_or("?").to_string(),
                match adjudication["roles"].as_array() {
                    Some(roles) if !roles.is_empty() => roles
                        .iter()
                        .map(|role| role.as_str().unwrap_or("?"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    _ => "n/a".to_string(),
                },
            ),
            None => ("unjudged".to_string(), "n/a".to_string()),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {outcome} | {candidate} | {mismatches} | {state} | {roles} |\n",
            case["case_id"].as_str().unwrap_or("?"),
            case["expected_direction"].as_str().unwrap_or("?"),
            case["row_kind"].as_str().unwrap_or("?"),
        ));
    }

    out.push_str("\n## Notes\n\n");
    for note in report["notes"].as_array().into_iter().flatten() {
        out.push_str(&format!("- {}\n", note.as_str().unwrap_or("?")));
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::{Value, json};

    use super::{
        AdjudicationRequest, RECORD_KIND, RECORD_SCHEMA_VERSION, RenderedReport, SPEC,
        acquire_record_lock, adjudicate_case_at, build_report_at, parse_replay_record_bytes,
    };

    const PANEL_DIR: &str = "fixtures/python-judged-pr-panel";

    struct TempFixture {
        root: PathBuf,
        /// Unique name fragment embedded in every path under this fixture;
        /// used to prove volatile paths never leak into report bytes.
        marker: String,
    }

    impl TempFixture {
        fn new(name: &str) -> Result<Self, String> {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "ripr-py-panel-report-{name}-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(root.join(format!("{PANEL_DIR}/diffs")))
                .map_err(|error| format!("create test fixture: {error}"))?;
            Ok(Self {
                marker: format!("{unique}"),
                root,
            })
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

        fn write_policy(&self, value: &Value) -> Result<String, String> {
            let body = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
            fs::write(self.root.join("policy.json"), body).map_err(|error| error.to_string())?;
            Ok("policy.json".to_string())
        }

        fn path(&self, name: &str) -> Result<String, String> {
            self.root
                .join(name)
                .to_str()
                .map(str::to_string)
                .ok_or_else(|| format!("non-UTF-8 temp path: `{}`", self.root.display()))
        }
    }

    impl Drop for TempFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    // Fully diff-proved synthetic diffs (hunks start at line 1), mirroring
    // the PR B replay test bodies.
    const GAP_BODY: &str = "--- a/pricing.py\n+++ b/pricing.py\n@@ -1,4 +1,4 @@\n def apply_discount(amount, threshold):\n-    if amount >= threshold:\n+    if amount > threshold:\n         return amount * 0.9\n     return amount\n";
    const QUIET_BODY: &str = "--- a/pricing.py\n+++ b/pricing.py\n@@ -1,4 +1,4 @@\n def apply_discount(amount, threshold):\n     if amount >= threshold:\n-        return amount * 0.9\n+        return amount * 0.85\n     return amount\n";
    const LIMIT_BODY: &str = "--- a/routes.py\n+++ b/routes.py\n@@ -1,3 +1,3 @@\n @app.route(\"/checkout\", methods=[\"POST\"])\n def checkout(order):\n-    return {\"total\": order.subtotal}\n+    return {\"total\": order.subtotal, \"tax\": order.subtotal * 0.2}\n";

    fn seed_row(
        id: &str,
        repo: &str,
        direction: &str,
        diff: &str,
        target: &str,
        owner: &str,
        expected: &str,
    ) -> Value {
        json!({
            "id": id, "repo": repo, "diff_path": diff, "shape": ["pytest_library"],
            "expected_direction": direction,
            "anchor": {"file": target, "line": 2, "owner": owner, "boundary": "predicate equality boundary"},
            "expected_classification": expected,
            "expected_static_limit_kind": if direction == "should_limit" {
                Value::String("decorator_indirection".to_string())
            } else {
                Value::Null
            },
            "labels": {
                "top_card_useful": null, "false_actionable": null, "false_exposed": null,
                "verify_command_valid": null, "suggested_location_valid": null,
                "packet_boundaries_safe": null, "limitation_quality": null
            },
            "authority_boundary": "review_advisory_only",
            "repair_packet_ready": false,
            "must_not_claim": ["Do not treat a null label as a passing judgment."],
            "reason": "synthetic report selection reason"
        })
    }

    /// Three fully replayable seed rows spanning all three directions, so a
    /// replay run produces one record per direction.
    fn write_inventory(fixture: &TempFixture) -> Result<Vec<String>, String> {
        let gap = fixture.write_diff("report-gap", GAP_BODY)?;
        let quiet = fixture.write_diff("report-quiet", QUIET_BODY)?;
        let limit = fixture.write_diff("report-limit", LIMIT_BODY)?;
        let items = vec![
            seed_row(
                "report-gap-row",
                "report-gap-repo",
                "should_gap",
                &gap,
                "pricing.py",
                "apply_discount",
                "weakly_exposed",
            ),
            seed_row(
                "report-quiet-row",
                "report-quiet-repo",
                "should_stay_quiet",
                &quiet,
                "pricing.py",
                "apply_discount",
                "exposed",
            ),
            seed_row(
                "report-limit-row",
                "report-limit-repo",
                "should_limit",
                &limit,
                "routes.py",
                "checkout",
                "static_unknown",
            ),
        ];
        Ok(vec![fixture.write_envelope(
            "report-panel.json",
            &json!({
                "schema_version": "0.1",
                "kind": "python_judged_pr_panel_manifest",
                "spec": "RIPR-SPEC-0092",
                "tier": "B",
                "description": "Synthetic report inventory over three replayable rows.",
                "limits": ["synthetic report inventory remains advisory only"],
                "items": items
            }),
        )?])
    }

    fn request(
        case_id: &str,
        role: &str,
        identity: &str,
        verdict: &str,
        recorded_at: &str,
    ) -> AdjudicationRequest {
        AdjudicationRequest {
            case_id: case_id.to_string(),
            verdict: verdict.to_string(),
            role: role.to_string(),
            identity: identity.to_string(),
            evidence: vec!["pricing.py:2 (assertion at line 2)".to_string()],
            false_actionable: None,
            false_exposed: None,
            wrong_target: None,
            invalid_command: None,
            limitation_quality: None,
            notes: None,
            recorded_at: recorded_at.to_string(),
        }
    }

    fn worktree_binary() -> Result<String, String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("xtask manifest has no repository parent")?;
        let binary = root
            .join("target")
            .join("debug")
            .join(format!("ripr{}", std::env::consts::EXE_SUFFIX));
        if !binary.is_file() {
            return Err(format!(
                "the worktree ripr debug binary is missing at `{}`; run `cargo build -p ripr` first (report tests resolve the binary and never spawn a nested build)",
                binary.display()
            ));
        }
        std::path::absolute(&binary)
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("resolve worktree ripr binary: {error}"))
    }

    fn ensure(condition: bool, message: &str) -> Result<(), String> {
        if condition {
            Ok(())
        } else {
            Err(format!("report test failed: {message}"))
        }
    }

    /// Two independent roles recorded over one case at `stamp`; `decide`
    /// fills each role's assessment axes.
    fn adjudicate_two_roles(
        fixture: &TempFixture,
        refs: &[&str],
        dir: &str,
        records: &str,
        case_id: &str,
        stamp: &str,
        decide: impl Fn(&mut AdjudicationRequest),
    ) -> Result<(), String> {
        for (role, identity) in [
            ("human_operator", "alice"),
            ("second_human_reviewer", "bob"),
        ] {
            let verdict = if case_id == "report-quiet-row" {
                "exposed"
            } else {
                "static_unknown"
            };
            let mut judged = request(case_id, role, identity, verdict, stamp);
            decide(&mut judged);
            adjudicate_case_at(fixture.root.as_path(), refs, dir, records, &judged)?;
        }
        Ok(())
    }

    /// The adjudication axes the shared pipeline records: the gap row is
    /// decided clean on false_exposed, the quiet row flags a false_actionable
    /// repair route, and the limit row grades its limitation as precise.
    fn pipeline_decide(case_id: &str, judged: &mut AdjudicationRequest) {
        match case_id {
            "report-gap-row" => {
                judged.false_exposed = Some(false);
                judged.wrong_target = Some(false);
            }
            "report-quiet-row" => {
                judged.false_actionable = Some(true);
                judged.wrong_target = Some(true);
                judged.invalid_command = Some(true);
            }
            _ => {
                judged.limitation_quality = Some("precise".to_string());
            }
        }
    }

    /// One shared end-to-end pipeline: two independent replay runs (their
    /// records embed different temp workspace paths and command lines) plus
    /// two two-role adjudication sets stamped at different times, and the
    /// empty-state render.
    struct Pipeline {
        fixture: TempFixture,
        refs: Vec<String>,
        report_empty: RenderedReport,
        report_a: RenderedReport,
        report_b: RenderedReport,
        records_a: String,
        adjudications: String,
    }

    fn pipeline(name: &str) -> Result<Pipeline, String> {
        let fixture = TempFixture::new(name)?;
        let refs = write_inventory(&fixture)?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
        let binary = worktree_binary()?;
        let records_a = fixture.root.join("records-a");
        let records_b = fixture.root.join("records-b");
        for records in [&records_a, &records_b] {
            let summary = crate::python_judged_panel_replay::replay_inventory_at(
                &fixture.root,
                &ref_strs,
                records,
                None,
                binary.as_str(),
            )?;
            ensure(
                summary.replayed == 3,
                "every replay run must replay all three rows",
            )?;
        }
        for (dir, stamp) in [
            (fixture.root.join("adjudications-a"), "2026-09-04T00:00:00Z"),
            (fixture.root.join("adjudications-b"), "2027-12-31T23:59:59Z"),
        ] {
            for case_id in ["report-gap-row", "report-quiet-row", "report-limit-row"] {
                adjudicate_two_roles(
                    &fixture,
                    &ref_strs,
                    dir.to_str().ok_or("utf-8 adjudications")?,
                    records_a.to_str().ok_or("utf-8 records")?,
                    case_id,
                    stamp,
                    |judged| pipeline_decide(case_id, judged),
                )?;
            }
        }
        let empty = build_report_at(
            fixture.root.as_path(),
            &ref_strs,
            Path::new("no-such-records"),
            "no-such-records",
            Path::new("no-such-adjudications"),
            "no-such-adjudications",
            None,
        )?;
        let render = |records: &Path, adjudications: &Path| {
            build_report_at(
                fixture.root.as_path(),
                &ref_strs,
                records,
                "records",
                adjudications,
                "adjudications",
                None,
            )
        };
        let report_a = render(&records_a, &fixture.root.join("adjudications-a"))?;
        let report_b = render(&records_b, &fixture.root.join("adjudications-b"))?;
        let adjudications = fixture.path("adjudications-a")?;
        let records_a_display = fixture.path("records-a")?;
        Ok(Pipeline {
            fixture,
            refs,
            report_empty: empty,
            report_a,
            report_b,
            records_a: records_a_display,
            adjudications,
        })
    }

    /// The byte-stability pin plus the shared-model pin: two independent
    /// replay runs (volatile record content differs) and two adjudication
    /// sets stamped at different times must render byte-identical JSON and
    /// Markdown, with no volatile path leaking; both surfaces state the same
    /// counts and keep the honesty notes visible.
    #[test]
    fn report_bytes_are_stable_across_independent_runs() -> Result<(), String> {
        let pipeline = pipeline("determinism")?;
        let (a, b) = (&pipeline.report_a, &pipeline.report_b);
        ensure(
            a.json == b.json && a.markdown == b.markdown,
            "two independent replay runs over identical inputs must render byte-identical reports",
        )?;
        ensure(
            !a.json.contains(&pipeline.fixture.marker),
            "the report must not echo volatile record content (fixture-root paths leak through the records' command field)",
        )?;
        // The adjudicated rows must be visible with both roles; the limit row
        // stays inconclusive (a real recorded state, never a pass).
        ensure(
            a.json.contains("\"adjudicated\": 2")
                && a.json.contains("\"inconclusive\": 1")
                && a.json.contains("\"selected\": 3"),
            "the two-role decided rows count as adjudicated; the undecided limit row is inconclusive",
        )?;
        ensure(
            a.json.contains("human_operator") && a.json.contains("second_human_reviewer"),
            "case-level roles must be disclosed",
        )?;
        // Markdown and JSON must describe the same model.
        ensure(
            a.markdown.contains("selected: 3") && a.json.contains("\"selected\": 3"),
            "both surfaces must state the same selected count",
        )?;
        ensure(
            a.markdown.contains("no combined score")
                && a.markdown.contains("relation basis: unavailable"),
            "markdown must state that no combined quality score exists and disclose the unavailable relation-basis dimension",
        )?;
        Ok(())
    }

    /// Separate error denominators: a decided false_exposed on a should_gap
    /// row feeds only the false_exposed rate; false_actionable keeps its own
    /// denominator; no denominator means no rate anywhere.
    #[test]
    fn report_derives_separate_error_denominators_and_rates() -> Result<(), String> {
        let pipeline = pipeline("rates")?;
        let empty = &pipeline.report_empty;
        for fragment in [
            "\"selected\": 3",
            "\"replayed\": 0",
            "\"not_run\": 3",
            "\"no_replay_record\": 3",
            "\"adjudicated\": 0",
            "\"unjudged\": 3",
        ] {
            ensure(
                empty.json.contains(fragment),
                &format!("the empty state must disclose its achieved denominator ({fragment})"),
            )?;
        }
        ensure(
            !empty.json.contains("\"rate\":"),
            "absent rates must be omitted from the JSON, not faked as zero",
        )?;
        ensure(
            empty
                .markdown
                .contains("rate not disclosed: no denominator"),
            "the markdown must keep the no-denominator-no-rate rule visible",
        )?;

        let report = &pipeline.report_a;
        let value =
            serde_json::from_str::<Value>(&report.json).map_err(|error| error.to_string())?;
        let rate = |key: &str, field: &str| value["rates"][key][field].as_u64().unwrap_or(u64::MAX);
        ensure(
            rate("false_exposed", "numerator") == 0 && rate("false_exposed", "denominator") == 1,
            "the false_exposed denominator must cover exactly the decided should_gap row",
        )?;
        ensure(
            rate("false_actionable", "numerator") == 1
                && rate("false_actionable", "denominator") == 1,
            "the false_actionable denominator must cover exactly the decided should_stay_quiet row",
        )?;
        ensure(
            rate("wrong_target", "flagged") == 1
                && rate("wrong_target", "assessed") == 2
                && rate("invalid_command", "flagged") == 1,
            "wrong-target and invalid-command counts must come from the adjudications",
        )?;
        ensure(
            value["rates"]["limitation_correctness"]["not_adjudicated"].as_u64() == Some(1)
                && value["rates"]["limitation_correctness"]["precise"].as_u64() == Some(0),
            "an inconclusive should_limit row must not enter the limitation-correctness counts",
        )?;
        ensure(
            value["rates"]["false_actionable"]["denominator_case_ids"]
                .as_array()
                .is_some_and(|ids| {
                    ids.iter()
                        .map(|id| id.as_str().unwrap_or("?"))
                        .collect::<Vec<_>>()
                        == ["report-quiet-row"]
                }),
            "the rate must name its exact denominator cases",
        )?;
        ensure(
            report
                .markdown
                .contains("false_actionable: numerator 1 / denominator 1"),
            "the markdown must carry the exact numerator and denominator",
        )?;
        ensure(
            report.json.contains("denominator_case_ids")
                && report.json.contains("coverage_boundary"),
            "every rate must carry its denominator case ids and coverage boundary",
        )?;
        Ok(())
    }

    /// Threshold evaluation is per-threshold pass/fail/not_evaluable, echoes
    /// the policy rationale, and never writes a tier claim.
    #[test]
    fn threshold_evaluation_is_explicit_per_threshold_and_non_authoritative() -> Result<(), String>
    {
        let pipeline = pipeline("thresholds")?;
        let refs = pipeline.refs.iter().map(String::as_str).collect::<Vec<_>>();
        let policy = pipeline.fixture.write_policy(&json!({
            "schema_version": "0.1",
            "kind": "python_judged_panel_threshold_policy",
            "spec": "RIPR-SPEC-0092",
            "rationale": "candidate for discussion only: zero tolerance on both error axes once two rows are adjudicated",
            "authority": "panel working group (candidate, not accepted)",
            "thresholds": [
                {"metric": "false_actionable_rate", "operator": "max", "value": 0.0},
                {"metric": "false_exposed_rate", "operator": "max", "value": 0.0},
                {"metric": "adjudicated_count", "operator": "min", "value": 2}
            ]
        }))?;
        let before = build_report_at(
            pipeline.fixture.root.as_path(),
            &refs,
            Path::new("no-such-records"),
            "no-such-records",
            Path::new("no-such-adjudications"),
            "no-such-adjudications",
            Some(&policy),
        )?;
        // With zero adjudications: both rate thresholds are not_evaluable and
        // the count threshold fails — never silently passes.
        ensure(
            before.json.matches("\"result\": \"not_evaluable\"").count() == 2
                && before.json.contains("\"result\": \"fail\""),
            "rate thresholds must be not_evaluable without denominators and the count threshold must fail below its minimum",
        )?;
        ensure(
            before.json.contains("candidate for discussion only")
                && before.json.contains("never promotes support")
                && before.json.contains("no operator tier ruling"),
            "the policy rationale must be echoed and the non-authority note present",
        )?;

        // Over the pipeline's two-role adjudications (one flagged
        // false_actionable): that rate fails its zero threshold while the
        // others pass.
        let after = build_report_at(
            pipeline.fixture.root.as_path(),
            &refs,
            Path::new("no-such-records"),
            "no-such-records",
            Path::new(&pipeline.adjudications),
            "adjudications",
            Some(&policy),
        )?;
        ensure(
            after.json.contains("\"result\": \"pass\"")
                && after.json.contains("\"result\": \"fail\""),
            "a failing measured rate must be reported next to passing ones",
        )?;
        Ok(())
    }

    /// The adjudication workflow's honesty rules: reviewer identity and own
    /// evidence citations are required, the verdict must use the conservative
    /// vocabulary, the direction lattice holds, and independence requires two
    /// distinct roles.
    #[test]
    fn adjudicate_rejects_unattributed_lattice_and_vocabulary_drift() -> Result<(), String> {
        let fixture = TempFixture::new("adjudicate-rejects")?;
        let refs = write_inventory(&fixture)?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
        let dir = fixture.path("adjudications")?;
        let judge = |judge_request: AdjudicationRequest| {
            adjudicate_case_at(
                fixture.root.as_path(),
                &ref_strs,
                &dir,
                "records",
                &judge_request,
            )
        };
        let rejection = |judge_request: AdjudicationRequest, what: &str| {
            judge(judge_request)
                .err()
                .ok_or_else(|| format!("report test failed: {what}: expected rejection"))
        };

        let error = rejection(
            request("no-such-case", "human_operator", "alice", "exposed", "t0"),
            "case",
        )?;
        ensure(
            error.contains("unknown case id `no-such-case`"),
            "an unknown case id must be rejected",
        )?;
        let mut no_reviewer = request("report-gap-row", "human_operator", "", "exposed", "t0");
        no_reviewer.evidence.clear();
        let error = rejection(no_reviewer, "identity")?;
        ensure(
            error.contains("reviewer identity is required")
                && error.contains("RIPR_PANEL_ADJUDICATOR"),
            "identity is required and the error must name the env fallback",
        )?;
        let mut no_evidence = request("report-gap-row", "human_operator", "alice", "exposed", "t0");
        no_evidence.evidence.clear();
        let error = rejection(no_evidence, "evidence")?;
        ensure(
            error.contains("evidence") && error.contains("do not copy"),
            "the evidence requirement must warn against copying the candidate classification",
        )?;
        let error = rejection(
            request(
                "report-gap-row",
                "human_operator",
                "alice",
                "proven_correct",
                "t0",
            ),
            "verdict",
        )?;
        ensure(
            error.contains("conservative static vocabulary"),
            "the verdict must stay in vocabulary",
        )?;
        let mut both_true = request("report-gap-row", "human_operator", "alice", "exposed", "t0");
        both_true.false_actionable = Some(true);
        both_true.false_exposed = Some(true);
        let error = rejection(both_true, "lattice")?;
        ensure(
            error.contains("cannot both be true"),
            "the lattice must hold",
        )?;
        let mut inadmissible = request(
            "report-gap-row",
            "human_operator",
            "alice",
            "weakly_exposed",
            "t0",
        );
        inadmissible.false_actionable = Some(true);
        let error = rejection(inadmissible, "direction")?;
        ensure(
            error.contains("not admitted by direction `should_gap`"),
            "the direction lattice must gate the true labels",
        )?;
        let mut limit_quality_on_gap = request(
            "report-gap-row",
            "human_operator",
            "alice",
            "weakly_exposed",
            "t0",
        );
        limit_quality_on_gap.limitation_quality = Some("precise".to_string());
        let error = rejection(limit_quality_on_gap, "limitation")?;
        ensure(
            error.contains("only to `should_limit`"),
            "limitation grading stays limit-scoped",
        )?;

        // A single role never counts as adjudicated; the same role under a
        // second identity changes nothing; a second independent role does.
        let first = judge(request(
            "report-gap-row",
            "human_operator",
            "alice",
            "weakly_exposed",
            "t0",
        ))?;
        ensure(
            first.contains("pending_second_role"),
            "one role must stay pending",
        )?;
        let again = judge(request(
            "report-gap-row",
            "human_operator",
            "alice_recheck",
            "weakly_exposed",
            "t1",
        ))?;
        ensure(
            again.contains("pending_second_role"),
            "two identities under one role are still not independent",
        )?;
        let second = judge(request(
            "report-gap-row",
            "second_human_reviewer",
            "bob",
            "weakly_exposed",
            "t2",
        ))?;
        ensure(
            second.contains("inconclusive") && second.contains("not a pass"),
            "an agreeing judgment with no decided axis must be inconclusive, not a pass",
        )?;

        // Disagreement is a named state with a recorded disposition required.
        judge(request(
            "report-quiet-row",
            "human_operator",
            "alice",
            "exposed",
            "t0",
        ))?;
        let disputed = judge(request(
            "report-quiet-row",
            "second_human_reviewer",
            "bob",
            "weakly_exposed",
            "t1",
        ))?;
        ensure(
            disputed.contains("disputed"),
            "role disagreement must surface as disputed",
        )?;
        Ok(())
    }

    /// The report reader fails closed on record-set rot against the real
    /// pipeline record set: unknown case ids, mixed as-of identity, and
    /// foreign record kinds are rejected instead of silently folded into the
    /// counts.
    #[test]
    fn report_fails_closed_on_record_set_rot() -> Result<(), String> {
        let pipeline = pipeline("record-rot")?;
        let records = Path::new(&pipeline.records_a);
        let record_path = |name: &str| records.join(name);
        let render = || {
            build_report_at(
                pipeline.fixture.root.as_path(),
                &pipeline.refs.iter().map(String::as_str).collect::<Vec<_>>(),
                records,
                "records",
                Path::new("adjudications"),
                "adjudications",
                None,
            )
        };
        let failure = |what: &str| -> Result<String, String> {
            render()
                .err()
                .ok_or_else(|| format!("report test failed: {what}: expected rejection"))
        };
        let rewrite = |name: &str, mutate: &dyn Fn(&mut Value)| -> Result<(), String> {
            let mut value = serde_json::from_str::<Value>(
                &fs::read_to_string(record_path(name)).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            mutate(&mut value);
            fs::write(
                record_path(name),
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
        };
        // The real records share one binary identity; rot stages must keep it
        // consistent so each stage exercises exactly one rejection.
        let shared_binary = serde_json::from_str::<Value>(
            &fs::read_to_string(record_path("report-quiet-row.json"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?["binary"]
            .clone();

        // Stage 1: a record naming a case outside the inventory is rejected
        // (built on the shared identity so the case check fires first).
        let ghost = json!({
            "schema_version": "0.1",
            "kind": "python_judged_panel_replay_record",
            "spec": "RIPR-SPEC-0092",
            "case_id": "ghost-case",
            "binary": shared_binary,
            "outcome": {"kind": "not_run"},
            "comparison": {"kind": "comparison_unavailable"}
        });
        fs::write(
            record_path("ghost-case.json"),
            serde_json::to_string_pretty(&ghost).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        ensure(
            failure("rot")?.contains("ghost-case"),
            "the foreign case must be named",
        )?;
        fs::remove_file(record_path("ghost-case.json")).map_err(|error| error.to_string())?;

        // Stage 2: mixed binary identity across two records is rejected.
        rewrite("report-gap-row.json", &|value: &mut Value| {
            value["binary"]["sha256"] = json!("c".repeat(64));
        })?;
        ensure(
            failure("rot")?.contains("mixed binary identity"),
            "mixed identity must be named",
        )?;

        // Stage 3: a foreign kind is rejected even with a consistent identity
        // (the record is rewritten back onto the shared identity so the kind
        // check — not the identity check — fires).
        rewrite("report-gap-row.json", &|value: &mut Value| {
            value["kind"] = json!("some_other_record");
            value["binary"] = shared_binary.clone();
        })?;
        ensure(
            failure("rot")?.contains("unknown identity"),
            "foreign kinds must be named",
        )?;
        Ok(())
    }

    /// FIX fqoP (devin round 2): a replay record file is addressable only by
    /// its case's stable slug — a renamed record is rejected instead of
    /// silently folding its evidence into the counts under filename order.
    #[test]
    fn report_replay_records_reject_a_slug_mismatch() -> Result<(), String> {
        let pipeline = pipeline("record-slug-binding")?;
        let records = Path::new(&pipeline.records_a);
        let body = fs::read_to_string(records.join("report-quiet-row.json"))
            .map_err(|error| error.to_string())?;
        fs::write(records.join("misnamed.json"), body).map_err(|error| error.to_string())?;
        let failure = build_report_at(
            pipeline.fixture.root.as_path(),
            &pipeline.refs.iter().map(String::as_str).collect::<Vec<_>>(),
            records,
            "records",
            Path::new("adjudications"),
            "adjudications",
            None,
        )
        .err()
        .ok_or("report test failed: expected slug-mismatch rejection")?;
        ensure(
            failure.contains("is not named"),
            &format!("the slug contract must be named, got: {failure}"),
        )?;
        Ok(())
    }

    /// FIX fqqq (devin round 2): the replay reader deliberately tolerates
    /// unknown producer fields — forward compatibility, so an older report
    /// reader keeps reading records from a newer replay producer. This test
    /// pins that contract: unknown fields at every level parse, and the
    /// projected view stays limited to the documented keys so a future
    /// producer fact can never silently leak into report bytes.
    #[test]
    fn replay_record_input_pins_the_compatibility_contract() -> Result<(), String> {
        let body = json!({
            "schema_version": RECORD_SCHEMA_VERSION,
            "kind": RECORD_KIND,
            "spec": SPEC,
            "case_id": "compat-case",
            "row_kind": "gap",
            "binary": {"version": "0.0.0-test", "sha256": "a".repeat(64), "future_binary_fact": 1},
            "diff": {"sha256": "b".repeat(64), "path": "case.diff", "future_diff_fact": true},
            "outcome": {"kind": "not_run", "future_outcome_fact": []},
            "comparison": {"kind": "comparison_unavailable", "future_comparison_fact": "x"},
            "future_top_level_fact": {"nested": [1, 2, 3]}
        });
        let serialized = serde_json::to_string(&body).map_err(|error| error.to_string())?;
        let record = parse_replay_record_bytes(&serialized, "compat-test")?;
        ensure(
            record.case_id == "compat-case" && record.row_kind == "gap",
            "the documented consumed fields must still project",
        )?;
        ensure(
            record.binary.is_some() && record.diff.is_some(),
            "the documented identity fields must still project",
        )?;
        Ok(())
    }

    /// FIX fqlm (devin round 2): adjudicating one case is a read-modify-write
    /// cycle, so a second concurrent adjudication while the record lock is
    /// held must fail closed with a named error instead of silently
    /// discarding one judgment.
    #[test]
    fn concurrent_adjudication_is_refused_while_a_record_lock_is_held() -> Result<(), String> {
        let fixture = TempFixture::new("adjudicate-lock")?;
        let record_path = fixture.root.join("adjudications").join("some-case.json");
        let parent_dir = record_path
            .parent()
            .ok_or("lock test failed: record path has no parent")?;
        fs::create_dir_all(parent_dir).map_err(|error| error.to_string())?;
        let _held = acquire_record_lock(&record_path)?;
        let second = acquire_record_lock(&record_path)
            .err()
            .ok_or("lock test failed: expected the second acquisition to be refused")?;
        ensure(
            second.contains("is locked"),
            &format!("the lock contract must be named, got: {second}"),
        )?;
        drop(_held);
        acquire_record_lock(&record_path)
            .map_err(|error| format!("the lock must release on drop: {error}"))?;
        Ok(())
    }
    /// FIX f2THL/f2XZZ: the record replacement is atomic and read-failure
    /// safe — an injected write failure preserves the prior record bytes and
    /// leaves no temp residue, and invalid UTF-8 at the record path is a
    /// named error instead of a silent overwrite.
    #[test]
    fn adjudication_writes_are_atomic_and_read_failures_are_refused() -> Result<(), String> {
        let fixture = TempFixture::new("atomic-writes")?;
        let refs = write_inventory(&fixture)?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
        let dir = fixture.path("adjudications")?;
        let dest = Path::new(&dir).join("report-gap-row.json");
        let judge = |judge_request: AdjudicationRequest| {
            adjudicate_case_at(
                fixture.root.as_path(),
                &ref_strs,
                &dir,
                "records",
                &judge_request,
            )
        };

        // A prior record exists from the first role.
        judge(request(
            "report-gap-row",
            "human_operator",
            "alice",
            "weakly_exposed",
            "t0",
        ))?;
        let prior = fs::read(&dest).map_err(|error| error.to_string())?;

        // Injected write failure: the staged rename cannot replace the
        // destination, so the writer must fail and the prior record must
        // survive byte-for-byte with no temp residue.
        #[cfg(windows)]
        {
            let mut permissions = fs::metadata(&dest)
                .map_err(|error| error.to_string())?
                .permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&dest, permissions).map_err(|error| error.to_string())?;
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(Path::new(&dir), fs::Permissions::from_mode(0o555))
                .map_err(|error| error.to_string())?;
        }
        let failure = judge(request(
            "report-gap-row",
            "second_human_reviewer",
            "bob",
            "weakly_exposed",
            "t1",
        ))
        .err()
        .ok_or("report test failed: injected write failure must be a named error")?;
        ensure(
            failure.contains("adjudication record"),
            "the write failure must name the record",
        )?;
        ensure(
            fs::read(&dest).map_err(|error| error.to_string())? == prior,
            "the prior record must survive an injected write failure",
        )?;
        #[cfg(windows)]
        {
            // Clearing FILE_ATTRIBUTE_READONLY is the only way to undo the
            // injected failure on Windows; the Unix-mode lint does not apply
            // to this cfg-gated branch.
            #[expect(
                clippy::permissions_set_readonly_false,
                reason = "restoring the Windows file attribute after the injected write failure"
            )]
            {
                let mut permissions = fs::metadata(&dest)
                    .map_err(|error| error.to_string())?
                    .permissions();
                permissions.set_readonly(false);
                fs::set_permissions(&dest, permissions).map_err(|error| error.to_string())?;
            }
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(Path::new(&dir), fs::Permissions::from_mode(0o755))
                .map_err(|error| error.to_string())?;
        }

        // After restoring, the second role records normally and no staged
        // temp file survives in the record directory.
        judge(request(
            "report-gap-row",
            "second_human_reviewer",
            "bob",
            "weakly_exposed",
            "t1",
        ))?;
        for entry in fs::read_dir(&dir).map_err(|error| error.to_string())? {
            let name = entry
                .map_err(|error| error.to_string())?
                .file_name()
                .to_string_lossy()
                .to_string();
            ensure(!name.contains(".tmp-"), "no temp residue may survive")?;
        }

        // Invalid UTF-8 at the record path is a named error, never a
        // truncated or replaced record.
        fs::write(&dest, [0xFF_u8, 0xFE, 0x00]).map_err(|error| error.to_string())?;
        let corrupted = fs::read(&dest).map_err(|error| error.to_string())?;
        let error = judge(request(
            "report-gap-row",
            "human_operator",
            "alice",
            "weakly_exposed",
            "t2",
        ))
        .err()
        .ok_or("report test failed: unreadable record must be refused")?;
        ensure(
            error.contains("read existing adjudication record") && error.contains("report-gap-row"),
            "the read failure must name the unreadable record path",
        )?;
        ensure(
            fs::read(&dest).map_err(|error| error.to_string())? == corrupted,
            "the unreadable record must not be overwritten",
        )?;
        Ok(())
    }

    /// A minimal stored adjudication record for report-side validation tests;
    /// the arguments carry the pieces under test.
    fn stored_record(
        case_id: &str,
        direction: &str,
        verdict: &str,
        evidence: Value,
        false_actionable: Value,
        false_exposed: Value,
    ) -> Value {
        json!({
            "schema_version": "0.1",
            "kind": "python_judged_panel_adjudication_record",
            "spec": "RIPR-SPEC-0092",
            "case_id": case_id,
            "source_envelope": "fixtures/python-judged-pr-panel/report-panel.json",
            "expected_direction": direction,
            "must_not_claim": ["Do not treat a null label as a passing judgment."],
            "judgments": [{
                "reviewer_role": "human_operator",
                "reviewer_identity": "alice",
                "recorded_at": "2026-09-04T00:00:00Z",
                "verdict": verdict,
                "false_actionable": false_actionable,
                "false_exposed": false_exposed,
                "wrong_target": null,
                "invalid_command": null,
                "limitation_quality": null,
                "evidence_references": evidence,
                "notes": null
            }],
            "authority_boundary": "review_advisory_only"
        })
    }

    fn write_stored(fixture: &TempFixture, value: &Value) -> Result<String, String> {
        let dir = fixture.path("adjudications")?;
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        let case_id = value["case_id"].as_str().unwrap_or("?").to_string();
        let slug = crate::python_judged_panel_replay::stable_case_slug(&case_id);
        fs::write(
            Path::new(&dir).join(format!("{slug}.json")),
            serde_json::to_string_pretty(value).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(dir)
    }

    /// FIX f2XZU: every stored judgment runs through the same semantic rules
    /// the CLI enforces — a malformed stored record fails the report with a
    /// violation named per case, it is never silently excluded.
    #[test]
    fn report_fails_on_semantically_invalid_stored_judgments() -> Result<(), String> {
        let fixture = TempFixture::new("stored-semantics")?;
        let refs = write_inventory(&fixture)?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
        for (name, record, fragment) in [
            (
                "empty evidence",
                stored_record(
                    "report-gap-row",
                    "should_gap",
                    "weakly_exposed",
                    json!([]),
                    json!(null),
                    json!(null),
                ),
                "evidence is required",
            ),
            (
                "unknown verdict",
                stored_record(
                    "report-gap-row",
                    "should_gap",
                    "proven_correct",
                    json!(["pricing.py:2"]),
                    json!(null),
                    json!(null),
                ),
                "conservative static vocabulary",
            ),
            (
                "both error flags",
                stored_record(
                    "report-gap-row",
                    "should_gap",
                    "weakly_exposed",
                    json!(["pricing.py:2"]),
                    json!(true),
                    json!(true),
                ),
                "cannot both be true",
            ),
        ] {
            let dir = write_stored(&fixture, &record)?;
            let error = build_report_at(
                fixture.root.as_path(),
                &ref_strs,
                Path::new("records"),
                "records",
                Path::new(&dir),
                "adjudications",
                None,
            )
            .err()
            .ok_or_else(|| format!("report test failed: {name}: expected rejection"))?;
            ensure(
                error.contains("report-gap-row") && error.contains(fragment),
                &format!(
                    "report test failed: {name}: violation must name the case and reason, found: {error}"
                ),
            )?;
            fs::remove_dir_all(&dir).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    /// FIX f2TIz: carryover rows (null expected_classification) cannot be
    /// adjudicated and never enter adjudicated counts, error denominators,
    /// quality counts, or thresholds — an injected record for one is
    /// excluded defensively.
    #[test]
    fn carryover_rows_are_never_adjudicated_or_counted() -> Result<(), String> {
        let fixture = TempFixture::new("carryover")?;
        let refs = write_inventory(&fixture)?;
        // The retained sqlalchemy-style carryover: robustness-only row with
        // null expected_classification, no anchor, and the grandfathered
        // timeout limit kind.
        let carryover_diff = fixture.write_diff("report-carryover", LIMIT_BODY)?;
        let carryover = json!({
            "id": "report-carryover-row",
            "repo": "report-carryover-repo",
            "diff_path": carryover_diff,
            "shape": ["pytest_library"],
            "expected_direction": "should_limit",
            "anchor": {"file": null, "line": null, "owner": "carried_owner", "boundary": "carried boundary"},
            "expected_classification": null,
            "expected_static_limit_kind": "timeout",
            "actual_classification": null,
            "actual_oracle_alignment": null,
            "labels": {
                "top_card_useful": null, "false_actionable": null, "false_exposed": null,
                "verify_command_valid": null, "suggested_location_valid": null,
                "packet_boundaries_safe": null, "limitation_quality": "imprecise"
            },
            "judgment_source": "manual_review",
            "judged_at": "2026-06-13",
            "judged_by": "campaign",
            "authority_boundary": "review_advisory_only",
            "repair_packet_ready": false,
            "must_not_claim": ["Robustness carryover; not a judged denominator row."],
            "reason": "retained robustness sweep carryover row"
        });
        let envelope_path = format!("{PANEL_DIR}/report-panel.json");
        let mut envelope = serde_json::from_str::<Value>(
            &fs::read_to_string(fixture.root.join(&envelope_path))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        envelope["items"]
            .as_array_mut()
            .ok_or("items must be an array")?
            .push(carryover);
        fs::write(
            fixture.root.join(&envelope_path),
            serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();

        // The CLI refuses to adjudicate a carryover row, with a named reason.
        let error = adjudicate_case_at(
            fixture.root.as_path(),
            &ref_strs,
            &fixture.path("adjudications")?,
            "records",
            &request(
                "report-carryover-row",
                "human_operator",
                "alice",
                "static_unknown",
                "t0",
            ),
        )
        .err()
        .ok_or("report test failed: carryover rows must refuse adjudication")?;
        ensure(
            error.contains("cannot be adjudicated"),
            "the refusal must name the carryover rule",
        )?;

        // An injected record for the carryover row is excluded defensively:
        // it never enters the adjudicated count, and the case entry carries
        // no adjudication.
        let mut injected = stored_record(
            "report-carryover-row",
            "should_limit",
            "static_unknown",
            json!(["routes.py:3"]),
            json!(false),
            json!(false),
        );
        injected["judgments"]
            .as_array_mut()
            .ok_or("judgments must be an array")?
            .push(json!({
                "reviewer_role": "second_human_reviewer",
                "reviewer_identity": "bob",
                "recorded_at": "2026-09-04T00:00:00Z",
                "verdict": "static_unknown",
                "false_actionable": false,
                "false_exposed": false,
                "wrong_target": null,
                "invalid_command": null,
                "limitation_quality": null,
                "evidence_references": ["routes.py:3"],
                "notes": null
            }));
        let dir = write_stored(&fixture, &injected)?;
        let report = build_report_at(
            fixture.root.as_path(),
            &ref_strs,
            Path::new("records"),
            "records",
            Path::new(&dir),
            "adjudications",
            None,
        )?;
        ensure(
            report.json.contains("\"adjudicated\": 0") && report.json.contains("\"stale_row\": 0"),
            "an injected carryover record must stay out of every adjudicated count",
        )?;
        ensure(
            report.json.contains("\"adjudication\": null"),
            "the carryover case entry must disclose no adjudication",
        )?;
        Ok(())
    }

    /// FIX f2TIA: verdict-to-error coherence mirrors the retained-panel
    /// validator's outcome table — `exposed` on a should_gap/should_limit row
    /// is an over-credit and requires false_exposed true, on the CLI and in
    /// every stored judgment.
    #[test]
    fn verdict_error_coherence_follows_the_direction_lattice() -> Result<(), String> {
        let fixture = TempFixture::new("verdict-coherence")?;
        let refs = write_inventory(&fixture)?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
        let dir = fixture.path("adjudications")?;

        // CLI: an exposed verdict on a should_gap row without the
        // false_exposed=true label is rejected.
        let error = adjudicate_case_at(
            fixture.root.as_path(),
            &ref_strs,
            &dir,
            "records",
            &request("report-gap-row", "human_operator", "alice", "exposed", "t0"),
        )
        .err()
        .ok_or("report test failed: incoherent verdict must be rejected")?;
        ensure(
            error.contains("over-credit") && error.contains("should_gap"),
            "the coherence violation must name the over-credit rule",
        )?;

        // Two roles recording the over-credit coherently: verdict exposed
        // plus false_exposed true is admitted and lands in the rate.
        for (role, identity) in [
            ("human_operator", "alice"),
            ("second_human_reviewer", "bob"),
        ] {
            let mut judged = request(
                "report-gap-row",
                role,
                identity,
                "exposed",
                "2026-09-04T00:00:00Z",
            );
            judged.false_exposed = Some(true);
            adjudicate_case_at(fixture.root.as_path(), &ref_strs, &dir, "records", &judged)?;
        }
        let report = build_report_at(
            fixture.root.as_path(),
            &ref_strs,
            Path::new("records"),
            "records",
            Path::new(&dir),
            "adjudications",
            None,
        )?;
        let value =
            serde_json::from_str::<Value>(&report.json).map_err(|error| error.to_string())?;
        ensure(
            value["rates"]["false_exposed"]["numerator"].as_u64() == Some(1)
                && value["counts"]["adjudicated"].as_u64() == Some(1),
            "the coherent over-credit judgment must count as adjudicated and feed the false_exposed numerator",
        )?;

        // Stored incoherence fails the report named per case.
        let incoherent = stored_record(
            "report-gap-row",
            "should_gap",
            "exposed",
            json!(["pricing.py:2"]),
            json!(null),
            json!(null),
        );
        let incoherent_dir = write_stored(&fixture, &incoherent)?;
        let error = build_report_at(
            fixture.root.as_path(),
            &ref_strs,
            Path::new("records"),
            "records",
            Path::new(&incoherent_dir),
            "adjudications",
            None,
        )
        .err()
        .ok_or("report test failed: stored incoherence must fail the report")?;
        ensure(
            error.contains("report-gap-row") && error.contains("over-credit"),
            "the stored incoherence must name the case and rule",
        )?;
        Ok(())
    }

    /// FIX f2TMb: a rate's as-of identity derives only from the denominator
    /// cases' own replay records; a denominator case without a record forces
    /// the `no_common_binary_identity` disclosure instead of a fabricated
    /// directory-wide identity.
    #[test]
    fn rate_as_of_identity_binds_the_denominator_cases_records() -> Result<(), String> {
        let fixture = TempFixture::new("rate-as-of")?;
        let refs = write_inventory(&fixture)?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
        let records = fixture.root.join("records");
        crate::python_judged_panel_replay::replay_inventory_at(
            &fixture.root,
            &ref_strs,
            &records,
            None,
            worktree_binary()?.as_str(),
        )?;
        let adjudications = fixture.path("adjudications")?;
        let records_display = records.to_str().ok_or("records utf-8")?;
        for (case_id, decided_axis) in [
            ("report-gap-row", "false_exposed"),
            ("report-quiet-row", "false_actionable"),
        ] {
            for (role, identity) in [
                ("human_operator", "alice"),
                ("second_human_reviewer", "bob"),
            ] {
                let mut judged = request(
                    case_id,
                    role,
                    identity,
                    if case_id == "report-quiet-row" {
                        "exposed"
                    } else {
                        "weakly_exposed"
                    },
                    "2026-09-04T00:00:00Z",
                );
                if decided_axis == "false_exposed" {
                    judged.false_exposed = Some(false);
                } else {
                    judged.false_actionable = Some(true);
                }
                adjudicate_case_at(
                    fixture.root.as_path(),
                    &ref_strs,
                    &adjudications,
                    records_display,
                    &judged,
                )?;
            }
        }
        let render = |records_dir: &Path,
                      adjudications_dir: &str|
         -> Result<super::RenderedReport, String> {
            build_report_at(
                fixture.root.as_path(),
                &ref_strs,
                records_dir,
                "records",
                Path::new(adjudications_dir),
                "adjudications",
                None,
            )
        };
        let full = render(&records, &adjudications)?;
        let value = serde_json::from_str::<Value>(&full.json).map_err(|error| error.to_string())?;
        ensure(
            value["rates"]["false_exposed"]["as_of_basis"] == "denominator_case_records"
                && value["rates"]["false_exposed"]["as_of"]["binary_version"]
                    .as_str()
                    .is_some(),
            "a denominator case with its own record binds the rate as-of identity",
        )?;

        // Drop the quiet row's record: the false_actionable denominator now
        // has no record to bind, so the rate discloses instead of citing.
        let partial = fixture.root.join("records-partial");
        fs::create_dir_all(&partial).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(&records).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.file_name().and_then(|name| name.to_str()) == Some("report-quiet-row.json") {
                continue;
            }
            fs::copy(&path, partial.join(path.file_name().ok_or("file name")?))
                .map_err(|error| error.to_string())?;
        }
        let partial_report = render(&partial, &adjudications)?;
        let value = serde_json::from_str::<Value>(&partial_report.json)
            .map_err(|error| error.to_string())?;
        ensure(
            value["rates"]["false_actionable"]["as_of_basis"] == "no_common_binary_identity"
                && value["rates"]["false_actionable"]["as_of"]["binary_version"].is_null(),
            "a denominator case without a record must disclose no_common_binary_identity",
        )?;
        ensure(
            value["rates"]["false_exposed"]["as_of_basis"] == "denominator_case_records",
            "denominator cases that do bind records still cite their identity",
        )?;

        // FIX fqNy: a denominator record whose bound diff no longer matches
        // the case's current diff is stale; its binary identity must not
        // label the rate either.
        let stale_dir = fixture.root.join("records-stale");
        fs::create_dir_all(&stale_dir).map_err(|error| error.to_string())?;
        for entry in fs::read_dir(&records).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            let name = path
                .file_name()
                .ok_or("record file name")?
                .to_string_lossy()
                .to_string();
            let body = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let mut value =
                serde_json::from_str::<Value>(&body).map_err(|error| error.to_string())?;
            if name == "report-quiet-row.json" {
                value["diff"]["sha256"] = json!("f".repeat(64));
            }
            fs::write(
                stale_dir.join(&name),
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
        let stale_report = render(&stale_dir, &adjudications)?;
        let value =
            serde_json::from_str::<Value>(&stale_report.json).map_err(|error| error.to_string())?;
        ensure(
            value["rates"]["false_actionable"]["as_of_basis"] == "no_common_binary_identity",
            "a stale denominator record must not label the rate with its identity",
        )?;
        Ok(())
    }

    /// FIX fqNa (devin round 4): stored provenance echoes (envelope,
    /// direction, non-claims) and the stored recorded_at must stay consistent
    /// with the validated row; a drifted echo fails the report and refuses
    /// re-adjudication instead of being preserved.
    #[test]
    fn stored_provenance_echoes_must_match_the_validated_row() -> Result<(), String> {
        let fixture = TempFixture::new("provenance-echo")?;
        let refs = write_inventory(&fixture)?;
        let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
        let dir = fixture.path("adjudications")?;
        adjudicate_case_at(
            fixture.root.as_path(),
            &ref_strs,
            &dir,
            "records",
            &request(
                "report-gap-row",
                "human_operator",
                "alice",
                "weakly_exposed",
                "2026-09-04T00:00:00Z",
            ),
        )?;
        let record_path = Path::new(&dir).join("report-gap-row.json");
        let mutate = |edit: &dyn Fn(&mut Value)| -> Result<(), String> {
            let mut value = serde_json::from_str::<Value>(
                &fs::read_to_string(&record_path).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
            edit(&mut value);
            fs::write(
                &record_path,
                serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
        };
        let report_error = || -> Result<String, String> {
            build_report_at(
                fixture.root.as_path(),
                &ref_strs,
                Path::new("records"),
                "records",
                Path::new(&dir),
                "adjudications",
                None,
            )
            .err()
            .ok_or("report test failed: drifted provenance must fail the report".to_string())
        };

        // Direction echo drift fails the report and refuses re-adjudication.
        mutate(&|value: &mut Value| value["expected_direction"] = json!("should_limit"))?;
        ensure(
            report_error()?.contains("stored provenance contradicts"),
            "a drifted direction echo must fail the report",
        )?;
        let refusal = adjudicate_case_at(
            fixture.root.as_path(),
            &ref_strs,
            &dir,
            "records",
            &request(
                "report-gap-row",
                "second_human_reviewer",
                "bob",
                "weakly_exposed",
                "2026-09-04T00:00:00Z",
            ),
        )
        .err()
        .ok_or("report test failed: drifted provenance must refuse re-adjudication")?;
        ensure(
            refusal.contains("contradicts the current validated row"),
            "re-adjudication must refuse a drifted record",
        )?;
        mutate(&|value: &mut Value| value["expected_direction"] = json!("should_gap"))?;

        // Non-claims echo drift fails the report too.
        mutate(&|value: &mut Value| value["must_not_claim"] = json!(["never claim X"]))?;
        ensure(
            report_error()?.contains("stored provenance contradicts"),
            "a drifted non-claims echo must fail the report",
        )?;
        mutate(&|value: &mut Value| {
            value["must_not_claim"] = json!(["Do not treat a null label as a passing judgment."])
        })?;

        // A stored timestamp that is not a parseable RFC 3339 instant is
        // rejected provenance.
        mutate(&|value: &mut Value| value["judgments"][0]["recorded_at"] = json!("not-a-time"))?;
        ensure(
            report_error()?.contains("parseable RFC 3339"),
            "a fabricated timestamp must fail the report",
        )?;
        Ok(())
    }

    /// FIX fqNz (devin round 4): cite_replay_record cites a record at the
    /// expected name only when it declares the requested case; foreign
    /// content under the right file name is never attributed as the case's
    /// advisory evidence.
    #[test]
    fn cite_replay_record_refuses_foreign_case_content() -> Result<(), String> {
        let fixture = TempFixture::new("cite-foreign")?;
        let records = fixture.root.join("records");
        fs::create_dir_all(&records).map_err(|error| error.to_string())?;
        let write_record = |case_id: &str| -> Result<(), String> {
            let body = json!({
                "schema_version": RECORD_SCHEMA_VERSION,
                "kind": RECORD_KIND,
                "spec": SPEC,
                "case_id": case_id,
                "binary": {"version": "0.0.0-test", "sha256": "a".repeat(64)},
                "diff": {"sha256": "b".repeat(64)},
                "outcome": {"kind": "not_run"},
                "comparison": {"kind": "comparison_unavailable"}
            });
            fs::write(
                records.join("report-gap-row.json"),
                serde_json::to_string(&body).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())
        };
        let records_display = records.to_str().ok_or("records utf-8")?;
        write_record("some-other-case")?;
        ensure(
            super::cite_replay_record(records_display, "report-gap-row").is_none(),
            "foreign case content under the expected name must not be cited",
        )?;
        write_record("report-gap-row")?;
        ensure(
            super::cite_replay_record(records_display, "report-gap-row").is_some(),
            "the honest record is cited",
        )?;
        Ok(())
    }

    /// FIX fqNv (devin round 4): report generations are serialized per output
    /// directory; a held generation lock refuses a second publisher with a
    /// named error instead of letting two renames interleave into a mixed
    /// json/markdown pair.
    #[test]
    fn report_generations_are_serialized_by_the_output_lock() -> Result<(), String> {
        let fixture = TempFixture::new("report-lock")?;
        let out = fixture.root.join("out");
        fs::create_dir_all(&out).map_err(|error| error.to_string())?;
        let _held = super::acquire_path_lock(
            out.join(".report-generation.lock"),
            "report generation",
            |_| "held".to_string(),
        )?;
        let failure = super::write_report_generation(
            &out.join("report.json"),
            &out.join("report.md"),
            "{\"v\":1}\n",
            "# v1\n",
        )
        .err()
        .ok_or("report test failed: the held lock must refuse a second publisher")?;
        ensure(
            failure.contains("another report generation"),
            "the lock contract must be named, got: {failure}",
        )?;
        Ok(())
    }

    /// FIX f2XZN: an adjudication is bound to the full row revision; a row
    /// or diff change after adjudication makes the record `stale_row` —
    /// excluded from every denominator and disclosed with stored-vs-current
    /// digests.
    #[test]
    fn adjudications_stale_against_a_changed_row_revision() -> Result<(), String> {
        let adjudicate_gap =
            |fixture: &TempFixture, refs: &[String], dir: &str| -> Result<(), String> {
                let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
                for (role, identity) in [
                    ("human_operator", "alice"),
                    ("second_human_reviewer", "bob"),
                ] {
                    let mut judged = request(
                        "report-gap-row",
                        role,
                        identity,
                        "weakly_exposed",
                        "2026-09-04T00:00:00Z",
                    );
                    judged.false_exposed = Some(false);
                    adjudicate_case_at(fixture.root.as_path(), &ref_strs, dir, "records", &judged)?;
                }
                Ok(())
            };
        let render = |fixture: &TempFixture, refs: &[String], dir: &str| -> Result<Value, String> {
            let ref_strs = refs.iter().map(String::as_str).collect::<Vec<_>>();
            let report = build_report_at(
                fixture.root.as_path(),
                &ref_strs,
                Path::new("records"),
                "records",
                Path::new(dir),
                "adjudications",
                None,
            )?;
            serde_json::from_str::<Value>(&report.json).map_err(|error| error.to_string())
        };

        // Sub-case 1: the row's own content changes after adjudication.
        let fixture = TempFixture::new("row-revision")?;
        let refs = write_inventory(&fixture)?;
        let dir = fixture.path("adjudications")?;
        adjudicate_gap(&fixture, &refs, &dir)?;
        let before = render(&fixture, &refs, &dir)?;
        let entry = &before["cases"][0];
        ensure(
            entry["case_id"] == "report-gap-row"
                && entry["adjudication"]["row_revision"]["stored"]
                    == entry["adjudication"]["row_revision"]["current"],
            "a fresh adjudication binds the current row revision",
        )?;
        let envelope_path = format!("{PANEL_DIR}/report-panel.json");
        let mut envelope = serde_json::from_str::<Value>(
            &fs::read_to_string(fixture.root.join(&envelope_path))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        envelope["items"][0]["reason"] = json!("reason changed after adjudication");
        fs::write(
            fixture.root.join(&envelope_path),
            serde_json::to_string_pretty(&envelope).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let after = render(&fixture, &refs, &dir)?;
        ensure(
            after["counts"]["stale_row"].as_u64() == Some(1)
                && after["counts"]["adjudicated"].as_u64() == Some(0),
            "a changed row makes the adjudication stale and drops it from the denominator",
        )?;
        let entry = &after["cases"][0];
        ensure(
            entry["adjudication"]["row_revision"]["stored"]
                != entry["adjudication"]["row_revision"]["current"],
            "the stale entry must disclose stored-vs-current digests",
        )?;

        // Sub-case 2: only the diff content changes after adjudication.
        let fixture = TempFixture::new("row-revision-diff")?;
        let refs = write_inventory(&fixture)?;
        let dir = fixture.path("adjudications")?;
        adjudicate_gap(&fixture, &refs, &dir)?;
        let diff_path = format!("{PANEL_DIR}/diffs/report-gap.diff");
        let diff =
            fs::read_to_string(fixture.root.join(&diff_path)).map_err(|error| error.to_string())?;
        fs::write(
            fixture.root.join(&diff_path),
            diff.replace("return amount * 0.9", "return amount * 0.95"),
        )
        .map_err(|error| error.to_string())?;
        let after = render(&fixture, &refs, &dir)?;
        ensure(
            after["counts"]["stale_row"].as_u64() == Some(1),
            "a changed diff breaks the row-revision binding too",
        )?;
        Ok(())
    }

    /// FIX f2TNz: report.json and report.md publish as one generation — a
    /// failure between the two publications is rolled back to the prior pair.
    #[test]
    fn report_publication_is_one_generation() -> Result<(), String> {
        let fixture = TempFixture::new("one-generation")?;
        let out = fixture.root.join("out");
        let json_path = out.join("report.json");
        let markdown_path = out.join("report.md");
        super::write_report_generation(&json_path, &markdown_path, "{\"v\":1}\n", "# v1\n")?;
        let prior_json = fs::read(&json_path).map_err(|e| format!("read prior json: {e}"))?;
        let prior_markdown = fs::read(&markdown_path).map_err(|e| format!("read prior md: {e}"))?;

        // Inject a failure between the two publications: the markdown path
        // is occupied by a directory, so its rename fails after report.json
        // was already replaced.
        fs::remove_file(&markdown_path)
            .map_err(|error| format!("remove prior markdown: {error}"))?;
        fs::create_dir(&markdown_path).map_err(|error| format!("create md dir: {error}"))?;
        let error =
            super::write_report_generation(&json_path, &markdown_path, "{\"v\":2}\n", "# v2\n")
                .err()
                .ok_or(
                    "report test failed: the injected second-publication failure must surface",
                )?;
        ensure(
            error.contains("prior report.json generation was restored"),
            "the failure must disclose the rollback",
        )?;
        ensure(
            fs::read(&json_path).map_err(|e| format!("read rolled-back json: {e}"))? == prior_json,
            "report.json must be rolled back to the prior generation",
        )?;
        ensure(
            markdown_path.is_dir(),
            "the prior markdown must be untouched (still the injected obstruction)",
        )?;
        let _ = prior_markdown;
        for entry in fs::read_dir(&out).map_err(|error| error.to_string())? {
            let name = entry
                .map_err(|error| error.to_string())?
                .file_name()
                .to_string_lossy()
                .to_string();
            ensure(!name.contains(".tmp-"), "no staged temp may survive")?;
        }

        // Removing the obstruction lets the next generation publish fully.
        fs::remove_dir(&markdown_path).map_err(|error| format!("remove md dir: {error}"))?;
        super::write_report_generation(&json_path, &markdown_path, "{\"v\":2}\n", "# v2\n")?;
        ensure(
            fs::read(&json_path).map_err(|error| error.to_string())? == b"{\"v\":2}\n"
                && fs::read(&markdown_path).map_err(|error| error.to_string())? == b"# v2\n",
            "the next generation publishes both files",
        )?;
        Ok(())
    }
}
