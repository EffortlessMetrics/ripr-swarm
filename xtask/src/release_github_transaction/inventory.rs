//! Classification of observed remote state against one request (ripr#1644).
//!
//! Observations come from a transport. "Could not read" is its own state and
//! is never treated as "absent".

use super::model::{GitHubReleaseTransactionRequest, Provenance, normalized_name};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub(crate) enum TagObservation {
    Absent,
    /// `object_sha` is the tag object itself; `peeled_sha` is the commit it
    /// names. A lightweight tag's object is the commit.
    Present {
        object_sha: String,
        peeled_sha: String,
        annotated: bool,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub(crate) enum BytesObservation {
    /// SHA-256 of the bytes actually downloaded, never API metadata.
    Downloaded {
        sha256: String,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteAsset {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) bytes: BytesObservation,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteRelease {
    pub(crate) id: u64,
    pub(crate) tag: String,
    pub(crate) draft: bool,
    pub(crate) prerelease: bool,
    pub(crate) title: String,
    pub(crate) body_sha256: String,
    pub(crate) assets: Vec<RemoteAsset>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub(crate) enum ReleasesObservation {
    /// Every release bound to the request's tag, drafts included.
    Listed {
        releases: Vec<RemoteRelease>,
    },
    Unavailable {
        reason: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AttestationState {
    Verified,
    Missing,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AttestationObservation {
    pub(crate) subject: String,
    pub(crate) state: AttestationState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteInventory {
    /// The `owner/name` the observations were read from.
    pub(crate) repository: String,
    /// The tag name the observations were read for. Both are recorded even
    /// when nothing was found, so an empty inventory for another target
    /// cannot stand in for this one.
    pub(crate) tag_name: String,
    /// The signer workflow and source ref the attestations were verified
    /// against. A verdict under another policy says nothing about this one.
    pub(crate) provenance: Provenance,
    pub(crate) tag: TagObservation,
    pub(crate) releases: ReleasesObservation,
    /// Provenance verification per expected subject. A subject with no entry
    /// is unverified.
    pub(crate) attestations: Vec<AttestationObservation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TagState {
    Absent,
    ExactCandidate,
    ConflictingTargetOrObject,
    UnknownOrUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ReleaseState {
    Absent,
    ExactDraft,
    ExactPublic,
    ConflictingTagTarget,
    ConflictingTitleBodyFlags,
    DuplicateOrAmbiguous,
    UnknownOrUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AssetState {
    Absent,
    ExactPublicBytes,
    ConflictingBytes,
    DuplicateOrAmbiguous,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Classified {
    pub(crate) tag: TagState,
    pub(crate) release: ReleaseState,
    pub(crate) release_id: Option<u64>,
    /// One entry per expected subject, in request order.
    pub(crate) assets: Vec<(String, AssetState)>,
    /// Remote asset names the request does not expect.
    pub(crate) unexpected_assets: Vec<String>,
}

/// `expected_tag_object` is the id of the authorized annotated tag object.
/// Another annotated tag at the right commit (another tagger, date or
/// message) is a conflict, not the authorized tag.
pub(crate) fn classify(
    request: &GitHubReleaseTransactionRequest,
    expected_tag_object: &str,
    inventory: &RemoteInventory,
) -> Classified {
    let tag = match &inventory.tag {
        TagObservation::Absent => TagState::Absent,
        TagObservation::Unavailable { .. } => TagState::UnknownOrUnavailable,
        TagObservation::Present {
            object_sha,
            peeled_sha,
            annotated,
        } => {
            if *annotated
                && *peeled_sha == request.candidate_sha
                && object_sha == expected_tag_object
            {
                TagState::ExactCandidate
            } else {
                TagState::ConflictingTargetOrObject
            }
        }
    };
    let absent_assets = || {
        request
            .subjects
            .iter()
            .map(|subject| (subject.name.clone(), AssetState::Absent))
            .collect::<Vec<_>>()
    };
    let unknown_assets = || {
        request
            .subjects
            .iter()
            .map(|subject| (subject.name.clone(), AssetState::Unavailable))
            .collect::<Vec<_>>()
    };
    let releases = match &inventory.releases {
        ReleasesObservation::Unavailable { .. } => {
            return Classified {
                tag,
                release: ReleaseState::UnknownOrUnavailable,
                release_id: None,
                assets: unknown_assets(),
                unexpected_assets: Vec::new(),
            };
        }
        ReleasesObservation::Listed { releases } => releases,
    };
    let release = match releases.as_slice() {
        [] => {
            return Classified {
                tag,
                release: ReleaseState::Absent,
                release_id: None,
                assets: absent_assets(),
                unexpected_assets: Vec::new(),
            };
        }
        [release] => release,
        _ => {
            return Classified {
                tag,
                release: ReleaseState::DuplicateOrAmbiguous,
                release_id: None,
                assets: unknown_assets(),
                unexpected_assets: Vec::new(),
            };
        }
    };
    let release_state = if release.tag != request.tag {
        ReleaseState::ConflictingTagTarget
    } else if release.title != request.title
        || release.body_sha256 != request.body_sha256
        || release.prerelease != request.prerelease
    {
        ReleaseState::ConflictingTitleBodyFlags
    } else if release.draft {
        ReleaseState::ExactDraft
    } else {
        ReleaseState::ExactPublic
    };
    let mut by_name: BTreeMap<String, Vec<&RemoteAsset>> = BTreeMap::new();
    for asset in &release.assets {
        by_name
            .entry(normalized_name(&asset.name))
            .or_default()
            .push(asset);
    }
    let mut assets = Vec::new();
    for subject in &request.subjects {
        let state = match by_name.remove(&normalized_name(&subject.name)).as_deref() {
            None => AssetState::Absent,
            Some([asset]) => match &asset.bytes {
                BytesObservation::Unavailable { .. } => AssetState::Unavailable,
                // The spelled name must match too: a case-variant name is a
                // different public URL carrying this subject's identity.
                BytesObservation::Downloaded { sha256 } => {
                    if asset.name == subject.name
                        && asset.size == subject.size
                        && *sha256 == subject.sha256
                    {
                        AssetState::ExactPublicBytes
                    } else {
                        AssetState::ConflictingBytes
                    }
                }
            },
            Some(_) => AssetState::DuplicateOrAmbiguous,
        };
        assets.push((subject.name.clone(), state));
    }
    let unexpected_assets = by_name
        .into_values()
        .flatten()
        .map(|asset| asset.name.clone())
        .collect();
    Classified {
        tag,
        release: release_state,
        release_id: Some(release.id),
        assets,
        unexpected_assets,
    }
}
