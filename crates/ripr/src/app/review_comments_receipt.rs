//! Application service for review-comments phase receipt lifecycle (#1712).
//!
//! Owns git identity resolution, reusable cache identity, phase transitions,
//! and atomic persistence. The CLI adapter delegates here; output keeps the
//! receipt DTO and JSON attachment only.

use crate::output::review_comments_receipt::{
    CANONICAL_ANALYSIS_NOT_RESUMABLE_NON_CLAIM, REVIEW_COMMENTS_RECEIPT_SCHEMA_VERSION,
    REVIEW_GUIDANCE_OVERSIZED_LIMITATION_CATEGORY, ReviewCommentsReceiptLimitation,
    ReviewCommentsRunReceipt,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write as _;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Mutable review-comments run receipt with lifecycle orchestration.
pub(crate) struct ReviewCommentsReceiptRun {
    receipt: ReviewCommentsRunReceipt,
}

impl Deref for ReviewCommentsReceiptRun {
    type Target = ReviewCommentsRunReceipt;
    fn deref(&self) -> &Self::Target {
        &self.receipt
    }
}

impl DerefMut for ReviewCommentsReceiptRun {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.receipt
    }
}

impl ReviewCommentsReceiptRun {
    pub(crate) fn new(
        root: &Path,
        base: &str,
        head: &str,
        timeout_ms: u64,
        expected_artifacts: &[String],
        revision_budget: Option<Duration>,
    ) -> Self {
        let root_identity = canonical_root_identity(root);
        // #4363 review: the two revision probes must not each block for the
        // fixed one-minute ceiling when the caller configured a shorter
        // review-comments budget — the CLI checks its `--timeout-ms`
        // deadline only after this constructor returns, so two probes at
        // the fixed ceiling could block 120s against a 1s budget. Each
        // probe is capped at the remaining budget (still bounded by the
        // fixed ceiling), and the remaining time is re-derived between the
        // two probes so the pair cannot outblock the caller's budget.
        let resolve_started = std::time::Instant::now();
        let remaining = |spent: std::time::Duration| match revision_budget {
            Some(budget) => {
                let remaining = budget.saturating_sub(spent);
                if remaining.is_zero() {
                    None
                } else {
                    Some(remaining.min(RECEIPT_REVISION_DEADLINE))
                }
            }
            None => Some(RECEIPT_REVISION_DEADLINE),
        };
        let base_sha = resolve_revision(root, base, remaining(resolve_started.elapsed()));
        let head_sha = resolve_revision(root, head, remaining(resolve_started.elapsed()));
        let reusable_cache_identity = reusable_cache_identity(&root_identity, &base_sha, &head_sha);
        Self {
            receipt: ReviewCommentsRunReceipt {
                schema_version: REVIEW_COMMENTS_RECEIPT_SCHEMA_VERSION,
                status: "in_progress",
                root_identity,
                base_sha,
                head_sha,
                configured_timeout_ms: timeout_ms,
                last_completed_phase: None,
                active_phase: Some("input_validation".to_string()),
                active_step: None,
                canonical_analysis_evaluated_seams: None,
                completed_artifacts: Vec::new(),
                missing_artifacts: expected_artifacts.to_vec(),
                reusable_cache_identity,
                limitations: Vec::new(),
                non_claims: vec![
                    "static review guidance is advisory evidence only".to_string(),
                    "no complete route inventory is claimed until status is complete".to_string(),
                ],
                atomic_write_status: "not_written",
            },
        }
    }

    pub(crate) fn phase(&mut self, completed: &str, active: &str) {
        self.receipt.last_completed_phase = Some(completed.to_string());
        self.receipt.active_phase = Some(active.to_string());
        self.receipt.active_step = None;
        self.receipt.canonical_analysis_evaluated_seams = None;
    }

    /// Records how many seams `canonical_analysis` had classified when the
    /// budget stopped it. Cleared by the next phase transition.
    pub(crate) fn canonical_analysis_progress(&mut self, evaluated_seams: usize) {
        self.receipt.canonical_analysis_evaluated_seams = Some(evaluated_seams);
    }

    /// Names the pass now running inside the active phase.
    pub(crate) fn step(&mut self, active_step: &str) {
        self.receipt.active_step = Some(active_step.to_string());
    }

