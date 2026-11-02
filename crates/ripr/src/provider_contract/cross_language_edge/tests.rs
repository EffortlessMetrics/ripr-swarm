use std::path::PathBuf;

use serde::Deserialize;
use serde_json::json;

use super::*;

const HASH: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_HASH: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const CONFIG: &str = "sha256:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

fn project(project_id: &str, package_id: Option<&str>) -> CrossLanguageProjectIdentityV1 {
    CrossLanguageProjectIdentityV1 {
        project_id: project_id.into(),
        package_id: package_id.map(str::to_string),
        config_digest: CONFIG.into(),
    }
}

fn subject(language: &str, subject_id: &str, project_id: &str) -> CrossLanguageSubjectV1 {
    CrossLanguageSubjectV1 {
        language: language.into(),
        subject_id: subject_id.into(),
        subject_body_digest: HASH.into(),
        display_name: Some(subject_id.into()),
        project: project(project_id, Some("pkg")),
    }
}

fn supported_families() -> Vec<BoundaryFamilyV1> {
    BoundaryFamilyV1::ALL.into()
}

fn stub_producer() -> CrossLanguageEdgeProducerRegistrationV1 {
    CrossLanguageEdgeProducerRegistrationV1 {
        producer_id: "producer.test".into(),
        producer_version: "1".into(),
        supported_families: supported_families(),
    }
}

fn base_registry(edges: Vec<CrossLanguageEdgeV1>) -> CrossLanguageEdgeRegistryV1 {
    CrossLanguageEdgeRegistryV1 {
        schema_version: RIPR_CROSS_LANGUAGE_EDGE_REGISTRY_SCHEMA_VERSION.into(),
        proof_generation: "proof.v1".into(),
        currentness_generation: "currentness.v1".into(),
        supported_boundary_families: supported_families(),
        registered_producers: vec![stub_producer()],
        edges,
        semantic_digest: None,
    }
}

fn positive_edge(
    edge_id: &str,
    family: BoundaryFamilyV1,
    direction: CrossLanguageEdgeDirectionV1,
    strength: EdgeStrengthV1,
) -> CrossLanguageEdgeV1 {
    CrossLanguageEdgeV1 {
        edge_id: edge_id.into(),
        source: subject("rust", &format!("{edge_id}.source"), "project.alpha"),
        target: subject("typescript", &format!("{edge_id}.target"), "project.beta"),
        direction,
        boundary_family: family,
        producer_id: "producer.test".into(),
        producer_basis: EdgeProducerBasisV1::RegisteredProducerFact,
        exactness: EdgeExactnessV1::Exact,
        declared_strength: strength,
        effective_strength: strength,
        propagation_ceiling: strength,
        generated_artifacts: vec![CrossLanguageGeneratedArtifactV1 {
            artifact_id: format!("{edge_id}.artifact"),
            generator_id: "generator.test".into(),
            input_digest: HASH.into(),
            output_digest: OTHER_HASH.into(),
        }],
        obligations: vec![EdgeObligationV1 {
            obligation_id: format!("{edge_id}.obligation"),
            description: "verify external oracle alignment".into(),
            required_strength: strength,
        }],
        first_unresolved_leg: None,
        currentness: EdgeCurrentnessV1::Current,
        disposition: EdgeDispositionV1::PositiveRegistered,
        limitations: vec![],
        non_claims: vec![EdgeNonClaimV1 {
            claim: "runtime_success".into(),
            reason: "static registry only".into(),
        }],
    }
}

#[derive(Debug, Deserialize)]
struct CorpusFile {
    cases: Vec<CorpusCase>,
}

#[derive(Debug, Deserialize)]
struct CorpusCase {
    id: String,
    expect: String,
    #[serde(default)]
    error_code: Option<String>,
    registry: CrossLanguageEdgeRegistryV1,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/cross-language-edge-registry-corpus/corpus.json")
}

