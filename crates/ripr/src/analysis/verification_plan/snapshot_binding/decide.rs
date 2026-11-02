use sha2::{Digest, Sha256};

use super::model::{
    BindingCompleteness, CompatibilityMismatchAxis, PortablePreparedSubjectV1,
    PortableRequestedSubjectV1, PortableRunBindingIdentityInputV1,
    PreparedVerificationSubjectRefV1, RequestedVerificationSubjectV1, SubjectCompleteness,
    VERIFICATION_RUN_SNAPSHOT_BINDING_CLAIM_BOUNDARY,
    VERIFICATION_RUN_SNAPSHOT_BINDING_SCHEMA_VERSION, VerificationLiveSubjectRelationV1,
    VerificationPlanBindingRefV1, VerificationRunSnapshotBindingV1,
    VerificationSnapshotBindingDecisionV1, VerificationSnapshotCompatibilityV1,
    VerifiedContentDigest,
};

pub(crate) fn requested_prepared_compatibility(
    requested: &RequestedVerificationSubjectV1,
    prepared: &PreparedVerificationSubjectRefV1,
) -> VerificationSnapshotCompatibilityV1 {
    if requested.subject_kind != prepared.subject_kind {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::SubjectKind,
            detail: format!(
                "requested `{}`, prepared `{}`",
                requested.subject_kind.as_str(),
                prepared.subject_kind.as_str()
            ),
        };
    }
    if requested.envelope.envelope_digest != prepared.envelope.envelope_digest {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::EnvelopeDigest,
            detail: "envelope digest mismatch".to_string(),
        };
    }
    if requested.envelope.authority != prepared.envelope.authority {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::EnvelopeDigest,
            detail: "envelope authority mismatch".to_string(),
        };
    }
    if requested.identities.repository != prepared.identities.repository {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::RepositoryIdentity,
            detail: "repository identity mismatch".to_string(),
        };
    }
    if cross_kind_law_violation(requested, prepared) {
        return cross_kind_incompatibility(requested, prepared);
    }
    if identity_triplet_mismatch(&requested.identities, &prepared.identities) {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::BaseHeadIdentity,
            detail: "base/head/tree identity mismatch".to_string(),
        };
    }
    if requested.identities.index != prepared.identities.index {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::IndexIdentity,
            detail: "index identity mismatch".to_string(),
        };
    }
    if requested.identities.worktree != prepared.identities.worktree {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::WorktreeIdentity,
            detail: "worktree identity mismatch".to_string(),
        };
    }
    if overlay_mismatch(&requested.identities, &prepared.identities) {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::OverlayIdentity,
            detail: "overlay identity mismatch".to_string(),
        };
    }
    if requested.identities.captured_external != prepared.identities.captured_external {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::CapturedExternalIdentity,
            detail: "captured external identity mismatch".to_string(),
        };
    }
    if requested.identities.staged_deletions != prepared.identities.staged_deletions {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::StagedDeletionSet,
            detail: "staged deletion set mismatch".to_string(),
        };
    }
    if requested.selected_closure_digest != prepared.selected_closure_digest {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::SelectedClosure,
            detail: "selected closure digest mismatch".to_string(),
        };
    }
    if requested.decision_bearing_inputs_digest != prepared.decision_bearing_inputs_digest {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::DecisionBearingInputs,
            detail: "decision-bearing inputs digest mismatch".to_string(),
        };
    }
    if requested.resource_configuration_digest != prepared.resource_configuration_digest {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::ResourceConfiguration,
            detail: "resource/configuration digest mismatch".to_string(),
        };
    }
    if !requested.closure_complete || !prepared.closure_complete {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::UnsupportedOrPartialInventory,
            detail: "incomplete source closure".to_string(),
        };
    }
    if prepared.completeness != SubjectCompleteness::Complete {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::UnsupportedOrPartialInventory,
            detail: format!("prepared completeness `{:?}`", prepared.completeness),
        };
    }
    if !requested.unsupported_entries.is_empty() || !prepared.unsupported_entries.is_empty() {
        return VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::UnsupportedOrPartialInventory,
            detail: "unsupported inventory entries remain".to_string(),
        };
    }
    VerificationSnapshotCompatibilityV1::Compatible {
        accepted_portable_equivalence: None,
    }
}

fn identity_triplet_mismatch(
    left: &super::model::TaggedSubjectIdentitiesV1,
    right: &super::model::TaggedSubjectIdentitiesV1,
) -> bool {
    left.commit != right.commit
        || left.tree != right.tree
        || left.base != right.base
        || left.head != right.head
}

fn overlay_mismatch(
    left: &super::model::TaggedSubjectIdentitiesV1,
    right: &super::model::TaggedSubjectIdentitiesV1,
) -> bool {
    left.overlay_set != right.overlay_set || left.overlay_base != right.overlay_base
}

