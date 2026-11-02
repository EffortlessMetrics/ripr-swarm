//! Review-comments run receipt DTO and JSON attachment helpers.
//!
//! Git identity resolution, cache identity, phase transitions, and atomic
//! persistence are owned by [`crate::app::review_comments_receipt`]. This
//! module exposes only the serializable receipt shape and rendering helpers
//! consumers need.

use serde::Serialize;
use serde_json::Value;

pub(crate) const REVIEW_COMMENTS_RECEIPT_SCHEMA_VERSION: &str = "0.1";

/// Named limitation category for a review-guidance dispatch refused at the
/// memory ceiling (#4388). Consumers match this category — not error prose —
/// when distinguishing an instrument-limited pass from a real gap.
pub(crate) const REVIEW_GUIDANCE_OVERSIZED_LIMITATION_CATEGORY: &str = "review_guidance_oversized";

/// Non-claim added when the budget expires inside `canonical_analysis` (#11107).
/// Whether a partial classification may be stored and reused is an open design
/// question (what it may claim, and the exact inputs each classification read);
/// until it is settled nothing from the timed-out phase is kept.
pub(crate) const CANONICAL_ANALYSIS_NOT_RESUMABLE_NON_CLAIM: &str = "canonical_analysis work done before the timeout is not persisted; an unchanged retry restarts it and no reuse is claimed";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ReviewCommentsReceiptLimitation {
    pub(crate) category: String,
    pub(crate) repair_route: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ReviewCommentsRunReceipt {
    pub(crate) schema_version: &'static str,
    pub(crate) status: &'static str,
    pub(crate) root_identity: String,
    pub(crate) base_sha: String,
    pub(crate) head_sha: String,
    pub(crate) configured_timeout_ms: u64,
    pub(crate) last_completed_phase: Option<String>,
    pub(crate) active_phase: Option<String>,
    /// The pass running inside `active_phase`, for phases that run several
    /// (`language_facts` runs corpus admission, test-change detection and
    /// owner attribution). Absent when the phase has no named passes, so a
    /// timeout can say which one spent the budget (#1604).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) active_step: Option<String>,
    /// Seams `canonical_analysis` had classified when its budget expired. Work
    /// done, not work kept: nothing classified in a timed-out run is persisted,
    /// so an unchanged retry restarts the phase (#11107). Absent unless the
    /// timeout landed in `canonical_analysis`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) canonical_analysis_evaluated_seams: Option<usize>,
    pub(crate) completed_artifacts: Vec<String>,
    pub(crate) missing_artifacts: Vec<String>,
    pub(crate) reusable_cache_identity: String,
    pub(crate) limitations: Vec<ReviewCommentsReceiptLimitation>,
    pub(crate) non_claims: Vec<String>,
    pub(crate) atomic_write_status: &'static str,
}

pub(crate) fn attach_to_json(
    rendered: &str,
    receipt: &ReviewCommentsRunReceipt,
) -> Result<String, String> {
    let mut value: Value = serde_json::from_str(rendered).map_err(|err| {
        format!("parse review-comments JSON for receipt attachment failed: {err}; regenerate the input with the command that wrote it, then rerun")
    })?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "review-comments JSON must be an object; regenerate the input with the command that wrote it, then rerun".to_string())?;
    object.insert(
        "run_receipt".to_string(),
        serde_json::to_value(receipt)
            .map_err(|err| format!("serialize review-comments receipt failed: {err}; this is a ripr bug, report it with the command you ran"))?,
    );
    serde_json::to_string_pretty(&value)
        .map_err(|err| format!("render review-comments JSON with receipt failed: {err}; this is a ripr bug, report it with the command you ran"))
}
