//! Hardened temporary diff-file creation for the `diff` command (#2102).
//!
//! The diff text is staged in a per-invocation private directory directly
//! under the OS temp dir so a crafted local environment cannot win a
//! symlink race against a predictable path, and so no fixed shared name
//! lets one local account deny the feature to another.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Write `diff_text` to a fresh private temporary file and return its path.
///
/// The caller owns cleanup: remove the file and its parent directory when
/// the analysis completes.
pub(crate) fn write_temporary_diff_file(diff_text: &str) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let temp_root = std::env::temp_dir();
    if let Err(err) = std::fs::create_dir_all(&temp_root) {
        return Err(format!(
            "create temporary diff root {} failed: {err}; check the temp directory is writable (set TMPDIR, or TEMP on Windows, to a writable directory)",
            temp_root.display()
        ));
    }
    write_diff_under(&temp_root, stamp, diff_text)
}

/// Stage `diff_text` in a fresh private directory directly under `temp_root`,
/// named from the process id, `stamp` and an attempt suffix.
fn write_diff_under(temp_root: &Path, stamp: u128, diff_text: &str) -> Result<PathBuf, String> {
    // Per-invocation private directory directly under the OS temp dir
    // (#2102). A fixed shared name like `/tmp/ripr-diff` lets the first local
    // account own the directory and deny every other account (cross-user
    // DoS), and lets an attacker pre-plant a symlink at the shared parent to
    // redirect the write. The unpredictable nanosecond stamp plus mkdir
    // EEXIST semantics prevent both: the directory is created fresh with
    // owner-only permissions, and a pre-existing name (including a symlink)
    // is never followed.
    for attempt in 0..16u32 {
        let suffix = if attempt == 0 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let dir = temp_root.join(format!("ripr-diff-{}-{stamp}{suffix}", std::process::id()));
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = std::fs::DirBuilder::new();
            builder.mode(0o700);
            builder
        };
        #[cfg(not(unix))]
        let builder = std::fs::DirBuilder::new();
        match builder.create(&dir) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => {
                return Err(format!(
                    "create temporary diff dir {} failed: {err}; check the temp directory is writable (set TMPDIR, or TEMP on Windows, to a writable directory)",
                    dir.display()
                ));
            }
        }
        let path = dir.join("diff.patch");
        // create_new fails if the file exists, including as a symlink, so a
        // pre-planted symlink cannot win a TOCTOU overwrite (same race class
        // as #1948). Owner-only file permissions keep the diff unreadable by
        // other local users even if the parent is somehow reachable.
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(mut file) => {
                use std::io::Write;
                if let Err(err) = file.write_all(diff_text.as_bytes()) {
                    // Do not leak the partial file (or the private dir) when
                    // the write fails: the caller never receives the path and
                    // cannot clean up.
                    let _ = std::fs::remove_file(&path);
                    let _ = std::fs::remove_dir(&dir);
                    return Err(format!(
                        "write temporary diff file {} failed: {err}; check free space in the temp directory (set TMPDIR, or TEMP on Windows, to a writable directory)",
                        path.display()
                    ));
                }
                return Ok(path);
            }
            Err(err) => {
                let _ = std::fs::remove_dir(&dir);
                return Err(format!(
                    "create temporary diff file {} failed: {err}; check the temp directory is writable (set TMPDIR, or TEMP on Windows, to a writable directory)",
                    path.display()
                ));
            }
        }
    }
    Err("create temporary diff dir failed: all name candidates exist; clear old ripr temp directories or set TMPDIR, or TEMP on Windows, to a writable directory".to_string())
}

#[cfg(test)]
mod tests {
    use super::{write_diff_under, write_temporary_diff_file};
    use std::path::{Path, PathBuf};

