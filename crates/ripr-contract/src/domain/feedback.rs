//! Local result-bound usefulness feedback identity (RIPR-SPEC-0179 / #4585).
//!
//! This module owns the typed receipt vocabulary and identity rules. It does
//! not write files, talk to the network, or change analyzer, gate, baseline,
//! suppression, or diagnostic state.

/// Schema version for usefulness-feedback receipts and join documents.
pub const FEEDBACK_SCHEMA_VERSION: &str = "0.1";

/// Maximum UTF-8 byte length of an optional note.
pub const FEEDBACK_NOTE_MAX_BYTES: usize = 1024;

/// Closed usefulness judgment. Parent reason codes map onto these five
/// classes so positive, incorrect, unclear, expensive, and intentional
/// no-action remain distinguishable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackJudgment {
    /// `useful`: the result helped.
    Useful,
    /// `incorrect`: the result was wrong.
    Incorrect,
    /// `unclear`: the result was hard to act on.
    Unclear,
    /// `expensive`: the result cost too much to use.
    Expensive,
    /// `intentional_no_action`: no action was taken on purpose, such as for
    /// intentional defensive code or a preference for manual review. It does
    /// not say whether the result was right.
    IntentionalNoAction,
}

impl FeedbackJudgment {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Useful => "useful",
            Self::Incorrect => "incorrect",
            Self::Unclear => "unclear",
            Self::Expensive => "expensive",
            Self::IntentionalNoAction => "intentional_no_action",
        }
    }

    /// Parse a wire name, or return an error naming the accepted values.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "useful" => Ok(Self::Useful),
            "incorrect" => Ok(Self::Incorrect),
            "unclear" => Ok(Self::Unclear),
            "expensive" => Ok(Self::Expensive),
            "intentional_no_action" => Ok(Self::IntentionalNoAction),
            other => Err(format!(
                "unknown feedback judgment {other:?}; expected useful, incorrect, unclear, expensive, or intentional_no_action"
            )),
        }
    }
}

/// Closed reason taxonomy. `other` is the escape hatch and requires a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackReason {
    /// `useful_actionable`: led to a useful action.
    UsefulActionable,
    /// `useful_limitation`: usefully named a limitation.
    UsefulLimitation,
    /// `correct_target`: named the right target.
    CorrectTarget,
    /// `correct_discriminator`: named the right missing discriminator.
    CorrectDiscriminator,
    /// `clear_explanation`: explained the result clearly.
    ClearExplanation,
    /// `wrong_target`: named the wrong target.
    WrongTarget,
    /// `wrong_discriminator`: named the wrong discriminator.
    WrongDiscriminator,
    /// `false_actionable`: was marked actionable but was not.
    FalseActionable,
    /// `duplicate_item`: repeated another item.
    DuplicateItem,
    /// `already_covered`: the result was already addressed.
    AlreadyCovered,
    /// `incorrect_causal_attribution`: tied the result to the wrong cause.
    IncorrectCausalAttribution,
    /// `stale_result`: no longer matched the code.
    StaleResult,
    /// `missing_context`: lacked context needed to act.
    MissingContext,
    /// `ambiguous_target`: the target could be read more than one way.
    AmbiguousTarget,
    /// `unclear_explanation`: the explanation was hard to follow.
    UnclearExplanation,
    /// `verify_command_failed`: the suggested verify command failed.
    VerifyCommandFailed,
    /// `receipt_command_failed`: the suggested receipt command failed.
    ReceiptCommandFailed,
    /// `route_too_broad`: the suggested route did more than needed.
    RouteTooBroad,
    /// `too_slow`.
    TooSlow,
    /// `too_noisy`.
    TooNoisy,
    /// `unsupported_workflow`: did not fit the user's workflow.
    UnsupportedWorkflow,
    /// `intentional_defensive_code`: the code is defensive on purpose.
    IntentionalDefensiveCode,
    /// `intentional_low_grip`: low test grip is accepted on purpose.
    IntentionalLowGrip,
    /// `manual_review_preferred`: a person reviews this instead.
    ManualReviewPreferred,
    /// `other`: none of the above; needs a note and an explicit judgment.
    Other,
}

