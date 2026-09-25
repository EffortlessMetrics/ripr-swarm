//! Capped file reads for the TypeScript preview adapter.
//!
//! Mirrors the repo-standard bounded-read pattern from `edit_cage.rs`
//! (`symlink_metadata` check → regular-file open → `take(limit + 1)` → reject
//! on excess) so a pathological workspace file can never be slurped fully into
//! memory. Two bounds apply per analysis run:
//!
//! - a per-file cap ([`DEFAULT_TS_MAX_FILE_READ_BYTES`], env
//!   `RIPR_TS_MAX_FILE_READ_BYTES`), and
//! - an aggregate per-run byte budget ([`DEFAULT_TS_MAX_WORKSPACE_READ_BYTES`],
//!   env `RIPR_TS_MAX_WORKSPACE_READ_BYTES`).
//!
//! Files that exceed either bound are surfaced as named limitations by the
//! adapter — never silently skipped. Plain IO failures (unreadable files) keep
//! the pre-existing silent-continue behaviour; lane `ts-d-silent-gaps` owns
//! disclosure for those, and this lane deliberately leaves that channel alone.

use std::collections::HashMap;
use std::io::Read as _;
use std::path::Path;

/// Env override for [`DEFAULT_TS_MAX_FILE_READ_BYTES`].
pub(crate) const TS_MAX_FILE_READ_BYTES_ENV: &str = "RIPR_TS_MAX_FILE_READ_BYTES";
/// Per-file read cap, mirroring `edit_cage.rs::MAX_CAPTURE_FILE_BYTES`.
pub(crate) const DEFAULT_TS_MAX_FILE_READ_BYTES: u64 = 16 * 1024 * 1024;
/// Env override for [`DEFAULT_TS_MAX_WORKSPACE_READ_BYTES`].
pub(crate) const TS_MAX_WORKSPACE_READ_BYTES_ENV: &str = "RIPR_TS_MAX_WORKSPACE_READ_BYTES";
/// Aggregate per-run byte budget across every workspace source read.
pub(crate) const DEFAULT_TS_MAX_WORKSPACE_READ_BYTES: u64 = 64 * 1024 * 1024;

/// Why a capped read did not produce source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CappedReadError {
    /// Metadata/open/decode failure. Lane `ts-d-silent-gaps` owns disclosure
    /// for unreadable files; this lane keeps the silent-continue there.
    Io(String),
    /// The file's size exceeds the per-file cap.
    OverFileLimit {
        /// The per-file cap that was exceeded, in bytes.
        limit: u64,
    },
    /// The per-run aggregate byte budget is (or would be) exhausted.
    OverWorkspaceBudget {
        /// Bytes remaining in the aggregate budget when the file was rejected.
        remaining: u64,
    },
}

impl CappedReadError {
    /// Distinguishable, human-readable reason for limitation disclosure.
    pub(crate) fn reason(&self) -> String {
        match self {
            Self::Io(err) => format!("read error: {err}"),
            Self::OverFileLimit { limit } => format!(
                "file_read_capped: file exceeds the {limit}-byte capped read limit ({TS_MAX_FILE_READ_BYTES_ENV})"
            ),
            Self::OverWorkspaceBudget { remaining } => format!(
                "workspace_read_budget_exhausted: per-run read budget exhausted with only {remaining} bytes remaining ({TS_MAX_WORKSPACE_READ_BYTES_ENV})"
            ),
        }
    }

    /// Whether this error is a size bound (surfaced as a named limitation)
    /// rather than a plain IO failure (left to the read-error lane).
    pub(crate) fn is_size_limit(&self) -> bool {
        matches!(
            self,
            Self::OverFileLimit { .. } | Self::OverWorkspaceBudget { .. }
        )
    }
}

/// Parse a positive byte limit from an env override, failing closed to the
/// error string on invalid input (mirrors `rust.rs::positive_limit_from_env`).
pub(crate) fn ts_byte_limit_from_env(
    env_name: &str,
    default: u64,
    value: Result<String, std::env::VarError>,
) -> Result<u64, String> {
    match value {
        Ok(raw) => {
            let parsed = raw
                .trim()
                .parse::<u64>()
                .map_err(|err| format!("{env_name} must be a positive integer: {err}"))?;
            if parsed == 0 {
                return Err(format!("{env_name} must be a positive integer"));
            }
            Ok(parsed)
        }
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("{env_name} must be a positive integer"))
        }
    }
}

/// Per-file cap resolved from the environment, failing closed to the default
/// so a malformed operator override cannot abort analysis.
pub(crate) fn ts_file_read_limit() -> u64 {
    ts_byte_limit_from_env(
        TS_MAX_FILE_READ_BYTES_ENV,
        DEFAULT_TS_MAX_FILE_READ_BYTES,
        std::env::var(TS_MAX_FILE_READ_BYTES_ENV),
    )
    .unwrap_or(DEFAULT_TS_MAX_FILE_READ_BYTES)
}

