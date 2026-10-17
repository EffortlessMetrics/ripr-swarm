//! Real issue-lifecycle closeout rows (#4932, RIPR-SPEC-0278).
//!
//! The RIPR-SPEC-0218 counting law landed with thirteen synthetic mechanics
//! scenarios and zero real rows. This module pins the first two REAL
//! captured rows admitted by #4932 (premise repaired 2026-10-17): one
//! complete issue-to-merge closeout journey (issue #7795 -> PR #8285 ->
//! squash merge `f3fa41d265...` -> issue CLOSED -> current-head verified ->
//! closeout completed) and one live negative control (PR #8325 merged while
//! parent #1706 stays OPEN with uncovered acceptance -> the row stays
//! `merged_pending_closeout`). Every bound identity was verified with
//! read-only `gh` commands at capture time; the commands and the immutable
//! snapshot bytes live in `fixtures/issue_lifecycle_real_rows/` and are
//! recomputed fail-closed by `cargo xtask check-fixture-contracts`.
//!
//! The adapter runs no triage, selects no work and mutates nothing: it
//! loads the committed corpus, assesses each row through the unchanged
//! RIPR-SPEC-0218 validator, and pins the required controls with in-test
//! fabricated variants (never committed).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::issue_lifecycle_attempt::{
    IssueLifecycleCorpusV1, IssueLifecycleDispositionV1, assess_issue_lifecycle_attempt,
    load_issue_lifecycle_corpus,
};

pub(crate) const ISSUE_LIFECYCLE_REAL_ROWS_SPEC: &str = "RIPR-SPEC-0278";
pub(crate) const ISSUE_LIFECYCLE_REAL_ROWS_ROOT: &str = "fixtures/issue_lifecycle_real_rows";

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

/// Load the committed real-rows corpus (plain `issue_lifecycle_corpus.v1`).
pub(crate) fn load_real_rows_corpus() -> Result<IssueLifecycleCorpusV1, String> {
    let body =
        fs::read_to_string(workspace_path(ISSUE_LIFECYCLE_REAL_ROWS_ROOT).join("corpus.json"))
            .map_err(|error| format!("read real issue lifecycle corpus: {error}"))?;
    load_issue_lifecycle_corpus(&body)
}

/// One committed provenance entry for a captured snapshot file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleRealRowsSnapshotV1 {
    pub file: String,
    pub sha256_lf: String,
    pub bytes_lf: u64,
}

/// One committed provenance entry binding one real row.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleRealRowsProvenanceRowV1 {
    pub lifecycle_id: String,
    pub observation_key: String,
    pub synthetic: bool,
    pub row_digest: String,
    pub issue: String,
    pub pr: Option<String>,
    pub merge_commit: Option<String>,
    pub disposition: IssueLifecycleDispositionV1,
}

/// The committed capture provenance (#4932): capture timestamp, base main,
/// the read-only verification commands and the LF-normalized digest of
/// every snapshot file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleRealRowsProvenanceV1 {
    pub schema_version: String,
    pub repository: String,
    pub captured_at: String,
    pub base_main: String,
    pub capture_method: String,
    pub digest_normalization: String,
    pub verification_commands: Vec<String>,
    pub snapshots: Vec<IssueLifecycleRealRowsSnapshotV1>,
    pub rows: Vec<IssueLifecycleRealRowsProvenanceRowV1>,
}

pub(crate) const ISSUE_LIFECYCLE_REAL_ROWS_PROVENANCE_SCHEMA_VERSION: &str =
    "issue_lifecycle_real_rows_provenance.v1";

/// Load and parse the committed provenance file.
pub(crate) fn load_real_rows_provenance() -> Result<IssueLifecycleRealRowsProvenanceV1, String> {
    let body =
        fs::read_to_string(workspace_path(ISSUE_LIFECYCLE_REAL_ROWS_ROOT).join("provenance.json"))
            .map_err(|error| format!("read real rows provenance: {error}"))?;
    let provenance: IssueLifecycleRealRowsProvenanceV1 = serde_json::from_str(&body)
        .map_err(|error| format!("parse real rows provenance: {error}"))?;
    if provenance.schema_version != ISSUE_LIFECYCLE_REAL_ROWS_PROVENANCE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported real rows provenance schema `{}`",
            provenance.schema_version
        ));
    }
    if provenance.repository.trim().is_empty()
        || provenance.captured_at.trim().is_empty()
        || provenance.base_main.trim().is_empty()
    {
        return Err(
            "real rows provenance must record repository, captured_at and base_main".to_string(),
        );
    }
    if provenance.capture_method.trim().is_empty() {
        return Err("real rows provenance must record the capture method".to_string());
    }
    if provenance.verification_commands.is_empty() {
        return Err("real rows provenance must record the verification commands".to_string());
    }
    if !provenance
        .digest_normalization
        .to_ascii_lowercase()
        .contains("lf")
    {
        return Err(
            "real rows provenance must record the LF digest normalization rule".to_string(),
        );
    }
    Ok(provenance)
}