impl FeedbackReason {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UsefulActionable => "useful_actionable",
            Self::UsefulLimitation => "useful_limitation",
            Self::CorrectTarget => "correct_target",
            Self::CorrectDiscriminator => "correct_discriminator",
            Self::ClearExplanation => "clear_explanation",
            Self::WrongTarget => "wrong_target",
            Self::WrongDiscriminator => "wrong_discriminator",
            Self::FalseActionable => "false_actionable",
            Self::DuplicateItem => "duplicate_item",
            Self::AlreadyCovered => "already_covered",
            Self::IncorrectCausalAttribution => "incorrect_causal_attribution",
            Self::StaleResult => "stale_result",
            Self::MissingContext => "missing_context",
            Self::AmbiguousTarget => "ambiguous_target",
            Self::UnclearExplanation => "unclear_explanation",
            Self::VerifyCommandFailed => "verify_command_failed",
            Self::ReceiptCommandFailed => "receipt_command_failed",
            Self::RouteTooBroad => "route_too_broad",
            Self::TooSlow => "too_slow",
            Self::TooNoisy => "too_noisy",
            Self::UnsupportedWorkflow => "unsupported_workflow",
            Self::IntentionalDefensiveCode => "intentional_defensive_code",
            Self::IntentionalLowGrip => "intentional_low_grip",
            Self::ManualReviewPreferred => "manual_review_preferred",
            Self::Other => "other",
        }
    }

    /// Parse a wire name, or return an error that points at `ripr feedback
    /// --help`.
    pub fn parse(value: &str) -> Result<Self, String> {
        for reason in Self::all() {
            if reason.as_str() == value {
                return Ok(reason);
            }
        }
        let accepted = Self::all()
            .iter()
            .map(|reason| reason.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        Err(format!(
            "unknown feedback reason {value:?}; expected one of: {accepted}"
        ))
    }

    /// The judgment this reason implies. `other` returns `unclear` here, but a
    /// payload with `other` must carry its own judgment.
    pub const fn judgment(self) -> FeedbackJudgment {
        match self {
            Self::UsefulActionable
            | Self::UsefulLimitation
            | Self::CorrectTarget
            | Self::CorrectDiscriminator
            | Self::ClearExplanation => FeedbackJudgment::Useful,
            Self::WrongTarget
            | Self::WrongDiscriminator
            | Self::FalseActionable
            | Self::DuplicateItem
            | Self::AlreadyCovered
            | Self::IncorrectCausalAttribution
            | Self::StaleResult => FeedbackJudgment::Incorrect,
            Self::MissingContext | Self::AmbiguousTarget | Self::UnclearExplanation => {
                FeedbackJudgment::Unclear
            }
            Self::VerifyCommandFailed
            | Self::ReceiptCommandFailed
            | Self::RouteTooBroad
            | Self::TooSlow
            | Self::TooNoisy
            | Self::UnsupportedWorkflow => FeedbackJudgment::Expensive,
            Self::IntentionalDefensiveCode
            | Self::IntentionalLowGrip
            | Self::ManualReviewPreferred => FeedbackJudgment::IntentionalNoAction,
            Self::Other => {
                // `other` has no implied class; callers must supply `--judgment`.
                FeedbackJudgment::Unclear
            }
        }
    }

    /// Whether the reason needs a note: only `other` does.
    pub const fn requires_note(self) -> bool {
        matches!(self, Self::Other)
    }

    /// Every reason, in declaration order.
    pub const fn all() -> [Self; 25] {
        [
            Self::UsefulActionable,
            Self::UsefulLimitation,
            Self::CorrectTarget,
            Self::CorrectDiscriminator,
            Self::ClearExplanation,
            Self::WrongTarget,
            Self::WrongDiscriminator,
            Self::FalseActionable,
            Self::DuplicateItem,
            Self::AlreadyCovered,
            Self::IncorrectCausalAttribution,
            Self::StaleResult,
            Self::MissingContext,
            Self::AmbiguousTarget,
            Self::UnclearExplanation,
            Self::VerifyCommandFailed,
            Self::ReceiptCommandFailed,
            Self::RouteTooBroad,
            Self::TooSlow,
            Self::TooNoisy,
            Self::UnsupportedWorkflow,
            Self::IntentionalDefensiveCode,
            Self::IntentionalLowGrip,
            Self::ManualReviewPreferred,
            Self::Other,
        ]
    }
}

/// Who recorded the receipt. Distinct from review status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    /// `human`.
    Human,
    /// `agent`.
    Agent,
    /// `unknown`.
    Unknown,
}

impl ActorKind {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
            Self::Unknown => "unknown",
        }
    }

    /// Parse a wire name, or return an error naming the accepted values.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "human" => Ok(Self::Human),
            "agent" => Ok(Self::Agent),
            "unknown" => Ok(Self::Unknown),
            other => Err(format!(
                "unknown feedback actor {other:?}; expected human, agent, or unknown"
            )),
        }
    }
}

