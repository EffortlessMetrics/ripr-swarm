//! Hostile corpus for run snapshot binding (#6347).

use std::fs;
use std::path::PathBuf;

use serde::Deserialize;

use super::decide::{
    decide_snapshot_binding, portable_run_binding_identity, requested_prepared_compatibility,
};
use super::model::{
    BindingCompleteness, CompatibilityMismatchAxis, PortablePreparedSubjectV1,
    PortableRequestedSubjectV1, PortableRunBindingIdentityInputV1, PreparedRootMetadataV1,
    PreparedVerificationSubjectRefV1, RepositorySnapshotEnvelopeRefV2,
    RequestedVerificationSubjectV1, SharedAcquisitionReceiptRefV1, SubjectCapturePolicyV1,
    SubjectCompleteness, TaggedSubjectIdentitiesV1, VerificationLiveSubjectRelationV1,
    VerificationPlanBindingRefV1, VerificationSnapshotCompatibilityV1,
    VerificationSourceSubjectKindV2, VerifiedContentDigest,
};

const CORPUS_DIR: &str = "fixtures/verification_run_snapshot_binding";

#[derive(Clone, Debug, Deserialize)]
struct CorpusFile {
    schema_version: String,
    cases: Vec<CorpusCase>,
}

#[derive(Clone, Debug, Deserialize)]
struct CorpusCase {
    id: String,
    plan: PlanFixture,
    requested: RequestedFixture,
    prepared: Option<PreparedFixture>,
    live_relation: VerificationLiveSubjectRelationV1,
    expect: ExpectFixture,
}

#[derive(Clone, Debug, Deserialize)]
struct PlanFixture {
    plan_id: String,
    plan_digest: String,
    declared_model_generation: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RequestedFixture {
    envelope_digest: String,
    subject_kind: VerificationSourceSubjectKindV2,
    identities: TaggedSubjectIdentitiesV1,
    selected_closure_digest: String,
    closure_complete: bool,
    decision_bearing_inputs_digest: String,
    resource_configuration_digest: Option<String>,
    policy: SubjectCapturePolicyV1,
    limitations: Vec<String>,
    unsupported_entries: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct PreparedFixture {
    envelope_digest: String,
    receipt_digest: String,
    subject_kind: VerificationSourceSubjectKindV2,
    identities: TaggedSubjectIdentitiesV1,
    content_digest: String,
    inventory_digest: String,
    selected_closure_digest: String,
    closure_complete: bool,
    decision_bearing_inputs_digest: String,
    resource_configuration_digest: Option<String>,
    prepared_root: String,
    completeness: SubjectCompleteness,
    policy: SubjectCapturePolicyV1,
    limitations: Vec<String>,
    unsupported_entries: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ExpectedCompatibility {
    Compatible,
    Incompatible,
}

#[derive(Clone, Debug, Deserialize)]
struct ExpectFixture {
    compatibility: ExpectedCompatibility,
    first_mismatch_axis: Option<CompatibilityMismatchAxis>,
    completeness: BindingCompleteness,
    live_relation: VerificationLiveSubjectRelationV1,
    portable_binding_identity: Option<String>,
    reorder_invariance: bool,
}

fn workspace_root() -> Result<PathBuf, String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| format!("workspace root: {error}"))
}

fn load_corpus() -> Result<CorpusFile, String> {
    let path = workspace_root()?.join(CORPUS_DIR).join("corpus.json");
    let body =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&body).map_err(|error| format!("parse corpus: {error}"))
}

fn digest(value: &str) -> Result<VerifiedContentDigest, String> {
    VerifiedContentDigest::parse(value)
}

fn plan_from(fixture: &PlanFixture) -> Result<VerificationPlanBindingRefV1, String> {
    Ok(VerificationPlanBindingRefV1 {
        plan_id: fixture.plan_id.clone(),
        plan_digest: digest(&fixture.plan_digest)?,
        declared_model_generation: fixture.declared_model_generation.clone(),
    })
}

fn requested_from(fixture: &RequestedFixture) -> Result<RequestedVerificationSubjectV1, String> {
    Ok(RequestedVerificationSubjectV1 {
        envelope: RepositorySnapshotEnvelopeRefV2::new(digest(&fixture.envelope_digest)?),
        subject_kind: fixture.subject_kind,
        identities: fixture.identities.clone(),
        selected_closure_digest: digest(&fixture.selected_closure_digest)?,
        closure_complete: fixture.closure_complete,
        decision_bearing_inputs_digest: digest(&fixture.decision_bearing_inputs_digest)?,
        resource_configuration_digest: fixture
            .resource_configuration_digest
            .as_ref()
            .map(|value| digest(value))
            .transpose()?,
        policy: fixture.policy.clone(),
        limitations: fixture.limitations.clone(),
        unsupported_entries: fixture.unsupported_entries.clone(),
    })
}

