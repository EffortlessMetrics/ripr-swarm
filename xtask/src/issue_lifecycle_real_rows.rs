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
}
