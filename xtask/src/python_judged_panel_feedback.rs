//! Regression feedback (RIPR-SPEC-0092, #3680): scans the derived panel
//! report for **confirmed over-credits** — anchored, current, adjudicated
//! rows where two independent roles agreed the verdict `exposed` while
//! deciding `false_exposed` true on a `should_gap` or `should_limit` row —
//! and stages deterministic proposal documents with full provenance under
//! `target/ripr/python-judged-panel/feedback/`.
//!
//! The staging is deliberately the whole capability: proposals never write
//! the evidence-promotion corpus and never change analyzer behaviour. Each
//! proposal carries the promotion recipe — extract a minimal *regular*
//! fixture (with golden `expected/check.json`; manifest-only fixture dirs are
//! rejected by the SPEC-0108 meta-gate) reproducing the row's false-promotion
//! shape, add the corpus case with `must_not_promote` assertions, and run
//! `cargo xtask check-evidence-promotion-honesty`. A promotion case that
//! fails that gate means the analyzer genuinely over-promotes, and the
//! analyzer fix lands in the same PR.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::python_judged_panel::INVENTORY_PATHS;
use crate::python_judged_panel_replay::stable_case_slug;
use crate::python_judged_panel_report::{RenderedReport, build_report_at};

const SPEC: &str = "RIPR-SPEC-0092";
const FEEDBACK_SCHEMA_VERSION: &str = "0.1";
const FEEDBACK_INDEX_KIND: &str = "python_judged_panel_feedback_index";
const FEEDBACK_PROPOSAL_KIND: &str = "python_judged_panel_feedback_proposal";
const FEEDBACK_OUT_DIR: &str = "target/ripr/python-judged-panel/feedback";
const ADJUDICATIONS_DIR: &str = "target/ripr/python-judged-panel/adjudications";
const RECORDS_DIR: &str = "target/ripr/python-judged-panel/replay";
const AUTHORITY_BOUNDARY: &str = "review_advisory_only";
const RERUN: &str = "cargo xtask python-judged-panel feedback";

/// Directions where `exposed` is an error claim: a confirmed over-credit on
/// these rows is exactly the false-promotion family the honesty corpus pins.
const OVER_CREDIT_DIRECTIONS: [&str; 2] = ["should_gap", "should_limit"];

pub(crate) fn run_feedback(args: &[String]) -> Result<(), String> {
    let mut records = RECORDS_DIR.to_string();
    let mut adjudications = ADJUDICATIONS_DIR.to_string();
    let mut out_dir = FEEDBACK_OUT_DIR.to_string();
    let mut check = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--records" => records = take_value(args, &mut index, "--records <dir>")?,
            "--adjudications" => {
                adjudications = take_value(args, &mut index, "--adjudications <dir>")?
            }
            "--out" => out_dir = take_value(args, &mut index, "--out <dir>")?,
            "--check" => check = true,
            other => {
                return Err(format!(
                    "unknown python-judged-panel feedback argument `{other}`; expected `feedback [--records <dir>] [--adjudications <dir>] [--out <dir>] [--check]`\nrerun: {RERUN}"
                ));
            }
        }
        index += 1;
    }
    let out_path = Path::new(&out_dir);
    let staged = derive_feedback(
        Path::new("."),
        &INVENTORY_PATHS,
        Path::new(&records),
        Path::new(&adjudications),
        &adjudications,
    )?;
    // (the CLI wrapper is anchored to the checked-out repository inventory)
    if check {
        verify_staged(out_path, &staged)?;
        println!(
            "python-judged-panel feedback --check: staged proposals match (confirmed_over_credits={})",
            staged.confirmed_over_credits
        );
        return Ok(());
    }
    write_staging(out_path, &staged)?;
    println!(
        "python-judged-panel feedback: staged {} proposal(s) for {} confirmed over-credit(s) under `{}`\nproposals never write the corpus; promote via the recipe in each proposal (fixture extraction + must_not_promote assertions + cargo xtask check-evidence-promotion-honesty)\nrerun: {RERUN}",
        staged.proposal_files.len(),
        staged.confirmed_over_credits,
        out_path.display()
    );
    Ok(())
}

