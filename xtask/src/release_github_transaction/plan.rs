//! The fail-closed decision for one transaction step (ripr#1644).
//!
//! `decide` is pure: given the request, an already-verified authorization and
//! one fresh remote inventory, it names the single next mutation, completion,
//! or a terminal stop. It never retries, replaces or deletes anything.

use super::inventory::{
    AssetState, AttestationState, Classified, ReleaseState, RemoteInventory, TagState, classify,
};
use super::model::{GitHubReleaseTransactionRequest, Mode, Operation, is_hex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One subject as the authorization packet names it.
#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AuthorizedSubject {
    pub(crate) name: String,
    /// The packet's subject class. It must equal the request subject's `kind`.
    pub(crate) class: String,
    /// The packet's digest form: `sha256:` followed by 64 lowercase hex. The
    /// request holds bare hex; `decide` compares the two forms exactly.
    pub(crate) digest: String,
}

/// What the release authorization verifier (ripr#1651) established, copied
/// field for field from the verified packet. This module trusts it only as far
/// as these fields and compares each with the request; producing it is not
/// done here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifiedAuthorization {
    pub(crate) authorization_id: String,
    /// Packet `profile`: `public_rc` for a public RC, `stable_core` for stable.
    pub(crate) profile: String,
    /// Packet `transaction.kind`: must be `github`.
    pub(crate) transaction_kind: String,
    pub(crate) transaction_request_id: String,
    /// Packet `source.commit`: must be the request's candidate.
    pub(crate) source_commit: String,
    /// Packet `event.ref`: must be `refs/tags/<request tag>`.
    pub(crate) event_ref: String,
    /// Id of the annotated tag object the authorization event pushes
    /// (ripr#1652). Its bytes derive from the packet, so the id is known
    /// before the push. It lives here rather than in the request because the
    /// packet names the request id, and the reverse would be a digest cycle.
    pub(crate) expected_tag_object: String,
    pub(crate) allowed_operations: BTreeSet<String>,
    /// Every subject the packet names. A GitHub packet's subject catalog is
    /// limited to GitHub asset classes, so the set must equal the request's.
    pub(crate) subjects: Vec<AuthorizedSubject>,
}

impl VerifiedAuthorization {
    /// Every way this authorization fails to name `request`. Empty means it
    /// names exactly this transaction.
    fn mismatches(
        &self,
        request: &GitHubReleaseTransactionRequest,
        request_id: &str,
    ) -> Vec<String> {
        let mut mismatches = Vec::new();
        let expected_profile = match request.mode {
            Mode::PublicRc => "public_rc",
            Mode::Stable => "stable_core",
        };
        if self.profile != expected_profile {
            mismatches.push(format!(
                "the authorization profile `{}` does not match mode {:?}",
                self.profile, request.mode
            ));
        }
        if self.transaction_kind != "github" {
            mismatches.push("the authorization is not for a github transaction".to_string());
        }
        if self.transaction_request_id != request_id {
            mismatches.push("the authorization names another transaction request".to_string());
        }
        if self.source_commit != request.candidate_sha {
            mismatches.push("the authorization names another source commit".to_string());
        }
        if self.event_ref != format!("refs/tags/{}", request.tag) {
            mismatches.push("the authorization names another tag ref".to_string());
        }
        if !is_hex(&self.expected_tag_object, 40) {
            mismatches.push("expected_tag_object must be 40 lowercase hex".to_string());
        }
        let mut expected: Vec<AuthorizedSubject> = request
            .subjects
            .iter()
            .map(|subject| AuthorizedSubject {
                name: subject.name.clone(),
                class: subject.kind.clone(),
                digest: format!("sha256:{}", subject.sha256),
            })
            .collect();
        expected.sort();
        let mut authorized = self.subjects.clone();
        authorized.sort();
        if authorized != expected {
            mismatches.push("the authorization names a different subject set".to_string());
        }
        mismatches
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "step")]
pub(crate) enum Step {
    CreateTag,
    CreateDraftRelease,
    UploadSubject {
        name: String,
    },
    FinalizeRelease {
        release_id: u64,
    },
    /// Every object is exact and public. Nothing is written.
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StopKind {
    RequestRejected,
    AuthorizationMismatch,
    NotAuthorized,
    RemoteUnavailable,
    RemoteConflict,
    AttestationNotVerified,
    /// A public release is missing an expected subject. This is an incident,
    /// not a resumable state: nothing is uploaded to a finalized release.
    PartialPublic,
    /// The inventory observed another repository or tag than the request
    /// names, so none of its absences say anything about this transaction.
    InventoryMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Stop {
    pub(crate) kind: StopKind,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "decision")]
pub(crate) enum Decision {
    Proceed { step: Step },
    Stop { stop: Stop },
}

pub(crate) const DECISION_SCHEMA: &str = "ripr.release.github_transaction.decision.v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Planned {
    /// Always [`DECISION_SCHEMA`].
    pub(crate) schema: &'static str,
    pub(crate) request_id: Option<String>,
    pub(crate) authorization_id: String,
    pub(crate) classified: Option<Classified>,
    pub(crate) decision: Decision,
}