/// SHA-256 hex over the LF-normalized bytes of one snapshot. Git stores LF
/// for these captures per `.gitattributes * text=auto eol=lf`, so CI
/// recomputes over LF and a CRLF rewrite or a content edit must reject.
pub(crate) fn real_rows_snapshot_digest(root: &Path, file: &str) -> Result<(String, u64), String> {
    let bytes = fs::read(root.join("snapshots").join(file))
        .map_err(|error| format!("read snapshot {file}: {error}"))?;
    // LF-normalize before hashing: git stores these captures as LF per
    // `.gitattributes * text=auto eol=lf`, so CI recomputes over LF bytes
    // and a CRLF working-tree rewrite must not drift the identity.
    let mut lf = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            lf.push(b'\n');
            index += 2;
        } else {
            lf.push(bytes[index]);
            index += 1;
        }
    }
    Ok((crate::blind_journey::sha256_hex(&lf), lf.len() as u64))
}

/// Recursively collect the values of `mergedAt`/`closedAt` keys from a
/// snapshot JSON body. These are GitHub's ISO-8601 UTC timestamps; as
/// `YYYY-MM-DDTHH:MM:SSZ` strings they order lexicographically, so no date
/// library is needed for the consistency check.
fn collect_event_timestamps(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "mergedAt" | "closedAt")
                    && let Some(timestamp) = value.as_str()
                {
                    out.push(timestamp.to_string());
                }
                collect_event_timestamps(value, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_event_timestamps(item, out);
            }
        }
        _ => {}
    }
}

/// Fail-closed provenance binding for the whole corpus: snapshot digests,
/// row digests, observation keys and lifecycle ids must all agree with the
/// committed captures; both rows must count at their recorded dispositions
/// and no row may claim `synthetic`.
pub(crate) fn verify_real_rows_bindings(
    root: &Path,
    corpus: &IssueLifecycleCorpusV1,
    provenance: &IssueLifecycleRealRowsProvenanceV1,
) -> Result<(), String> {
    let snapshot_dir = root.join("snapshots");
    if !snapshot_dir.is_dir() {
        return Err("real rows corpus is missing its snapshots directory".to_string());
    }
    let mut latest_event: Option<String> = None;
    for snapshot in &provenance.snapshots {
        let (computed, bytes) = real_rows_snapshot_digest(root, &snapshot.file)?;
        if computed != snapshot.sha256_lf {
            return Err(format!(
                "snapshot {} digest drifted: recorded `{}`, recomputed `{computed}`",
                snapshot.file, snapshot.sha256_lf
            ));
        }
        if bytes != snapshot.bytes_lf {
            return Err(format!(
                "snapshot {} byte count drifted: recorded {}, recomputed {bytes}",
                snapshot.file, snapshot.bytes_lf
            ));
        }
        let body = fs::read(snapshot_dir.join(&snapshot.file))
            .map_err(|error| format!("read snapshot {} for timestamps: {error}", snapshot.file))?;
        let value: serde_json::Value = serde_json::from_slice(&body)
            .map_err(|error| format!("parse snapshot {} for timestamps: {error}", snapshot.file))?;
        let mut events = Vec::new();
        collect_event_timestamps(&value, &mut events);
        for event in events {
            if event.len() < 20 || !event.ends_with('Z') {
                return Err(format!(
                    "snapshot {} records a non-UTC mergedAt/closedAt `{event}`",
                    snapshot.file
                ));
            }
            if latest_event
                .as_deref()
                .is_none_or(|latest| event.as_str() > latest)
            {
                latest_event = Some(event);
            }
        }
    }
    if let Some(latest) = latest_event
        && provenance.captured_at < latest
    {
        return Err(format!(
            "provenance captured_at `{}` predates the latest captured merge/close event `{latest}`; the capture session cannot record an event that had not happened yet",
            provenance.captured_at
        ));
    }
    if provenance.rows.len() != corpus.rows.len() {
        return Err(format!(
            "real rows provenance must bind {} rows, got {}",
            corpus.rows.len(),
            provenance.rows.len()
        ));
    }
    for row in &corpus.rows {
        let assessment = assess_issue_lifecycle_attempt(row);
        let entry = provenance
            .rows
            .iter()
            .find(|entry| entry.lifecycle_id == row.lifecycle_id)
            .ok_or_else(|| {
                format!(
                    "real rows provenance is missing lifecycle `{}`",
                    row.lifecycle_id
                )
            })?;
        if entry.observation_key != row.observation_key {
            return Err(format!(
                "provenance observation key `{}` disagrees with row `{}`",
                entry.observation_key, row.observation_key
            ));
        }
        if entry.synthetic || row.synthetic {
            return Err(format!(
                "real rows corpus must contain no synthetic row, got `{}`",
                row.lifecycle_id
            ));
        }
        if entry.row_digest != row.row_digest {
            return Err(format!(
                "provenance row digest for `{}` disagrees with the committed row",
                row.lifecycle_id
            ));
        }
        if entry.issue != row.issue.issue_ref {
            return Err(format!(
                "provenance issue `{}` disagrees with row `{}` issue `{}`",
                entry.issue, row.lifecycle_id, row.issue.issue_ref
            ));
        }
        if entry.pr.as_deref() != row.execution_refs.pr.as_deref() {
            return Err(format!(
                "provenance pr for `{}` disagrees with the committed row",
                row.lifecycle_id
            ));
        }
        if entry.merge_commit.as_deref() != row.execution_refs.merge.as_deref() {
            return Err(format!(
                "provenance merge commit for `{}` disagrees with the committed row",
                row.lifecycle_id
            ));
        }
        if !assessment.counted {
            return Err(format!(
                "real row `{}` was rejected by the counting law: {:?}",
                row.lifecycle_id, assessment.reasons
            ));
        }
        if assessment.disposition != Some(entry.disposition) {
            return Err(format!(
                "real row `{}` assessed {:?} but provenance records {:?}",
                row.lifecycle_id, assessment.disposition, entry.disposition
            ));
        }
    }
    Ok(())
}

