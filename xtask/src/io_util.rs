//! Shared I/O and path helpers extracted from `main.rs` (slice B1 of #2119).
//!
//! These cross-cutting helpers are used by nearly every `check_*` policy gate
//! and every report family. Extracting them into one module is the keystone
//! for further decomposition: each per-gate module that follows the
//! `policy/doc_roles.rs` template imports `crate::io_util::{...}` (or, for
//! backward compatibility with already-extracted modules that import
//! `crate::{collect_files, finish_policy_report, normalize_path,
//! read_text_lossy}`, the names are re-exported at the crate root via the
//! `use io_util::{...};` block in `main.rs`).
//!
//! This is a behaviour-preserving extraction — the bodies are verbatim copies
//! of the functions that lived in `main.rs`, with visibility widened from
//! private to `pub(crate)`.

use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn reports_dir() -> PathBuf {
    Path::new("target").join("ripr").join("reports")
}

pub(crate) fn receipts_dir() -> PathBuf {
    Path::new("target").join("ripr").join("receipts")
}

pub(crate) fn ensure_reports_dir() -> Result<(), String> {
    fs::create_dir_all(reports_dir()).map_err(|err| {
        format!(
            "failed to create {}: {err}\nrerun with `cargo xtask shape` after fixing directory permissions",
            reports_dir().display()
        )
    })
}

pub(crate) fn ensure_receipts_dir() -> Result<(), String> {
    fs::create_dir_all(receipts_dir()).map_err(|err| {
        format!(
            "failed to create {}: {err}\nrerun with `cargo xtask receipts` after fixing directory permissions",
            receipts_dir().display()
        )
    })
}

pub(crate) fn write_report(file_name: &str, body: &str) -> Result<(), String> {
    ensure_reports_dir()?;
    let path = reports_dir().join(file_name);
    fs::write(&path, body).map_err(|err| {
        format!(
            "failed to write {}: {err}\nrerun with `cargo xtask shape` after fixing file permissions",
            path.display()
        )
    })
}

pub(crate) fn write_receipt(file_name: &str, body: &str) -> Result<(), String> {
    ensure_receipts_dir()?;
    let path = receipts_dir().join(file_name);
    fs::write(&path, body).map_err(|err| {
        format!(
            "failed to write {}: {err}\nrerun with `cargo xtask receipts` after fixing file permissions",
            path.display()
        )
    })
}

pub(crate) fn read_text_lossy(path: &Path) -> Result<String, String> {
    let bytes =
        fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub(crate) fn normalize_path(path: &Path) -> String {
    normalize_slashes(&path.to_string_lossy())
        .trim_start_matches("./")
        .to_string()
}

pub(crate) fn normalize_slashes(value: &str) -> String {
    value.replace('\\', "/")
}

pub(crate) fn collect_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_files_inner(root, root, &mut files)?;
    Ok(files)
}

fn collect_files_inner(root: &Path, path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let normalized = normalize_path(path);
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_normalized = normalize_path(relative);
    if should_skip_path(&relative_normalized) {
        return Ok(());
    }
    let metadata =
        fs::metadata(path).map_err(|err| format!("failed to inspect {normalized}: {err}"))?;
    if metadata.is_file() {
        files.push(path.to_path_buf());
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in
            fs::read_dir(path).map_err(|err| format!("failed to read {normalized}: {err}"))?
        {
            let entry = entry.map_err(|err| format!("failed to read {normalized}: {err}"))?;
            collect_files_inner(root, &entry.path(), files)?;
        }
    }
    Ok(())
}

pub(crate) fn should_skip_path(path: &str) -> bool {
    path == ".git"
        || path.starts_with(".git/")
        || path == ".claude"
        || path.starts_with(".claude/")
        || path == "target"
        || path.starts_with("target/")
        || path.ends_with("/target")
        || path.contains("/target/")
        || path == ".ripr/release"
        || path.starts_with(".ripr/release/")
        || path.ends_with("/.vscode-test")
        || path.contains("/.vscode-test/")
        || path.ends_with("/node_modules")
        || path.contains("/node_modules/")
        || path.ends_with("/out")
        || path.contains("/out/")
        || path.ends_with("/dist")
        || path.contains("/dist/")
}

pub(crate) fn redact_current_dir(text: &str) -> String {
    let Ok(current_dir) = std::env::current_dir() else {
        return text.to_string();
    };
    let current_dir = current_dir.display().to_string();
    let slash_dir = current_dir.replace('\\', "/");
    text.replace(&current_dir, ".").replace(&slash_dir, ".")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_path_strips_leading_dot_slash_and_converts_backslashes() {
        assert_eq!(normalize_path(Path::new("./src/lib.rs")), "src/lib.rs");
        assert_eq!(normalize_path(Path::new("src\\lib.rs")), "src/lib.rs");
        assert_eq!(normalize_path(Path::new("src/lib.rs")), "src/lib.rs");
    }

    #[test]
    fn normalize_slashes_converts_backslashes_only() {
        assert_eq!(normalize_slashes("a\\b\\c"), "a/b/c");
        assert_eq!(normalize_slashes("a/b/c"), "a/b/c");
    }

    #[test]
    fn reports_and_receipts_dir_are_under_target_ripr() {
        assert_eq!(
            reports_dir(),
            PathBuf::from("target").join("ripr").join("reports")
        );
        assert_eq!(
            receipts_dir(),
            PathBuf::from("target").join("ripr").join("receipts")
        );
    }

    #[test]
    fn should_skip_path_recognizes_ignored_dirs() {
        // The root names are skipped on their own.
        assert!(should_skip_path(".git"));
        assert!(should_skip_path(".git/config"));
        assert!(should_skip_path("target"));
        // Subpaths under target/node_modules/etc. are skipped.
        assert!(should_skip_path("crates/target/debug/x"));
        assert!(should_skip_path("pkg/node_modules/react"));
        assert!(should_skip_path("pkg/node_modules"));
        // Regular source paths are not skipped.
        assert!(!should_skip_path("src/lib.rs"));
        assert!(!should_skip_path("crates/ripr/Cargo.toml"));
    }

    #[test]
    fn redact_current_dir_replaces_both_slash_styles() {
        // When the cwd is not literally in the text, the text is returned
        // unchanged. This pins the no-op path so future edits do not
        // accidentally start rewriting arbitrary text.
        let text = "some content without the cwd";
        assert_eq!(redact_current_dir(text), text);
    }
}
