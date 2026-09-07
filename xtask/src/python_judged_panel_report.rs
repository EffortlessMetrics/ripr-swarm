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

mod cli;
mod judgment_semantics;
mod threshold;
mod view;

#[cfg(test)]
mod tests;

pub(crate) use cli::{run_adjudicate, run_report};

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::branch_inventory::parse_rfc3339_epoch_seconds;
use crate::python_judged_panel::{
    KNOWN_LIMITATION_QUALITIES, PythonJudgedPanelItem, RowKind, direction_admits_error,
    load_validated_inventory, parse_json_without_duplicate_keys, row_kind,
};
use crate::python_judged_panel_replay::{sha256_hex, stable_case_slug};

use cli::AdjudicationRequest;
use judgment_semantics::{JudgmentSemantics, validate_judgment_semantics};
use threshold::evaluate_threshold_policy;
use view::{AdjudicationState, AxisValue, bool_token, derive_adjudication_view};

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
// Adjudication: the typed current-judgment record
// ---------------------------------------------------------------------------

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
    // FIX fqGSD (CodeRabbit #3681): one judgment per role — a role's current
    // judgment replaces whatever that role recorded before (including a
    // re-record under a changed reviewer identity), so role-to-identity stays
    // one-to-one and the record always carries each role's current view.
    record
        .judgments
        .retain(|existing| existing.reviewer_role != request.role);
    // One identity may never occupy two roles: the independence claim behind
    // `adjudicated` would be false. (After the retain above, no remaining
    // judgment carries this role — the role-condition simplification is
    // credited to the gemini review.)
    if record
        .judgments
        .iter()
        .any(|existing| existing.reviewer_identity == request.identity)
    {
        return Err(format!(
            "identity `{}` is already recorded under a different role on case `{}`; one identity must never occupy two roles — independence requires distinct people\nrerun: {ADJUDICATE_RERUN}",
            request.identity, request.case_id
        ));
    }
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
            let mut seen_roles = BTreeSet::new();
            let mut seen_identities = BTreeSet::new();
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
                seen_roles.insert(judgment.reviewer_role.as_str());
                seen_identities.insert(judgment.reviewer_identity.as_str());
            }
            // FIX fqGSD (CodeRabbit #3681): independence needs a one-to-one
            // role-to-identity mapping — one identity occupying two roles (or
            // one role carrying two identities) can satisfy both count checks
            // while the "independent roles" claim is false. With duplicate
            // pairs already rejected above, the mapping is one-to-one exactly
            // when every set has the pair count (simplification credited to
            // the gemini review), so a hand-edited record can never reach
            // `Adjudicated` with a shared identity.
            if seen_pairs.len() != seen_roles.len() || seen_pairs.len() != seen_identities.len() {
                return Err(format!(
                    "adjudication record `{file_name}` case `{case_id}`: role-to-identity mapping is not one-to-one; one identity must never occupy two roles and one role must never carry two identities — independence requires it"
                ));
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
/// Publish one report generation: both files staged, then renamed in — the
/// json first, the markdown second, with the json rolled back when the
/// markdown rename fails. Residual, disclosed (#3674 review round 5): a
/// process termination between the two renames can still leave a mixed pair
/// on disk; the one-generation guarantee holds for observable errors, not
/// for a crash mid-publication. A crashed pair self-heals on the next
/// successful publication, and the generation lock excludes concurrent
/// publishers.
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
