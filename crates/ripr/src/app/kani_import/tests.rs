use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) fn tests_root(label: &str) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-kani-import-{}-{}-{label}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|err| format!("create Kani test root: {err}"))?;
    Ok(root)
}

pub(crate) fn test_inputs(root: &Path) -> Result<(String, String), String> {
    // Authored source and wire binding for an upstream fixture, not a captured run.
    let source = b"fn add_one(n: u8) -> u8 { n + 1 }\n// contract-fixture source placeholder\n";
    std::fs::write(root.join("test.rs"), source)
        .map_err(|err| format!("write Kani test source: {err}"))?;
    std::fs::write(root.join("Cargo.lock"), b"version = 4\n")
        .map_err(|err| format!("write Kani test lockfile: {err}"))?;
    let native = include_str!("fixtures/completed-failure.json").to_string();
    let lock_sha256 = sha256(b"version = 4\n");
    let source_sha256 = sha256(source);
    let source_digest =
        sha256(format!("Cargo.lock\t{lock_sha256}\ntest.rs\t{source_sha256}\n").as_bytes());
    let binding = serde_json::json!({
        "schema_version": "ripr_kani_binding.v1",
        "repository": {
            "repository_id": "authored-kani-import-control",
            "snapshot_id": format!("captured:{source_digest}"),
            "source_view": "captured_source_set",
            "source_digest": source_digest,
        },
        "run_id": "contract-fixture-not-a-native-run",
        "artifact_sha256": sha256(native.as_bytes()),
        "source_files": [
            {
                "path": "Cargo.lock",
                "sha256": lock_sha256,
                "roles": ["lockfile"],
            },
            {
                "path": "test.rs",
                "sha256": source_sha256,
                "roles": ["implementation", "harness"],
            },
        ],
        "context": {
            "target": "x86_64-unknown-linux-gnu",
            "cargo_features": [],
            "default_features": true,
            "toolchain": "upstream-contract-fixture",
            "environment_sha256": sha256(b"authored fixture context"),
            "invocation": ["kani", "test.rs", "--export-json", "native.json"],
        },
        "selected_harnesses": [{
            "crate_name": "test",
            "name": "check_two",
        }],
    });
    let binding = serde_json::to_string_pretty(&binding)
        .map_err(|err| format!("serialize Kani test binding: {err}"))?;
    Ok((native, binding))
}

fn encode(binding: &KaniBinding) -> Result<String, String> {
    serde_json::to_string(binding).map_err(|err| format!("encode test binding: {err}"))
}

fn refuses(root: &Path, native: &str, binding: &KaniBinding, reason: &str) -> Result<(), String> {
    let error = import_kani_report(root, native, &encode(binding)?)
        .err()
        .ok_or("import must refuse this binding")?;
    assert!(error.contains(reason), "{error}");
    Ok(())
}

