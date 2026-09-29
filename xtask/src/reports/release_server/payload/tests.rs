use super::identity::{
    FinalNativePayloadIdentity, NativeRuntimeEvidence, PAYLOAD_SCHEMA_VERSION,
    PayloadBuildEnvironment, PayloadFileIdentity, PayloadToolchainIdentity,
    RUNTIME_EVIDENCE_SOURCE, RUNTIME_EVIDENCE_STATE, payload_aggregate_sha256,
    payload_identity_json, payload_identity_markdown,
};
use super::validate_payload_relative_path;

fn sample_files() -> Vec<PayloadFileIdentity> {
    vec![
        PayloadFileIdentity {
            relative_path: "LICENSE-MIT".to_string(),
            role: "license".to_string(),
            size: 12,
            sha256: "a".repeat(64),
        },
        PayloadFileIdentity {
            relative_path: "ripr".to_string(),
            role: "executable".to_string(),
            size: 34,
            sha256: "b".repeat(64),
        },
    ]
}

fn sample_identity() -> FinalNativePayloadIdentity {
    let payload_files = sample_files();
    FinalNativePayloadIdentity {
        schema_version: PAYLOAD_SCHEMA_VERSION,
        product: "ripr".to_string(),
        native_version: "0.11.0".to_string(),
        target: "x86_64-unknown-linux-gnu".to_string(),
        executable: "ripr".to_string(),
        candidate_sha: "c".repeat(40),
        candidate_tree: "d".repeat(40),
        cargo_lock_sha256: "e".repeat(64),
        cargo_default_features: true,
        cargo_features: vec![
            "lang-python".to_string(),
            "lang-rust".to_string(),
            "lang-typescript".to_string(),
        ],
        toolchain: PayloadToolchainIdentity {
            rustc_verbose_version: "rustc 1.95.0".to_string(),
            cargo_verbose_version: "cargo 1.95.0".to_string(),
        },
        build_environment: PayloadBuildEnvironment {
            provider: "github-actions".to_string(),
            runner_os: Some("Linux".to_string()),
            runner_arch: Some("X64".to_string()),
            image_os: Some("ubuntu22".to_string()),
            image_version: Some("20260928.1".to_string()),
        },
        payload_sha256: payload_aggregate_sha256(&payload_files),
        payload_files,
        native_runtime_evidence: NativeRuntimeEvidence {
            state: RUNTIME_EVIDENCE_STATE.to_string(),
            source: RUNTIME_EVIDENCE_SOURCE.to_string(),
        },
    }
}

#[test]
fn aggregate_digest_changes_when_payload_identity_changes() {
    let files = sample_files();
    let original_digest = payload_aggregate_sha256(&files);
    let mut changed_files = files;
    changed_files[1].size += 1;
    let changed_digest = payload_aggregate_sha256(&changed_files);
    assert_eq!(original_digest.len(), 64);
    assert_ne!(original_digest, changed_digest);
}

#[test]
fn payload_paths_must_be_one_portable_component() {
    assert!(validate_payload_relative_path("ripr").is_ok());
    assert!(validate_payload_relative_path("ripr.exe").is_ok());
    assert!(validate_payload_relative_path("../ripr").is_err());
    assert!(validate_payload_relative_path("bin/ripr").is_err());
    assert!(validate_payload_relative_path(r"bin\ripr.exe").is_err());
    assert!(validate_payload_relative_path(".").is_err());
}

#[test]
fn identity_parser_rejects_unknown_fields() -> Result<(), String> {
    let identity = sample_identity();
    let mut value = serde_json::to_value(identity)
        .map_err(|err| format!("failed to encode sample identity: {err}"))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| "sample identity did not encode as an object".to_string())?;
    object.insert("unexpected".to_string(), serde_json::Value::Bool(true));
    let text = serde_json::to_string(&value)
        .map_err(|err| format!("failed to render mutated sample identity: {err}"))?;
    let parsed = serde_json::from_str::<FinalNativePayloadIdentity>(&text);
    assert!(parsed.is_err());
    Ok(())
}

#[test]
fn identity_rendering_is_canonical_and_mentions_runtime_limit() -> Result<(), String> {
    let identity = sample_identity();
    let json = payload_identity_json(&identity)?;
    let reparsed: FinalNativePayloadIdentity = serde_json::from_str(&json)
        .map_err(|err| format!("failed to parse rendered sample identity: {err}"))?;
    assert_eq!(reparsed, identity);
    let markdown = payload_identity_markdown(&identity);
    assert!(markdown.contains("issue:#4489"));
    assert!(markdown.contains(identity.payload_sha256()));
    Ok(())
}