/// Review disposition, independent of [`ActorKind`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewStatus {
    /// `unreviewed`.
    Unreviewed,
    /// `reviewed_accepted`: a reviewer accepted the feedback.
    ReviewedAccepted,
    /// `reviewed_rejected`: a reviewer rejected the feedback.
    ReviewedRejected,
}

impl ReviewStatus {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreviewed => "unreviewed",
            Self::ReviewedAccepted => "reviewed_accepted",
            Self::ReviewedRejected => "reviewed_rejected",
        }
    }

    /// Parse a wire name, or return an error naming the accepted values.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "unreviewed" => Ok(Self::Unreviewed),
            "reviewed_accepted" => Ok(Self::ReviewedAccepted),
            "reviewed_rejected" => Ok(Self::ReviewedRejected),
            other => Err(format!(
                "unknown feedback review status {other:?}; expected unreviewed, reviewed_accepted, or reviewed_rejected"
            )),
        }
    }

    /// Whether the feedback has been reviewed, accepted or rejected.
    pub const fn is_reviewed(self) -> bool {
        !matches!(self, Self::Unreviewed)
    }
}

/// Whether a stored identity still names the live result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceState {
    /// `current`: names the live result, or there is no live result to compare.
    Current,
    /// `historical`: the same snapshot, item and route, with a different
    /// attempt or receipt id. The ids are not ordered, so this does not check
    /// which one came later.
    Historical,
    /// `mismatched`: a different snapshot, item or route.
    Mismatched,
}

impl ReferenceState {
    /// The snake_case wire name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Historical => "historical",
            Self::Mismatched => "mismatched",
        }
    }
}

/// Immutable analysis/result identity. File/line is not part of this type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResultIdentity {
    /// The analysis snapshot id; required.
    pub snapshot_id: String,
    /// The canonical item id, when known.
    pub canonical_item: Option<String>,
    /// The route digest, when known.
    pub route_digest: Option<String>,
    /// The repair attempt id, when known.
    pub attempt_id: Option<String>,
    /// The receipt id, when known.
    pub receipt_id: Option<String>,
}

impl ResultIdentity {
    /// Check that the snapshot id is present and every given id is non-blank
    /// with no control characters.
    pub fn validate(&self) -> Result<(), String> {
        if self.snapshot_id.trim().is_empty() {
            return Err(
                "feedback requires --snapshot; file and line are not a result identity; pass the snapshot id from the finding".to_string(),
            );
        }
        if self.snapshot_id.chars().any(char::is_control) {
            return Err("feedback --snapshot must not contain control characters; pass the snapshot id exactly as the finding printed it".to_string());
        }
        for (label, value) in [
            ("--item", self.canonical_item.as_deref()),
            ("--route-digest", self.route_digest.as_deref()),
            ("--attempt", self.attempt_id.as_deref()),
            ("--receipt", self.receipt_id.as_deref()),
        ] {
            if let Some(value) = value
                && (value.trim().is_empty() || value.chars().any(char::is_control))
            {
                return Err(format!(
                    "feedback {label} must be a non-empty identity without control characters; pass the id exactly as the finding printed it"
                ));
            }
        }
        Ok(())
    }
}

/// In-memory usefulness-feedback receipt. Timestamp is outside payload identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackReceipt {
    /// The receipt id.
    pub feedback_id: String,
    /// The key that makes repeat recording of the same payload idempotent.
    pub idempotency_key: String,
    /// The result the feedback is about.
    pub identity: ResultIdentity,
    /// Who recorded the feedback.
    pub actor_kind: ActorKind,
    /// Whether the feedback was reviewed.
    pub review_status: ReviewStatus,
    /// Who reviewed it; present exactly when `review_status` is reviewed.
    pub review_actor_kind: Option<ActorKind>,
    /// Why the feedback was given.
    pub reason: FeedbackReason,
    /// Required when `reason` is `other`; otherwise derived from the reason.
    pub judgment_override: Option<FeedbackJudgment>,
    /// An optional note of at most [`FEEDBACK_NOTE_MAX_BYTES`] bytes; required
    /// for `other`.
    pub note: Option<String>,
    /// How `identity` compared with the live result when the feedback was
    /// recorded. It is stored, not rechecked, and is `current` when no live
    /// identity was supplied.
    pub reference_state: ReferenceState,
    /// When the feedback was recorded; not part of the payload identity.
    pub recorded_at: String,
}

impl FeedbackReceipt {
    /// The judgment: the override when set, else the one the reason implies.
    pub fn judgment(&self) -> FeedbackJudgment {
        self.judgment_override
            .unwrap_or_else(|| self.reason.judgment())
    }
}

