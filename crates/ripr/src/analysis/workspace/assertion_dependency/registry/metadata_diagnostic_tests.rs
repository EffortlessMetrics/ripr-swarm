//! Parsing and eligibility are distinct from native transport success.
use super::*;

#[test]
fn malformed_success_is_not_valid_zero_target_metadata() -> Result<(), String> {
    for bytes in [
        b"".as_slice(),
        b"not metadata".as_slice(),
        b"{private".as_slice(),
    ] {
        assert_eq!(
            decode_registry_metadata(bytes),
            Err(MetadataProbeFailure::InvalidJson)
        );
    }
    let bytes = br#"{"packages":[],"workspace_members":[],"resolve":null}"#;
    let metadata = decode_registry_metadata(bytes).map_err(|failure| failure.to_string())?;
    assert_eq!(metadata["packages"], serde_json::json!([]));
    assert_eq!(metadata["workspace_members"], serde_json::json!([]));
    // Decoding succeeds, but no package, provider, target or receipt is granted.
    assert!(metadata["resolve"].is_null());
    assert!(
        !MetadataProbeFailure::InvalidJson
            .to_string()
            .contains("private")
    );
    Ok(())
}

#[test]
fn unavailable_non_candidate_root_is_not_a_probe_failure() {
    // A path containing an invalid component cannot be an eligible Cargo root.
    // The preflight must return a refused premise before trying Cargo.
    let root = Path::new("\0-not-a-cargo-root");
    assert!(matches!(verified_proptest_identity_result(root), Ok(None)));
    assert!(verified_proptest_identity(root).is_none());
}