fn stop(kind: StopKind, detail: impl Into<String>) -> Decision {
    Decision::Stop {
        stop: Stop {
            kind,
            detail: detail.into(),
        },
    }
}

fn authorized(authorization: &VerifiedAuthorization, operation: Operation, step: Step) -> Decision {
    if authorization.allowed_operations.contains(operation.name()) {
        Decision::Proceed { step }
    } else {
        stop(
            StopKind::NotAuthorized,
            format!("the authorization does not allow {}", operation.name()),
        )
    }
}

fn decide_classified(
    request: &GitHubReleaseTransactionRequest,
    authorization: &VerifiedAuthorization,
    inventory: &RemoteInventory,
    classified: &Classified,
) -> Decision {
    match classified.tag {
        TagState::UnknownOrUnavailable => {
            return stop(StopKind::RemoteUnavailable, "tag state could not be read");
        }
        TagState::ConflictingTargetOrObject => {
            return stop(
                StopKind::RemoteConflict,
                "the tag exists but is not the authorized annotated tag object at the candidate",
            );
        }
        TagState::Absent | TagState::ExactCandidate => {}
    }
    match classified.release {
        ReleaseState::UnknownOrUnavailable => {
            return stop(
                StopKind::RemoteUnavailable,
                "release state could not be read",
            );
        }
        ReleaseState::DuplicateOrAmbiguous => {
            return stop(
                StopKind::RemoteConflict,
                "more than one release is bound to the tag",
            );
        }
        ReleaseState::ConflictingTagTarget => {
            return stop(
                StopKind::RemoteConflict,
                "the release is bound to another tag",
            );
        }
        ReleaseState::ConflictingTitleBodyFlags => {
            return stop(
                StopKind::RemoteConflict,
                "the release title, body or prerelease flag differs from the request",
            );
        }
        ReleaseState::Absent | ReleaseState::ExactDraft | ReleaseState::ExactPublic => {}
    }
    if let Some(name) = classified.unexpected_assets.first() {
        return stop(
            StopKind::RemoteConflict,
            format!("the release carries an unexpected asset `{name}`"),
        );
    }
    for (name, state) in &classified.assets {
        match state {
            AssetState::Absent | AssetState::ExactPublicBytes => {}
            AssetState::Unavailable => {
                return stop(
                    StopKind::RemoteUnavailable,
                    format!("the bytes of `{name}` could not be read"),
                );
            }
            AssetState::ConflictingBytes => {
                return stop(
                    StopKind::RemoteConflict,
                    format!("`{name}` exists with other bytes; it is never replaced"),
                );
            }
            AssetState::DuplicateOrAmbiguous => {
                return stop(
                    StopKind::RemoteConflict,
                    format!("`{name}` exists more than once"),
                );
            }
        }
    }
    if classified.tag == TagState::Absent {
        if classified.release != ReleaseState::Absent {
            return stop(
                StopKind::RemoteConflict,
                "a release exists for a tag that does not",
            );
        }
        return authorized(
            authorization,
            Operation::CreateAnnotatedTag,
            Step::CreateTag,
        );
    }
    let release_operation = request.release_operation();
    let Some(release_id) = classified.release_id else {
        return authorized(authorization, release_operation, Step::CreateDraftRelease);
    };
    let first_absent = classified
        .assets
        .iter()
        .find(|(_, state)| *state == AssetState::Absent)
        .map(|(name, _)| name.clone());
    if classified.release == ReleaseState::ExactPublic {
        if let Some(name) = first_absent {
            return stop(
                StopKind::PartialPublic,
                format!("the public release is missing `{name}`"),
            );
        }
    } else if let Some(name) = first_absent {
        return authorized(
            authorization,
            Operation::GithubAssetUpload,
            Step::UploadSubject { name },
        );
    }
    for subject in &request.subjects {
        // Exactly one observation, and it is verified. Two observations for
        // one subject are ambiguous whatever their order.
        let states: Vec<AttestationState> = inventory
            .attestations
            .iter()
            .filter(|observation| observation.subject == subject.name)
            .map(|observation| observation.state)
            .collect();
        match states.as_slice() {
            [AttestationState::Verified] => {}
            [state] => {
                return stop(
                    StopKind::AttestationNotVerified,
                    format!("provenance for `{}` is {state:?}", subject.name),
                );
            }
            [] => {
                return stop(
                    StopKind::AttestationNotVerified,
                    format!("provenance for `{}` is Missing", subject.name),
                );
            }
            _ => {
                return stop(
                    StopKind::AttestationNotVerified,
                    format!(
                        "provenance for `{}` has {} observations; one is required",
                        subject.name,
                        states.len()
                    ),
                );
            }
        }
    }
    if classified.release == ReleaseState::ExactPublic {
        Decision::Proceed {
            step: Step::Complete,
        }
    } else {
        authorized(
            authorization,
            release_operation,
            Step::FinalizeRelease { release_id },
        )
    }
}