fn cross_kind_law_violation(
    requested: &RequestedVerificationSubjectV1,
    prepared: &PreparedVerificationSubjectRefV1,
) -> bool {
    use super::model::VerificationSourceSubjectKindV2 as Kind;
    match requested.subject_kind {
        Kind::CommittedHead | Kind::CommittedRange => {
            prepared.identities.worktree.is_some()
                && (requested.identities.worktree.is_none()
                    || prepared.identities.worktree.as_deref()
                        != prepared.identities.head.as_deref())
        }
        Kind::StagedIndex => prepared
            .identities
            .extra
            .contains_key("unstaged_replacement_for_staged_deletion"),
        Kind::SavedWorktree => prepared
            .identities
            .extra
            .get("changed_untracked_after_save")
            .map(|value| value == "true")
            .unwrap_or(false),
        Kind::Overlay => prepared
            .identities
            .extra
            .get("overlay_document_generation")
            .is_some_and(|value| {
                requested
                    .identities
                    .extra
                    .get("overlay_document_generation")
                    .is_some_and(|expected| expected != value)
            }),
        Kind::CapturedExternal => false,
    }
}

fn cross_kind_incompatibility(
    requested: &RequestedVerificationSubjectV1,
    _prepared: &PreparedVerificationSubjectRefV1,
) -> VerificationSnapshotCompatibilityV1 {
    use super::model::VerificationSourceSubjectKindV2 as Kind;
    match requested.subject_kind {
        Kind::CommittedHead | Kind::CommittedRange => {
            VerificationSnapshotCompatibilityV1::Incompatible {
                first_mismatch_axis: CompatibilityMismatchAxis::CommittedVsDirtyWorktree,
                detail: "committed subject cannot consume dirty worktree bytes".to_string(),
            }
        }
        Kind::StagedIndex => VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::StagedVsWorktreeReplacement,
            detail: "staged subject cannot fall back to unstaged replacement".to_string(),
        },
        Kind::SavedWorktree => VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::WorktreeIdentity,
            detail: "saved worktree closure changed after capture".to_string(),
        },
        Kind::Overlay => VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::OverlayIdentity,
            detail: "overlay base or document generation mismatch".to_string(),
        },
        Kind::CapturedExternal => VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::CapturedExternalIdentity,
            detail: "captured external subject mismatch".to_string(),
        },
    }
}