pub(crate) struct StagedFeedback {
    /// (file name, bytes) in stable order; per-proposal documents sorted by
    /// case id, then `index.json`.
    files: Vec<(String, Vec<u8>)>,
    pub(crate) proposal_files: Vec<String>,
    pub(crate) confirmed_over_credits: usize,
}

/// Derives the proposals from the shared report derivation, in memory. The
/// staging directory is tool-owned: a write clears it first so a proposal can
/// never survive the adjudication state it was derived from.
pub(crate) fn derive_feedback(
    root: &Path,
    displays: &[&str],
    records_dir: &Path,
    adjudications_dir: &Path,
    adjudications_display: &str,
) -> Result<StagedFeedback, String> {
    let report = build_report_at(
        root,
        displays,
        records_dir,
        &records_dir.display().to_string(),
        adjudications_dir,
        adjudications_display,
        None,
    )?;
    let value = parse_report(&report)?;
    let proposals = confirmed_over_credits(&value)?;
    derive_staging(&proposals)
}

fn parse_report(report: &RenderedReport) -> Result<Value, String> {
    serde_json::from_str(&report.json)
        .map_err(|error| format!("parse derived panel report: {error}"))
}

/// One proposal per confirmed over-credit, in stable case-id order.
fn confirmed_over_credits(report: &Value) -> Result<Vec<Value>, String> {
    let mut proposals = Vec::new();
    for case in report["cases"].as_array().into_iter().flatten() {
        let adjudication = &case["adjudication"];
        let replay = &case["replay"];
        let direction = case["expected_direction"].as_str().unwrap_or("?");
        let confirmed = adjudication["state"].as_str() == Some("adjudicated")
            && adjudication["verdict"].as_str() == Some("exposed")
            && adjudication["false_exposed"].as_str() == Some("true")
            && OVER_CREDIT_DIRECTIONS.contains(&direction)
            && replay["identity_current"].as_bool() == Some(true);
        if !confirmed {
            continue;
        }
        let case_id = case["case_id"]
            .as_str()
            .ok_or_else(|| format!("derived report case carries no case id: {case}"))?;
        proposals.push(json!({
            "schema_version": FEEDBACK_SCHEMA_VERSION,
            "kind": FEEDBACK_PROPOSAL_KIND,
            "spec": SPEC,
            "case_id": case_id,
            "authority_boundary": AUTHORITY_BOUNDARY,
            "confirmed_over_credit": {
                "verdict": adjudication["verdict"].clone(),
                "false_exposed": adjudication["false_exposed"].clone(),
                "roles": adjudication["roles"].clone(),
                "expected_direction": case["expected_direction"].clone(),
                "row_kind": case["row_kind"].clone(),
                "behavior_family": case["behavior_family"].clone(),
            },
            "provenance": {
                "panel_digest": report["as_of"]["panel_digest"].clone(),
                "source_envelope": case["source_envelope"].clone(),
                "diff_path": case["diff_path"].clone(),
                "adjudication_record": adjudication["record"].clone(),
                "replay_record": replay["record"].clone(),
                "replay_candidate_classification": replay["candidate_classification"].clone(),
            },
            "promotion_recipe": {
                "step_1": "extract a minimal REGULAR fixture (with golden expected/check.json) reproducing this row's false-promotion shape; the retained diff at the provenance diff_path is the extraction source — manifest-only fixture dirs are rejected by the SPEC-0108 meta-gate",
                "step_2": "add a corpus case to fixtures/evidence-promotion-honesty-corpus/corpus.json with assertions [\"must_not_promote\"] (plus expected_finding_count where the shape pins one)",
                "step_3": "run cargo xtask check-evidence-promotion-honesty — it must pass; a failing promotion case means the analyzer genuinely over-promotes and the analyzer fix lands in this same PR",
                "step_4": "re-bless affected goldens in the same PR with zero unexplained drift",
            },
        }));
    }
    Ok(proposals)
}

