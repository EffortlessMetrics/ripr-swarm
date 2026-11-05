//! Runtime resources for policy tests, shared by unit and integration harnesses.
//! Cargo supplies these variables for ordinary tests; nextest remaps them for
//! archive consumers. Embedded fixture payloads and production identity are separate.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "source_identity.rs"]
pub(crate) mod source_identity;

pub fn workspace_root() -> Result<PathBuf, String> {
    let manifest = runtime_path(
        std::env::var_os("CARGO_MANIFEST_DIR"),
        env!("CARGO_MANIFEST_DIR"),
        std::env::var_os("NEXTEST").is_some(),
        "CARGO_MANIFEST_DIR",
    )?;
    workspace_from_manifest(&manifest, env!("REPO_POLICY_SOURCE_ID"))
}

fn workspace_from_manifest(manifest: &Path, expected: &str) -> Result<PathBuf, String> {
    // Resolve the manifest itself before walking up: Windows folds `..`
    // lexically, so `missing/../..` would name an existing directory there
    // while Unix refuses it.
    let resource_root_error = |e: &dyn std::fmt::Display| {
        format!("policy test resource root {}: {e}", manifest.display())
    };
    let manifest = manifest
        .canonicalize()
        .map_err(|e| resource_root_error(&e))?;
    let root = manifest
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| resource_root_error(&"no workspace above the manifest"))?
        .to_path_buf();
    let expected_manifest = root
        .join("tools/repo-policy")
        .canonicalize()
        .map_err(|e| format!("policy test resource root {}: {e}", root.display()))?;
    if expected_manifest != manifest {
        return Err(format!(
            "policy test resource manifest mismatch: manifest {}, root {}, expected manifest {}",
            manifest.display(),
            root.display(),
            expected_manifest.display()
        ));
    }
    let observed = source_identity::source_identity(&root)?;
    if observed != expected {
        return Err(format!(
            "policy test resource source digest mismatch: manifest {}, root {}, expected {expected}, observed {observed}",
            manifest.display(),
            root.display()
        ));
    }
    Ok(root)
}

pub fn policy_binary(compiled: &str) -> Result<PathBuf, String> {
    // Artifact custody is supplied by Cargo/nextest, not by searching PATH or
    // a default target. Production commands still verify source/compiler identity.
    policy_binary_from_runtime(
        std::env::var_os("CARGO_BIN_EXE_repo-policy"),
        compiled,
        std::env::var_os("NEXTEST").is_some(),
    )
}

fn policy_binary_from_runtime(
    runtime: Option<std::ffi::OsString>,
    compiled: &str,
    nextest: bool,
) -> Result<PathBuf, String> {
    let path = runtime_path(runtime, compiled, nextest, "CARGO_BIN_EXE_repo-policy")?;
    if !path.is_file() {
        return Err(format!(
            "policy test executable is missing: {}",
            path.display()
        ));
    }
    Ok(path)
}

fn runtime_path(
    runtime: Option<std::ffi::OsString>,
    compiled: &str,
    nextest: bool,
    name: &str,
) -> Result<PathBuf, String> {
    let path = match runtime {
        Some(value) => PathBuf::from(value),
        None if !nextest => PathBuf::from(compiled),
        None => return Err(format!("policy test runtime is missing {name}")),
    };
    if !path.is_absolute() {
        return Err(format!("policy test runtime {name} must be absolute"));
    }
    Ok(path)
}