/// Aggregate per-run byte budget resolved from the environment.
pub(crate) fn ts_workspace_read_budget() -> u64 {
    ts_byte_limit_from_env(
        TS_MAX_WORKSPACE_READ_BYTES_ENV,
        DEFAULT_TS_MAX_WORKSPACE_READ_BYTES,
        std::env::var(TS_MAX_WORKSPACE_READ_BYTES_ENV),
    )
    .unwrap_or(DEFAULT_TS_MAX_WORKSPACE_READ_BYTES)
}

/// Result of reading every discovered workspace source under the caps.
pub(crate) struct CappedWorkspaceSources {
    /// Files read successfully, keyed by workspace-relative path. Also serves
    /// as the Phase-1 source cache: every consumer (parse check, owner/test
    /// extraction, re-export index) reads from here instead of re-reading.
    pub(crate) sources: HashMap<std::path::PathBuf, String>,
    /// Named read-limit entries: one per over-limit file, plus at most one
    /// workspace-budget entry (the first file that tripped it).
    pub(crate) limits: Vec<(std::path::PathBuf, CappedReadError)>,
    /// Plain IO failures (missing file, permission, non-regular file,
    /// decode error), one per failed file. These are NOT read-limit
    /// entries: disclosure is owned by the read-failure lane, which counts
    /// them as skipped files and names unreadable changed paths.
    pub(crate) io_failures: Vec<(std::path::PathBuf, String)>,
}

/// Read every `files` entry under `root` with a per-file cap and an aggregate
/// byte budget.
///
/// - Files over the per-file cap each produce an `OverFileLimit` entry.
/// - The first file that does not fit the remaining aggregate budget produces
///   a single `OverWorkspaceBudget` entry; subsequent files are skipped
///   silently because that one entry already discloses the bound.
/// - Plain IO failures are returned in `io_failures` for the read-failure
///   disclosure lane; they never produce a read-limit entry.
pub(crate) fn read_workspace_sources_capped(
    root: &Path,
    files: &[std::path::PathBuf],
    file_limit: u64,
    workspace_budget: u64,
) -> CappedWorkspaceSources {
    let mut sources = HashMap::new();
    let mut limits = Vec::new();
    let mut io_failures = Vec::new();
    let mut budget_reported = false;
    let mut consumed = 0u64;
    for relative in files {
        let mut remaining = workspace_budget.saturating_sub(consumed);
        let outcome = read_source_capped(&root.join(relative), file_limit, Some(&mut remaining));
        if let Ok(source) = &outcome {
            consumed = consumed.saturating_add(source.len() as u64);
        }
        match outcome {
            Ok(source) => {
                sources.insert(relative.clone(), source);
            }
            Err(err) => match err {
                CappedReadError::OverFileLimit { .. } => {
                    limits.push((relative.clone(), err));
                }
                CappedReadError::OverWorkspaceBudget { .. } => {
                    if !budget_reported {
                        limits.push((relative.clone(), err));
                        budget_reported = true;
                    }
                }
                CappedReadError::Io(message) => {
                    io_failures.push((relative.clone(), message));
                }
            },
        }
    }
    CappedWorkspaceSources {
        sources,
        limits,
        io_failures,
    }
}

/// Read a single config-style file (`tsconfig.json`, `package.json`) under the
/// per-file cap, without touching the workspace aggregate budget.
pub(crate) fn read_config_capped(path: &Path) -> Result<String, CappedReadError> {
    read_source_capped(path, ts_file_read_limit(), None)
}