#[test]
fn corpus_cases_cover_registry_acceptance_matrix() -> Result<(), String> {
    let text =
        std::fs::read_to_string(corpus_path()).map_err(|error| format!("read corpus: {error}"))?;
    let corpus: CorpusFile =
        serde_json::from_str(&text).map_err(|error| format!("parse corpus: {error}"))?;
    assert!(
        corpus.cases.len() >= 8,
        "expected corpus acceptance/rejection matrix cases"
    );
    let matrix = corpus
        .cases
        .iter()
        .find(|case| case.id == "matrix_all_boundary_families")
        .ok_or_else(|| "missing matrix_all_boundary_families corpus case".to_string())?;
    assert_eq!(
        matrix.registry.edges.len(),
        BoundaryFamilyV1::ALL.len(),
        "matrix case must cover every boundary family"
    );
    for case in corpus.cases {
        let result = case.registry.validate();
        match case.expect.as_str() {
            "accept" => result.map_err(|error| format!("{}: {error}", case.id))?,
            "reject" => {
                let error = result
                    .err()
                    .ok_or_else(|| format!("{} expected rejection but validated", case.id))?;
                if let Some(expected) = &case.error_code {
                    let actual = format!("{:?}", error.code);
                    if actual != *expected {
                        return Err(format!(
                            "{} expected error {:?}, got {:?}",
                            case.id, expected, error
                        ));
                    }
                }
            }
            other => return Err(format!("{} has unknown expect `{other}`", case.id)),
        }
    }
    Ok(())
}

#[test]
fn same_display_name_without_edge_is_unrelated() {
    let left = CrossLanguageSubjectV1 {
        display_name: Some("Blob::from_js".into()),
        ..subject("rust", "blob.from_js", "project.alpha")
    };
    let right = CrossLanguageSubjectV1 {
        display_name: Some("Blob::from_js".into()),
        ..subject("typescript", "Blob.from_js", "project.beta")
    };
    let registry = base_registry(vec![]);
    assert!(!subjects_related_by_registered_edge(
        &left, &right, &registry
    ));
}

fn require_ok<T, E: std::fmt::Display>(result: Result<T, E>, label: &str) -> Result<T, String> {
    result.map_err(|error| format!("{label}: {error}"))
}

fn require_err<T, E: std::fmt::Display>(result: Result<T, E>, label: &str) -> Result<E, String> {
    match result {
        Err(error) => Ok(error),
        Ok(_) => Err(format!("{label}: expected error")),
    }
}

#[test]
fn registered_edge_connects_exact_subjects() -> Result<(), String> {
    let left = subject("rust", "connect.source", "project.alpha");
    let right = subject("typescript", "connect.target", "project.beta");
    let edge = positive_edge(
        "connect",
        BoundaryFamilyV1::FfiBinding,
        CrossLanguageEdgeDirectionV1::SourceToTarget,
        EdgeStrengthV1::RegisteredBinding,
    );
    let registry = base_registry(vec![edge]);
    require_ok(registry.validate(), "valid registry")?;
    assert!(subjects_related_by_registered_edge(
        &left, &right, &registry
    ));
    Ok(())
}

#[test]
fn duplicate_edge_ids_fail_precisely() -> Result<(), String> {
    let edge = positive_edge(
        "dup",
        BoundaryFamilyV1::NativeAddon,
        CrossLanguageEdgeDirectionV1::TargetToSource,
        EdgeStrengthV1::PropagationLimited,
    );
    let registry = base_registry(vec![edge.clone(), edge]);
    let error = require_err(registry.validate(), "duplicate edge")?;
    if error.code != CrossLanguageEdgeRegistryErrorCodeV1::DuplicateEdgeId {
        return Err(format!("duplicate edge: got {:?}", error.code));
    }
    Ok(())
}

#[test]
fn incompatible_family_strength_fails_precisely() -> Result<(), String> {
    let edge = positive_edge(
        "subprocess-too-strong",
        BoundaryFamilyV1::SubprocessCli,
        CrossLanguageEdgeDirectionV1::SourceToTarget,
        EdgeStrengthV1::OracleEstablished,
    );
    let registry = base_registry(vec![edge]);
    let error = require_err(registry.validate(), "family ceiling")?;
    if error.code != CrossLanguageEdgeRegistryErrorCodeV1::IncompatibleFamilyStrength {
        return Err(format!("family ceiling: got {:?}", error.code));
    }
    Ok(())
}

#[test]
fn unknown_fields_are_rejected_at_deserialize() -> Result<(), String> {
    let encoded = require_ok(
        serde_json::to_value(base_registry(vec![])),
        "encode registry",
    )?;
    let mut value = encoded;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "registry did not serialize as object".to_string())?;
    object.insert("unexpected".into(), json!(true));
    match serde_json::from_value::<CrossLanguageEdgeRegistryV1>(value) {
        Err(error) if error.to_string().contains("unknown field") => Ok(()),
        Err(error) => Err(format!("unknown field failed for other reason: {error}")),
        Ok(_) => Err("unknown field accepted".into()),
    }
}

