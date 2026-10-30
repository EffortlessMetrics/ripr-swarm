//! Stored terminal evidence keeps structural validity without live currentness.
use super::*;

fn history_root() -> Result<PathBuf, String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-issued-history-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    root.canonicalize().map_err(|error| error.to_string())
}

fn history_document(root: &Path, producer: &str) -> Value {
    let input = format!("input:{CHECK_FINDINGS_INPUT_IDENTITY_VERSION}:fnv1a64:0123456789abcdef");
    let head = "0123456789abcdef0123456789abcdef01234567";
    json!({
        "schema_version": "1",
        "artifact": {
            "kind": "check_findings",
            "schema_version": ARTIFACT_IDENTITY_SCHEMA_VERSION,
            "canonicalization": CONTENT_COMMITMENT_CANONICALIZATION,
            "producer": {"tool": "ripr", "version": producer},
            "repository": {"root": root.to_string_lossy(), "head": head},
            "analysis": {
                "format": CHECK_FINDINGS_ANALYSIS_FORMAT, "mode": "draft", "profile": "draft",
                "command": CHECK_FINDINGS_PRODUCER_COMMAND, "input_identity": input,
                "base_revision": head, "worktree": "clean"
            },
            "snapshot_identity": repo_exposure_snapshot_identity(&input, head),
            "content_sha256": CONTENT_SHA256_PLACEHOLDER
        },
        "analysis_outcome": {"analysis_complete": true, "document_complete": true},
        "findings": [{"id": "recorded-finding", "canonical_gap_id": "gap:recorded",
            "classification": "exposed", "probe": {"file": "src/a.ts", "line": 1}}]
    })
}

fn seal_history(document: &Value) -> Result<String, String> {
    let raw = serde_json::to_string(document).map_err(|error| error.to_string())?;
    let commitment = recompute_content_commitment(&raw)?;
    replace_content_commitment(&raw, &commitment).map_err(|error| error.to_string())
}

#[test]
fn issued_history_accepts_recorded_producer_without_live_git() -> Result<(), String> {
    let root = history_root()?;
    let result = (|| -> Result<(), String> {
        assert!(!root.join(".git").exists());
        for producer in [env!("CARGO_PKG_VERSION"), "0.0.0-recorded-history"] {
            let raw = seal_history(&history_document(&root, producer))?;
            let issued =
                validate_issued_check_findings_commitment(&root, &raw, "history", producer)?;
            assert_eq!(
                issued.repository_head,
                "0123456789abcdef0123456789abcdef01234567"
            );
            assert_eq!(
                issued.base_revision.as_deref(),
                Some(issued.repository_head.as_str())
            );
            assert!(issued.content_sha256.starts_with("sha256:"));
            assert!(
                validate_issued_check_findings_commitment(
                    &root,
                    &raw,
                    "history",
                    "different-producer"
                )
                .is_err()
            );
        }
        Ok(())
    })();
    std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    result
}

#[test]
fn issued_history_and_live_findings_refuse_pre_run_owned_v4_identity() -> Result<(), String> {
    let root = history_root()?;
    let producer = env!("CARGO_PKG_VERSION");
    let result = (|| -> Result<(), String> {
        let mut legacy = history_document(&root, producer);
        let input = "input:v4:fnv1a64:0123456789abcdef";
        let head = legacy["artifact"]["repository"]["head"]
            .as_str()
            .ok_or("missing recorded head")?;
        let snapshot = repo_exposure_snapshot_identity(input, head);
        legacy["artifact"]["analysis"]["input_identity"] = json!(input);
        legacy["artifact"]["snapshot_identity"] = json!(snapshot);
        // Recompute both commitments so refusal discriminates the unsupported
        // identity algorithm, not a stale snapshot or raw-content digest.
        let raw = seal_history(&legacy)?;
        let history_error =
            validate_issued_check_findings_commitment(&root, &raw, "history", producer)
                .err()
                .ok_or("issued history accepted pre-run-owned v4 findings")?;
        let live_error = validate_check_findings_artifact(&root, &raw, "live")
            .err()
            .ok_or("live evidence accepted pre-run-owned v4 findings")?;
        for error in [history_error, live_error] {
            assert!(
                error.contains("unsupported input identity version"),
                "{error}"
            );
            assert!(error.contains("expected `input:v5:"), "{error}");
        }
        Ok(())
    })();
    std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    result
}

#[test]
fn issued_history_preserves_structure_completeness_root_and_commitment_refusals()
-> Result<(), String> {
    let root = history_root()?;
    let producer = "0.0.0-recorded-history";
    let result = (|| -> Result<(), String> {
        let valid = history_document(&root, producer);
        for (pointer, value) in [
            ("/artifact/kind", json!("repo_exposure")),
            ("/artifact/schema_version", json!("unknown")),
            ("/artifact/producer/tool", json!("other")),
            ("/artifact/producer/version", json!("different-producer")),
            ("/artifact/analysis/input_identity", json!("input:unknown")),
            ("/artifact/snapshot_identity", json!("wrong")),
            (
                "/artifact/repository/root",
                json!(root.parent().ok_or("no parent")?.to_string_lossy()),
            ),
            ("/artifact/repository/head", json!("unavailable")),
            ("/artifact/analysis/worktree", json!("unavailable")),
            ("/analysis_outcome/analysis_complete", json!(false)),
            ("/analysis_outcome/document_complete", json!(false)),
            ("/findings/0/probe/line", json!(0)),
        ] {
            let mut invalid = valid.clone();
            *invalid
                .pointer_mut(pointer)
                .ok_or_else(|| format!("missing {pointer}"))? = value;
            let raw = seal_history(&invalid)?;
            assert!(
                validate_issued_check_findings_commitment(&root, &raw, "history", producer)
                    .is_err(),
                "accepted {pointer}"
            );
        }
        for (key, value) in [
            ("analysis_scope", json!({"partition": 1})),
            ("run_limitations", json!([{"reason": "bounded"}])),
            (
                "findings",
                json!([valid["findings"][0].clone(), valid["findings"][0].clone()]),
            ),
        ] {
            let mut invalid = valid.clone();
            invalid[key] = value;
            assert!(
                validate_issued_check_findings_commitment(
                    &root,
                    &seal_history(&invalid)?,
                    "history",
                    producer
                )
                .is_err(),
                "accepted {key}"
            );
        }
        let damaged = seal_history(&valid)?.replace("recorded-finding", "changed-finding");
        assert!(
            validate_issued_check_findings_commitment(&root, &damaged, "history", producer)
                .is_err()
        );
        Ok(())
    })();
    std::fs::remove_dir_all(root).map_err(|error| error.to_string())?;
    result
}