/// Derives the staging bytes deterministically: `index.json` first, then one
/// proposal per case in case-id order. `generated_at` is the single
/// disclosed non-deterministic field.
pub(crate) fn derive_staging(proposals: &[Value]) -> Result<StagedFeedback, String> {
    let mut files = Vec::new();
    let mut proposal_files = Vec::new();
    for proposal in proposals {
        let case_id = proposal["case_id"].as_str().ok_or("proposal case id")?;
        let file_name = format!("{}.proposal.json", stable_case_slug(case_id));
        files.push((file_name.clone(), pretty_bytes(proposal)?));
        proposal_files.push(file_name);
    }
    let index = json!({
        "schema_version": FEEDBACK_SCHEMA_VERSION,
        "kind": FEEDBACK_INDEX_KIND,
        "spec": SPEC,
        "generated_at": generated_at()?,
        "confirmed_over_credits": proposals.len(),
        "proposal_files": proposal_files,
        "authority_boundary": AUTHORITY_BOUNDARY,
        "note": "staged proposals never write the evidence-promotion corpus; promotion is a human-reviewed PR following each proposal's recipe",
    });
    files.push(("index.json".to_string(), pretty_bytes(&index)?));
    Ok(StagedFeedback {
        proposal_files,
        confirmed_over_credits: proposals.len(),
        files,
    })
}

/// Writes the tool-owned staging: cleared first, then every derived file.
pub(crate) fn write_staging(out_dir: &Path, staged: &StagedFeedback) -> Result<(), String> {
    if out_dir.exists() {
        fs::remove_dir_all(out_dir)
            .map_err(|error| format!("clear feedback staging `{}`: {error}", out_dir.display()))?;
    }
    fs::create_dir_all(out_dir)
        .map_err(|error| format!("create feedback staging `{}`: {error}", out_dir.display()))?;
    for (file_name, bytes) in &staged.files {
        fs::write(out_dir.join(file_name), bytes).map_err(|error| {
            format!(
                "write feedback staging `{}`: {error}",
                out_dir.join(file_name).display()
            )
        })?;
    }
    Ok(())
}

/// `--check`: the staged files must match a fresh derivation byte-for-byte
/// except the disclosed `generated_at` field, which is stripped from both
/// sides before comparison.
pub(crate) fn verify_staged(out_dir: &Path, staged: &StagedFeedback) -> Result<(), String> {
    for (file_name, fresh_bytes) in &staged.files {
        let path = out_dir.join(file_name);
        let staged_bytes = fs::read(&path).map_err(|error| {
            format!(
                "read staged feedback `{}`: {error}; run `cargo xtask python-judged-panel feedback` first\nrerun: {RERUN}",
                path.display()
            )
        })?;
        if strip_generated_at(&PathBuf::from(file_name), &staged_bytes)?
            != strip_generated_at(&PathBuf::from(file_name), fresh_bytes)?
        {
            return Err(format!(
                "staged feedback `{}` does not match a fresh derivation; re-run `cargo xtask python-judged-panel feedback` to restage\nrerun: {RERUN}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn strip_generated_at(file_name: &Path, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| {
        format!(
            "feedback staging `{}` is not UTF-8: {error}",
            file_name.display()
        )
    })?;
    let mut value = serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("parse feedback staging `{}`: {error}", file_name.display()))?;
    if let Some(object) = value.as_object_mut() {
        object.remove("generated_at");
    }
    serde_json::to_vec_pretty(&value).map_err(|error| format!("re-serialize staging: {error}"))
}

fn pretty_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let mut text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("serialize feedback staging: {error}"))?;
    text.push('\n');
    Ok(text.into_bytes())
}

fn generated_at() -> Result<String, String> {
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| {
            format!(
                "system clock is before the Unix epoch ({error}); refusing to stamp feedback provenance with a fabricated timestamp"
            )
        })?
        .as_secs() as i64;
    Ok(crate::branch_inventory::rfc3339_from_epoch_seconds(epoch))
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires a value\nrerun: {RERUN}"))?;
    *index += 1;
    Ok(value.clone())
}

#[cfg(test)]
mod tests;