/// Decide the next step from one fresh inventory.
pub(crate) fn decide(
    request: &GitHubReleaseTransactionRequest,
    authorization: &VerifiedAuthorization,
    inventory: &RemoteInventory,
) -> Planned {
    let planned = |request_id, classified, decision| Planned {
        schema: DECISION_SCHEMA,
        request_id,
        authorization_id: authorization.authorization_id.clone(),
        classified,
        decision,
    };
    let violations = request.violations();
    if !violations.is_empty() {
        return planned(
            None,
            None,
            stop(StopKind::RequestRejected, violations.join("; ")),
        );
    }
    let request_id = match request.request_id() {
        Ok(id) => id,
        Err(error) => return planned(None, None, stop(StopKind::RequestRejected, error)),
    };
    let mismatches = authorization.mismatches(request, &request_id);
    if !mismatches.is_empty() {
        return planned(
            Some(request_id),
            None,
            stop(StopKind::AuthorizationMismatch, mismatches.join("; ")),
        );
    }
    if inventory.repository != request.repository
        || inventory.tag_name != request.tag
        || inventory.provenance != request.provenance
    {
        return planned(
            Some(request_id),
            None,
            stop(
                StopKind::InventoryMismatch,
                format!(
                    "the inventory observed {}@{} under signer {} at {}, not {}@{} under signer {} at {}",
                    inventory.repository,
                    inventory.tag_name,
                    inventory.provenance.signer_workflow,
                    inventory.provenance.source_ref,
                    request.repository,
                    request.tag,
                    request.provenance.signer_workflow,
                    request.provenance.source_ref
                ),
            ),
        );
    }
    let classified = classify(request, &authorization.expected_tag_object, inventory);
    let decision = decide_classified(request, authorization, inventory, &classified);
    planned(Some(request_id), Some(classified), decision)
}