#[test]
fn every_boundary_family_has_a_constructible_edge_shape() -> Result<(), String> {
    for family in BoundaryFamilyV1::ALL {
        let mut edge = positive_edge(
            family.as_str(),
            family,
            CrossLanguageEdgeDirectionV1::SourceToTarget,
            EdgeStrengthV1::VerificationObligation,
        );
        match family {
            BoundaryFamilyV1::SubprocessCli => {
                edge.declared_strength = EdgeStrengthV1::ProcessBoundary;
                edge.effective_strength = EdgeStrengthV1::ProcessBoundary;
                edge.propagation_ceiling = EdgeStrengthV1::ProcessBoundary;
                edge.obligations[0].required_strength = EdgeStrengthV1::ProcessBoundary;
            }
            BoundaryFamilyV1::UnknownDynamicBoundary => {
                edge.disposition = EdgeDispositionV1::UnsupportedFamily;
                edge.producer_basis = EdgeProducerBasisV1::UnsupportedManual;
                edge.declared_strength = EdgeStrengthV1::Unsupported;
                edge.effective_strength = EdgeStrengthV1::Unsupported;
                edge.propagation_ceiling = EdgeStrengthV1::Unsupported;
                edge.exactness = EdgeExactnessV1::Unsupported;
                edge.obligations[0].required_strength = EdgeStrengthV1::Unsupported;
            }
            BoundaryFamilyV1::SchemaProtocol
            | BoundaryFamilyV1::SharedResource
            | BoundaryFamilyV1::SerializationContract => {
                edge.declared_strength = EdgeStrengthV1::VerificationObligation;
                edge.effective_strength = EdgeStrengthV1::VerificationObligation;
                edge.propagation_ceiling = EdgeStrengthV1::VerificationObligation;
                edge.obligations[0].required_strength = EdgeStrengthV1::VerificationObligation;
            }
            _ => {}
        }
        let registry = base_registry(vec![edge]);
        require_ok(registry.validate(), family.as_str())?;
    }
    Ok(())
}

#[test]
fn config_digest_is_load_bearing_for_subject_identity() {
    let left = subject("rust", "edge.source", "project.alpha");
    let mut right = subject("typescript", "edge.target", "project.beta");
    right.project.config_digest = OTHER_HASH.into();
    let edge = positive_edge(
        "connect",
        BoundaryFamilyV1::FfiBinding,
        CrossLanguageEdgeDirectionV1::SourceToTarget,
        EdgeStrengthV1::RegisteredBinding,
    );
    let registry = base_registry(vec![edge]);
    assert!(!subjects_related_by_registered_edge(
        &left, &right, &registry
    ));
}

#[test]
fn semantic_digest_is_order_and_relocation_invariant() -> Result<(), String> {
    let edge_a = positive_edge(
        "b",
        BoundaryFamilyV1::WasmBoundary,
        CrossLanguageEdgeDirectionV1::Bidirectional,
        EdgeStrengthV1::RegisteredBinding,
    );
    let edge_b = positive_edge(
        "a",
        BoundaryFamilyV1::ManualRegisteredBridge,
        CrossLanguageEdgeDirectionV1::SourceToTarget,
        EdgeStrengthV1::VerificationObligation,
    );
    let mut reordered = base_registry(vec![edge_a, edge_b]);
    let digest = cross_language_edge_registry_semantic_digest(&reordered)
        .map_err(|error| error.to_string())?;
    reordered.edges.reverse();
    reordered.semantic_digest = Some(digest.clone());
    reordered.validate().map_err(|error| error.to_string())?;
    let mut relocated = reordered.clone();
    relocated.edges[0].source.display_name = Some("/tmp/checkout/src/foo.rs".into());
    relocated.edges[0].target.display_name = Some("/tmp/other-checkout/src/foo.ts".into());
    relocated.semantic_digest = None;
    let relocated_digest = cross_language_edge_registry_semantic_digest(&relocated)
        .map_err(|error| error.to_string())?;
    assert_eq!(digest, relocated_digest);
    Ok(())
}