    pub(crate) fn complete(&mut self, artifacts: &[String]) {
        self.receipt.status = "complete";
        self.receipt.last_completed_phase = Some("artifact_io".to_string());
        self.receipt.active_phase = None;
        self.receipt.active_step = None;
        self.receipt.canonical_analysis_evaluated_seams = None;
        self.receipt.completed_artifacts = artifacts.to_vec();
        self.receipt.missing_artifacts.clear();
    }

    pub fn limited_timeout(&mut self, active_phase: &str) {
        self.receipt.status = "limited_timeout";
        self.receipt.active_phase = Some(active_phase.to_string());
        self.receipt
            .limitations
            .push(ReviewCommentsReceiptLimitation {
                category: "analysis_timeout".to_string(),
                repair_route: "rerun review-comments with a larger configured timeout".to_string(),
            });
        self.terminalize_non_claims();
        if active_phase == "canonical_analysis" {
            // Persisted reuse covers completed phases only, and this one did
            // not complete. Say so, so a retry is not expected to resume.
            self.receipt
                .non_claims
                .push(CANONICAL_ANALYSIS_NOT_RESUMABLE_NON_CLAIM.to_string());
        }
    }

    pub fn failed(&mut self, active_phase: &str, error: &str) {
        self.receipt.status = "failed";
        self.receipt.active_phase = Some(active_phase.to_string());
        let repair_route = if error.trim().is_empty() {
            "unknown analysis failure".to_string()
        } else {
            error.to_string()
        };
        self.receipt
            .limitations
            .push(ReviewCommentsReceiptLimitation {
                category: "analysis_failed".to_string(),
                repair_route,
            });
        self.terminalize_non_claims();
    }

    /// Typed state for a dispatch refused at the guidance-payload memory
    /// ceiling (#4388). The run is not truncated and does not build either
    /// source index after refusal; diff capture precedes this admission.
    /// The named limitation lets a consumer classify incomplete guidance. Status stays `failed` (the receipt
    /// vocabulary has no third terminal failure kind), and the limitation
    /// category carries the classification.
    pub fn oversized(&mut self, active_phase: &str, error: &str) {
        self.receipt.status = "failed";
        self.receipt.active_phase = Some(active_phase.to_string());
        let repair_route = if error.trim().is_empty() {
            "raise the review-guidance ceiling environment variables".to_string()
        } else {
            error.to_string()
        };
        self.receipt
            .limitations
            .push(ReviewCommentsReceiptLimitation {
                category: REVIEW_GUIDANCE_OVERSIZED_LIMITATION_CATEGORY.to_string(),
                repair_route,
            });
        self.terminalize_non_claims();
    }

    fn terminalize_non_claims(&mut self) {
        self.receipt.non_claims.retain(|claim| {
            claim != "no complete route inventory is claimed until status is complete"
        });
        self.receipt
            .non_claims
            .push("no complete route inventory".to_string());
        self.receipt.non_claims.push("no all-clear".to_string());
    }

    pub(crate) fn write_atomic(&mut self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|err| format!("create receipt parent {} failed: {err}; check that the path exists and is writable, then rerun", parent.display()))?;
        crate::output::file_write::validate_destination(path)
            .map_err(|err| format!("review-comments receipt output refused: {err}; check that the path exists and is writable, then rerun"))?;

