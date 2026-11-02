use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub(crate) const VERIFICATION_RUN_SNAPSHOT_BINDING_SCHEMA_VERSION: &str =
    "verification_run_snapshot_binding.v1";

pub(crate) const REPOSITORY_SNAPSHOT_ENVELOPE_V2_AUTHORITY: &str =
    "effortless-repo-snapshot/RepositorySnapshotEnvelopeV2";

pub(crate) const VERIFICATION_RUN_SNAPSHOT_BINDING_CLAIM_BOUNDARY: &str = "Pure requested/prepared snapshot binding and compatibility over accepted shared snapshot \
     identities; no acquisition, materialization, provider execution, or live-worktree \
     verification claim.";

const SHA256_PREFIX: &str = "sha256:";

/// Portable content digest (`sha256:<64 lowercase hex>`).
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct VerifiedContentDigest(String);

impl VerifiedContentDigest {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        let Some(hex) = value.strip_prefix(SHA256_PREFIX) else {
            return Err(format!("digest must start with `{SHA256_PREFIX}`"));
        };
        if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("digest hex must be 64 characters".to_string());
        }
        Ok(Self(format!("{SHA256_PREFIX}{}", hex.to_ascii_lowercase())))
    }

    pub(crate) fn from_hex(hex: &str) -> Result<Self, String> {
        Self::parse(&format!("{SHA256_PREFIX}{hex}"))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reference to the accepted shared neutral snapshot envelope (not a rival DTO).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RepositorySnapshotEnvelopeRefV2 {
    pub authority: String,
    pub envelope_digest: VerifiedContentDigest,
}

impl RepositorySnapshotEnvelopeRefV2 {
    pub(crate) fn new(envelope_digest: VerifiedContentDigest) -> Self {
        Self {
            authority: REPOSITORY_SNAPSHOT_ENVELOPE_V2_AUTHORITY.to_string(),
            envelope_digest,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SharedAcquisitionReceiptRefV1 {
    pub receipt_digest: VerifiedContentDigest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortableSourceContentIdentityV1 {
    pub content_digest: VerifiedContentDigest,
    pub inventory_digest: VerifiedContentDigest,
}

/// Non-portable execution metadata retained outside portable digests.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreparedRootMetadataV1 {
    pub concrete_read_root: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationSourceSubjectKindV2 {
    CommittedHead,
    CommittedRange,
    StagedIndex,
    SavedWorktree,
    Overlay,
    CapturedExternal,
}

impl VerificationSourceSubjectKindV2 {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::CommittedHead => "committed_head",
            Self::CommittedRange => "committed_range",
            Self::StagedIndex => "staged_index",
            Self::SavedWorktree => "saved_worktree",
            Self::Overlay => "overlay",
            Self::CapturedExternal => "captured_external",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SubjectCapturePolicyV1 {
    pub include_untracked_when_selected: bool,
    pub include_ignored_when_selected: bool,
    pub admit_symlinks: bool,
    pub admit_submodules: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SubjectCompleteness {
    Complete,
    Partial,
    Unsupported,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BindingCompleteness {
    Complete,
    Partial,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaggedSubjectIdentitiesV1 {
    pub repository: String,
    pub commit: Option<String>,
    pub tree: Option<String>,
    pub base: Option<String>,
    pub head: Option<String>,
    pub index: Option<String>,
    pub worktree: Option<String>,
    pub overlay_set: Option<String>,
    pub overlay_base: Option<String>,
    pub captured_external: Option<String>,
    pub staged_deletions: Vec<String>,
    pub extra: BTreeMap<String, String>,
}

impl TaggedSubjectIdentitiesV1 {
    pub(crate) fn normalize(&mut self) {
        self.staged_deletions.sort();
        self.staged_deletions.dedup();
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RequestedVerificationSubjectV1 {
    pub envelope: RepositorySnapshotEnvelopeRefV2,
    pub subject_kind: VerificationSourceSubjectKindV2,
    pub identities: TaggedSubjectIdentitiesV1,
    pub selected_closure_digest: VerifiedContentDigest,
    pub closure_complete: bool,
    pub decision_bearing_inputs_digest: VerifiedContentDigest,
    pub resource_configuration_digest: Option<VerifiedContentDigest>,
    pub policy: SubjectCapturePolicyV1,
    pub limitations: Vec<String>,
    pub unsupported_entries: Vec<String>,
}

impl RequestedVerificationSubjectV1 {
    pub(crate) fn normalize(&mut self) {
        self.identities.normalize();
        self.limitations.sort();
        self.limitations.dedup();
        self.unsupported_entries.sort();
        self.unsupported_entries.dedup();
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PreparedVerificationSubjectRefV1 {
    pub envelope: RepositorySnapshotEnvelopeRefV2,
    pub acquisition_receipt: SharedAcquisitionReceiptRefV1,
    pub subject_kind: VerificationSourceSubjectKindV2,
    pub identities: TaggedSubjectIdentitiesV1,
    pub portable_content: PortableSourceContentIdentityV1,
    pub selected_closure_digest: VerifiedContentDigest,
    pub closure_complete: bool,
    pub decision_bearing_inputs_digest: VerifiedContentDigest,
    pub resource_configuration_digest: Option<VerifiedContentDigest>,
    pub prepared_root: PreparedRootMetadataV1,
    pub completeness: SubjectCompleteness,
    pub policy: SubjectCapturePolicyV1,
    pub limitations: Vec<String>,
    pub unsupported_entries: Vec<String>,
}

impl PreparedVerificationSubjectRefV1 {
    pub(crate) fn normalize(&mut self) {
        self.identities.normalize();
        self.limitations.sort();
        self.limitations.dedup();
        self.unsupported_entries.sort();
        self.unsupported_entries.dedup();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CompatibilityMismatchAxis {
    SubjectKind,
    EnvelopeDigest,
    RepositoryIdentity,
    BaseHeadIdentity,
    IndexIdentity,
    WorktreeIdentity,
    OverlayIdentity,
    CapturedExternalIdentity,
    StagedDeletionSet,
    SelectedClosure,
    DecisionBearingInputs,
    ResourceConfiguration,
    CommittedVsDirtyWorktree,
    StagedVsWorktreeReplacement,
    UnsupportedOrPartialInventory,
    PlanIdentity,
    PortableContentIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum VerificationSnapshotCompatibilityV1 {
    Compatible {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accepted_portable_equivalence: Option<String>,
    },
    Incompatible {
        first_mismatch_axis: CompatibilityMismatchAxis,
        detail: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VerificationLiveSubjectRelationV1 {
    SameAsCaptured,
    MovedAfterCapture,
    Unavailable,
    NotApplicable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationPlanBindingRefV1 {
    pub plan_id: String,
    pub plan_digest: VerifiedContentDigest,
    pub declared_model_generation: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationRunSnapshotBindingV1 {
    pub schema_version: String,
    pub plan: VerificationPlanBindingRefV1,
    pub requested: RequestedVerificationSubjectV1,
    pub prepared: Option<PreparedVerificationSubjectRefV1>,
    pub compatibility: VerificationSnapshotCompatibilityV1,
    pub normalized_binding_identity: VerifiedContentDigest,
    pub live_relation: VerificationLiveSubjectRelationV1,
    pub completeness: BindingCompleteness,
    pub limitations: Vec<String>,
    pub claim_boundary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct VerificationSnapshotBindingDecisionV1 {
    pub binding: VerificationRunSnapshotBindingV1,
    /// Always false in this passive slice: compatibility records preparation
    /// requirements only.
    pub ready_for_execution_authorization: bool,
}

/// Portable view used for binding identity (excludes checkout-root spelling).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortableRequestedSubjectV1 {
    pub envelope: RepositorySnapshotEnvelopeRefV2,
    pub subject_kind: VerificationSourceSubjectKindV2,
    pub identities: TaggedSubjectIdentitiesV1,
    pub selected_closure_digest: VerifiedContentDigest,
    pub closure_complete: bool,
    pub decision_bearing_inputs_digest: VerifiedContentDigest,
    pub resource_configuration_digest: Option<VerifiedContentDigest>,
    pub policy: SubjectCapturePolicyV1,
}

impl From<&RequestedVerificationSubjectV1> for PortableRequestedSubjectV1 {
    fn from(value: &RequestedVerificationSubjectV1) -> Self {
        Self {
            envelope: value.envelope.clone(),
            subject_kind: value.subject_kind,
            identities: value.identities.clone(),
            selected_closure_digest: value.selected_closure_digest.clone(),
            closure_complete: value.closure_complete,
            decision_bearing_inputs_digest: value.decision_bearing_inputs_digest.clone(),
            resource_configuration_digest: value.resource_configuration_digest.clone(),
            policy: value.policy.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortablePreparedSubjectV1 {
    pub envelope: RepositorySnapshotEnvelopeRefV2,
    pub acquisition_receipt: SharedAcquisitionReceiptRefV1,
    pub subject_kind: VerificationSourceSubjectKindV2,
    pub identities: TaggedSubjectIdentitiesV1,
    pub portable_content: PortableSourceContentIdentityV1,
    pub selected_closure_digest: VerifiedContentDigest,
    pub closure_complete: bool,
    pub decision_bearing_inputs_digest: VerifiedContentDigest,
    pub resource_configuration_digest: Option<VerifiedContentDigest>,
    pub completeness: SubjectCompleteness,
    pub policy: SubjectCapturePolicyV1,
}

impl From<&PreparedVerificationSubjectRefV1> for PortablePreparedSubjectV1 {
    fn from(value: &PreparedVerificationSubjectRefV1) -> Self {
        Self {
            envelope: value.envelope.clone(),
            acquisition_receipt: value.acquisition_receipt.clone(),
            subject_kind: value.subject_kind,
            identities: value.identities.clone(),
            portable_content: value.portable_content.clone(),
            selected_closure_digest: value.selected_closure_digest.clone(),
            closure_complete: value.closure_complete,
            decision_bearing_inputs_digest: value.decision_bearing_inputs_digest.clone(),
            resource_configuration_digest: value.resource_configuration_digest.clone(),
            completeness: value.completeness,
            policy: value.policy.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PortableRunBindingIdentityInputV1 {
    pub plan: VerificationPlanBindingRefV1,
    pub requested: PortableRequestedSubjectV1,
    pub prepared: Option<PortablePreparedSubjectV1>,
    pub compatibility: VerificationSnapshotCompatibilityV1,
    pub live_relation: VerificationLiveSubjectRelationV1,
    pub completeness: BindingCompleteness,
}
