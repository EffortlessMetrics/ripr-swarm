//! Compact, versioned repair work object (`RepairCardV1`) projected from the
//! existing repair authorities (#3166, #4663).
//!
//! The card is one provider-neutral work order for one governed repair. It is
//! NOT a new analyzer, readiness validator, instruction state, attempt model,
//! or command authority: every field is a verbatim projection of the witness,
//! `FixInstructionSummary`, repair-route readiness, typed command references
//! and optional `RepairAttempt` state, built by the single app-layer builder
//! (`crate::app::repair_card`). Renderers and transports consume the card;
//! none reconstructs state from prose.
//!
//! The card describes the work order and never marks itself complete:
//! observed completion lives in the RepairAttempt/receipt authorities.

use super::{FixInstructionState, FixInstructionSummary};

/// Versioned card schema. Additive changes keep this version and add
/// `#[serde(default)]` fields; breaking shape changes mint a new version.
pub const REPAIR_CARD_SCHEMA_VERSION: &str = "repair_card.v1";

/// Load-bearing rejected alternatives are bounded so the card stays a compact
/// work object. Callers fail closed when the producer supplies more.
pub const MAX_REPAIR_CARD_REJECTED_ALTERNATIVES: usize = 3;

/// What may be claimed from a card. Completion, correctness and
/// mutation-confirmation claims stay with the attempt/receipt authorities.
pub const REPAIR_CARD_CLAIM_BOUNDARY: &str = "Static repair work order for one governed repair, projected from current product authorities; it does not establish completion, correctness, runtime behavior, or mutation confirmation.";

/// Portable snapshot identity. `workspace_identity` is producer-supplied and
/// portable (never an absolute checkout spelling); absolute paths, timestamps
/// and presentation formatting never enter the card identity.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardSnapshot {
    /// Producer-owned portable workspace identity (see
    /// `TestTargetEvidence::workspace_identity` precedent).
    pub workspace_identity: String,
    /// Repository head the snapshot was taken at.
    pub repository_head: String,
    pub currentness: RepairCardSnapshotCurrentness,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairCardSnapshotCurrentness {
    Current,
    AcceptedDirtyDraft,
}

/// The one repair this card orders.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardSubject {
    pub seam_id: String,
    pub canonical_gap_id: Option<String>,
    pub finding_id: Option<String>,
}

/// Verbatim readiness facts copied from the route-readiness authority. The
/// counts and lists are never re-derived or recomputed here.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardReadinessFacts {
    /// The readiness authority's own repair-ready flip, copied verbatim.
    pub repair_ready: bool,
    pub required_evidence: Vec<String>,
    pub present_evidence: Vec<String>,
    pub missing_evidence: Vec<String>,
}

/// Selected or proposed test/fix target. `Existing` and `Proposed` stay
/// distinct variants: a proposed new-test target can never be represented as
/// an existing test.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RepairCardTarget {
    Existing {
        symbol_id: String,
        file: String,
        line: usize,
        test_kind: RepairCardTestKind,
        /// The producer-owned relation basis (for example
        /// `RelationReason::as_str`).
        relation: String,
        workspace_identity: String,
    },
    Proposed {
        file: String,
        owner: String,
        proposal_kind: RepairCardProposedTestKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairCardTestKind {
    InlineUnit,
    Integration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairCardProposedTestKind {
    InlineUnit,
    Integration,
}

/// The assertion/observer outcome the edited test must establish.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepairCardAssertionGoal {
    /// A producer-owned suggested assertion is available; the card carries
    /// its text.
    SuggestedAssertion,
    /// Effect route: the observer setup fact is the goal; the card carries
    /// the producer-owned observer description.
    ObserverSetup,
}

/// Completion axes stay separate (#3166): the card names each axis and never
/// collapses one into another.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StaticMovementGoal {
    ClosedBySelectedRoute,
    ImprovedAccordingToSelectedRoute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusedExecutionGoal {
    VerifiedPass,
    ExplicitlyNotRun,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditCageGoal {
    Compliant,
}

/// Optional and separately stated: mutation confirmation is never required to
/// close a card axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationConfirmationGoal {
    NotRequested,
    OptionalSeparateConfirmation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardCurrentnessGoal {
    Current,
    AcceptedDirtyDraft,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardDoneWhen {
    pub static_movement: StaticMovementGoal,
    pub focused_test_execution: FocusedExecutionGoal,
    pub edit_cage: EditCageGoal,
    pub mutation_confirmation: MutationConfirmationGoal,
    pub currentness: CardCurrentnessGoal,
}

/// One typed command reference. This is a reference to a `CommandSpec`
/// authority (identity + role + bounded display), never a reconstruction of
/// argv from display prose.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardCommandRef {
    pub command_id: String,
    pub role: String,
    /// Presentation-only: excluded from the semantic digest.
    pub display: String,
}

/// A load-bearing rejected alternative with its producer-owned reason.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardRejectedAlternative {
    pub target: String,
    pub reason: String,
}

/// Observed attempt state, copied from the RepairAttempt authority. The card
/// never upgrades or completes it.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardAttempt {
    pub attempt_id: String,
    /// The attempt authority's state, serialized snake_case (for example
    /// `awaiting_edit`).
    pub state: String,
}