/// Semantic payload used for idempotency comparison. Timestamp is excluded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackPayload {
    /// The result the feedback is about.
    pub identity: ResultIdentity,
    /// Who recorded the feedback.
    pub actor_kind: ActorKind,
    /// Whether the feedback was reviewed.
    pub review_status: ReviewStatus,
    /// Who reviewed it; present exactly when `review_status` is reviewed.
    pub review_actor_kind: Option<ActorKind>,
    /// Why the feedback was given.
    pub reason: FeedbackReason,
    /// An explicit judgment; required for `other` and otherwise must match the
    /// reason's.
    pub judgment_override: Option<FeedbackJudgment>,
    /// An optional note; required for `other`.
    pub note: Option<String>,
}

impl FeedbackPayload {
    /// The payload of a receipt, without its id, key, reference state or
    /// timestamp.
    pub fn from_receipt(receipt: &FeedbackReceipt) -> Self {
        Self {
            identity: receipt.identity.clone(),
            actor_kind: receipt.actor_kind,
            review_status: receipt.review_status,
            review_actor_kind: receipt.review_actor_kind,
            reason: receipt.reason,
            judgment_override: receipt.judgment_override,
            note: receipt.note.clone(),
        }
    }

    /// The judgment: the override when set, else the one the reason implies.
    pub fn judgment(&self) -> FeedbackJudgment {
        self.judgment_override
            .unwrap_or_else(|| self.reason.judgment())
    }

    /// Collapse a matching `--judgment` onto the derived class so reload and
    /// repeat recording compare the same payload.
    pub fn normalized(mut self) -> Self {
        if self.reason != FeedbackReason::Other
            && self.judgment_override == Some(self.reason.judgment())
        {
            self.judgment_override = None;
        }
        self
    }