fn prepared_from(fixture: &PreparedFixture) -> Result<PreparedVerificationSubjectRefV1, String> {
    Ok(PreparedVerificationSubjectRefV1 {
        envelope: RepositorySnapshotEnvelopeRefV2::new(digest(&fixture.envelope_digest)?),
        acquisition_receipt: SharedAcquisitionReceiptRefV1 {
            receipt_digest: digest(&fixture.receipt_digest)?,
        },
        subject_kind: fixture.subject_kind,
        identities: fixture.identities.clone(),
        portable_content: super::model::PortableSourceContentIdentityV1 {
            content_digest: digest(&fixture.content_digest)?,
            inventory_digest: digest(&fixture.inventory_digest)?,
        },
        selected_closure_digest: digest(&fixture.selected_closure_digest)?,
        closure_complete: fixture.closure_complete,
        decision_bearing_inputs_digest: digest(&fixture.decision_bearing_inputs_digest)?,
        resource_configuration_digest: fixture
            .resource_configuration_digest
            .as_ref()
            .map(|value| digest(value))
            .transpose()?,
        prepared_root: PreparedRootMetadataV1 {
            concrete_read_root: fixture.prepared_root.clone(),
        },
        completeness: fixture.completeness,
        policy: fixture.policy.clone(),
        limitations: fixture.limitations.clone(),
        unsupported_entries: fixture.unsupported_entries.clone(),
    })
}

fn assert_axis(
    case_id: &str,
    observed: &VerificationSnapshotCompatibilityV1,
    expected: &ExpectFixture,
) -> Result<(), String> {
    match (&expected.compatibility, observed) {
        (
            ExpectedCompatibility::Compatible,
            VerificationSnapshotCompatibilityV1::Compatible { .. },
        ) => Ok(()),
        (
            ExpectedCompatibility::Incompatible,
            VerificationSnapshotCompatibilityV1::Incompatible {
                first_mismatch_axis,
                ..
            },
        ) => {
            let expected_axis = expected.first_mismatch_axis.ok_or_else(|| {
                format!("case `{case_id}` incompatible expectation must name axis")
            })?;
            if *first_mismatch_axis != expected_axis {
                return Err(format!(
                    "case `{case_id}` mismatch axis: expected {expected_axis:?}, got {first_mismatch_axis:?}"
                ));
            }
            Ok(())
        }
        (expected_status, observed_status) => Err(format!(
            "case `{case_id}` compatibility: expected {expected_status:?}, got {observed_status:?}"
        )),
    }
}

fn run_case(case: &CorpusCase) -> Result<(), String> {
    let plan = plan_from(&case.plan)?;
    let requested = requested_from(&case.requested)?;
    let prepared = case.prepared.as_ref().map(prepared_from).transpose()?;
    if let (Some(requested_subject), Some(prepared_subject)) = (Some(&requested), prepared.as_ref())
    {
        let compatibility = requested_prepared_compatibility(requested_subject, prepared_subject);
        assert_axis(&case.id, &compatibility, &case.expect)?;
    }
    let decision = decide_snapshot_binding(plan, requested, prepared, case.live_relation)?;
    assert_axis(&case.id, &decision.binding.compatibility, &case.expect)?;
    assert_eq!(
        decision.binding.completeness, case.expect.completeness,
        "case `{}` completeness",
        case.id
    );
    assert_eq!(
        decision.binding.live_relation, case.expect.live_relation,
        "case `{}` live relation",
        case.id
    );
    assert!(
        !decision.ready_for_execution_authorization,
        "passive slice must not authorize execution"
    );
    if let Some(expected_identity) = &case.expect.portable_binding_identity {
        assert_eq!(
            decision.binding.normalized_binding_identity.as_str(),
            expected_identity,
            "case `{}` binding identity",
            case.id
        );
    }
    if case.expect.reorder_invariance {
        let mut reordered = decision.binding.requested.clone();
        reordered.limitations.reverse();
        let identity_a = portable_run_binding_identity(&PortableRunBindingIdentityInputV1 {
            plan: decision.binding.plan.clone(),
            requested: PortableRequestedSubjectV1::from(&decision.binding.requested),
            prepared: decision
                .binding
                .prepared
                .as_ref()
                .map(PortablePreparedSubjectV1::from),
            compatibility: decision.binding.compatibility.clone(),
            live_relation: decision.binding.live_relation,
            completeness: decision.binding.completeness,
        })?;
        let decision_b = decide_snapshot_binding(
            decision.binding.plan.clone(),
            reordered,
            decision.binding.prepared.clone(),
            decision.binding.live_relation,
        )?;
        let identity_b = decision_b.binding.normalized_binding_identity;
        assert_eq!(
            identity_a, identity_b,
            "case `{}` reorder invariance",
            case.id
        );
    }
    Ok(())
}