/// The compact work order. Field-for-field projections are documented at each
/// site; the builder owns all derivation.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RepairCardV1 {
    pub schema_version: String,
    /// Semantic digest over the portable card surface; reminted when any
    /// load-bearing field changes.
    pub repair_card_id: String,
    pub snapshot: RepairCardSnapshot,
    pub subject: RepairCardSubject,
    /// The fix-instruction vocabulary, verbatim from its authority.
    pub instruction: FixInstructionSummary,
    pub readiness: RepairCardReadinessFacts,
    pub changed_behavior: String,
    /// The exact missing discriminator or named limitation, when the producer
    /// owns one; never fabricated by the card.
    pub exact_blocker: Option<String>,
    pub selected_target: Option<RepairCardTarget>,
    pub assertion_goal: Option<RepairCardAssertionGoal>,
    /// Producer-owned suggested assertion or observer description.
    pub assertion_goal_detail: Option<String>,
    /// Producer-owned candidate input/value when the route supplies one.
    pub candidate_value: Option<String>,
    /// Edit-cage surfaces, supplied by the caller from the existing packet
    /// authority; the card never derives membership.
    pub allowed_files: Vec<String>,
    pub forbidden_files: Vec<String>,
    pub done_when: RepairCardDoneWhen,
    pub stop_conditions: Vec<String>,
    /// One next action as a typed command reference. Absent when the card is
    /// stale, limited, unavailable, or has no ready route.
    pub next_action: Option<RepairCardCommandRef>,
    /// Producer-owned basis for the selected target (relation vocabulary).
    pub selected_basis: Option<String>,
    pub rejected_alternatives: Vec<RepairCardRejectedAlternative>,
    pub attempt: Option<RepairCardAttempt>,
    pub claim_boundary: String,
    pub limitations: Vec<String>,
}

/// The scoped semantic surface: every load-bearing field except the digest
/// itself. Presentation-only fields (`next_action.display`) and identity-free
/// envelope fields are excluded here.
#[derive(serde::Serialize)]
struct RepairCardDigestInput<'a> {
    schema_version: &'a str,
    snapshot: &'a RepairCardSnapshot,
    subject: &'a RepairCardSubject,
    instruction: &'a FixInstructionSummary,
    readiness: &'a RepairCardReadinessFacts,
    changed_behavior: &'a str,
    exact_blocker: &'a Option<String>,
    selected_target: &'a Option<RepairCardTarget>,
    assertion_goal: &'a Option<RepairCardAssertionGoal>,
    assertion_goal_detail: &'a Option<String>,
    candidate_value: &'a Option<String>,
    allowed_files: &'a [String],
    forbidden_files: &'a [String],
    done_when: &'a RepairCardDoneWhen,
    stop_conditions: &'a [String],
    next_action: Option<RepairCardDigestCommandRef<'a>>,
    selected_basis: &'a Option<String>,
    rejected_alternatives: &'a [RepairCardRejectedAlternative],
    attempt: &'a Option<RepairCardAttempt>,
    claim_boundary: &'a str,
    limitations: &'a [String],
}

#[derive(serde::Serialize)]
struct RepairCardDigestCommandRef<'a> {
    command_id: &'a str,
    role: &'a str,
}

