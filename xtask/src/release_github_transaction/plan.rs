//! The fail-closed decision for one transaction step (ripr#1644).
//!
//! `decide` is pure: given the request, an already-verified authorization and
//! one fresh remote inventory, it names the single next mutation, completion,
//! or a terminal stop. It never retries, replaces or deletes anything.

use super::inventory::{
    AssetState, AttestationState, Classified, ReleaseState, RemoteInventory, TagState, classify,
};
use super::model::{GitHubReleaseTransactionRequest, Operation};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What the release authorization verifier (ripr#1651) established. This
/// module trusts it only as far as these fields; producing it is not done here.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerifiedAuthorization {
    pub(crate) authorization_id: String,
    pub(crate) transaction_request_id: String,
    pub(crate) allowed_operations: BTreeSet<String>,
    /// `(name, sha256)` of every subject the packet names, in packet order.
    pub(crate) subjects: Vec<(String, String)>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Planned {
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
                "the tag exists but is not an annotated tag at the candidate",
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
        let state = inventory
            .attestations
            .iter()
            .find(|observation| observation.subject == subject.name)
            .map_or(AttestationState::Missing, |observation| observation.state);
        if state != AttestationState::Verified {
            return stop(
                StopKind::AttestationNotVerified,
                format!("provenance for `{}` is {state:?}", subject.name),
            );
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
    if authorization.transaction_request_id != request_id {
        return planned(
            Some(request_id),
            None,
            stop(
                StopKind::AuthorizationMismatch,
                "the authorization names another transaction request",
            ),
        );
    }
    let expected: Vec<(String, String)> = request
        .subjects
        .iter()
        .map(|subject| (subject.name.clone(), subject.sha256.clone()))
        .collect();
    let mut authorized_subjects = authorization.subjects.clone();
    authorized_subjects.sort();
    if authorized_subjects != expected {
        return planned(
            Some(request_id),
            None,
            stop(
                StopKind::AuthorizationMismatch,
                "the authorization names a different subject set",
            ),
        );
    }
    let classified = classify(request, inventory);
    let decision = decide_classified(request, authorization, inventory, &classified);
    planned(Some(request_id), Some(classified), decision)
}