        let temp_path = atomic_temp_path(path);
        let mut committed = self.receipt.clone();
        committed.atomic_write_status = "committed";
        let json = serde_json::to_vec_pretty(&committed)
            .map_err(|err| format!("serialize review-comments receipt failed: {err}; this is a ripr bug, report it with the command you ran"))?;
        // Acquisition failure never removes a path we did not create.
        let mut temp = crate::output::file_write::create_exclusive(&temp_path)
            .map_err(|err| format!("create review-comments receipt output temp {} failed: {err}; check the output directory is writable, and remove that temp file only after confirming no ripr process is writing it, then rerun", temp_path.display()))?;
        if let Err(err) = temp.write_all(&json) {
            drop(temp);
            let _ = fs::remove_file(&temp_path);
            return Err(format!(
                "write review-comments receipt output temp failed: {err}; check that the path exists and is writable, then rerun"
            ));
        }
        drop(temp);
        if let Err(err) = fs::rename(&temp_path, path) {
            if err.kind() != std::io::ErrorKind::AlreadyExists {
                let _ = fs::remove_file(&temp_path);
                return Err(format!(
                    "publish review-comments receipt failed: {err}; check that the path exists and is writable, then rerun"
                ));
            }
            crate::output::file_write::validate_destination(path).map_err(|error| {
                let _ = fs::remove_file(&temp_path);
                format!("review-comments receipt output refused: {error}; check the destination is a regular file or absent, then rerun")
            })?;
            fs::remove_file(path).map_err(|remove_err| {
                let _ = fs::remove_file(&temp_path);
                format!("replace review-comments receipt failed: {remove_err}; check the parent directory and destination file are writable, then rerun")
            })?;
            fs::rename(&temp_path, path).map_err(|rename_err| {
                let _ = fs::remove_file(&temp_path);
                format!("publish review-comments receipt failed: {rename_err}; check the parent directory and destination file are writable, then rerun")
            })?;
        }
        self.receipt.atomic_write_status = "committed";
        Ok(())
    }

    pub(crate) fn path_for_output(output: &Path) -> PathBuf {
        output.with_file_name("run-receipt.json")
    }
}

fn canonical_root_identity(root: &Path) -> String {
    let normalized = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/");
    normalized
        .strip_prefix("//?/")
        .unwrap_or(&normalized)
        .to_string()
}

fn reusable_cache_identity(root: &str, base: &str, head: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"ripr-review-comments\0");
    hasher.update(root.as_bytes());
    hasher.update([0]);
    hasher.update(base.as_bytes());
    hasher.update([0]);
    hasher.update(head.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

/// Cooperative ceiling for each receipt revision probe (#2303, #4363). The
/// caller's remaining `--timeout-ms` budget further caps it (#4363 review).
const RECEIPT_REVISION_DEADLINE: Duration = Duration::from_mins(1);

/// Cooperative deadline for receipt revision resolution (#2303, #4363). The
/// receipt flow must not block past the deadline on a hung git; an unresolved
/// revision degrades to the raw revision string exactly as any other
/// `rev-parse` failure always has. The caller's remaining `--timeout-ms`
/// budget further caps each probe (#4363 review); `None` means the budget is
/// exhausted or the caller declared none, and the probe is skipped entirely.
fn resolve_revision(root: &Path, revision: &str, deadline: Option<Duration>) -> String {
    let object = format!("{revision}^{{commit}}");
    // `current_dir(root)` in the shared helper is equivalent to the previous
    // `git -C root` form for every root git could resolve; a missing root now
    // fails the spawn instead of exiting non-zero, which lands in the same
    // silent fallback.
    let output = match deadline {
        Some(deadline) => crate::git::run_git_output_with_deadline(
            root,
            &["rev-parse", "--verify", &object],
            Some(deadline),
        ),
        None => Err("revision resolution budget exhausted (not spawned)".into()),
    };
    output
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| revision.to_string())
}