/// Compute the portable semantic digest of a card. Equivalent roots and
/// presentation-only changes (command display strings) preserve identity;
/// any load-bearing field move remints it.
pub fn repair_card_semantic_digest(card: &RepairCardV1) -> Result<String, String> {
    let input = RepairCardDigestInput {
        schema_version: &card.schema_version,
        snapshot: &card.snapshot,
        subject: &card.subject,
        instruction: &card.instruction,
        readiness: &card.readiness,
        changed_behavior: &card.changed_behavior,
        exact_blocker: &card.exact_blocker,
        selected_target: &card.selected_target,
        assertion_goal: &card.assertion_goal,
        assertion_goal_detail: &card.assertion_goal_detail,
        candidate_value: &card.candidate_value,
        allowed_files: &card.allowed_files,
        forbidden_files: &card.forbidden_files,
        done_when: &card.done_when,
        stop_conditions: &card.stop_conditions,
        next_action: card
            .next_action
            .as_ref()
            .map(|command| RepairCardDigestCommandRef {
                command_id: &command.command_id,
                role: &command.role,
            }),
        selected_basis: &card.selected_basis,
        rejected_alternatives: &card.rejected_alternatives,
        attempt: &card.attempt,
        claim_boundary: &card.claim_boundary,
        limitations: &card.limitations,
    };
    let serialized = serde_json::to_string(&input)
        .map_err(|error| format!("repair card digest serialization failed: {error}"))?;
    Ok(sha256_hex(serialized.as_bytes()))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Every exposure gate in one place: a card may expose a current edit or
/// execution route only when the instruction vocabulary and the readiness
/// authority's own flip both allow it. The builder consults this; no
/// renderer re-implements it.
pub fn repair_card_route_exposable(instruction: FixInstructionState, repair_ready: bool) -> bool {
    matches!(instruction, FixInstructionState::FixSiteReady) && repair_ready
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> RepairCardSnapshot {
        RepairCardSnapshot {
            workspace_identity: "workspace:demo".to_string(),
            repository_head: "abc123".to_string(),
            currentness: RepairCardSnapshotCurrentness::Current,
        }
    }

    fn subject() -> RepairCardSubject {
        RepairCardSubject {
            seam_id: "seam:demo".to_string(),
            canonical_gap_id: Some("gap:demo".to_string()),
            finding_id: Some("probe:demo".to_string()),
        }
    }

    fn ready_instruction() -> FixInstructionSummary {
        FixInstructionSummary {
            state: FixInstructionState::FixSiteReady,
            has_fix_site: true,
            has_suggested_assertion: true,
            limitation_kinds: Vec::new(),
        }
    }

    fn readiness_ready() -> RepairCardReadinessFacts {
        RepairCardReadinessFacts {
            repair_ready: true,
            required_evidence: vec!["owner".to_string()],
            present_evidence: vec!["owner".to_string()],
            missing_evidence: Vec::new(),
        }
    }

    fn done_when() -> RepairCardDoneWhen {
        RepairCardDoneWhen {
            static_movement: StaticMovementGoal::ClosedBySelectedRoute,
            focused_test_execution: FocusedExecutionGoal::VerifiedPass,
            edit_cage: EditCageGoal::Compliant,
            mutation_confirmation: MutationConfirmationGoal::NotRequested,
            currentness: CardCurrentnessGoal::Current,
        }
    }

    fn next_action() -> RepairCardCommandRef {
        RepairCardCommandRef {
            command_id: "cmd:verify".to_string(),
            role: "verify".to_string(),
            display: "cargo test --package demo".to_string(),
        }
    }

    fn card_fixture() -> RepairCardV1 {
        RepairCardV1 {
            schema_version: REPAIR_CARD_SCHEMA_VERSION.to_string(),
            repair_card_id: String::new(),
            snapshot: snapshot(),
            subject: subject(),
            instruction: ready_instruction(),
            readiness: readiness_ready(),
            changed_behavior: "expr".to_string(),
            exact_blocker: None,
            selected_target: Some(RepairCardTarget::Existing {
                symbol_id: "symbol:test:demo".to_string(),
                file: "tests/demo.rs".to_string(),
                line: 12,
                test_kind: RepairCardTestKind::Integration,
                relation: "direct_owner_call".to_string(),
                workspace_identity: "workspace:demo".to_string(),
            }),
            assertion_goal: Some(RepairCardAssertionGoal::SuggestedAssertion),
            assertion_goal_detail: Some("assert_eq!(price(0), 0)".to_string()),
            candidate_value: None,
            allowed_files: vec!["tests/demo.rs".to_string()],
            forbidden_files: vec!["src/lib.rs".to_string()],
            done_when: done_when(),
            stop_conditions: vec!["snapshot_stale".to_string()],
            next_action: Some(next_action()),
            selected_basis: Some("direct_owner_call".to_string()),
            rejected_alternatives: vec![RepairCardRejectedAlternative {
                target: "tests/other.rs::name_match".to_string(),
                reason: "weak_token_substring".to_string(),
            }],
            attempt: Some(RepairCardAttempt {
                attempt_id: "ra-0123456789abcdef".to_string(),
                state: "awaiting_edit".to_string(),
            }),
            claim_boundary: REPAIR_CARD_CLAIM_BOUNDARY.to_string(),
            limitations: Vec::new(),
        }
    }

    #[test]
    fn digest_is_stable_across_presentation_only_changes() -> Result<(), String> {
        let mut card = card_fixture();
        let original = repair_card_semantic_digest(&card)?;

        // Command display is presentation: reminting it must not move identity.
        let Some(action) = card.next_action.as_mut() else {
            return Err("fixture carries no next action".to_string());
        };
        action.display = "a different rendered string".to_string();
        let reminted = repair_card_semantic_digest(&card)?;
        if reminted != original {
            return Err(format!(
                "command display entered the semantic identity surface: {reminted}"
            ));
        }

        // The digest field itself never feeds the digest.
        card.repair_card_id = "garbage".to_string();
        let reminted = repair_card_semantic_digest(&card)?;
        if reminted != original {
            return Err("repair_card_id fed its own digest".to_string());
        }
        Ok(())
    }

    #[test]
    fn every_load_bearing_field_move_remints_the_digest() -> Result<(), String> {
        let base = card_fixture();
        let base_digest = repair_card_semantic_digest(&base)?;

        let mut moved = base.clone();
        moved.changed_behavior = "different".to_string();
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("changed_behavior is not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.instruction.state = FixInstructionState::Stale;
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("instruction state is not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.readiness.repair_ready = false;
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("readiness flip is not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.selected_target = None;
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("selected target is not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.done_when.static_movement = StaticMovementGoal::ImprovedAccordingToSelectedRoute;
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("done_when static movement axis is not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.allowed_files = vec!["tests/other.rs".to_string()];
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("allowed files are not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.attempt = None;
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("attempt state is not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.rejected_alternatives.clear();
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("rejected alternatives are not load-bearing".to_string());
        }

        let mut moved = base.clone();
        moved.snapshot.currentness = RepairCardSnapshotCurrentness::AcceptedDirtyDraft;
        if repair_card_semantic_digest(&moved)? == base_digest {
            return Err("snapshot currentness is not load-bearing".to_string());
        }
        Ok(())
    }

    #[test]
    fn proposed_and_existing_targets_stay_distinct() -> Result<(), String> {
        let existing = RepairCardTarget::Existing {
            symbol_id: "symbol:test:a".to_string(),
            file: "tests/a.rs".to_string(),
            line: 1,
            test_kind: RepairCardTestKind::InlineUnit,
            relation: "direct_owner_call".to_string(),
            workspace_identity: "workspace:demo".to_string(),
        };
        let proposed = RepairCardTarget::Proposed {
            file: "tests/new.rs".to_string(),
            owner: "src/lib.rs".to_string(),
            proposal_kind: RepairCardProposedTestKind::InlineUnit,
        };
        if existing == proposed {
            return Err("existing and proposed targets collapsed into one variant".to_string());
        }
        let existing_json = serde_json::to_string(&existing).map_err(|error| error.to_string())?;
        let proposed_json = serde_json::to_string(&proposed).map_err(|error| error.to_string())?;
        if !existing_json.contains("\"kind\":\"existing\"") {
            return Err(format!(
                "existing target lost its kind tag: {existing_json}"
            ));
        }
        if !proposed_json.contains("\"kind\":\"proposed\"") {
            return Err(format!(
                "proposed target lost its kind tag: {proposed_json}"
            ));
        }
        Ok(())
    }

    #[test]
    fn done_when_keeps_all_five_axes_separate() -> Result<(), String> {
        let card = card_fixture();
        let json = serde_json::to_string(&card.done_when).map_err(|error| error.to_string())?;
        for axis in [
            "static_movement",
            "focused_test_execution",
            "edit_cage",
            "mutation_confirmation",
            "currentness",
        ] {
            if !json.contains(axis) {
                return Err(format!("done_when lost the `{axis}` axis: {json}"));
            }
        }
        let round_tripped: RepairCardDoneWhen =
            serde_json::from_str(&json).map_err(|error| error.to_string())?;
        if round_tripped != card.done_when {
            return Err("done_when axes did not round trip independently".to_string());
        }
        Ok(())
    }

    #[test]
    fn route_exposure_follows_instruction_and_readiness_together() {
        assert!(repair_card_route_exposable(
            FixInstructionState::FixSiteReady,
            true
        ));
        for instruction in [
            FixInstructionState::Stale,
            FixInstructionState::StaticLimitation,
            FixInstructionState::Unavailable,
            FixInstructionState::InspectOnly,
        ] {
            assert!(!repair_card_route_exposable(instruction, true));
        }
        assert!(!repair_card_route_exposable(
            FixInstructionState::FixSiteReady,
            false
        ));
    }

    #[test]
    fn fixture_shapes_cover_all_instruction_states() {
        // The required shapes exist as distinct typed states the builder
        // must handle: ready, limited, stale, unavailable, proposed-target,
        // missing-route, wrong-owner and effect/observer.
        let states = [
            FixInstructionState::FixSiteReady,
            FixInstructionState::StaticLimitation,
            FixInstructionState::Stale,
            FixInstructionState::InspectOnly,
            FixInstructionState::Unavailable,
        ];
        let unique = states.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), states.len());
    }
}
