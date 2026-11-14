//! Source digest for dirty or commit-less builds.
//!
//! `build.rs` bakes this digest into `RIPR_BUILD_SOURCE_DIGEST` only when the
//! build explicitly asks for persisted analysis caches
//! (`RIPR_PERSISTED_CACHE=1`). A dev or test build therefore keeps a stable
//! compile-time environment across source edits, so sccache can reuse the
//! compilation units it already has. Without the flag `build_identity` falls
//! back to a per-process identity and nothing persisted is reused, so a stale
//! cache still cannot be served.
//!
//! The digest is FNV-1a over the crate sources and the nearest workspace
//! manifest and lockfile: each file's path relative to that workspace root, its
//! length and its bytes, in sorted path order. `None` when any input cannot be
//! read, so an unreadable source never yields a digest that another build could
//! share.

use std::fs;
use std::path::{Path, PathBuf};

/// Crate sources that decide whether the built binary differs from the commit,
/// relative to the manifest directory.
pub(crate) const CRATE_SOURCES: [&str; 4] = ["src", "Cargo.toml", "build.rs", "vendor"];

/// Repository-root Cargo inputs a workspace member inherits: the package
/// version, lints and profiles, the lockfile, and Cargo configuration.
pub(crate) const WORKSPACE_INPUTS: [&str; 3] = ["Cargo.toml", "Cargo.lock", ".cargo"];

/// Whether this build should bake a source digest: only when the caller asked
/// for persisted analysis caches and the build has no commit that describes it.
pub(crate) fn should_bake(flag: Option<&str>, commit: &str, dirty: bool) -> bool {
    flag == Some("1") && (commit.is_empty() || dirty)
}

/// The nearest ancestor that holds a `Cargo.lock`, or `dir` itself.
pub(crate) fn workspace_root(dir: &Path) -> PathBuf {
    dir.ancestors()
        .find(|ancestor| ancestor.join("Cargo.lock").is_file())
        .unwrap_or(dir)
        .to_path_buf()
}

/// Every file the digest covers, sorted and deduplicated. `None` when a crate
/// source cannot be read.
pub(crate) fn source_files(dir: &Path) -> Option<Vec<PathBuf>> {
    let mut files = Vec::new();
    for source in CRATE_SOURCES {
        collect_files(&dir.join(source), &mut files)?;
    }
    let root = workspace_root(dir);
    for input in ["Cargo.toml", "Cargo.lock"] {
        let path = root.join(input);
        if path.is_file() {
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Some(files)
}

/// The digest of `files`, keyed by their path relative to `root`.
pub(crate) fn digest_files(root: &Path, files: &[PathBuf]) -> Option<String> {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for file in files {
        let bytes = fs::read(file).ok()?;
        let relative = file.strip_prefix(root).unwrap_or(file);
        feed(relative.to_string_lossy().replace('\\', "/").as_bytes());
        feed(&[0]);
        feed(&(bytes.len() as u64).to_le_bytes());
        feed(&bytes);
    }
    Some(format!("{hash:016x}"))
}

/// The digest of the crate at `dir`.
pub(crate) fn source_digest(dir: &Path) -> Option<String> {
    let root = workspace_root(dir);
    let files = source_files(dir)?;
    digest_files(&root, &files)
}

fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Option<()> {
    let metadata = fs::metadata(path).ok()?;
    if metadata.is_file() {
        files.push(path.to_path_buf());
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path).ok()? {
            collect_files(&entry.ok()?.path(), files)?;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_tree(name: &str, source: &str) -> Result<PathBuf, String> {
        let dir =
            std::env::temp_dir().join(format!("ripr-source-digest-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).map_err(|err| err.to_string())?;
        fs::create_dir_all(dir.join("vendor")).map_err(|err| err.to_string())?;
        fs::write(dir.join("src/lib.rs"), source).map_err(|err| err.to_string())?;
        fs::write(dir.join("Cargo.toml"), "[package]\nname = \"probe\"\n")
            .map_err(|err| err.to_string())?;
        fs::write(dir.join("Cargo.lock"), "version = 4\n").map_err(|err| err.to_string())?;
        fs::write(dir.join("build.rs"), "fn main() {}\n").map_err(|err| err.to_string())?;
        fs::write(dir.join("vendor/README.md"), "vendored\n").map_err(|err| err.to_string())?;
        Ok(dir)
    }

    #[test]
    fn digest_is_opt_in_and_only_for_builds_without_a_commit() {
        assert!(should_bake(Some("1"), "e4da2d4a", true));
        assert!(should_bake(Some("1"), "", false));
        assert!(!should_bake(None, "e4da2d4a", true));
        assert!(!should_bake(Some("0"), "e4da2d4a", true));
        assert!(!should_bake(Some("1"), "e4da2d4a", false));
    }

    #[test]
    fn digest_distinguishes_two_dirty_trees_and_matches_identical_ones() -> Result<(), String> {
        let first = temp_tree("first", "pub fn value() -> u8 { 1 }\n")?;
        let second = temp_tree("second", "pub fn value() -> u8 { 2 }\n")?;
        let copy = temp_tree("copy", "pub fn value() -> u8 { 1 }\n")?;

        let first_digest = source_digest(&first).ok_or("first digest")?;
        let second_digest = source_digest(&second).ok_or("second digest")?;
        let copy_digest = source_digest(&copy).ok_or("copy digest")?;

        assert_ne!(
            first_digest, second_digest,
            "two dirty trees with different sources must not share a persisted-cache digest"
        );
        assert_eq!(
            first_digest, copy_digest,
            "identical sources must produce the same digest"
        );

        for dir in [first, second, copy] {
            let _ = fs::remove_dir_all(dir);
        }
        Ok(())
    }
}