#[test]
fn corpus_cases_match_acceptance_fixtures() -> Result<(), String> {
    let corpus = load_corpus()?;
    assert_eq!(
        corpus.schema_version, "verification_run_snapshot_binding_corpus.v1",
        "corpus schema"
    );
    assert_eq!(corpus.cases.len(), 10, "expected ten hostile fixtures");
    for case in &corpus.cases {
        run_case(case)?;
    }
    Ok(())
}

#[test]
fn root_relocation_preserves_portable_identity_while_concrete_root_differs() -> Result<(), String> {
    let corpus = load_corpus()?;
    let case = corpus
        .cases
        .iter()
        .find(|case| case.id == "equivalent_checkout_root_relocation")
        .ok_or_else(|| "relocation fixture missing".to_string())?;
    let plan = plan_from(&case.plan)?;
    let requested = requested_from(&case.requested)?;
    let mut prepared_a = case
        .prepared
        .as_ref()
        .map(prepared_from)
        .transpose()?
        .ok_or_else(|| "relocation fixture missing prepared subject".to_string())?;
    prepared_a.prepared_root.concrete_read_root = "/mnt/a/ripr".to_string();
    let mut prepared_b = prepared_a.clone();
    prepared_b.prepared_root.concrete_read_root = "/mnt/b/ripr".to_string();
    let decision_a = decide_snapshot_binding(
        plan.clone(),
        requested.clone(),
        Some(prepared_a),
        case.live_relation,
    )?;
    let decision_b =
        decide_snapshot_binding(plan, requested, Some(prepared_b), case.live_relation)?;
    assert_eq!(
        decision_a.binding.normalized_binding_identity,
        decision_b.binding.normalized_binding_identity,
        "portable identity must ignore checkout root spelling"
    );
    assert_ne!(
        decision_a
            .binding
            .prepared
            .as_ref()
            .map(|subject| subject.prepared_root.concrete_read_root.clone()),
        decision_b
            .binding
            .prepared
            .as_ref()
            .map(|subject| subject.prepared_root.concrete_read_root.clone()),
        "concrete roots should differ in the fixture"
    );
    Ok(())
}

#[test]
fn shared_envelope_is_reference_not_embedded_copy() -> Result<(), String> {
    let envelope = RepositorySnapshotEnvelopeRefV2::new(digest(
        "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    )?);
    if !envelope.authority.contains("RepositorySnapshotEnvelopeV2") {
        return Err("must reference shared authority".to_string());
    }
    if envelope.envelope_digest.as_str().len() != 71 {
        return Err("digest spelling must stay fixed-width".to_string());
    }
    Ok(())
}

#[test]
fn plan_config_resource_change_changes_portable_identity() -> Result<(), String> {
    let corpus = load_corpus()?;
    let base = corpus
        .cases
        .iter()
        .find(|case| case.id == "committed_exact_pair")
        .ok_or_else(|| "base fixture missing".to_string())?;
    let changed = corpus
        .cases
        .iter()
        .find(|case| case.id == "plan_identity_invalidates_compatibility")
        .ok_or_else(|| "plan-change fixture missing".to_string())?;
    let decision_base = decide_snapshot_binding(
        plan_from(&base.plan)?,
        requested_from(&base.requested)?,
        base.prepared.as_ref().map(prepared_from).transpose()?,
        base.live_relation,
    )?;
    let decision_changed = decide_snapshot_binding(
        plan_from(&changed.plan)?,
        requested_from(&changed.requested)?,
        changed.prepared.as_ref().map(prepared_from).transpose()?,
        changed.live_relation,
    )?;
    assert_ne!(
        decision_base.binding.normalized_binding_identity,
        decision_changed.binding.normalized_binding_identity,
        "plan/config/resource identity must affect portable binding digest"
    );
    Ok(())
}