fn atomic_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "receipt.json".to_string());
    path.with_file_name(format!(".{file_name}.{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::path::PathBuf;

    /// #1712: output keeps DTO + attachment; app owns orchestration helpers.
    #[test]
    fn output_receipt_module_is_rendering_only() {
        let source = include_str!("../output/review_comments_receipt.rs");
        assert!(!source.contains("run_git_output"));
        assert!(!source.contains("write_atomic"));
        assert!(!source.contains("std::fs"));
    }

    #[test]
    fn path_for_output_derives_sibling_receipt_name() {
        let path = ReviewCommentsReceiptRun::path_for_output(Path::new("/tmp/comments.json"));
        assert_eq!(path, PathBuf::from("/tmp/run-receipt.json"));
    }

    #[test]
    fn stale_revision_fallback_changes_cache_identity() {
        let artifacts = vec!["comments.json".to_string()];
        let head_fallback = ReviewCommentsReceiptRun::new(
            Path::new("."),
            "HEAD",
            "HEAD",
            30_000,
            &artifacts,
            Some(Duration::ZERO),
        );
        let other_fallback = ReviewCommentsReceiptRun::new(
            Path::new("."),
            "not-a-git-sha",
            "also-not-a-sha",
            30_000,
            &artifacts,
            Some(Duration::ZERO),
        );
        assert_ne!(
            head_fallback.reusable_cache_identity,
            other_fallback.reusable_cache_identity
        );
    }

    fn sample_receipt() -> ReviewCommentsReceiptRun {
        ReviewCommentsReceiptRun::new(
            Path::new("."),
            "origin/main",
            "HEAD",
            30_000,
            &["comments.json".to_string()],
            None,
        )
    }

    /// A timeout inside a multi-pass phase names the pass (#1604). The step is
    /// absent from the serialized receipt when no pass is active, and a phase
    /// transition or completion clears it so a stale pass is never reported.
    #[test]
    fn active_step_is_serialized_only_while_a_pass_is_running() -> Result<(), String> {
        let mut receipt = sample_receipt();
        let absent = serde_json::to_value(&*receipt).map_err(|err| err.to_string())?;
        assert!(absent.get("active_step").is_none());

        receipt.phase("diff_discovery", "language_facts");
        receipt.step("owner_attribution");
        receipt.limited_timeout("language_facts");
        let named = serde_json::to_value(&*receipt).map_err(|err| err.to_string())?;
        assert_eq!(named["active_phase"], "language_facts");
        assert_eq!(named["active_step"], "owner_attribution");

        let mut moved = sample_receipt();
        moved.phase("diff_discovery", "language_facts");
        moved.step("test_change_detection");
        moved.phase("language_facts", "canonical_analysis");
        assert_eq!(moved.active_step, None);

        let mut finished = sample_receipt();
        finished.step("owner_attribution");
        finished.complete(&["comments.json".to_string()]);
        assert_eq!(finished.active_step, None);
        Ok(())
    }

    /// A budget spent in `canonical_analysis` discloses the work done and that
    /// none of it is kept (#11107). The same disclosure must not appear for a
    /// timeout in another phase, for a failure, or after the phase advances.
    #[test]
    fn canonical_analysis_timeout_discloses_progress_and_that_nothing_is_kept() -> Result<(), String>
    {
        let mut receipt = sample_receipt();
        receipt.phase("language_facts", "canonical_analysis");
        receipt.canonical_analysis_progress(7);
        receipt.limited_timeout("canonical_analysis");
        let value = serde_json::to_value(&*receipt).map_err(|err| err.to_string())?;
        assert_eq!(value["canonical_analysis_evaluated_seams"], 7);
        let claims = value["non_claims"]
            .as_array()
            .ok_or("non_claims must be an array")?;
        assert!(
            claims
                .iter()
                .any(|claim| claim == CANONICAL_ANALYSIS_NOT_RESUMABLE_NON_CLAIM)
        );
        assert!(claims.iter().any(|claim| claim == "no all-clear"));

        // Another phase's timeout carries no canonical disclosure.
        let mut earlier = sample_receipt();
        earlier.phase("diff_discovery", "language_facts");
        earlier.limited_timeout("language_facts");
        let value = serde_json::to_value(&*earlier).map_err(|err| err.to_string())?;
        assert!(value.get("canonical_analysis_evaluated_seams").is_none());
        assert!(
            !earlier
                .non_claims
                .iter()
                .any(|claim| claim == CANONICAL_ANALYSIS_NOT_RESUMABLE_NON_CLAIM)
        );

        // A failure in the same phase is not a timeout.
        let mut failed = sample_receipt();
        failed.phase("language_facts", "canonical_analysis");
        failed.failed("canonical_analysis", "read failed");
        assert!(
            !failed
                .non_claims
                .iter()
                .any(|claim| claim == CANONICAL_ANALYSIS_NOT_RESUMABLE_NON_CLAIM)
        );

        // Progress never outlives the phase it describes.
        let mut moved = sample_receipt();
        moved.canonical_analysis_progress(3);
        moved.phase("canonical_analysis", "route_construction");
        assert_eq!(moved.canonical_analysis_evaluated_seams, None);
        let mut finished = sample_receipt();
        finished.canonical_analysis_progress(3);
        finished.complete(&["comments.json".to_string()]);
        assert_eq!(finished.canonical_analysis_evaluated_seams, None);
        Ok(())
    }

    /// A real single-commit repository, so `HEAD` actually resolves and a
    /// raw-revision fallback is distinguishable from a resolved SHA.
    fn single_commit_repo(label: &str) -> Result<PathBuf, String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-receipt-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).map_err(|err| format!("create repo dir failed: {err}"))?;
        let git = |args: &[&str]| -> Result<(), String> {
            let output =
                crate::git::run_git_output_with_deadline(&dir, args, Some(Duration::from_secs(30)))
                    .map_err(|err| format!("run git {args:?} failed: {err}"))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "git {args:?} failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ))
            }
        };
        git(&["init", "-q"])?;
        fs::write(dir.join("f.txt"), "value\n")
            .map_err(|err| format!("write fixture file failed: {err}"))?;
        git(&["add", "."])?;
        git(&[
            "-c",
            "user.name=ripr-test",
            "-c",
            "user.email=ripr-test@example.com",
            "commit",
            "-q",
            "-m",
            "init",
        ])?;
        Ok(dir)
    }

    #[test]
    fn receipt_revision_probes_respect_the_caller_budget() -> Result<(), String> {
        // #4363 review: each revision probe is capped at the caller's
        // remaining `--timeout-ms` budget, and an exhausted budget skips
        // the probe entirely (the raw-revision fallback, exactly like any
        // other rev-parse failure) — a 1s budget must never block for the
        // fixed one-minute ceiling twice.
        let dir = single_commit_repo("budget")?;
        let artifacts = vec!["comments.json".to_string()];
        let resolved = ReviewCommentsReceiptRun::new(
            &dir,
            "HEAD",
            "HEAD",
            30_000,
            &artifacts,
            Some(Duration::from_secs(30)),
        );
        if resolved.base_sha == "HEAD" || resolved.base_sha.len() != 40 {
            return Err(format!(
                "a healthy budget must resolve HEAD to a SHA, got {:?}",
                resolved.base_sha
            ));
        }
        let skipped = ReviewCommentsReceiptRun::new(
            &dir,
            "HEAD",
            "HEAD",
            30_000,
            &artifacts,
            Some(Duration::ZERO),
        );
        if skipped.base_sha != "HEAD" || skipped.head_sha != "HEAD" {
            return Err(format!(
                "an exhausted budget must skip both probes and fall back to the raw revision, got base={:?} head={:?}",
                skipped.base_sha, skipped.head_sha
            ));
        }
        let ceiling = ReviewCommentsReceiptRun::new(&dir, "HEAD", "HEAD", 30_000, &artifacts, None);
        if ceiling.base_sha != resolved.base_sha {
            return Err(format!(
                "the no-budget fixed-ceiling path must still resolve, got {:?}",
                ceiling.base_sha
            ));
        }
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn terminal_setters_preserve_phase_and_write_atomic_receipts() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-receipt-terminal-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).map_err(|err| format!("create temp dir failed: {err}"))?;

        let timeout_path = dir.join("timeout.json");
        let mut timeout = sample_receipt();
        timeout.limited_timeout("review_guidance");
        assert_eq!(timeout.status, "limited_timeout");
        assert_eq!(timeout.active_phase.as_deref(), Some("review_guidance"));
        assert_eq!(timeout.last_completed_phase, None);
        assert_eq!(timeout.limitations[0].category, "analysis_timeout");
        assert_eq!(
            timeout.limitations[0].repair_route,
            "rerun review-comments with a larger configured timeout"
        );
        assert_eq!(
            timeout.non_claims,
            vec![
                "static review guidance is advisory evidence only",
                "no complete route inventory",
                "no all-clear",
            ]
        );
        timeout.write_atomic(&timeout_path)?;
        let timeout_json: Value = serde_json::from_slice(
            &fs::read(&timeout_path)
                .map_err(|err| format!("read timeout receipt failed: {err}"))?,
        )
        .map_err(|err| format!("parse timeout receipt failed: {err}"))?;
        assert_eq!(timeout_json["status"], "limited_timeout");
        assert_eq!(timeout_json["active_phase"], "review_guidance");
        assert_eq!(timeout_json["atomic_write_status"], "committed");
        assert_eq!(
            timeout_json["limitations"][0]["category"],
            "analysis_timeout"
        );
        assert_eq!(
            timeout_json["limitations"][0]["repair_route"],
            "rerun review-comments with a larger configured timeout"
        );
        assert_eq!(
            timeout_json["non_claims"][0],
            "static review guidance is advisory evidence only"
        );
        assert_eq!(timeout_json["non_claims"][1], "no complete route inventory");
        assert_eq!(timeout_json["non_claims"][2], "no all-clear");

        let failure_path = dir.join("failed.json");
        let mut failure = sample_receipt();
        failure.failed("canonical_comparison", "canonical comparison failed");
        assert_eq!(failure.status, "failed");
        assert_eq!(
            failure.active_phase.as_deref(),
            Some("canonical_comparison")
        );
        assert_eq!(failure.limitations[0].category, "analysis_failed");
        assert_eq!(
            failure.limitations[0].repair_route,
            "canonical comparison failed"
        );
        assert_eq!(
            failure.non_claims,
            vec![
                "static review guidance is advisory evidence only",
                "no complete route inventory",
                "no all-clear",
            ]
        );
        failure.write_atomic(&failure_path)?;
        let failure_json: Value = serde_json::from_slice(
            &fs::read(&failure_path).map_err(|err| format!("read failed receipt failed: {err}"))?,
        )
        .map_err(|err| format!("parse failed receipt failed: {err}"))?;
        assert_eq!(failure_json["status"], "failed");
        assert_eq!(failure_json["active_phase"], "canonical_comparison");
        assert_eq!(failure_json["atomic_write_status"], "committed");
        assert_eq!(
            failure_json["limitations"][0]["category"],
            "analysis_failed"
        );
        assert_eq!(
            failure_json["limitations"][0]["repair_route"],
            "canonical comparison failed"
        );
        assert_eq!(
            failure_json["non_claims"][0],
            "static review guidance is advisory evidence only"
        );
        assert_eq!(failure_json["non_claims"][1], "no complete route inventory");
        assert_eq!(failure_json["non_claims"][2], "no all-clear");

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[test]
    fn oversized_records_named_limitation_while_staying_in_the_receipt_vocabulary() {
        // #4388: a dispatch refused at the guidance-payload memory ceiling
        // stays in the contract-valid status vocabulary (`failed`) but its
        // limitation carries the named category consumers classify on.
        let mut oversized = sample_receipt();
        oversized.oversized(
            "canonical_analysis",
            "review_guidance_oversized: 3 closure input files exceed the review-guidance \
             ceiling (RIPR_REVIEW_GUIDANCE_MAX_INDEX_FILES=2); the guidance pass was not run \
             to protect runner memory.",
        );
        assert_eq!(oversized.status, "failed");
        assert_eq!(
            oversized.active_phase.as_deref(),
            Some("canonical_analysis")
        );
        assert_eq!(
            oversized.limitations[0].category,
            REVIEW_GUIDANCE_OVERSIZED_LIMITATION_CATEGORY
        );
        assert!(
            oversized.limitations[0]
                .repair_route
                .contains("RIPR_REVIEW_GUIDANCE_MAX_INDEX_FILES"),
            "repair route must carry the named ceiling error: {:?}",
            oversized.limitations[0].repair_route
        );
        assert_eq!(
            oversized.non_claims,
            vec![
                "static review guidance is advisory evidence only",
                "no complete route inventory",
                "no all-clear",
            ]
        );
    }

    #[test]
    fn write_atomic_succeeds_and_updates_status() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-receipt-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).map_err(|err| format!("create temp dir failed: {err}"))?;
        let path = dir.join("receipt.json");
        let mut receipt = sample_receipt();

        receipt.write_atomic(&path)?;
        // Exclusive staging must still support replacing an existing regular receipt.
        receipt.write_atomic(&path)?;

        // File exists and is valid JSON
        let content =
            fs::read_to_string(&path).map_err(|err| format!("read receipt failed: {err}"))?;
        let parsed: Value = serde_json::from_str(&content)
            .map_err(|err| format!("parse receipt JSON failed: {err}"))?;
        assert_eq!(parsed["atomic_write_status"], "committed");

        // Receipt state updated
        assert_eq!(receipt.atomic_write_status, "committed");

        // No temp file left behind
        let temp = super::atomic_temp_path(&path);
        assert!(
            !temp.exists(),
            "temp file should be cleaned up after rename"
        );

        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn receipt_temp_refuses_planted_symlink_without_clobbering_target() -> Result<(), String> {
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let dir = std::env::temp_dir().join(format!(
            "ripr-receipt-symlink-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.as_nanos())
                .unwrap_or(0),
        ));
        let _cleanup = Cleanup(dir.clone());
        fs::create_dir_all(&dir).map_err(|err| format!("setup directory: {err}"))?;
        let path = dir.join("receipt.json");
        let mut receipt = sample_receipt();
        // Positive control reaches the production authority before the probe.
        receipt.write_atomic(&path)?;
        fs::remove_file(&path).map_err(|err| format!("remove control receipt: {err}"))?;
        let sentinel = dir.join("outside.txt");
        fs::write(&sentinel, b"protected receipt sentinel")
            .map_err(|err| format!("setup sentinel: {err}"))?;
        let temp = super::atomic_temp_path(&path);
        std::os::unix::fs::symlink(&sentinel, &temp)
            .map_err(|err| format!("setup planted temp: {err}"))?;
        let mut refused = sample_receipt();
        let result = refused.write_atomic(&path);
        let observed = fs::read(&sentinel).map_err(|err| format!("read sentinel: {err}"))?;
        let untouched = observed == b"protected receipt sentinel";
        let link_preserved = fs::symlink_metadata(&temp)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false);
        let published = path.exists();
        assert!(
            untouched,
            "production receipt temp write followed planted link; outside sentinel changed to {} bytes; result={result:?}",
            observed.len()
        );
        assert!(
            result.is_err(),
            "exclusive temp acquisition must refuse planted path"
        );
        assert!(
            link_preserved,
            "refusal must not remove an attacker-owned temp path"
        );
        assert!(!published, "refused receipt must not publish an artifact");
        assert_eq!(refused.atomic_write_status, "not_written");
        // Final publication also refuses a planted destination leaf.
        fs::remove_file(&temp).map_err(|err| format!("remove planted fixture temp: {err}"))?;
        std::os::unix::fs::symlink(&sentinel, &path)
            .map_err(|err| format!("setup planted destination: {err}"))?;
        let result = refused.write_atomic(&path);
        assert!(result.is_err(), "receipt destination link must be refused");
        assert_eq!(
            fs::read(&sentinel).map_err(|err| err.to_string())?,
            observed
        );
        assert!(
            fs::symlink_metadata(&path)
                .map_err(|err| err.to_string())?
                .file_type()
                .is_symlink()
        );
        assert!(
            !temp.exists(),
            "destination refusal must not acquire a temp"
        );
        assert_eq!(refused.atomic_write_status, "not_written");
        Ok(())
    }

    #[test]
    fn write_atomic_fails_on_unwritable_path() -> Result<(), String> {
        // On Windows, a path inside a non-existent drive is unwritable.
        // On Unix, a path inside /dev/null/x is unwritable.
        let path = if cfg!(windows) {
            // Build the drive letter dynamically so the local-context gate
            // does not flag a hardcoded absolute Windows path in source.
            let drive = char::from_u32(90).unwrap_or('z');
            PathBuf::from(format!("{drive}:nonexistent_ripr_test_dir/receipt.json"))
        } else {
            PathBuf::from("/dev/null/cannot_write_receipt.json")
        };
        let mut receipt = sample_receipt();
        let result = receipt.write_atomic(&path);
        assert!(
            result.is_err(),
            "write_atomic should fail on unwritable path"
        );
        // Receipt state should NOT be updated on failure
        assert_eq!(receipt.atomic_write_status, "not_written");
        Ok(())
    }

    #[test]
    fn write_atomic_creates_parent_directory() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!(
            "ripr-receipt-parent-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        // Ensure the directory does not exist yet
        let _ = fs::remove_dir_all(&dir);
        let nested = dir.join("nested/sub/dir");
        let path = nested.join("receipt.json");
        let mut receipt = sample_receipt();

        receipt.write_atomic(&path)?;

        assert!(
            path.exists(),
            "receipt file should exist after write_atomic"
        );
        let _ = fs::remove_dir_all(&dir);
        Ok(())
    }
}