pub(crate) fn portable_run_binding_identity(
    input: &PortableRunBindingIdentityInputV1,
) -> Result<VerifiedContentDigest, String> {
    let json = serde_json::to_string(input).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(json.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    VerifiedContentDigest::from_hex(&hex)
}

pub(crate) fn decide_snapshot_binding(
    plan: VerificationPlanBindingRefV1,
    mut requested: RequestedVerificationSubjectV1,
    mut prepared: Option<PreparedVerificationSubjectRefV1>,
    live_relation: VerificationLiveSubjectRelationV1,
) -> Result<VerificationSnapshotBindingDecisionV1, String> {
    requested.normalize();
    if let Some(prepared_subject) = prepared.as_mut() {
        prepared_subject.normalize();
    }

    let compatibility = match prepared.as_ref() {
        Some(prepared_subject) => requested_prepared_compatibility(&requested, prepared_subject),
        None => VerificationSnapshotCompatibilityV1::Incompatible {
            first_mismatch_axis: CompatibilityMismatchAxis::UnsupportedOrPartialInventory,
            detail: "no prepared subject".to_string(),
        },
    };

    let completeness = binding_completeness(&requested, prepared.as_ref(), &compatibility);
    let mut limitations = requested.limitations.clone();
    if let Some(prepared_subject) = prepared.as_ref() {
        limitations.extend_from_slice(&prepared_subject.limitations);
    }
    limitations.sort();
    limitations.dedup();

    let identity_input = PortableRunBindingIdentityInputV1 {
        plan: plan.clone(),
        requested: PortableRequestedSubjectV1::from(&requested),
        prepared: prepared.as_ref().map(PortablePreparedSubjectV1::from),
        compatibility: compatibility.clone(),
        live_relation,
        completeness,
    };
    let normalized_binding_identity = portable_run_binding_identity(&identity_input)?;

    let binding = VerificationRunSnapshotBindingV1 {
        schema_version: VERIFICATION_RUN_SNAPSHOT_BINDING_SCHEMA_VERSION.to_string(),
        plan,
        requested,
        prepared,
        compatibility,
        normalized_binding_identity,
        live_relation,
        completeness,
        limitations,
        claim_boundary: VERIFICATION_RUN_SNAPSHOT_BINDING_CLAIM_BOUNDARY.to_string(),
    };

    Ok(VerificationSnapshotBindingDecisionV1 {
        binding,
        ready_for_execution_authorization: false,
    })
}

fn binding_completeness(
    requested: &RequestedVerificationSubjectV1,
    prepared: Option<&PreparedVerificationSubjectRefV1>,
    compatibility: &VerificationSnapshotCompatibilityV1,
) -> BindingCompleteness {
    if !requested.closure_complete
        || !requested.unsupported_entries.is_empty()
        || matches!(
            compatibility,
            VerificationSnapshotCompatibilityV1::Incompatible { .. }
        )
    {
        if requested.unsupported_entries.is_empty()
            && prepared.is_some_and(|subject| subject.completeness == SubjectCompleteness::Partial)
        {
            return BindingCompleteness::Partial;
        }
        if !requested.unsupported_entries.is_empty()
            || prepared
                .is_some_and(|subject| subject.completeness == SubjectCompleteness::Unsupported)
        {
            return BindingCompleteness::Unsupported;
        }
        return BindingCompleteness::Partial;
    }
    if matches!(
        compatibility,
        VerificationSnapshotCompatibilityV1::Compatible { .. }
    ) {
        BindingCompleteness::Complete
    } else {
        BindingCompleteness::Partial
    }
}

#[cfg(test)]
mod decide_tests {
    use super::*;
    use crate::analysis::verification_plan::snapshot_binding::model::{
        RepositorySnapshotEnvelopeRefV2, SharedAcquisitionReceiptRefV1, SubjectCapturePolicyV1,
        TaggedSubjectIdentitiesV1, VerificationSourceSubjectKindV2,
    };

    fn digest(byte: u8) -> Result<VerifiedContentDigest, String> {
        VerifiedContentDigest::from_hex(&format!("{byte:064x}"))
    }

    fn sample_requested(
        kind: VerificationSourceSubjectKindV2,
    ) -> Result<RequestedVerificationSubjectV1, String> {
        Ok(RequestedVerificationSubjectV1 {
            envelope: RepositorySnapshotEnvelopeRefV2::new(digest(1)?),
            subject_kind: kind,
            identities: TaggedSubjectIdentitiesV1 {
                repository: "repo:alpha".to_string(),
                commit: Some("c1".to_string()),
                tree: Some("t1".to_string()),
                base: Some("b1".to_string()),
                head: Some("h1".to_string()),
                index: None,
                worktree: None,
                overlay_set: None,
                overlay_base: None,
                captured_external: None,
                staged_deletions: Vec::new(),
                extra: Default::default(),
            },
            selected_closure_digest: digest(2)?,
            closure_complete: true,
            decision_bearing_inputs_digest: digest(3)?,
            resource_configuration_digest: Some(digest(4)?),
            policy: SubjectCapturePolicyV1 {
                include_untracked_when_selected: true,
                include_ignored_when_selected: false,
                admit_symlinks: false,
                admit_submodules: false,
            },
            limitations: Vec::new(),
            unsupported_entries: Vec::new(),
        })
    }

    fn sample_prepared(
        kind: VerificationSourceSubjectKindV2,
    ) -> Result<PreparedVerificationSubjectRefV1, String> {
        Ok(PreparedVerificationSubjectRefV1 {
            envelope: RepositorySnapshotEnvelopeRefV2::new(digest(1)?),
            acquisition_receipt: SharedAcquisitionReceiptRefV1 {
                receipt_digest: digest(9)?,
            },
            subject_kind: kind,
            identities: TaggedSubjectIdentitiesV1 {
                repository: "repo:alpha".to_string(),
                commit: Some("c1".to_string()),
                tree: Some("t1".to_string()),
                base: Some("b1".to_string()),
                head: Some("h1".to_string()),
                index: None,
                worktree: None,
                overlay_set: None,
                overlay_base: None,
                captured_external: None,
                staged_deletions: Vec::new(),
                extra: Default::default(),
            },
            portable_content: super::super::model::PortableSourceContentIdentityV1 {
                content_digest: digest(5)?,
                inventory_digest: digest(6)?,
            },
            selected_closure_digest: digest(2)?,
            closure_complete: true,
            decision_bearing_inputs_digest: digest(3)?,
            resource_configuration_digest: Some(digest(4)?),
            prepared_root: super::super::model::PreparedRootMetadataV1 {
                concrete_read_root: "/tmp/prepared-root".to_string(),
            },
            completeness: SubjectCompleteness::Complete,
            policy: SubjectCapturePolicyV1 {
                include_untracked_when_selected: true,
                include_ignored_when_selected: false,
                admit_symlinks: false,
                admit_submodules: false,
            },
            limitations: Vec::new(),
            unsupported_entries: Vec::new(),
        })
    }

    #[test]
    fn committed_pair_is_compatible() -> Result<(), String> {
        let requested = sample_requested(VerificationSourceSubjectKindV2::CommittedHead)?;
        let prepared = sample_prepared(VerificationSourceSubjectKindV2::CommittedHead)?;
        match requested_prepared_compatibility(&requested, &prepared) {
            VerificationSnapshotCompatibilityV1::Compatible { .. } => Ok(()),
            other => Err(format!("expected compatible pair, got {other:?}")),
        }
    }

    #[test]
    fn dirty_worktree_rejects_committed_request() -> Result<(), String> {
        let requested = sample_requested(VerificationSourceSubjectKindV2::CommittedHead)?;
        let mut prepared = sample_prepared(VerificationSourceSubjectKindV2::CommittedHead)?;
        prepared.identities.worktree = Some("dirty-wt".to_string());
        match requested_prepared_compatibility(&requested, &prepared) {
            VerificationSnapshotCompatibilityV1::Incompatible {
                first_mismatch_axis: CompatibilityMismatchAxis::CommittedVsDirtyWorktree,
                ..
            } => Ok(()),
            other => Err(format!("expected dirty-worktree rejection, got {other:?}")),
        }
    }
}
