use serde_json::json;

use crate::domain::ExposureClass;

use super::*;

const HASH: &str = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_HASH: &str = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const TREE_ID: &str = "git-tree:0123456789abcdef0123456789abcdef01234567";

fn error_code(
    result: Result<(), RiprProviderContractErrorV1>,
) -> Option<RiprProviderContractErrorCodeV1> {
    result.err().map(|error| error.code)
}

fn required_excluded_claims() -> Vec<String> {
    RIPR_REQUIRED_EXCLUDED_CLAIMS
        .iter()
        .map(|claim| (*claim).to_string())
        .collect()
}

fn snapshot(source_view: RiprSourceViewV1) -> RiprRepositorySnapshotV1 {
    let snapshot_id = match source_view {
        RiprSourceViewV1::GitTree => TREE_ID.to_string(),
        RiprSourceViewV1::GitIndex => format!("git-index:{HASH}"),
        RiprSourceViewV1::Worktree => format!("worktree:{HASH}"),
        RiprSourceViewV1::CapturedSourceSet => format!("captured:{HASH}"),
    };
    RiprRepositorySnapshotV1 {
        repository_id: "EffortlessMetrics/ripr-swarm".into(),
        snapshot_id,
        source_view,
        source_digest: HASH.into(),
    }
}

fn request() -> RiprAnalysisRequestV1 {
    RiprAnalysisRequestV1 {
        schema_version: RIPR_ANALYSIS_REQUEST_SCHEMA_VERSION.into(),
        request_id: "request-1".into(),
        capability: RiprProviderCapabilityV1::TestGripSummary,
        repository: snapshot(RiprSourceViewV1::GitTree),
        subject: RiprEvidenceSubjectV1 {
            requirement_id: Some("REQ-1".into()),
            evidence_purpose: Some("boundary discriminator".into()),
            seam_id: "seam-1".into(),
            subject_id: "subject-1".into(),
            subject_body_digest: HASH.into(),
        },
        analysis_mode: "ready".into(),
        profile: "rust".into(),
        config_digest: HASH.into(),
        analyzer_generation: "ripr-analyzer.v1".into(),
        output_root: "target/ripr-provider".into(),
        requested_claim: "static test-grip evidence".into(),
    }
}

fn summary() -> RiprProviderEvidenceSummaryV1 {
    RiprProviderEvidenceSummaryV1 {
        seam_id: "seam-1".into(),
        analyzed_subject_count: 1,
        related_tests: vec![RiprProviderEvidenceEntryV1 {
            test_name: "grips_boundary".into(),
            file: "crates/ripr/tests/grip.rs".into(),
            line: 42,
            oracle_kind: "exact_value".into(),
            oracle_strength: "strong".into(),
            relation_reason: "direct_owner_call".into(),
            has_test_target: true,
        }],
        missing_discriminator_count: 0,
        strongest_oracle: "strong".into(),
        fingerprint: "fp:exact_value:strong:direct_owner_call".into(),
    }
}

fn receipt() -> RiprAnalysisReceiptV1 {
    RiprAnalysisReceiptV1 {
        schema_version: RIPR_ANALYSIS_RECEIPT_SCHEMA_VERSION.into(),
        request: request(),
        provider_version: "0.4.0".into(),
        binary_digest: HASH.into(),
        analyzer_generation: "ripr-analyzer.v1".into(),
        result_class: RiprProviderResultClassV1::Completed,
        native_status: Some(ExposureClass::Exposed),
        analysis_complete: true,
        truncated: false,
        summary: Some(summary()),
        diagnostics: Vec::new(),
        limitations: Vec::new(),
        claim_boundary: RIPR_PROVIDER_CLAIM_BOUNDARY.into(),
        excluded_claims: required_excluded_claims(),
    }
}

#[test]
fn read_only_capabilities_validate_with_canonical_non_claims() {
    let capabilities = RiprProviderCapabilitySetV1::read_only("0.4.0");
    assert_eq!(capabilities.validate(), Ok(()));
    assert!(capabilities.capabilities.iter().all(|descriptor| {
        !descriptor.writes_source
            && !descriptor.executes_project_commands
            && !descriptor.uses_network
            && descriptor.claim_boundary == RIPR_PROVIDER_CLAIM_BOUNDARY
            && descriptor.excluded_claims == required_excluded_claims()
    }));
}