/// Bounded read mirroring `edit_cage.rs`: metadata check → regular-file open
/// → `take(limit + 1)` → reject on excess.
fn read_source_capped(
    path: &Path,
    file_limit: u64,
    mut budget: Option<&mut u64>,
) -> Result<String, CappedReadError> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|err| CappedReadError::Io(format!("inspect {}: {err}", path.display())))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() || !file_type.is_file() {
        return Err(CappedReadError::Io(format!(
            "not a regular file: {}",
            path.display()
        )));
    }
    if metadata.len() > file_limit {
        return Err(CappedReadError::OverFileLimit { limit: file_limit });
    }
    if let Some(remaining) = budget.as_deref() {
        if metadata.len() > *remaining {
            return Err(CappedReadError::OverWorkspaceBudget {
                remaining: *remaining,
            });
        }
    }
    let file = std::fs::File::open(path)
        .map_err(|err| CappedReadError::Io(format!("open {}: {err}", path.display())))?;
    let mut bytes = Vec::new();
    file.take(file_limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|err| CappedReadError::Io(format!("read {}: {err}", path.display())))?;
    if bytes.len() as u64 > file_limit {
        return Err(CappedReadError::OverFileLimit { limit: file_limit });
    }
    let text = String::from_utf8(bytes)
        .map_err(|err| CappedReadError::Io(format!("decode {}: {err}", path.display())))?;
    if let Some(remaining) = budget.as_deref_mut() {
        *remaining = remaining.saturating_sub(text.len() as u64);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!(
                "ripr-ts-bounded-read-{label}-{}-{stamp}",
                std::process::id()
            ));
            let created = fs::create_dir_all(&root);
            assert!(
                created.is_ok(),
                "create temp dir {}: {:?}",
                root.display(),
                created.err()
            );
            Self(root)
        }

        fn write(&self, name: &str, bytes: &[u8]) {
            let written = fs::write(self.0.join(name), bytes);
            assert!(
                written.is_ok(),
                "write temp file {name}: {:?}",
                written.err()
            );
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn over_limit_file_is_rejected_with_named_reason() {
        let dir = TempDir::new("over-limit");
        dir.write("big.ts", &[b'a'; 100]);
        let err = read_source_capped(&dir.0.join("big.ts"), 50, None)
            .expect_err("over-limit read must fail");
        assert_eq!(err, CappedReadError::OverFileLimit { limit: 50 });
        assert!(err.is_size_limit());
        assert!(err.reason().contains("file_read_capped"));
        assert!(err.reason().contains("50"));
    }

    #[test]
    fn under_limit_file_reads_and_reports_budget() {
        let dir = TempDir::new("under-limit");
        dir.write("ok.ts", b"export const value = 1;\n");
        let mut remaining = 10u64;
        let outcome = read_source_capped(&dir.0.join("ok.ts"), 1024, Some(&mut remaining));
        assert!(outcome.is_ok(), "under-limit read must succeed");
        let text = outcome.unwrap_or_default();
        assert!(text.contains("value = 1"));
        assert_eq!(remaining, 10 - text.len() as u64);
    }

    #[test]
    fn workspace_budget_exhaustion_is_distinguishable() {
        let dir = TempDir::new("budget");
        dir.write("a.ts", &[b'a'; 40]);
        dir.write("b.ts", &[b'b'; 40]);
        let mut remaining = 50u64;
        let first_outcome = read_source_capped(&dir.0.join("a.ts"), 1024, Some(&mut remaining));
        assert!(first_outcome.is_ok(), "first read must succeed");
        let first = first_outcome.unwrap_or_default();
        assert_eq!(remaining, 50 - first.len() as u64);
        let err = read_source_capped(&dir.0.join("b.ts"), 1024, Some(&mut remaining))
            .expect_err("budget-exhausting read must fail");
        assert!(matches!(err, CappedReadError::OverWorkspaceBudget { .. }));
        assert!(err.reason().contains("workspace_read_budget_exhausted"));
    }

    #[test]
    fn capped_workspace_read_collects_limit_once_per_trigger() {
        let dir = TempDir::new("workspace");
        dir.write("small.ts", b"export const a = 1;\n");
        dir.write("big.ts", &[b'x'; 200]);
        dir.write("also_big.ts", &[b'y'; 200]);
        let files = vec![
            PathBuf::from("small.ts"),
            PathBuf::from("big.ts"),
            PathBuf::from("also_big.ts"),
        ];
        let outcome = read_workspace_sources_capped(&dir.0, &files, 100, 1024);
        assert_eq!(outcome.sources.len(), 1, "only the small file is cached");
        assert_eq!(outcome.limits.len(), 2, "one entry per over-limit file");
        assert!(
            outcome
                .limits
                .iter()
                .all(|(_, err)| matches!(err, CappedReadError::OverFileLimit { .. }))
        );
    }

    #[test]
    fn capped_workspace_read_reports_budget_exhaustion_once() {
        let dir = TempDir::new("workspace-budget");
        dir.write("a.ts", &[b'a'; 60]);
        dir.write("b.ts", &[b'b'; 60]);
        dir.write("c.ts", &[b'c'; 60]);
        let files = vec![
            PathBuf::from("a.ts"),
            PathBuf::from("b.ts"),
            PathBuf::from("c.ts"),
        ];
        let outcome = read_workspace_sources_capped(&dir.0, &files, 1024, 100);
        assert_eq!(outcome.sources.len(), 1);
        assert_eq!(outcome.limits.len(), 1, "budget exhaustion reported once");
        assert!(
            matches!(
                outcome.limits[0].1,
                CappedReadError::OverWorkspaceBudget { .. }
            ),
            "expected budget error, got {:?}",
            outcome.limits[0].1
        );
    }

    #[test]
    fn byte_limit_env_parsing_matches_repo_conventions() {
        assert_eq!(
            ts_byte_limit_from_env("RIPR_TEST_X", 16, Err(std::env::VarError::NotPresent)),
            Ok(16)
        );
        assert_eq!(
            ts_byte_limit_from_env("RIPR_TEST_X", 16, Ok(" 64 ".to_string())),
            Ok(64)
        );
        assert!(ts_byte_limit_from_env("RIPR_TEST_X", 16, Ok("0".to_string())).is_err());
        assert!(ts_byte_limit_from_env("RIPR_TEST_X", 16, Ok("nope".to_string())).is_err());
    }
}