    #[test]
    fn write_temporary_diff_file_writes_content_in_restricted_dir() -> Result<(), String> {
        let diff_text =
            "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n";
        let path = write_temporary_diff_file(diff_text)?;

        let content = std::fs::read_to_string(&path).map_err(|err| err.to_string())?;
        assert_eq!(content, diff_text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let parent = path
                .parent()
                .ok_or_else(|| "temporary diff path has no parent".to_string())?;
            // Per-invocation private dir is owner-only (#2102).
            let dir_mode = std::fs::metadata(parent)
                .map_err(|err| err.to_string())?
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(dir_mode, 0o700);
            // The diff file itself is owner-read/write only.
            let file_mode = std::fs::metadata(&path)
                .map_err(|err| err.to_string())?
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(file_mode, 0o600);
        }
        std::fs::remove_file(&path).map_err(|err| err.to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::remove_dir(parent).map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    const STAMP: u128 = 4_242;
    const DIFF: &str = "diff --git a/src/lib.rs b/src/lib.rs\n";

    /// A fresh, test-owned root so candidate names can be pre-planted.
    fn fresh_root(name: &str) -> Result<PathBuf, String> {
        let root =
            std::env::temp_dir().join(format!("ripr-temp-diff-test-{}-{name}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).map_err(|err| err.to_string())?;
        }
        std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        Ok(root)
    }

    /// Candidate names spelled out literally, so the test does not share the
    /// production suffix rule: no suffix first, then `-1`, `-2`, ...
    fn candidate(root: &Path, attempt: u32) -> PathBuf {
        let pid = std::process::id();
        let name = match attempt {
            0 => format!("ripr-diff-{pid}-4242"),
            n => format!("ripr-diff-{pid}-4242-{n}"),
        };
        root.join(name)
    }

    fn plant(root: &Path, attempts: std::ops::Range<u32>) -> Result<(), String> {
        for attempt in attempts {
            std::fs::create_dir(candidate(root, attempt)).map_err(|err| err.to_string())?;
        }
        Ok(())
    }

    #[test]
    fn first_free_name_is_the_unsuffixed_candidate_under_the_given_root() -> Result<(), String> {
        let root = fresh_root("first")?;
        let path = write_diff_under(&root, STAMP, DIFF)?;
        assert_eq!(path, candidate(&root, 0).join("diff.patch"));
        assert_eq!(
            std::fs::read_to_string(&path).map_err(|err| err.to_string())?,
            DIFF
        );
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn an_existing_directory_is_skipped_and_left_untouched() -> Result<(), String> {
        let root = fresh_root("skip")?;
        plant(&root, 0..1)?;
        let path = write_diff_under(&root, STAMP, DIFF)?;
        assert_eq!(path, candidate(&root, 1).join("diff.patch"));
        let planted = std::fs::read_dir(candidate(&root, 0))
            .map_err(|err| err.to_string())?
            .count();
        assert_eq!(planted, 0, "the pre-existing directory gained an entry");
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    /// A symlink planted at the predictable name is never followed: the diff
    /// does not reach the link target (#2102).
    #[cfg(unix)]
    #[test]
    fn a_planted_symlink_is_skipped_rather_than_followed() -> Result<(), String> {
        let root = fresh_root("symlink")?;
        let target = root.join("attacker-owned");
        std::fs::create_dir(&target).map_err(|err| err.to_string())?;
        std::os::unix::fs::symlink(&target, candidate(&root, 0)).map_err(|err| err.to_string())?;
        let path = write_diff_under(&root, STAMP, DIFF)?;
        assert_eq!(path, candidate(&root, 1).join("diff.patch"));
        assert!(!target.join("diff.patch").exists());
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn the_sixteenth_candidate_is_the_last_one_tried() -> Result<(), String> {
        let root = fresh_root("last")?;
        plant(&root, 0..15)?;
        let path = write_diff_under(&root, STAMP, DIFF)?;
        assert_eq!(path, candidate(&root, 15).join("diff.patch"));
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn sixteen_taken_names_refuse_with_a_cleanup_hint() -> Result<(), String> {
        let root = fresh_root("exhausted")?;
        plant(&root, 0..16)?;
        assert_eq!(
            write_diff_under(&root, STAMP, DIFF),
            Err("create temporary diff dir failed: all name candidates exist; clear old ripr temp directories or set TMPDIR, or TEMP on Windows, to a writable directory".to_string())
        );
        assert!(!candidate(&root, 16).exists());
        std::fs::remove_dir_all(&root).map_err(|err| err.to_string())
    }

    #[test]
    fn a_missing_root_refuses_at_once_instead_of_retrying() -> Result<(), String> {
        let parent = fresh_root("missing")?;
        let root = parent.join("absent");
        let message = match write_diff_under(&root, STAMP, DIFF) {
            Ok(path) => return Err(format!("expected a refusal, got {}", path.display())),
            Err(message) => message,
        };
        let expected_prefix = format!(
            "create temporary diff dir {} failed: ",
            candidate(&root, 0).display()
        );
        assert!(message.starts_with(&expected_prefix), "{message}");
        assert!(
            message.ends_with("check the temp directory is writable (set TMPDIR, or TEMP on Windows, to a writable directory)"),
            "{message}"
        );
        assert!(!root.exists());
        std::fs::remove_dir_all(&parent).map_err(|err| err.to_string())
    }
}