#[test]
fn capability_validation_rejects_schema_duplicates_empty_and_authority_drift() {
    let mut unsupported = RiprProviderCapabilitySetV1::read_only("0.4.0");
    unsupported.schema_version = "ripr_provider_capabilities.v2".into();
    assert_eq!(
        error_code(unsupported.validate()),
        Some(RiprProviderContractErrorCodeV1::UnsupportedSchema)
    );

    let mut empty = RiprProviderCapabilitySetV1::read_only("0.4.0");
    empty.capabilities.clear();
    assert_eq!(
        error_code(empty.validate()),
        Some(RiprProviderContractErrorCodeV1::MissingField)
    );

    let mut duplicate = RiprProviderCapabilitySetV1::read_only("0.4.0");
    if let Some(first) = duplicate.capabilities.first().cloned() {
        duplicate.capabilities.push(first);
    }
    assert_eq!(
        error_code(duplicate.validate()),
        Some(RiprProviderContractErrorCodeV1::DuplicateCapability)
    );

    let mut authority = RiprProviderCapabilitySetV1::read_only("0.4.0");
    if let Some(first) = authority.capabilities.first_mut() {
        first.writes_source = true;
    }
    assert_eq!(
        error_code(authority.validate()),
        Some(RiprProviderContractErrorCodeV1::AuthorityViolation)
    );
}

#[test]
fn capability_and_receipt_require_the_complete_excluded_claim_set() {
    let mut capabilities = RiprProviderCapabilitySetV1::read_only("0.4.0");
    if let Some(first) = capabilities.capabilities.first_mut() {
        first
            .excluded_claims
            .retain(|claim| claim != "runtime_test_execution");
    }
    assert_eq!(
        error_code(capabilities.validate()),
        Some(RiprProviderContractErrorCodeV1::MissingField)
    );

    let mut receipt = receipt();
    receipt
        .excluded_claims
        .retain(|claim| claim != "runtime_test_execution");
    assert_eq!(
        error_code(receipt.validate()),
        Some(RiprProviderContractErrorCodeV1::MissingField)
    );
}

#[test]
fn every_source_view_has_one_owned_snapshot_identity_rule() {
    for source_view in [
        RiprSourceViewV1::GitTree,
        RiprSourceViewV1::GitIndex,
        RiprSourceViewV1::Worktree,
        RiprSourceViewV1::CapturedSourceSet,
    ] {
        assert_eq!(snapshot(source_view).validate(), Ok(()));
    }

    let mut invalid_tree = snapshot(RiprSourceViewV1::GitTree);
    invalid_tree.snapshot_id = "git-tree:not-hex".into();
    assert_eq!(
        error_code(invalid_tree.validate()),
        Some(RiprProviderContractErrorCodeV1::MalformedIdentity)
    );

    for source_view in [
        RiprSourceViewV1::GitIndex,
        RiprSourceViewV1::Worktree,
        RiprSourceViewV1::CapturedSourceSet,
    ] {
        let mut invalid = snapshot(source_view);
        invalid.source_digest = OTHER_HASH.into();
        assert_eq!(
            error_code(invalid.validate()),
            Some(RiprProviderContractErrorCodeV1::IdentityMismatch)
        );
    }
}

#[test]
fn request_rejects_nonportable_output_roots_and_malformed_digests() {
    for output_root in [
        "../outside",
        "/outside",
        "C:/outside",
        "target/C:/outside",
        "target//receipt",
        "target/./receipt",
        "target\\receipt",
        "target/receipt/",
    ] {
        let mut request = request();
        request.output_root = output_root.into();
        assert_eq!(
            error_code(request.validate()),
            Some(RiprProviderContractErrorCodeV1::UnsafeOutputRoot),
            "output_root={output_root}"
        );
    }

    let mut malformed = request();
    malformed.config_digest = "sha256:short".into();
    assert_eq!(
        error_code(malformed.validate()),
        Some(RiprProviderContractErrorCodeV1::MalformedIdentity)
    );
}

