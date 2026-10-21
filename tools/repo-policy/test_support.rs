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
    let root = manifest
        .join("../..")
        .canonicalize()
        .map_err(|e| format!("policy test resource root {}: {e}", manifest.display()))?;
    if root
        .join("tools/repo-policy")
        .canonicalize()
        .map_err(|e| e.to_string())?
        != manifest.canonicalize().map_err(|e| e.to_string())?
        || source_identity::source_identity(&root)? != expected
    {
        return Err("policy test resource source identity differs from the built harness".into());
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
        assert_eq!(
            refusal(workspace_from_manifest(
                &root.join("tools/repo-policy"),
                "wrong-source"
            ))?,
            "policy test resource source identity differs from the built harness"
        );
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
}