/// The committed fresh-root reconstruction receipt (#4932): a cold-start
/// agent with no prior chat context re-runs the scorecard and row tests
/// offline, re-verifies every live identity the rows bind with read-only
/// `gh` commands, records the observed facts and the fail-closed drift
/// conclusions, and pins that the reconstructed dispositions agree with the
/// committed corpus. The validator binds the receipt against the corpus and
/// the provenance; any identity drift, disposition disagreement, missing
/// row, extra lifecycle or stale timestamp rejects instead of matching.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleFreshRootReceiptLiveIssueV1 {
    pub state: String,
    #[serde(default)]
    pub closed_at: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub unchecked_acceptance_rows: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleFreshRootReceiptLivePrV1 {
    pub state: String,
    pub merged_at: String,
    pub merge_commit: String,
    #[serde(default)]
    pub head: Option<String>,
    #[serde(default)]
    pub checks: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleFreshRootReceiptAncestryV1 {
    pub merge_base: String,
    pub behind_by: u64,
    #[serde(default)]
    pub ahead_by: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleFreshRootReceiptLiveFactsV1 {
    pub issue_7795: IssueLifecycleFreshRootReceiptLiveIssueV1,
    pub pr_8285: IssueLifecycleFreshRootReceiptLivePrV1,
    pub issue_1706: IssueLifecycleFreshRootReceiptLiveIssueV1,
    pub pr_8325: IssueLifecycleFreshRootReceiptLivePrV1,
    pub main_head_at_reconstruction: String,
    pub row_a_merge_ancestry: IssueLifecycleFreshRootReceiptAncestryV1,
    pub row_b_merge_ancestry: IssueLifecycleFreshRootReceiptAncestryV1,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleFreshRootReceiptDispositionV1 {
    pub lifecycle_id: String,
    pub observation_key: String,
    pub issue: String,
    pub pr: String,
    pub merge_commit: String,
    pub corpus_disposition: IssueLifecycleDispositionV1,
    pub reconstructed_disposition: IssueLifecycleDispositionV1,
    pub matches_corpus: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IssueLifecycleFreshRootReceiptV1 {
    pub schema_version: String,
    pub issue: u64,
    pub spec: String,
    pub reconstructed_by: String,
    pub reconstructed_at: String,
    pub branch: String,
    pub base_main: String,
    pub commands: std::collections::BTreeMap<String, Vec<String>>,
    pub live_facts: IssueLifecycleFreshRootReceiptLiveFactsV1,
    pub reconstructed_dispositions: Vec<IssueLifecycleFreshRootReceiptDispositionV1>,
    pub drift_checks: Vec<std::collections::BTreeMap<String, String>>,
    pub result: String,
}

pub(crate) const ISSUE_LIFECYCLE_FRESH_ROOT_RECEIPT_SCHEMA_VERSION: &str =
    "issue_lifecycle_fresh_root_reconstruction.v1";

/// Load and parse the committed fresh-root reconstruction receipt.
pub(crate) fn load_fresh_root_receipt() -> Result<IssueLifecycleFreshRootReceiptV1, String> {
    let body = fs::read_to_string(
        workspace_path(ISSUE_LIFECYCLE_REAL_ROWS_ROOT).join("fresh_root_reconstruction.json"),
    )
    .map_err(|error| format!("read fresh-root reconstruction receipt: {error}"))?;
    let receipt: IssueLifecycleFreshRootReceiptV1 = serde_json::from_str(&body)
        .map_err(|error| format!("parse fresh-root reconstruction receipt: {error}"))?;
    if receipt.schema_version != ISSUE_LIFECYCLE_FRESH_ROOT_RECEIPT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported fresh-root reconstruction receipt schema `{}`",
            receipt.schema_version
        ));
    }
    Ok(receipt)
}

/// Fail-closed binding of the fresh-root reconstruction receipt against the
/// committed corpus and provenance: the receipt must bind exactly the real
/// rows (no synthetic lifecycle may enter), every recorded identity must
/// agree with the corpus, the provenance and the receipt's own live facts,
/// and every reconstructed disposition must equal the disposition the
/// unchanged counting law assesses for the committed row. Any drift —
/// including a live fact that moved since capture — rejects here instead of
/// fabricating a match.
pub(crate) fn verify_fresh_root_receipt(
    corpus: &IssueLifecycleCorpusV1,
    provenance: &IssueLifecycleRealRowsProvenanceV1,
    receipt: &IssueLifecycleFreshRootReceiptV1,
) -> Result<(), String> {
    if receipt.issue != 4932 {
        return Err(format!(
            "fresh-root receipt must bind issue 4932, got {}",
            receipt.issue
        ));
    }
    if !receipt.spec.contains(ISSUE_LIFECYCLE_REAL_ROWS_SPEC) {
        return Err(format!(
            "fresh-root receipt must name its {} decision",
            ISSUE_LIFECYCLE_REAL_ROWS_SPEC
        ));
    }
    if receipt.reconstructed_at.trim().is_empty() || receipt.reconstructed_by.trim().is_empty() {
        return Err("fresh-root receipt must record who reconstructed and when".to_string());
    }
    if receipt.reconstructed_at < provenance.captured_at {
        return Err(format!(
            "fresh-root receipt reconstructed_at `{}` predates the capture provenance `{}`; the receipt cannot record a reconstruction before the capture existed",
            receipt.reconstructed_at, provenance.captured_at
        ));
    }
    if receipt.branch.trim().is_empty()
        || receipt.base_main.len() != 40
        || receipt.result.trim().is_empty()
    {
        return Err(
            "fresh-root receipt must record its branch, its 40-hex base main and its result"
                .to_string(),
        );
    }
    if !receipt
        .base_main
        .chars()
        .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!(
            "fresh-root receipt base_main `{}` is not a 40-hex commit SHA",
            receipt.base_main
        ));
    }
    if receipt.commands.len() < 2
        || receipt
            .commands
            .values()
            .any(|commands| commands.is_empty())
    {
        return Err(
            "fresh-root receipt must record the offline and live verification commands".to_string(),
        );
    }
    if receipt.reconstructed_dispositions.len() != corpus.rows.len() {
        return Err(format!(
            "fresh-root receipt must reconstruct all {} real rows, got {}",
            corpus.rows.len(),
            receipt.reconstructed_dispositions.len()
        ));
    }
    if receipt.drift_checks.is_empty() {
        return Err("fresh-root receipt must record its fail-closed drift checks".to_string());
    }
    let facts = &receipt.live_facts;
    if facts.issue_7795.state != "CLOSED" || facts.issue_1706.state != "OPEN" {
        return Err(format!(
            "fresh-root receipt live facts drifted from the captured outcome: issue 7795 `{}`, issue 1706 `{}`",
            facts.issue_7795.state, facts.issue_1706.state
        ));
    }
    if facts
        .issue_7795
        .closed_at
        .as_deref()
        .is_none_or(str::is_empty)
    {
        return Err(
            "fresh-root receipt must record the observed close timestamp of issue 7795".to_string(),
        );
    }
    if facts.issue_1706.unchecked_acceptance_rows != Some(10) {
        return Err(format!(
            "fresh-root receipt must keep the ten uncovered issue 1706 acceptance rows visible, got {:?}",
            facts.issue_1706.unchecked_acceptance_rows
        ));
    }
    if !facts
        .issue_1706
        .labels
        .iter()
        .any(|label| label == "status/needs-work")
    {
        return Err(
            "fresh-root receipt must record the observed status/needs-work label on issue 1706"
                .to_string(),
        );
    }
    if facts.pr_8285.state != "MERGED" || facts.pr_8325.state != "MERGED" {
        return Err(format!(
            "fresh-root receipt live facts drifted from the captured outcome: pr 8285 `{}`, pr 8325 `{}`",
            facts.pr_8285.state, facts.pr_8325.state
        ));
    }
    if facts.pr_8285.merged_at.trim().is_empty() || facts.pr_8325.merged_at.trim().is_empty() {
        return Err(
            "fresh-root receipt must record the observed merge timestamps of both PRs".to_string(),
        );
    }
    if facts.pr_8285.merged_at > receipt.reconstructed_at
        || facts.pr_8325.merged_at > receipt.reconstructed_at
    {
        return Err(
            "fresh-root receipt records a merge after the reconstruction instant; the receipt cannot observe an event that had not happened yet"
                .to_string(),
        );
    }
    if facts
        .issue_7795
        .closed_at
        .as_deref()
        .is_some_and(|closed_at| closed_at < facts.pr_8285.merged_at.as_str())
    {
        return Err(
            "fresh-root receipt records issue 7795 closed before its PR merged; the observation is inconsistent"
                .to_string(),
        );
    }
    if facts.pr_8285.head.as_deref().is_none_or(str::is_empty)
        || facts.pr_8285.checks.as_deref().is_none_or(str::is_empty)
    {
        return Err(
            "fresh-root receipt must record the observed head and check outcome of pr 8285"
                .to_string(),
        );
    }
    if facts.main_head_at_reconstruction.len() != 40
        || !facts
            .main_head_at_reconstruction
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(format!(
            "fresh-root receipt main_head_at_reconstruction `{}` is not a 40-hex commit SHA",
            facts.main_head_at_reconstruction
        ));
    }
    if facts.row_a_merge_ancestry.behind_by != 0 || facts.row_b_merge_ancestry.behind_by != 0 {
        return Err(
            "fresh-root receipt ancestry observations must keep both merge commits on the recorded main head (behind_by 0); a merge that left main ancestry breaks the current-head identity"
                .to_string(),
        );
    }
    if facts.row_a_merge_ancestry.ahead_by == Some(0)
        || facts.row_b_merge_ancestry.ahead_by == Some(0)
    {
        return Err(
            "fresh-root receipt ancestry observations must record main strictly ahead of each merge commit"
                .to_string(),
        );
    }
    if facts.row_a_merge_ancestry.merge_base != facts.pr_8285.merge_commit
        || facts.row_b_merge_ancestry.merge_base != facts.pr_8325.merge_commit
    {
        return Err(
            "fresh-root receipt ancestry observations must name the recorded merge commits as merge base"
                .to_string(),
        );
    }
    for row in &corpus.rows {
        if row.synthetic {
            return Err(format!(
                "fresh-root receipt binding found a synthetic row `{}`; synthetic lifecycles must never enter the real reconstruction",
                row.lifecycle_id
            ));
        }
        let entry = receipt
            .reconstructed_dispositions
            .iter()
            .find(|entry| entry.lifecycle_id == row.lifecycle_id)
            .ok_or_else(|| {
                format!(
                    "fresh-root receipt is missing reconstruction of lifecycle `{}`",
                    row.lifecycle_id
                )
            })?;
        let provenance_row = provenance
            .rows
            .iter()
            .find(|candidate| candidate.lifecycle_id == row.lifecycle_id)
            .ok_or_else(|| {
                format!(
                    "capture provenance is missing lifecycle `{}`",
                    row.lifecycle_id
                )
            })?;
        if entry.observation_key != row.observation_key
            || entry.issue != row.issue.issue_ref
            || Some(entry.pr.as_str()) != row.execution_refs.pr.as_deref()
            || Some(entry.merge_commit.as_str()) != row.execution_refs.merge.as_deref()
        {
            return Err(format!(
                "fresh-root receipt identity for `{}` disagrees with the committed row",
                row.lifecycle_id
            ));
        }
        if Some(entry.merge_commit.as_str()) != provenance_row.merge_commit.as_deref() {
            return Err(format!(
                "fresh-root receipt merge identity for `{}` disagrees with the capture provenance",
                row.lifecycle_id
            ));
        }
        let assessment = assess_issue_lifecycle_attempt(row);
        if assessment.disposition != Some(entry.reconstructed_disposition) {
            return Err(format!(
                "fresh-root receipt reconstructed `{:?}` for `{}` but the counting law assesses `{:?}`",
                entry.reconstructed_disposition, row.lifecycle_id, assessment.disposition
            ));
        }
        if entry.corpus_disposition != provenance_row.disposition
            || !entry.matches_corpus
            || entry.reconstructed_disposition != entry.corpus_disposition
        {
            return Err(format!(
                "fresh-root receipt for `{}` does not match the committed corpus; record the drift instead of fabricating a match",
                row.lifecycle_id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_lifecycle_attempt::{
        IssueLifecycleAttemptV1, IssueLifecycleCloseoutStateV1, IssueLifecycleFixtureCorpusV1,
        IssueLifecycleRowAssessmentV1, load_issue_lifecycle_fixture_corpus,
    };
    use crate::reports::build_issue_lifecycle_scorecard;

    const ROW_A_LIFECYCLE_ID: &str = "lifecycle-real-7795-8285-closeout";
    const ROW_B_LIFECYCLE_ID: &str = "lifecycle-real-8325-1706-pending-closeout";

    fn real_root() -> PathBuf {
        workspace_path(ISSUE_LIFECYCLE_REAL_ROWS_ROOT)
    }

    fn load_real_rows() -> Result<Vec<IssueLifecycleAttemptV1>, String> {
        Ok(load_real_rows_corpus()?.rows)
    }

    fn row_a(rows: &[IssueLifecycleAttemptV1]) -> Result<IssueLifecycleAttemptV1, String> {
        rows.iter()
            .find(|row| row.lifecycle_id == ROW_A_LIFECYCLE_ID)
            .cloned()
            .ok_or_else(|| format!("committed corpus is missing row {ROW_A_LIFECYCLE_ID}"))
    }

    fn row_b(rows: &[IssueLifecycleAttemptV1]) -> Result<IssueLifecycleAttemptV1, String> {
        rows.iter()
            .find(|row| row.lifecycle_id == ROW_B_LIFECYCLE_ID)
            .cloned()
            .ok_or_else(|| format!("committed corpus is missing row {ROW_B_LIFECYCLE_ID}"))
    }

    fn assessed(row: &IssueLifecycleAttemptV1) -> IssueLifecycleRowAssessmentV1 {
        assess_issue_lifecycle_attempt(row)
    }

    /// Control baseline (adapter contract): both real rows load from the
    /// committed corpus, no intake question caps them, and the provenance
    /// snapshot bindings recompute clean over LF bytes.
    #[test]
    fn both_real_rows_load_with_intact_provenance_bindings() -> Result<(), String> {
        let rows = load_real_rows()?;
        if rows.len() != 2 {
            return Err(format!(
                "the real corpus must contain exactly two rows, got {}",
                rows.len()
            ));
        }
        let provenance = load_real_rows_provenance()?;
        verify_real_rows_bindings(&real_root(), &load_real_rows_corpus()?, &provenance)
    }

    /// Row A dispositions through to the one positive terminal: merge,
    /// current-head verification, acceptance coverage, burn-down and
    /// closeout stay distinct bound identities and the claimed `completed`
    /// survives the unchanged counting law without a downgrade reason.
    #[test]
    fn row_a_dispositions_through_to_completed_closeout() -> Result<(), String> {
        let rows = load_real_rows()?;
        let row = row_a(&rows)?;
        let assessment = assessed(&row);
        if !assessment.counted
            || assessment.disposition != Some(IssueLifecycleDispositionV1::Completed)
        {
            return Err(format!(
                "row A must count as completed, got counted={} disposition={:?} reasons={:?}",
                assessment.counted, assessment.disposition, assessment.reasons
            ));
        }
        if !assessment.reasons.is_empty() {
            return Err(format!(
                "row A must assess without downgrade reasons, got {:?}",
                assessment.reasons
            ));
        }
        if row.closeout.state != IssueLifecycleCloseoutStateV1::Completed
            || row.closeout.current_head_verification.is_none()
            || row.execution_refs.merge.is_none()
            || row.burn_down.uncovered_rows.is_empty() && row.plan.acceptance_coverage.is_empty()
        {
            return Err(
                "row A must bind merge, current-head verification and non-empty acceptance coverage separately"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Row B is the captured negative control: a bound merge identity while
    /// parent issue #1706 stays OPEN with uncovered acceptance rows and no
    /// current-head receipt can never disposition beyond
    /// `merged_pending_closeout` — merge alone cannot close.
    #[test]
    fn row_b_stays_merged_pending_closeout() -> Result<(), String> {
        let rows = load_real_rows()?;
        let row = row_b(&rows)?;
        let assessment = assessed(&row);
        if !assessment.counted
            || assessment.disposition != Some(IssueLifecycleDispositionV1::MergedPendingCloseout)
        {
            return Err(format!(
                "row B must stay merged_pending_closeout, got counted={} disposition={:?} reasons={:?}",
                assessment.counted, assessment.disposition, assessment.reasons
            ));
        }
        if row.execution_refs.merge.is_none()
            || row.closeout.current_head_verification.is_some()
            || row.burn_down.uncovered_rows.is_empty()
        {
            return Err(
                "row B must keep the merge bound, the current-head receipt absent and uncovered rows visible"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Capture-time consistency (review follow-up on #8440): the committed
    /// captured_at must not predate the latest mergedAt/closedAt event any
    /// bound snapshot records; a provenance claiming to capture evidence
    /// before that evidence existed is internally inconsistent and must
    /// reject.
    #[test]
    fn capture_time_predating_a_bound_event_rejects() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let mut provenance = load_real_rows_provenance()?;
        verify_real_rows_bindings(&real_root(), &corpus, &provenance)?;
        provenance.captured_at = "2026-10-16T00:00:00Z".to_string();
        let error = match verify_real_rows_bindings(&real_root(), &corpus, &provenance) {
            Ok(()) => {
                return Err(
                    "a captured_at before every bound merge/close event must reject".to_string(),
                );
            }
            Err(error) => error,
        };
        if !error.contains("predates") {
            return Err(format!("expected a predates rejection, got: {error}"));
        }
        Ok(())
    }

    /// Control 1 (wrong identity): a fabricated wrong-head or wrong-merge
    /// variant of a real row — recorded without restamping the retained
    /// digest — rejects on the row_digest binding and can never count.
    #[test]
    fn wrong_head_or_merge_identity_variant_rejects() -> Result<(), String> {
        let rows = load_real_rows()?;
        let mut wrong_head = row_a(&rows)?;
        wrong_head.execution_refs.pr = Some("pr:EffortlessMetrics/ripr-swarm#9999".to_string());
        let assessment = assessed(&wrong_head);
        if assessment.counted
            || !assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("row_digest mismatch"))
        {
            return Err(format!(
                "a wrong-head variant must reject on the digest binding, got counted={} reasons={:?}",
                assessment.counted, assessment.reasons
            ));
        }
        let mut wrong_merge = row_a(&rows)?;
        wrong_merge.execution_refs.merge =
            Some("gh-merge-commit:0000000000000000000000000000000000000000".to_string());
        let assessment = assessed(&wrong_merge);
        if assessment.counted
            || !assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("row_digest mismatch"))
        {
            return Err(format!(
                "a wrong-merge variant must reject on the digest binding, got counted={} reasons={:?}",
                assessment.counted, assessment.reasons
            ));
        }
        Ok(())
    }

    /// Control 4 (zero-subject verification): a Row A variant whose
    /// verification receipt and every check subject were stripped cannot
    /// keep a completed closeout; the merge-versus-closeout law demotes it
    /// to `merged_pending_closeout`.
    #[test]
    fn zero_subject_verification_cannot_close() -> Result<(), String> {
        let rows = load_real_rows()?;
        let mut row = row_a(&rows)?;
        row.execution_refs.verification = None;
        row.execution_refs.checks = Vec::new();
        row.closeout.current_head_verification = None;
        row.row_digest = crate::issue_lifecycle_attempt::issue_lifecycle_row_digest(&row)?;
        let assessment = assessed(&row);
        if assessment.disposition != Some(IssueLifecycleDispositionV1::MergedPendingCloseout)
            || !assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("current-head"))
        {
            return Err(format!(
                "a zero-subject verification must demote completed to merged_pending_closeout, got {:?} reasons={:?}",
                assessment.disposition, assessment.reasons
            ));
        }
        Ok(())
    }

    /// Control 2 and 5 (merge without current-head proof; stale receipt):
    /// removing the closeout state or rebinding the receipt against an
    /// older main can never close the journey.
    #[test]
    fn missing_or_stale_current_head_proof_blocks_closeout() -> Result<(), String> {
        let rows = load_real_rows()?;
        let mut missing = row_a(&rows)?;
        missing.closeout.state = IssueLifecycleCloseoutStateV1::Pending;
        missing.row_digest = crate::issue_lifecycle_attempt::issue_lifecycle_row_digest(&missing)?;
        let assessment = assessed(&missing);
        if assessment.disposition != Some(IssueLifecycleDispositionV1::MergedPendingCloseout) {
            return Err(format!(
                "a pending closeout with a merge must demote to merged_pending_closeout, got {:?}",
                assessment.disposition
            ));
        }
        let mut stale = row_a(&rows)?;
        stale.execution_refs.current_main = "be4cee9ec702a89918668156c680ab70ad352e68".to_string();
        stale.row_digest = crate::issue_lifecycle_attempt::issue_lifecycle_row_digest(&stale)?;
        let assessment = assessed(&stale);
        if assessment.disposition != Some(IssueLifecycleDispositionV1::Stale)
            || !assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("stale"))
        {
            return Err(format!(
                "a stale current-main receipt must force stale, got {:?} reasons={:?}",
                assessment.disposition, assessment.reasons
            ));
        }
        Ok(())
    }

    /// Control 3 (uncovered acceptance stays partial): Row A with an
    /// uncovered acceptance row and a bound merge demotes to
    /// `partially_landed`, mirroring Row B's live state.
    #[test]
    fn uncovered_acceptance_row_keeps_a_merge_partial() -> Result<(), String> {
        let rows = load_real_rows()?;
        let mut row = row_a(&rows)?;
        row.burn_down.uncovered_rows =
            vec!["acceptance #7795 predicate 5: hosted CI evidence not yet bound".to_string()];
        row.row_digest = crate::issue_lifecycle_attempt::issue_lifecycle_row_digest(&row)?;
        let assessment = assessed(&row);
        if assessment.disposition != Some(IssueLifecycleDispositionV1::PartiallyLanded)
            || !assessment
                .reasons
                .iter()
                .any(|reason| reason.contains("uncovered"))
        {
            return Err(format!(
                "an uncovered acceptance row with a merge must demote to partially_landed, got {:?} reasons={:?}",
                assessment.disposition, assessment.reasons
            ));
        }
        Ok(())
    }

    /// Control 7 (duplicate import): importing the committed real corpus
    /// twice — as two roles would — deduplicates by `observation_key` into
    /// the same two lifecycles with two observations each.
    #[test]
    fn duplicate_import_deduplicates_by_observation_key() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let mut assessed_rows: Vec<IssueLifecycleRowAssessmentV1> = Vec::new();
        for _ in 0..2 {
            assessed_rows.extend(corpus.rows.iter().map(assess_issue_lifecycle_attempt));
        }
        let scorecard = build_issue_lifecycle_scorecard(
            &assessed_rows,
            "real-rows-duplicates".to_string(),
            "fixtures/issue_lifecycle_real_rows/corpus.json",
        );
        if scorecard.real_lifecycles != 2 || !scorecard.rejected_rows.is_empty() {
            return Err(format!(
                "duplicate imports must deduplicate to two lifecycles with no rejects, got real={} rejected={}",
                scorecard.real_lifecycles,
                scorecard.rejected_rows.len()
            ));
        }
        for row in &scorecard.real_rows {
            if row.observations != 2 {
                return Err(format!(
                    "each deduplicated lifecycle must record two observations, got {:?}",
                    row
                ));
            }
        }
        Ok(())
    }

    /// Control 8 (synthetic stays outside the real count) and control 9 (a
    /// pending unrelated lifecycle does not block a valid closeout): the
    /// thirteen synthetic mechanics fixtures and the two real rows assessed
    /// together keep synthetic rows outside every real denominator, count
    /// exactly two real lifecycles with one implementation success, and Row
    /// A stays `completed` even while Row B stays pending beside it.
    #[test]
    fn synthetic_fixtures_stay_outside_the_real_count() -> Result<(), String> {
        let fixture_body = fs::read_to_string(
            workspace_path("fixtures/issue_lifecycle_attempts").join("corpus.json"),
        )
        .map_err(|error| format!("read synthetic fixture corpus: {error}"))?;
        let fixture_corpus: IssueLifecycleFixtureCorpusV1 =
            load_issue_lifecycle_fixture_corpus(&fixture_body)?;
        let real_corpus = load_real_rows_corpus()?;
        let mut assessed_rows: Vec<IssueLifecycleRowAssessmentV1> = Vec::new();
        assessed_rows.extend(
            fixture_corpus
                .scenarios
                .iter()
                .map(|scenario| assess_issue_lifecycle_attempt(&scenario.attempt)),
        );
        assessed_rows.extend(real_corpus.rows.iter().map(assess_issue_lifecycle_attempt));
        let scorecard = build_issue_lifecycle_scorecard(
            &assessed_rows,
            "real-plus-synthetic".to_string(),
            "fixtures/issue_lifecycle_real_rows/corpus.json",
        );
        let countable_synthetic = fixture_corpus
            .scenarios
            .iter()
            .filter(|scenario| scenario.expected.countable)
            .count();
        if scorecard.synthetic_lifecycles != countable_synthetic {
            return Err(format!(
                "every counted synthetic mechanics row must stay outside real denominators, got synthetic={} expected={countable_synthetic}",
                scorecard.synthetic_lifecycles
            ));
        }
        if scorecard.real_lifecycles != 2 || scorecard.implementation_successes != 1 {
            return Err(format!(
                "the combined projection must count two real lifecycles and one success, got real={} successes={}",
                scorecard.real_lifecycles, scorecard.implementation_successes
            ));
        }
        let row_a_projected = scorecard
            .real_rows
            .iter()
            .find(|row| row.lifecycle_id == ROW_A_LIFECYCLE_ID)
            .ok_or_else(|| "row A must stay visible in the combined projection".to_string())?;
        if !row_a_projected.implementation_success {
            return Err("row A must stay completed beside the pending Row B".to_string());
        }
        Ok(())
    }

    /// Acceptance box "a fresh root reconstructs the outcome without prior
    /// chat": the committed reconstruction receipt loads and binds against
    /// the committed corpus and capture provenance — identity binding,
    /// disposition agreement through the unchanged counting law, and every
    /// row real (no synthetic lifecycle enters the reconstruction).
    #[test]
    fn fresh_root_reconstruction_receipt_matches_the_committed_corpus() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let provenance = load_real_rows_provenance()?;
        verify_real_rows_bindings(&real_root(), &corpus, &provenance)?;
        let receipt = load_fresh_root_receipt()?;
        verify_fresh_root_receipt(&corpus, &provenance, &receipt)
    }

    /// Fail-closed drift: a receipt recording a drifted live fact (issue
    /// #1706 closed) must reject instead of fabricating agreement with the
    /// corpus.
    #[test]
    fn fresh_root_receipt_with_drifted_live_fact_rejects() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let provenance = load_real_rows_provenance()?;
        let mut receipt = load_fresh_root_receipt()?;
        receipt.live_facts.issue_1706.state = "CLOSED".to_string();
        let error = match verify_fresh_root_receipt(&corpus, &provenance, &receipt) {
            Ok(()) => {
                return Err(
                    "a receipt claiming issue 1706 closed must reject as drifted".to_string(),
                );
            }
            Err(error) => error,
        };
        if !error.contains("drifted") {
            return Err(format!("expected a drifted rejection, got: {error}"));
        }
        Ok(())
    }

    /// Fail-closed disagreement: a receipt reconstructing a disposition the
    /// counting law does not assess for the committed row must reject, even
    /// when every identity still binds.
    #[test]
    fn fresh_root_receipt_with_disposition_disagreement_rejects() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let provenance = load_real_rows_provenance()?;
        let mut receipt = load_fresh_root_receipt()?;
        for entry in &mut receipt.reconstructed_dispositions {
            if entry.lifecycle_id == ROW_B_LIFECYCLE_ID {
                entry.reconstructed_disposition = IssueLifecycleDispositionV1::Completed;
                entry.matches_corpus = true;
            }
        }
        match verify_fresh_root_receipt(&corpus, &provenance, &receipt) {
            Ok(()) => Err(
                "a receipt upgrading Row B to completed must reject against the counting law"
                    .to_string(),
            ),
            Err(_) => Ok(()),
        }
    }

    /// Fail-closed contamination: a receipt reconstructing a lifecycle the
    /// committed corpus does not contain (a synthetic import sneaking into
    /// the real reconstruction) must reject on the row-count binding.
    #[test]
    fn fresh_root_receipt_with_unknown_lifecycle_rejects() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let provenance = load_real_rows_provenance()?;
        let mut receipt = load_fresh_root_receipt()?;
        receipt
            .reconstructed_dispositions
            .push(IssueLifecycleFreshRootReceiptDispositionV1 {
                lifecycle_id: "lifecycle-synthetic-import".to_string(),
                observation_key: "issue-lifecycle:synthetic".to_string(),
                issue: "EffortlessMetrics/ripr-swarm#0".to_string(),
                pr: "pr:EffortlessMetrics/ripr-swarm#0".to_string(),
                merge_commit: "gh-merge-commit:0000000000000000000000000000000000000000"
                    .to_string(),
                corpus_disposition: IssueLifecycleDispositionV1::Completed,
                reconstructed_disposition: IssueLifecycleDispositionV1::Completed,
                matches_corpus: true,
            });
        match verify_fresh_root_receipt(&corpus, &provenance, &receipt) {
            Ok(()) => Err(
                "a receipt binding a lifecycle outside the committed corpus must reject"
                    .to_string(),
            ),
            Err(_) => Ok(()),
        }
    }

    /// Fail-closed ordering: a receipt claiming reconstruction before the
    /// capture provenance existed must reject, mirroring the captured_at
    /// consistency law.
    #[test]
    fn fresh_root_receipt_predating_the_capture_rejects() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let provenance = load_real_rows_provenance()?;
        let mut receipt = load_fresh_root_receipt()?;
        receipt.reconstructed_at = "2026-10-16T00:00:00Z".to_string();
        let error = match verify_fresh_root_receipt(&corpus, &provenance, &receipt) {
            Ok(()) => {
                return Err("a receipt predating the capture provenance must reject".to_string());
            }
            Err(error) => error,
        };
        if !error.contains("predates") {
            return Err(format!("expected a predates rejection, got: {error}"));
        }
        Ok(())
    }
}