#[test]
fn canonical_exposure_class_is_the_only_complete_native_status() {
    let receipt = receipt();
    assert_eq!(receipt.validate(), Ok(()));
    assert_eq!(receipt.native_status, Some(ExposureClass::Exposed));

    let wire = match serde_json::to_value(&receipt) {
        Ok(value) => value,
        Err(error) => {
            assert!(false, "serialization failed: {error}");
            return;
        }
    };
    assert_eq!(wire["native_status"], json!("exposed"));
}

#[test]
fn completed_receipts_require_a_nonzero_analyzed_subject_denominator() {
    let mut zero = receipt();
    if let Some(summary) = zero.summary.as_mut() {
        summary.analyzed_subject_count = 0;
        summary.related_tests.clear();
    }
    assert_eq!(
        error_code(zero.validate()),
        Some(RiprProviderContractErrorCodeV1::CompletenessConflict)
    );
}

#[test]
fn non_authoritative_results_require_explicit_disclosure() {
    let mut partial = receipt();
    partial.result_class = RiprProviderResultClassV1::Partial;
    partial.native_status = None;
    partial.analysis_complete = false;
    partial.summary = None;
    partial.limitations = vec!["analysis_scope_truncated".into()];
    assert_eq!(partial.validate(), Ok(()));

    partial.limitations.clear();
    assert_eq!(
        error_code(partial.validate()),
        Some(RiprProviderContractErrorCodeV1::MissingField)
    );

    let mut stale = receipt();
    stale.result_class = RiprProviderResultClassV1::StaleInput;
    stale.native_status = None;
    stale.analysis_complete = false;
    stale.summary = None;
    stale.diagnostics = vec![RiprProviderDiagnosticV1 {
        code: "stale_snapshot".into(),
        message: "source snapshot changed".into(),
        source_path: None,
        start_line: None,
        start_column: None,
        next_action: Some("rerun the provider".into()),
    }];
    assert_eq!(stale.validate(), Ok(()));
}

#[test]
fn receipt_validation_rejects_identity_schema_and_completeness_conflicts() {
    let mut analyzer = receipt();
    analyzer.analyzer_generation = "other".into();
    assert_eq!(
        error_code(analyzer.validate()),
        Some(RiprProviderContractErrorCodeV1::IdentityMismatch)
    );

    let mut truncated = receipt();
    truncated.truncated = true;
    assert_eq!(
        error_code(truncated.validate()),
        Some(RiprProviderContractErrorCodeV1::CompletenessConflict)
    );

    let mut unsupported = receipt();
    unsupported.schema_version = "ripr_analysis_receipt.v2".into();
    assert_eq!(
        error_code(unsupported.validate()),
        Some(RiprProviderContractErrorCodeV1::UnsupportedSchema)
    );

    let mut cross_seam = receipt();
    if let Some(summary) = cross_seam.summary.as_mut() {
        summary.seam_id = "other-seam".into();
    }
    assert_eq!(
        error_code(cross_seam.validate()),
        Some(RiprProviderContractErrorCodeV1::IdentityMismatch)
    );
}

#[test]
fn public_wire_round_trips_and_rejects_unknown_fields() {
    let receipt = receipt();
    let serialized = match serde_json::to_string(&receipt) {
        Ok(serialized) => serialized,
        Err(error) => {
            assert!(false, "serialization failed: {error}");
            return;
        }
    };
    match serde_json::from_str::<RiprAnalysisReceiptV1>(&serialized) {
        Ok(decoded) => assert_eq!(decoded, receipt),
        Err(error) => assert!(false, "round-trip deserialization failed: {error}"),
    }

    let mut request_value = match serde_json::to_value(request()) {
        Ok(value) => value,
        Err(error) => {
            assert!(false, "request serialization failed: {error}");
            return;
        }
    };
    if let Some(object) = request_value.as_object_mut() {
        object.insert("unexpected".into(), json!(true));
    } else {
        assert!(false, "request did not serialize as an object");
        return;
    }
    assert!(serde_json::from_value::<RiprAnalysisRequestV1>(request_value).is_err());
}