#[test]
fn imports_failure_without_promoting_it_or_authenticating_the_producer() -> Result<(), String> {
    let root = tests_root("adverse")?;
    let (native, binding) = test_inputs(&root)?;
    let report = import_kani_report(&root, &native, &binding)?;
    assert_eq!(report.provenance, "external_unattested");
    assert_eq!(report.source_binding_state, "listed_files_match");
    assert_eq!(report.native.document["summary"]["failed"], 1);
    assert_eq!(report.native.run.harnesses.len(), 1);
    assert_eq!(report.artifact_sha256, sha256(native.as_bytes()));
    assert_eq!(report.binding_sha256, sha256(binding.as_bytes()));
    let json = serde_json::to_value(report).map_err(|err| err.to_string())?;
    for field in [
        "static_status",
        "receipt",
        "proof_success",
        "repair_packet",
        "verify_command",
    ] {
        assert_eq!(json.get(field), None, "must not create {field}");
    }
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn refuses_changed_source_lockfile_artifact_and_context() -> Result<(), String> {
    let root = tests_root("mismatch")?;
    let (native, text) = test_inputs(&root)?;
    let original: KaniBinding = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    for path in ["test.rs", "Cargo.lock"] {
        std::fs::write(root.join(path), b"changed\n").map_err(|err| err.to_string())?;
        let err = import_kani_report(&root, &native, &text)
            .err()
            .ok_or("changed source must be refused")?;
        assert!(err.contains("source digest mismatch"), "{err}");
        assert!(err.contains(path), "{err}");
        test_inputs(&root)?;
    }
    let err = import_kani_report(&root, &format!("{native}\n"), &text)
        .err()
        .ok_or("changed artifact bytes must be refused")?;
    assert!(err.contains("artifact digest mismatch"), "{err}");
    let mut binding = original.clone();
    binding.context.target = "aarch64-unknown-linux-gnu".into();
    refuses(&root, &native, &binding, "target differs")?;
    binding = original;
    binding.selected_harnesses[0].name = "different".into();
    refuses(&root, &native, &binding, "unbound harness")?;
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn refuses_uncommitted_inventory_and_ambiguous_source_paths() -> Result<(), String> {
    let root = tests_root("inventory")?;
    let (native, text) = test_inputs(&root)?;
    let original: KaniBinding = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    let mut binding = original.clone();
    binding.source_files.truncate(1);
    refuses(&root, &native, &binding, "implementation and harness")?;
    for path in [
        "../test.rs",
        "/test.rs",
        "./test.rs",
        "x\\test.rs",
        "C:test.rs",
        "x//test.rs",
        "x\ntest.rs",
    ] {
        binding = original.clone();
        binding.source_files[0].path = path.into();
        let err = import_kani_report(&root, &native, &encode(&binding)?)
            .err()
            .ok_or("ambiguous path must be refused")?;
        assert!(err.contains("must be portable"), "{path}: {err}");
    }
    binding = original.clone();
    binding.source_files[0].sha256 = sha256(b"different lockfile");
    refuses(&root, &native, &binding, "inventory digest")?;
    binding = original.clone();
    binding.source_files.push(original.source_files[0].clone());
    refuses(&root, &native, &binding, "duplicate Kani source path")?;
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}

#[test]
fn rejects_missing_fields_unknown_fields_duplicate_fields_and_unsupported_views()
-> Result<(), String> {
    let root = tests_root("shape")?;
    let (native, text) = test_inputs(&root)?;
    for bad in [
        "{}".to_string(),
        text.replace("\"run_id\":", "\"run_id\":\"duplicate\",\"run_id\":"),
        text.replacen('{', "{\"receipt\":true,", 1),
    ] {
        let err = import_kani_report(&root, &native, &bad)
            .err()
            .ok_or("invalid binding shape must fail")?;
        assert!(err.contains("invalid Kani binding JSON"), "{err}");
    }
    let mut binding: KaniBinding = serde_json::from_str(&text).map_err(|err| err.to_string())?;
    binding.repository.source_view = RiprSourceViewV1::Worktree;
    binding.repository.snapshot_id = format!("worktree:{}", binding.repository.source_digest);
    refuses(&root, &native, &binding, "captured_source_set")?;
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}

#[cfg(unix)]
#[test]
fn refuses_source_symlinks_even_when_bytes_match() -> Result<(), String> {
    let root = tests_root("symlink")?;
    let (native, binding) = test_inputs(&root)?;
    std::fs::rename(root.join("test.rs"), root.join("actual.rs")).map_err(|err| err.to_string())?;
    std::os::unix::fs::symlink("actual.rs", root.join("test.rs")).map_err(|err| err.to_string())?;
    let err = import_kani_report(&root, &native, &binding)
        .err()
        .ok_or("source symlink must fail")?;
    assert!(err.contains("symlinks are unsupported"), "{err}");
    std::fs::remove_dir_all(root).map_err(|err| err.to_string())?;
    Ok(())
}