    /// Check the identity, the note and judgment rules for `other`, the note
    /// limits, and that a review actor is present exactly when the feedback is
    /// reviewed.
    pub fn validate(&self) -> Result<(), String> {
        self.identity.validate()?;
        if self.reason.requires_note()
            && self.note.as_ref().is_none_or(|note| note.trim().is_empty())
        {
            return Err("feedback reason `other` requires a bounded --note; add a short --note explaining the reason".to_string());
        }
        if self.reason == FeedbackReason::Other && self.judgment_override.is_none() {
            return Err(
                "feedback reason `other` requires --judgment useful|incorrect|unclear|expensive|intentional_no_action; add one of those values"
                    .to_string(),
            );
        }
        if self.reason != FeedbackReason::Other
            && self
                .judgment_override
                .is_some_and(|judgment| judgment != self.reason.judgment())
        {
            return Err(
                "feedback --judgment must match the closed reason class unless --reason is `other`; pass the --judgment that matches --reason, or use --reason other"
                    .to_string(),
            );
        }
        if let Some(note) = &self.note {
            if note.len() > FEEDBACK_NOTE_MAX_BYTES {
                return Err(format!(
                    "feedback --note exceeds {FEEDBACK_NOTE_MAX_BYTES} bytes; use a shorter note"
                ));
            }
            if note.chars().any(|ch| ch == '\0') {
                return Err(
                    "feedback --note must not contain NUL; remove the NUL character and retry"
                        .to_string(),
                );
            }
        }
        if self.review_status.is_reviewed() && self.review_actor_kind.is_none() {
            return Err(
                "reviewed feedback requires --review-actor (human, agent, or unknown)".to_string(),
            );
        }
        if !self.review_status.is_reviewed() && self.review_actor_kind.is_some() {
            return Err(
                "unreviewed feedback cannot carry --review-actor; drop it or set --review"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// The canonical text form of the payload, which feeds the automatic
    /// idempotency key and the feedback id.
    /// Recording compares normalized payloads, not this text, so an absent
    /// note and an empty note give the same text but are not equal payloads.
    pub fn canonical_bytes(&self) -> String {
        format!(
            "v1\nsnapshot={}\nitem={}\nroute={}\nattempt={}\nreceipt={}\nactor={}\nreview={}\nreview_actor={}\nreason={}\njudgment={}\nnote={}\n",
            self.identity.snapshot_id,
            self.identity.canonical_item.as_deref().unwrap_or(""),
            self.identity.route_digest.as_deref().unwrap_or(""),
            self.identity.attempt_id.as_deref().unwrap_or(""),
            self.identity.receipt_id.as_deref().unwrap_or(""),
            self.actor_kind.as_str(),
            self.review_status.as_str(),
            self.review_actor_kind.map(ActorKind::as_str).unwrap_or(""),
            self.reason.as_str(),
            self.judgment().as_str(),
            self.note.as_deref().unwrap_or(""),
        )
    }
}

/// Classify a stored identity against a live result identity.
///
/// Absent live identity is not evidence of staleness: recording and export
/// without a comparison subject keep the binding `current`. Historical is
/// reserved for a later attempt/receipt on the same result. Mismatch is a
/// different snapshot or item, not an absent comparison.
pub fn classify_reference(
    stored: &ResultIdentity,
    live: Option<&ResultIdentity>,
) -> ReferenceState {
    let Some(live) = live else {
        return ReferenceState::Current;
    };
    if stored == live {
        return ReferenceState::Current;
    }
    if stored.snapshot_id == live.snapshot_id
        && stored.canonical_item == live.canonical_item
        && stored.route_digest == live.route_digest
    {
        // Same result, later attempt/receipt: retain the historical binding.
        return ReferenceState::Historical;
    }
    ReferenceState::Mismatched
}

#[cfg(kani)]
#[path = "feedback_kani.rs"]
mod kani_proofs;

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(snapshot: &str, item: Option<&str>) -> ResultIdentity {
        ResultIdentity {
            snapshot_id: snapshot.to_string(),
            canonical_item: item.map(str::to_string),
            route_digest: Some("add_missing_test".to_string()),
            attempt_id: None,
            receipt_id: None,
        }
    }

    #[test]
    fn unknown_reason_lists_codes_without_naming_a_flag() -> Result<(), String> {
        // Stored receipts read by `feedback export` also go through `parse`,
        // and export takes no `--reason`; only `feedback record` adds the
        // flag to this message (#15257).
        let message = match FeedbackReason::parse("future_reason") {
            Ok(reason) => return Err(format!("unknown reason parsed as {reason:?}")),
            Err(message) => message,
        };
        assert!(!message.contains("--reason"), "{message}");
        for reason in FeedbackReason::all() {
            assert!(message.contains(reason.as_str()), "{message}");
        }
        Ok(())
    }

    #[test]
    fn every_reason_maps_onto_one_of_the_five_judgments() {
        for reason in FeedbackReason::all() {
            let _ = reason.judgment();
            assert_eq!(FeedbackReason::parse(reason.as_str()).ok(), Some(reason));
        }
        assert_eq!(
            FeedbackReason::UsefulLimitation.judgment(),
            FeedbackJudgment::Useful
        );
        assert_eq!(
            FeedbackReason::WrongDiscriminator.judgment(),
            FeedbackJudgment::Incorrect
        );
        assert_eq!(
            FeedbackReason::UnclearExplanation.judgment(),
            FeedbackJudgment::Unclear
        );
        assert_eq!(
            FeedbackReason::TooSlow.judgment(),
            FeedbackJudgment::Expensive
        );
        assert_eq!(
            FeedbackReason::ManualReviewPreferred.judgment(),
            FeedbackJudgment::IntentionalNoAction
        );
        assert!(FeedbackReason::Other.requires_note());
        assert!(!FeedbackReason::UsefulLimitation.requires_note());
    }

    #[test]
    fn actor_kind_is_not_review_status() {
        assert_ne!(
            ActorKind::Agent.as_str(),
            ReviewStatus::ReviewedAccepted.as_str()
        );
        assert!(!ReviewStatus::Unreviewed.is_reviewed());
        assert!(ReviewStatus::ReviewedAccepted.is_reviewed());
    }

    #[test]
    fn snapshot_is_required_and_file_line_is_not_identity() -> Result<(), String> {
        let missing = ResultIdentity {
            snapshot_id: String::new(),
            canonical_item: None,
            route_digest: None,
            attempt_id: None,
            receipt_id: None,
        };
        let error = missing
            .validate()
            .err()
            .ok_or_else(|| "empty snapshot must fail".to_string())?;
        assert!(error.contains("--snapshot"));
        assert!(error.contains("file and line"));
        Ok(())
    }

    #[test]
    fn useful_limitation_may_omit_canonical_item() -> Result<(), String> {
        identity("snap-1", None)
            .validate()
            .map_err(|error| format!("limitation without a gap id is a valid identity: {error}"))?;
        Ok(())
    }

    #[test]
    fn other_requires_a_note_and_explicit_judgment() -> Result<(), String> {
        let mut payload = FeedbackPayload {
            identity: identity("snap-1", None),
            actor_kind: ActorKind::Human,
            review_status: ReviewStatus::Unreviewed,
            review_actor_kind: None,
            reason: FeedbackReason::Other,
            judgment_override: None,
            note: None,
        };
        let error = payload
            .validate()
            .err()
            .ok_or_else(|| "other without note".to_string())?;
        assert!(error.contains("other"));
        assert!(error.contains("--note"));
        payload.note = Some("custom operator judgment".to_string());
        let error = payload
            .validate()
            .err()
            .ok_or_else(|| "other without judgment".to_string())?;
        assert!(error.contains("--judgment"));
        payload.judgment_override = Some(FeedbackJudgment::Expensive);
        payload
            .validate()
            .map_err(|error| format!("other with note and judgment: {error}"))?;
        assert_eq!(payload.judgment(), FeedbackJudgment::Expensive);
        Ok(())
    }

    #[test]
    fn closed_reason_rejects_a_mismatched_judgment() -> Result<(), String> {
        let payload = FeedbackPayload {
            identity: identity("snap-1", None),
            actor_kind: ActorKind::Human,
            review_status: ReviewStatus::Unreviewed,
            review_actor_kind: None,
            reason: FeedbackReason::UsefulLimitation,
            judgment_override: Some(FeedbackJudgment::Incorrect),
            note: None,
        };
        let error = payload
            .validate()
            .err()
            .ok_or_else(|| "mismatched judgment".to_string())?;
        assert!(error.contains("--judgment"));
        assert!(error.contains("other"));
        Ok(())
    }

    #[test]
    fn absent_live_identity_is_current_not_staleness() {
        assert_eq!(
            classify_reference(&identity("snap-1", None), None),
            ReferenceState::Current
        );
    }

    #[test]
    fn reviewed_feedback_requires_a_review_actor() -> Result<(), String> {
        let payload = FeedbackPayload {
            identity: identity("snap-1", Some("gap:example")),
            actor_kind: ActorKind::Agent,
            review_status: ReviewStatus::ReviewedAccepted,
            review_actor_kind: None,
            reason: FeedbackReason::UsefulActionable,
            judgment_override: None,
            note: None,
        };
        let error = payload
            .validate()
            .err()
            .ok_or_else(|| "reviewed without actor".to_string())?;
        assert!(error.contains("--review-actor"));
        Ok(())
    }

    #[test]
    fn same_file_line_different_item_is_a_different_identity() {
        let left = identity("snap-1", Some("gap:alpha"));
        let right = identity("snap-1", Some("gap:beta"));
        assert_ne!(left, right);
        assert_eq!(
            classify_reference(&left, Some(&right)),
            ReferenceState::Mismatched
        );
    }

    #[test]
    fn a_later_attempt_keeps_the_historical_reference() {
        let mut stored = identity("snap-1", Some("gap:alpha"));
        stored.attempt_id = Some("attempt-a".to_string());
        let mut live = stored.clone();
        live.attempt_id = Some("attempt-b".to_string());
        assert_eq!(
            classify_reference(&stored, Some(&live)),
            ReferenceState::Historical
        );
    }

    #[test]
    fn payload_excludes_timestamp_from_identity() {
        let payload = FeedbackPayload {
            identity: identity("snap-1", Some("gap:alpha")),
            actor_kind: ActorKind::Human,
            review_status: ReviewStatus::Unreviewed,
            review_actor_kind: None,
            reason: FeedbackReason::UsefulLimitation,
            judgment_override: None,
            note: None,
        };
        let first = payload.canonical_bytes();
        let second = payload.canonical_bytes();
        assert_eq!(first, second);
        assert!(!first.contains("recorded_at"));
        assert!(!first.contains("unix_ms"));
    }

    fn closed_reason_payload() -> FeedbackPayload {
        FeedbackPayload {
            identity: identity("snap-1", Some("item-1")),
            actor_kind: ActorKind::Agent,
            review_status: ReviewStatus::Unreviewed,
            review_actor_kind: None,
            reason: FeedbackReason::TooSlow,
            judgment_override: None,
            note: None,
        }
    }

    #[test]
    fn one_control_character_in_an_identity_is_refused_by_flag() -> Result<(), String> {
        let refusal = |edit: fn(&mut ResultIdentity)| {
            let mut subject = identity("snap-1", Some("item-1"));
            edit(&mut subject);
            subject
                .validate()
                .err()
                .ok_or_else(|| "a control character was accepted".to_string())
        };
        assert!(
            refusal(|subject| subject.snapshot_id = "snap\u{7}1".to_string())?
                .contains("--snapshot must not contain control")
        );
        assert!(
            refusal(|subject| subject.canonical_item = Some("item\n1".to_string()))?
                .contains("feedback --item must be")
        );
        assert!(
            refusal(|subject| subject.attempt_id = Some(" ".to_string()))?
                .contains("feedback --attempt must be")
        );
        assert!(
            refusal(|subject| subject.receipt_id = Some("r\t1".to_string()))?
                .contains("feedback --receipt must be")
        );
        assert_eq!(identity("snap-1", Some("item-1")).validate(), Ok(()));
        Ok(())
    }

    #[test]
    fn normalized_drops_only_a_matching_override_on_a_closed_reason() {
        let mut matching = closed_reason_payload();
        matching.judgment_override = Some(FeedbackJudgment::Expensive);
        assert_eq!(matching.normalized().judgment_override, None);

        let mut mismatched = closed_reason_payload();
        mismatched.judgment_override = Some(FeedbackJudgment::Useful);
        assert_eq!(
            mismatched.normalized().judgment_override,
            Some(FeedbackJudgment::Useful)
        );

        let mut other = closed_reason_payload();
        other.reason = FeedbackReason::Other;
        other.judgment_override = Some(FeedbackReason::Other.judgment());
        assert_eq!(
            other.normalized().judgment_override,
            Some(FeedbackReason::Other.judgment())
        );
    }

    #[test]
    fn note_limit_admits_exactly_the_maximum_and_one_nul_is_refused() -> Result<(), String> {
        let mut at_limit = closed_reason_payload();
        at_limit.note = Some("n".repeat(FEEDBACK_NOTE_MAX_BYTES));
        assert_eq!(at_limit.validate(), Ok(()));

        let mut over_limit = closed_reason_payload();
        over_limit.note = Some("n".repeat(FEEDBACK_NOTE_MAX_BYTES + 1));
        assert!(
            over_limit
                .validate()
                .err()
                .ok_or_else(|| "an oversized note was accepted".to_string())?
                .contains("--note exceeds")
        );

        let mut nul = closed_reason_payload();
        nul.note = Some("a\0b".to_string());
        assert!(
            nul.validate()
                .err()
                .ok_or_else(|| "a NUL in the note was accepted".to_string())?
                .contains("must not contain NUL")
        );
        Ok(())
    }

    fn payload(reason: FeedbackReason) -> FeedbackPayload {
        FeedbackPayload {
            identity: identity("snap-1", Some("gap:alpha")),
            actor_kind: ActorKind::Agent,
            review_status: ReviewStatus::Unreviewed,
            review_actor_kind: None,
            reason,
            judgment_override: None,
            note: None,
        }
    }

    fn other(judgment: FeedbackJudgment, note: &str) -> FeedbackPayload {
        FeedbackPayload {
            judgment_override: Some(judgment),
            note: Some(note.to_string()),
            ..payload(FeedbackReason::Other)
        }
    }

    #[test]
    fn every_closed_token_parses_back_to_its_value() {
        for judgment in [
            FeedbackJudgment::Useful,
            FeedbackJudgment::Incorrect,
            FeedbackJudgment::Unclear,
            FeedbackJudgment::Expensive,
            FeedbackJudgment::IntentionalNoAction,
        ] {
            assert_eq!(FeedbackJudgment::parse(judgment.as_str()), Ok(judgment));
        }
        assert_eq!(
            FeedbackJudgment::IntentionalNoAction.as_str(),
            "intentional_no_action"
        );
        for actor in [ActorKind::Human, ActorKind::Agent, ActorKind::Unknown] {
            assert_eq!(ActorKind::parse(actor.as_str()), Ok(actor));
        }
        assert_eq!(
            [ActorKind::Human, ActorKind::Agent, ActorKind::Unknown].map(ActorKind::as_str),
            ["human", "agent", "unknown"]
        );
        for status in [
            ReviewStatus::Unreviewed,
            ReviewStatus::ReviewedAccepted,
            ReviewStatus::ReviewedRejected,
        ] {
            assert_eq!(ReviewStatus::parse(status.as_str()), Ok(status));
        }
        assert_eq!(ReviewStatus::ReviewedRejected.as_str(), "reviewed_rejected");
        assert_eq!(
            [
                ReferenceState::Current,
                ReferenceState::Historical,
                ReferenceState::Mismatched
            ]
            .map(ReferenceState::as_str),
            ["current", "historical", "mismatched"]
        );
        for reason in FeedbackReason::all() {
            assert_eq!(FeedbackReason::parse(reason.as_str()), Ok(reason));
        }
        assert_eq!(FeedbackJudgment::parse("Useful").ok(), None);
        assert_eq!(ActorKind::parse("bot").ok(), None);
        assert_eq!(ReviewStatus::parse("reviewed").ok(), None);
        assert_eq!(FeedbackReason::parse("").ok(), None);
    }

    #[test]
    fn each_optional_identity_is_refused_when_blank_or_holding_a_control_character() {
        type Field = fn(&mut ResultIdentity) -> &mut Option<String>;
        let fields: [(&str, Field); 4] = [
            ("--item", |id| &mut id.canonical_item),
            ("--route-digest", |id| &mut id.route_digest),
            ("--attempt", |id| &mut id.attempt_id),
            ("--receipt", |id| &mut id.receipt_id),
        ];
        for (label, field) in fields {
            for bad in [" ", "", "id\n2", "id\u{7}"] {
                let mut subject = identity("snap-1", Some("gap:alpha"));
                *field(&mut subject) = Some(bad.to_string());
                assert_eq!(
                    subject.validate(),
                    Err(format!(
                        "feedback {label} must be a non-empty identity without control characters; pass the id exactly as the finding printed it"
                    )),
                    "{label} {bad:?}"
                );
            }
            let mut absent = identity("snap-1", Some("gap:alpha"));
            *field(&mut absent) = None;
            assert_eq!(absent.validate(), Ok(()), "{label}");
        }
        let mut control = identity("snap\t1", None);
        assert!(control.validate().err().is_some());
        control.snapshot_id = "snap-1".to_string();
        assert_eq!(control.validate(), Ok(()));
    }

    #[test]
    fn the_note_limit_counts_bytes_and_admits_exactly_the_maximum() {
        let at_limit = other(
            FeedbackJudgment::Useful,
            &"a".repeat(FEEDBACK_NOTE_MAX_BYTES),
        );
        assert_eq!(at_limit.validate(), Ok(()));
        let over = other(
            FeedbackJudgment::Useful,
            &"a".repeat(FEEDBACK_NOTE_MAX_BYTES + 1),
        );
        assert_eq!(
            over.validate(),
            Err("feedback --note exceeds 1024 bytes; use a shorter note".to_string())
        );
        // 512 two-byte characters fit; one more does not.
        let multibyte = other(FeedbackJudgment::Useful, &"\u{e9}".repeat(513));
        assert!(multibyte.validate().err().is_some());
        assert_eq!(
            other(FeedbackJudgment::Useful, "a\0b").validate(),
            Err(
                "feedback --note must not contain NUL; remove the NUL character and retry"
                    .to_string()
            )
        );
        // A closed reason may still carry a note.
        let noted = FeedbackPayload {
            note: Some("context".to_string()),
            ..payload(FeedbackReason::TooSlow)
        };
        assert_eq!(noted.validate(), Ok(()));
    }

    #[test]
    fn an_unreviewed_payload_cannot_carry_a_review_actor() {
        let carried = FeedbackPayload {
            review_actor_kind: Some(ActorKind::Human),
            ..payload(FeedbackReason::TooSlow)
        };
        assert_eq!(
            carried.validate(),
            Err(
                "unreviewed feedback cannot carry --review-actor; drop it or set --review"
                    .to_string()
            )
        );
        let reviewed = FeedbackPayload {
            review_status: ReviewStatus::ReviewedRejected,
            review_actor_kind: Some(ActorKind::Agent),
            ..payload(FeedbackReason::TooSlow)
        };
        assert_eq!(reviewed.validate(), Ok(()));
    }

    #[test]
    fn canonical_bytes_list_every_identity_field_in_a_fixed_order() {
        let full = FeedbackPayload {
            identity: ResultIdentity {
                snapshot_id: "snap-1".to_string(),
                canonical_item: Some("gap:alpha".to_string()),
                route_digest: Some("route-1".to_string()),
                attempt_id: Some("attempt-2".to_string()),
                receipt_id: Some("receipt-3".to_string()),
            },
            actor_kind: ActorKind::Human,
            review_status: ReviewStatus::ReviewedAccepted,
            review_actor_kind: Some(ActorKind::Agent),
            reason: FeedbackReason::Other,
            judgment_override: Some(FeedbackJudgment::Expensive),
            note: Some("slow on CI".to_string()),
        };
        assert_eq!(
            full.canonical_bytes(),
            "v1\nsnapshot=snap-1\nitem=gap:alpha\nroute=route-1\nattempt=attempt-2\nreceipt=receipt-3\nactor=human\nreview=reviewed_accepted\nreview_actor=agent\nreason=other\njudgment=expensive\nnote=slow on CI\n"
        );
        let mut sparse = payload(FeedbackReason::TooSlow);
        sparse.identity.canonical_item = None;
        sparse.identity.route_digest = None;
        assert_eq!(
            sparse.canonical_bytes(),
            "v1\nsnapshot=snap-1\nitem=\nroute=\nattempt=\nreceipt=\nactor=agent\nreview=unreviewed\nreview_actor=\nreason=too_slow\njudgment=expensive\nnote=\n"
        );
    }
}