pub fn scratch(label: &str) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = parent.join(format!(
        "{label}-{}-{stamp}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path)
        .map_err(|e| format!("policy test scratch {}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refusal<T>(result: Result<T, String>) -> Result<String, String> {
        match result {
            Err(error) => Ok(error),
            Ok(_) => Err("expected policy runtime resource refusal".into()),
        }
    }

    #[test]
    fn runner_paths_never_fall_back_or_accept_relative_paths() -> Result<(), String> {
        assert_eq!(
            refusal(runtime_path(
                None,
                env!("CARGO_MANIFEST_DIR"),
                true,
                "manifest"
            ))?,
            "policy test runtime is missing manifest"
        );
        assert_eq!(
            refusal(runtime_path(
                Some("relative".into()),
                env!("CARGO_MANIFEST_DIR"),
                true,
                "manifest"
            ))?,
            "policy test runtime manifest must be absolute"
        );
        let root = workspace_root()?;
        assert_eq!(
            workspace_from_manifest(
                &root.join("tools/repo-policy"),
                env!("REPO_POLICY_SOURCE_ID")
            )?,
            root
        );
        let wrong_source = refusal(workspace_from_manifest(
            &root.join("tools/repo-policy"),
            "wrong-source",
        ))?;
        assert!(wrong_source.contains("policy test resource source digest mismatch:"));
        assert!(wrong_source.contains("expected wrong-source"));
        assert!(
            refusal(workspace_from_manifest(
                &root.join("missing"),
                env!("REPO_POLICY_SOURCE_ID")
            ))?
            .starts_with("policy test resource root ")
        );
        if std::env::var_os("CARGO_BIN_EXE_repo-policy").is_some() {
            assert!(policy_binary("unused-compiled-fallback")?.is_absolute());
        } else {
            assert_eq!(
                refusal(policy_binary("missing-policy-executable"))?,
                if std::env::var_os("NEXTEST").is_some() {
                    "policy test runtime is missing CARGO_BIN_EXE_repo-policy"
                } else {
                    "policy test runtime CARGO_BIN_EXE_repo-policy must be absolute"
                }
            );
        }
        let first = scratch("policy-resource-control")?;
        let second = scratch("policy-resource-control")?;
        assert_ne!(first, second);
        assert!(first.starts_with(std::env::temp_dir()));
        assert!(second.starts_with(std::env::temp_dir()));
        let missing = first.join("missing-policy-executable");
        assert_eq!(
            refusal(policy_binary_from_runtime(
                Some(missing.clone().into_os_string()),
                "unused",
                true
            ))?,
            format!("policy test executable is missing: {}", missing.display())
        );
        std::fs::remove_dir(&first).map_err(|e| e.to_string())?;
        std::fs::remove_dir(&second).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn custody_distinguishes_manifest_and_source_digest_mismatches() -> Result<(), String> {
        let root = scratch("policy-source-custody")?;
        let manifest = root.join("tools/repo-policy");
        let other_manifest = root.join("other/repo-policy");
        std::fs::create_dir_all(&manifest).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&other_manifest).map_err(|e| e.to_string())?;
        for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml"] {
            std::fs::write(root.join(name), name).map_err(|e| e.to_string())?;
        }
        let source = manifest.join("fixture.rs");
        std::fs::write(&source, "original").map_err(|e| e.to_string())?;
        let expected = source_identity::source_identity(&root)?;
        let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
        assert_eq!(workspace_from_manifest(&manifest, &expected)?, canonical_root);

        let path_error = refusal(workspace_from_manifest(&other_manifest, &expected))?;
        let actual_manifest = other_manifest.canonicalize().map_err(|e| e.to_string())?;
        let expected_manifest = manifest.canonicalize().map_err(|e| e.to_string())?;
        assert_eq!(
            path_error,
            format!(
                "policy test resource manifest mismatch: manifest {}, root {}, expected manifest {}",
                actual_manifest.display(),
                canonical_root.display(),
                expected_manifest.display()
            )
        );

        std::fs::write(&source, "changed").map_err(|e| e.to_string())?;
        let observed = source_identity::source_identity(&root)?;
        assert_ne!(expected, observed);
        let digest_error = refusal(workspace_from_manifest(&manifest, &expected))?;
        assert_eq!(
            digest_error,
            format!(
                "policy test resource source digest mismatch: manifest {}, root {}, expected {expected}, observed {observed}",
                expected_manifest.display(),
                canonical_root.display()
            )
        );
        std::fs::remove_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(())
    }
}
