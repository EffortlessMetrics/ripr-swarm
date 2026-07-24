//! Workspace-root path confinement.
//!
//! Diff-supplied paths and editor-supplied file selectors reach the analyzer
//! from untrusted sources (the contents of a `--diff` file, an editor
//! argument). Before any such path is joined to a workspace root and used for
//! file I/O or embedded into findings/SARIF, it must be confined to stay
//! under that root. This module owns that confinement check.
//!
//! The logic mirrors `crates/ripr/src/lsp/uri.rs::path_is_within_root`:
//! existing paths are canonicalized so symlink/junction escapes are rejected;
//! missing paths fall back to normalized lexical containment for diagnostics
//! and probe locations that refer to a future file. The two implementations
//! are duplicated for now (the LSP helper is `pub(super)`-scoped to the
//! `lsp` module); a future cleanup could extract a single shared helper.
//!
//! See issues #2099 (diff parser path traversal) and #2100 (`agent brief`
//! path confinement).

use std::path::{Component, Path, PathBuf};

/// Confine `path` to stay under `root`, returning the canonical absolute
/// candidate on success.
///
/// Relative paths are joined to `root` first. Existing paths are canonicalized
/// (so symlink and junction escapes are rejected); missing tail components
/// fall back to lexical normalization so a probe that points at a not-yet-
/// written file can still be confined. The error message follows the
/// established `"<context> {} must stay under root {}"` shape used by the
/// `validate_agent_*_path` family in `cli/commands_agent_support.rs`.
pub(crate) fn confine_path_to_root(root: &Path, path: &Path) -> Result<PathBuf, String> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let canonical_root = canonical_or_normalized(root);
    let canonical_candidate = canonical_or_normalized(&candidate);
    if paths_equal_or_below(&canonical_root, &canonical_candidate) {
        Ok(canonical_candidate)
    } else {
        Err(format!(
            "{} must stay under root {}",
            path.display(),
            root.display()
        ))
    }
}

fn canonical_or_normalized(path: &Path) -> PathBuf {
    canonicalize_with_missing_tail(path).unwrap_or_else(|| normalize_path(path))
}

/// Canonicalize the longest existing ancestor of `path`, then re-append the
/// missing tail. This confines paths whose final components refer to a file
/// that does not exist yet (a common case for diff-derived probe locations
/// and editor briefs that name a future file).
fn canonicalize_with_missing_tail(path: &Path) -> Option<PathBuf> {
    let mut current = path.to_path_buf();
    let mut missing = Vec::new();
    loop {
        if let Ok(mut canonical) = current.canonicalize() {
            for component in missing.iter().rev() {
                canonical.push(component);
            }
            return Some(canonical);
        }

        let component = current.file_name()?.to_os_string();
        missing.push(component);
        if !current.pop() {
            return None;
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
        }
    }
    normalized
}

fn paths_equal_or_below(root: &Path, candidate: &Path) -> bool {
    if cfg!(windows) {
        let root_components = root
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>();
        let candidate_components = candidate
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_ascii_lowercase())
            .collect::<Vec<_>>();
        candidate_components.len() >= root_components.len()
            && candidate_components[..root_components.len()] == root_components[..]
    } else {
        candidate == root || candidate.starts_with(root)
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "Confinement tests assert an expected Ok/Err variant via `.expect(\"why\")`; the message records the contract under test."
)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Create a unique temp dir for one test and return its path plus a
    /// cleanup guard. Mirrors the pattern in `lsp/uri.rs` tests.
    struct ScratchDir {
        path: PathBuf,
    }

    impl ScratchDir {
        fn new(label: &str) -> Self {
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("ripr-path-{label}-{suffix}"));
            std::fs::create_dir_all(&path).expect("create scratch dir");
            Self { path }
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn relative_path_under_root_is_confined() {
        let dir = ScratchDir::new("in-root");
        // The Ok contract is what we test: the helper must not reject an
        // in-root relative path. (We do not assert on the returned PathBuf's
        // prefix because canonicalization may rewrite the temp-dir prefix on
        // Windows junctions / short-name paths.)
        confine_path_to_root(&dir.path, Path::new("src/lib.rs"))
            .expect("relative path under root should be confined");
    }

    #[test]
    fn parent_traversal_is_rejected() {
        let dir = ScratchDir::new("traversal");
        let err = confine_path_to_root(&dir.path, Path::new("../outside.rs"))
            .expect_err("parent traversal should be rejected");
        assert!(
            err.contains("must stay under root"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn foreign_absolute_path_is_rejected() {
        let dir = ScratchDir::new("foreign");
        let foreign = if cfg!(windows) {
            Path::new("C:/Windows/System32/drivers/etc/hosts")
        } else {
            Path::new("/etc/passwd")
        };
        let err = confine_path_to_root(&dir.path, foreign)
            .expect_err("foreign absolute path should be rejected");
        assert!(err.contains("must stay under root"));
    }

    #[test]
    fn missing_tail_under_root_is_confined() {
        // A path whose final component does not exist yet should still be
        // confined when its ancestor is under root. This is the diff/probe
        // case: a probe may point at a file added by the diff that has not
        // been written to disk in the base workspace.
        let dir = ScratchDir::new("missing-tail");
        confine_path_to_root(&dir.path, Path::new("src/not_yet_written.rs"))
            .expect("missing tail under root should be confined via lexical fallback");
    }

    #[test]
    fn missing_tail_via_parent_traversal_is_rejected() {
        let dir = ScratchDir::new("missing-traversal");
        let err = confine_path_to_root(&dir.path, Path::new("../sibling/not_yet_written.rs"))
            .expect_err("parent-traversal missing tail should be rejected");
        assert!(err.contains("must stay under root"));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_is_rejected() {
        use std::os::unix::fs::symlink;
        let inside = ScratchDir::new("symlink-inside");
        let outside = ScratchDir::new("symlink-outside");
        let link = inside.path.join("linked");
        symlink(&outside.path, &link).expect("create symlink");
        let err = confine_path_to_root(&inside.path, Path::new("linked/secret.rs"))
            .expect_err("symlink escape should be rejected");
        assert!(err.contains("must stay under root"));
    }
}
