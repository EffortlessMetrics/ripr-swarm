//! Capped diff-mode workspace walk for the Python preview adapter.
//!
//! Mirrors the bounded-read pattern from `typescript/bounded_read.rs` (itself
//! the repo-standard `edit_cage.rs` contract: `symlink_metadata` check →
//! regular-file open → `take(limit + 1)` → reject on excess) so a
//! pathological workspace can never be slurped fully into memory. Three
//! bounds apply per diff-mode analysis run:
//!
//! - a cap on the number of discovered `.py` files
//!   ([`DEFAULT_PYTHON_MAX_WORKSPACE_FILES`], env
//!   `RIPR_PYTHON_MAX_WORKSPACE_FILES`), defaulting to 4000 so a Django-sized
//!   workspace (2,937 `.py` files) is analyzed completely in diff mode
//!   (#10441). Diff mode reads the workspace only to resolve the changed
//!   lines, unlike the repo-mode working set (`RIPR_MAX_REPO_INDEX_FILES`,
//!   800, `repo/discovery.rs`) that analyzes every file;
//! - a per-file read cap ([`DEFAULT_PYTHON_MAX_FILE_READ_BYTES`], env
//!   `RIPR_PYTHON_MAX_FILE_READ_BYTES`), mirroring the TypeScript 16 MiB knob;
//! - an aggregate per-run byte budget
//!   ([`DEFAULT_PYTHON_MAX_WORKSPACE_READ_BYTES`], env
//!   `RIPR_PYTHON_MAX_WORKSPACE_READ_BYTES`), mirroring the TypeScript 64 MiB
//!   knob.
//!
//! Files refused by any bound are surfaced by the adapter as named typed
//! limitations — never silently skipped — with the per-run disclosure
//! sampled (#5022, `read_limit_disclosure.rs`). Plain IO failures
//! (unreadable or non-UTF-8 files) are disclosed by the adapter's
//! diff-scoped read-failure lane, mirroring the TypeScript adapter's #4099
//! fix: unreadable changed files become named limitations, unreadable
//! unchanged files count as skipped files.

use std::collections::HashMap;
use std::io::Read as _;
use std::path::Path;

/// Env override for [`DEFAULT_PYTHON_MAX_WORKSPACE_FILES`].
pub(crate) const PYTHON_MAX_WORKSPACE_FILES_ENV: &str = "RIPR_PYTHON_MAX_WORKSPACE_FILES";
/// Default cap on discovered `.py` files for one diff-mode walk.
///
/// Measured on 4 cores (#10441): Django at 6de4b35a (2,937 files, 18.6 MiB)
/// completes in about 5.5 s at 290 MiB peak RSS, where 800 left it partial.
/// Cost grows faster than the file count past that point (a 5,855-file
/// synthetic workspace took 13.7 s at 500 MiB), so larger workspaces keep the
/// named `diff_scope_oversized` limitation and raise the bound explicitly.
pub(crate) const DEFAULT_PYTHON_MAX_WORKSPACE_FILES: usize = 4000;
/// Env override for [`DEFAULT_PYTHON_MAX_FILE_READ_BYTES`].
pub(crate) const PYTHON_MAX_FILE_READ_BYTES_ENV: &str = "RIPR_PYTHON_MAX_FILE_READ_BYTES";
/// Per-file read cap, mirroring `typescript/bounded_read.rs`
/// (`DEFAULT_TS_MAX_FILE_READ_BYTES`) and `edit_cage.rs`.
pub(crate) const DEFAULT_PYTHON_MAX_FILE_READ_BYTES: u64 = 16 * 1024 * 1024;
/// Env override for [`DEFAULT_PYTHON_MAX_WORKSPACE_READ_BYTES`].
pub(crate) const PYTHON_MAX_WORKSPACE_READ_BYTES_ENV: &str = "RIPR_PYTHON_MAX_WORKSPACE_READ_BYTES";
/// Aggregate per-run byte budget across every workspace source read,
/// mirroring `typescript/bounded_read.rs`.
pub(crate) const DEFAULT_PYTHON_MAX_WORKSPACE_READ_BYTES: u64 = 64 * 1024 * 1024;

/// Resolved diff-mode walk bounds. `analyze_diff` fills this from the
/// environment; tests and inner entry points inject it explicitly because
/// edition 2024 makes `std::env::set_var` unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis::language::python) struct PythonDiffWalkLimits {
    /// Maximum number of discovered `.py` files analyzed per run.
    pub(in crate::analysis::language::python) max_workspace_files: usize,
    /// Per-file read cap in bytes.
    pub(in crate::analysis::language::python) max_file_read_bytes: u64,
    /// Aggregate per-run byte budget in bytes.
    pub(in crate::analysis::language::python) max_workspace_read_bytes: u64,
}

impl PythonDiffWalkLimits {
    /// Resolve every bound from the environment, failing closed to each
    /// default so a malformed operator override cannot abort analysis
    /// (mirrors `ts_file_read_limit` / `ts_workspace_read_budget`).
    pub(in crate::analysis::language::python) fn from_env() -> Self {
        Self {
            max_workspace_files: python_workspace_file_limit(),
            max_file_read_bytes: python_file_read_limit(),
            max_workspace_read_bytes: python_workspace_read_budget(),
        }
    }
}

/// Why a capped read did not produce source text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CappedReadError {
    /// Metadata/open/decode failure. The adapter owns disclosure for
    /// unreadable files (the #4099 model); the raw message is kept here so
    /// the limitation can name the concrete failure.
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
                "file_read_capped: file exceeds the {limit}-byte capped read limit ({PYTHON_MAX_FILE_READ_BYTES_ENV})"
            ),
            Self::OverWorkspaceBudget { remaining } => format!(
                "workspace_read_budget_exhausted: per-run read budget exhausted with only {remaining} bytes remaining ({PYTHON_MAX_WORKSPACE_READ_BYTES_ENV})"
            ),
        }
    }
}

/// Parse a positive byte limit from an env override, failing closed to the
/// error string on invalid input (mirrors `ts_byte_limit_from_env`).
pub(crate) fn python_byte_limit_from_env(
    env_name: &str,
    default: u64,
    value: Result<String, std::env::VarError>,
) -> Result<u64, String> {
    match value {
        Ok(raw) => {
            let parsed = raw
                .trim()
                .parse::<u64>()
                .map_err(|err| format!("{env_name} must be a positive integer: {err}; set it to a whole number above zero, or unset it to use the default"))?;
            if parsed == 0 {
                return Err(format!(
                    "{env_name} must be a positive integer; set it to a whole number above zero, or unset it to use the default"
                ));
            }
            Ok(parsed)
        }
        Err(std::env::VarError::NotPresent) => Ok(default),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "{env_name} must be a positive integer; set it to a whole number above zero, or unset it to use the default"
        )),
    }
}

/// Per-file cap resolved from the environment, failing closed to the default
/// so a malformed operator override cannot abort analysis.
pub(crate) fn python_file_read_limit() -> u64 {
    python_byte_limit_from_env(
        PYTHON_MAX_FILE_READ_BYTES_ENV,
        DEFAULT_PYTHON_MAX_FILE_READ_BYTES,
        std::env::var(PYTHON_MAX_FILE_READ_BYTES_ENV),
    )
    .unwrap_or(DEFAULT_PYTHON_MAX_FILE_READ_BYTES)
}

/// Aggregate per-run byte budget resolved from the environment.
pub(crate) fn python_workspace_read_budget() -> u64 {
    python_byte_limit_from_env(
        PYTHON_MAX_WORKSPACE_READ_BYTES_ENV,
        DEFAULT_PYTHON_MAX_WORKSPACE_READ_BYTES,
        std::env::var(PYTHON_MAX_WORKSPACE_READ_BYTES_ENV),
    )
    .unwrap_or(DEFAULT_PYTHON_MAX_WORKSPACE_READ_BYTES)
}

/// Parse a positive workspace file-count limit, failing closed to the error
/// string on invalid input (mirrors `ts_workspace_file_limit_from_env`).
pub(crate) fn python_workspace_file_limit_from_env(
    value: Result<String, std::env::VarError>,
) -> Result<usize, String> {
    match value {
        Ok(raw) => {
            let parsed = raw.trim().parse::<usize>().map_err(|err| {
                format!("{PYTHON_MAX_WORKSPACE_FILES_ENV} must be a positive integer: {err}; set it to a whole number above zero, or unset it to use the default")
            })?;
            if parsed == 0 {
                return Err(format!(
                    "{PYTHON_MAX_WORKSPACE_FILES_ENV} must be a positive integer; set it to a whole number above zero, or unset it to use the default"
                ));
            }
            Ok(parsed)
        }
        Err(std::env::VarError::NotPresent) => Ok(DEFAULT_PYTHON_MAX_WORKSPACE_FILES),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "{PYTHON_MAX_WORKSPACE_FILES_ENV} must be a positive integer; set it to a whole number above zero, or unset it to use the default"
        )),
    }
}

/// Resolved discovered-file cap, failing closed to the default so a
/// malformed operator override cannot abort analysis.
pub(crate) fn python_workspace_file_limit() -> usize {
    python_workspace_file_limit_from_env(std::env::var(PYTHON_MAX_WORKSPACE_FILES_ENV))
        .unwrap_or(DEFAULT_PYTHON_MAX_WORKSPACE_FILES)
}

/// Streaming selection of discovered workspace files under the walk cap.
///
/// Files for which `is_changed` holds are always retained, even past the cap:
/// they are the subject of the diff, so the cap may only cut context around
/// them (#10461). The rest of the cap is filled with the earliest in path order
/// context paths. Paths are pushed as the walk finds them, in any order, and
/// only the changed paths plus at most `max_files` context paths are held at
/// once (#10594), so discovery memory follows the cap rather than the
/// workspace size. The retained set depends only on the discovered paths and
/// the diff, never on directory iteration order.
pub(crate) struct WorkspaceFileSelection<F> {
    max_files: usize,
    is_changed: F,
    changed: Vec<std::path::PathBuf>,
    /// Max-heap of the earliest context paths seen so far, never longer than
    /// `max_files`.
    context: std::collections::BinaryHeap<std::path::PathBuf>,
    discovered: usize,
    #[cfg(test)]
    peak_context: usize,
}

impl<F: Fn(&Path) -> bool> WorkspaceFileSelection<F> {
    pub(crate) fn new(max_files: usize, is_changed: F) -> Self {
        Self {
            max_files,
            is_changed,
            changed: Vec::new(),
            context: std::collections::BinaryHeap::new(),
            discovered: 0,
            #[cfg(test)]
            peak_context: 0,
        }
    }

    /// Admit one discovered path.
    pub(crate) fn push(&mut self, file: std::path::PathBuf) {
        self.discovered += 1;
        if (self.is_changed)(&file) {
            self.changed.push(file);
            return;
        }
        if self.context.len() < self.max_files {
            self.context.push(file);
        } else if self.context.peek().is_some_and(|latest| file < *latest) {
            self.context.pop();
            self.context.push(file);
        }
        #[cfg(test)]
        {
            self.peak_context = self.peak_context.max(self.context.len());
        }
    }

    /// The retained files in sorted order and the number of refused files.
    pub(crate) fn finish(self) -> (Vec<std::path::PathBuf>, usize) {
        let mut context = self.context.into_sorted_vec();
        context.truncate(self.max_files.saturating_sub(self.changed.len()));
        let mut kept = self.changed;
        kept.extend(context);
        kept.sort();
        let refused = self.discovered - kept.len();
        (kept, refused)
    }
}

/// Result of reading every retained workspace source under the caps.
pub(crate) struct CappedWorkspaceSources {
    /// Files read successfully, keyed by workspace-relative path. Serves as
    /// the diff-mode source cache: every consumer reads from here instead of
    /// re-reading the file.
    pub(crate) sources: HashMap<std::path::PathBuf, String>,
    /// Named read-limit entries: one per over-limit file, plus at most one
    /// workspace-budget entry (the first file that tripped it).
    pub(crate) limits: Vec<(std::path::PathBuf, CappedReadError)>,
    /// Plain IO failures (missing file, permission, non-regular file,
    /// decode error), one per failed file with the raw error message. These
    /// are NOT read-limit entries: the adapter counts them as skipped files
    /// and names the ones the diff touches.
    pub(crate) io_failures: Vec<(std::path::PathBuf, String)>,
}

/// Read every `files` entry under `root` with a per-file cap and an aggregate
/// byte budget.
///
/// - Files over the per-file cap each produce an `OverFileLimit` entry.
/// - Every file that does not fit the remaining aggregate budget produces
///   its own `OverWorkspaceBudget` entry, so the adapter's disclosure can
///   name each refused path instead of disclosing only the first. The
///   disclosure itself is sampled per run (#5022,
///   `read_limit_disclosure.rs`): a stable-sorted sample of refused paths
///   plus one summary entry carrying the true refused count, so a capped
///   monorepo cannot emit one limitation object per refused file.
/// - Plain IO failures are returned in `io_failures` for the adapter's
///   read-failure disclosure lane; they never produce a read-limit entry.
pub(crate) fn read_workspace_sources_capped(
    root: &Path,
    files: &[std::path::PathBuf],
    file_limit: u64,
    workspace_budget: u64,
) -> CappedWorkspaceSources {
    let mut sources = HashMap::new();
    let mut limits = Vec::new();
    let mut io_failures = Vec::new();
    let mut consumed = 0u64;
    for relative in files {
        let mut remaining = workspace_budget.saturating_sub(consumed);
        let outcome = match crate::analysis::committed_source::lookup(root, relative) {
            crate::analysis::committed_source::CommittedSourceRead::Worktree => {
                read_source_capped(&root.join(relative), file_limit, Some(&mut remaining))
            }
            // A committed-history diff reads HEAD content for a dirty tracked
            // file under the same caps; a path absent at HEAD is not source.
            crate::analysis::committed_source::CommittedSourceRead::Committed(bytes) => {
                committed_source_capped(&root.join(relative), bytes, file_limit, remaining)
            }
            crate::analysis::committed_source::CommittedSourceRead::AbsentAtHead => continue,
        };
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
                    limits.push((relative.clone(), err));
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

/// Where a diff-reach scan (#10785) reads one discovered file from. The
/// committed-source overlay is thread-local, so it is resolved on the calling
/// thread before the scan fans out.
pub(crate) enum ScanSource {
    /// Read the working tree under the per-file cap.
    Worktree(std::path::PathBuf),
    /// Committed bytes of a dirty tracked file, or the decode/size failure.
    Committed(Result<String, CappedReadError>),
    /// A dirty path with no content at `HEAD`: not source.
    Absent,
}

/// Resolve the scan source of one workspace-relative file.
pub(crate) fn scan_source_for(root: &Path, relative: &Path, file_limit: u64) -> ScanSource {
    match crate::analysis::committed_source::lookup(root, relative) {
        crate::analysis::committed_source::CommittedSourceRead::Worktree => {
            ScanSource::Worktree(root.join(relative))
        }
        crate::analysis::committed_source::CommittedSourceRead::Committed(bytes) => {
            ScanSource::Committed(committed_source_capped(
                &root.join(relative),
                bytes,
                file_limit,
                u64::MAX,
            ))
        }
        crate::analysis::committed_source::CommittedSourceRead::AbsentAtHead => ScanSource::Absent,
    }
}

impl ScanSource {
    /// The text a scan sees: the per-file cap applies, the aggregate budget
    /// does not, because a scan keeps no source once it has looked.
    pub(crate) fn read(&self, file_limit: u64) -> Option<Result<String, CappedReadError>> {
        match self {
            Self::Worktree(path) => Some(read_source_capped(path, file_limit, None)),
            Self::Committed(read) => Some(read.clone()),
            Self::Absent => None,
        }
    }
}

/// Apply the per-file cap, the remaining aggregate budget, and UTF-8
/// decoding to committed bytes, exactly as a working-tree read would.
fn committed_source_capped(
    path: &Path,
    bytes: Vec<u8>,
    file_limit: u64,
    remaining: u64,
) -> Result<String, CappedReadError> {
    let len = bytes.len() as u64;
    if len > file_limit {
        return Err(CappedReadError::OverFileLimit { limit: file_limit });
    }
    if len > remaining {
        return Err(CappedReadError::OverWorkspaceBudget { remaining });
    }
    String::from_utf8(bytes)
        .map_err(|err| CappedReadError::Io(format!("decode {}: {err}", path.display())))
}

/// Bounded read mirroring `edit_cage.rs` and the TypeScript adapter:
/// metadata check → regular-file open → `take(limit + 1)` → reject on excess.
///
/// The read itself is clamped to the remaining aggregate budget (plus one
/// sentinel byte): the metadata length was checked above, but the file can
/// grow between that check and this read, and an unclamped read would cache
/// an over-budget source as a success.
pub(super) fn source_matches_capped(path: &Path, expected: &str) -> bool {
    read_source_capped(path, expected.len() as u64, None).is_ok_and(|actual| actual == expected)
}

fn read_source_capped(
    path: &Path,
    file_limit: u64,
    budget: Option<&mut u64>,
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
    let over_budget = budget
        .as_ref()
        .is_some_and(|remaining| metadata.len() > **remaining);
    if over_budget {
        let remaining = budget.as_ref().map(|remaining| **remaining).unwrap_or(0);
        return Err(CappedReadError::OverWorkspaceBudget { remaining });
    }
    let file = open_source_read_no_follow(path)?;
    let mut bytes = Vec::new();
    let read_limit = budget
        .as_ref()
        .map_or(file_limit, |remaining| file_limit.min(**remaining));
    file.take(read_limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| CappedReadError::Io(format!("read {}: {err}", path.display())))?;
    if bytes.len() as u64 > file_limit {
        return Err(CappedReadError::OverFileLimit { limit: file_limit });
    }
    let over_budget_read = budget
        .as_ref()
        .is_some_and(|remaining| bytes.len() as u64 > **remaining);
    if over_budget_read {
        let remaining = budget.as_ref().map(|remaining| **remaining).unwrap_or(0);
        return Err(CappedReadError::OverWorkspaceBudget { remaining });
    }
    let text = String::from_utf8(bytes)
        .map_err(|err| CappedReadError::Io(format!("decode {}: {err}", path.display())))?;
    if let Some(remaining) = budget {
        *remaining = remaining.saturating_sub(text.len() as u64);
    }
    Ok(text)
}

/// Open `path` for reading without following symlinks and without blocking on
/// FIFO or device replacement. Mirrors the edit_cage capture-open flags so a
/// concurrent path swap cannot stall the bounded read between the metadata
/// check above and the open; the opened handle is then validated as a regular
/// file before any bytes are read.
fn open_source_read_no_follow(path: &Path) -> Result<std::fs::File, CappedReadError> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Linux O_NOFOLLOW | O_NONBLOCK. The nonblocking bit prevents FIFO or
        // device replacement from stalling the read before handle validation.
        options.custom_flags(0x0002_0000 | 0x0000_0800);
    }
    #[cfg(all(
        target_os = "macos",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        // Darwin O_NOFOLLOW | O_NONBLOCK.
        options.custom_flags(0x0000_0100 | 0x0000_0004);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        // FILE_FLAG_OPEN_REPARSE_POINT keeps a replacement symlink from being
        // followed; the opened-handle metadata check below rejects it.
        options.custom_flags(0x0020_0000);
    }
    #[cfg(not(any(
        windows,
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        ),
        all(
            target_os = "macos",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    )))]
    {
        return Err(CappedReadError::Io(format!(
            "safe no-follow open unsupported on this target: {}",
            path.display()
        )));
    }
    let file = options
        .open(path)
        .map_err(|err| CappedReadError::Io(format!("open {}: {err}", path.display())))?;
    // `File::metadata` describes the opened handle itself (fstat on the fd),
    // so a reparse point or swapped non-regular file is rejected here even
    // when the pre-open path check raced.
    let opened = file
        .metadata()
        .map_err(|err| CappedReadError::Io(format!("inspect opened {}: {err}", path.display())))?;
    if !opened.file_type().is_file() {
        return Err(CappedReadError::Io(format!(
            "not a regular file after open: {}",
            path.display()
        )));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir().join(format!(
                "ripr-py-bounded-read-{label}-{}-{stamp}",
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
    fn no_follow_open_rejects_symlink_and_accepts_regular_file() -> Result<(), String> {
        let dir = TempDir::new("no-follow-open");
        dir.write("real.py", b"def run():\n    return 1\n");
        // A regular file opens cleanly through the guarded helper.
        let regular = open_source_read_no_follow(&dir.0.join("real.py"));
        assert!(
            regular.is_ok(),
            "regular file must open, got {:?}",
            regular.err()
        );
        // A symlink must not be followed even when the pre-open path check
        // raced (review: nonblocking, no-follow open flags). The symlink
        // scenario is unix-only, so its bindings live inside the cfg block
        // and do not trip unused-variable clippy on Windows hosts.
        #[cfg(unix)]
        {
            let link_target = dir.0.join("real.py");
            let link = dir.0.join("linked.py");
            let created = std::os::unix::fs::symlink(&link_target, &link);
            assert!(created.is_ok(), "create symlink: {:?}", created.err());
            let outcome = open_source_read_no_follow(&link);
            let Err(err) = &outcome else {
                return Err(format!("symlink open must fail, got {outcome:?}"));
            };
            assert!(
                err.reason().contains("not a regular file") || err.reason().contains("open"),
                "symlink refusal must name the cause: {}",
                err.reason()
            );
        }
        Ok(())
    }

    #[test]
    fn over_limit_file_is_rejected_with_named_reason() -> Result<(), String> {
        let dir = TempDir::new("over-limit");
        dir.write("big.py", &[b'a'; 100]);
        let outcome = read_source_capped(&dir.0.join("big.py"), 50, None);
        let Err(err) = &outcome else {
            return Err(format!("over-limit read must fail, got {outcome:?}"));
        };
        assert_eq!(err, &CappedReadError::OverFileLimit { limit: 50 });
        assert!(err.reason().contains("file_read_capped"));
        assert!(err.reason().contains("50"));
        assert!(err.reason().contains(PYTHON_MAX_FILE_READ_BYTES_ENV));
        Ok(())
    }

    #[test]
    fn under_limit_file_reads_and_reports_budget() {
        let dir = TempDir::new("under-limit");
        dir.write("ok.py", b"def run():\n    return 1\n");
        let mut remaining = 100u64;
        let outcome = read_source_capped(&dir.0.join("ok.py"), 1024, Some(&mut remaining));
        assert!(outcome.is_ok(), "under-limit read must succeed");
        let text = outcome.unwrap_or_default();
        assert!(text.contains("return 1"));
        assert_eq!(remaining, 100 - text.len() as u64);
    }

    #[test]
    fn non_utf8_file_is_an_io_failure_not_a_size_limit() -> Result<(), String> {
        let dir = TempDir::new("decode-failure");
        dir.write("bad.py", &[0xff, 0xfe, 0xfd]);
        let outcome = read_source_capped(&dir.0.join("bad.py"), 1024, None);
        let Err(err) = &outcome else {
            return Err(format!("decode failure must fail, got {outcome:?}"));
        };
        assert!(matches!(err, CappedReadError::Io(_)), "got {err:?}");
        assert!(err.reason().contains("decode"));
        Ok(())
    }

    #[test]
    fn workspace_budget_exhaustion_is_distinguishable() -> Result<(), String> {
        let dir = TempDir::new("budget");
        dir.write("a.py", &[b'a'; 40]);
        dir.write("b.py", &[b'b'; 40]);
        let mut remaining = 50u64;
        let first_outcome = read_source_capped(&dir.0.join("a.py"), 1024, Some(&mut remaining));
        assert!(first_outcome.is_ok(), "first read must succeed");
        let first = first_outcome.unwrap_or_default();
        assert_eq!(remaining, 50 - first.len() as u64);
        let outcome = read_source_capped(&dir.0.join("b.py"), 1024, Some(&mut remaining));
        let Err(err) = &outcome else {
            return Err(format!("budget-exhausting read must fail, got {outcome:?}"));
        };
        assert!(matches!(err, CappedReadError::OverWorkspaceBudget { .. }));
        assert!(err.reason().contains("workspace_read_budget_exhausted"));
        Ok(())
    }

    #[test]
    fn capped_workspace_read_collects_limit_once_per_trigger() {
        let dir = TempDir::new("workspace");
        dir.write("small.py", b"def a():\n    return 1\n");
        dir.write("big.py", &[b'x'; 200]);
        dir.write("also_big.py", &[b'y'; 200]);
        let files = vec![
            PathBuf::from("small.py"),
            PathBuf::from("big.py"),
            PathBuf::from("also_big.py"),
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
        assert!(outcome.io_failures.is_empty());
    }

    #[test]
    fn capped_workspace_read_reports_every_budget_refusal() {
        let dir = TempDir::new("workspace-budget");
        dir.write("a.py", &[b'a'; 60]);
        dir.write("b.py", &[b'b'; 60]);
        dir.write("c.py", &[b'c'; 60]);
        let files = vec![
            PathBuf::from("a.py"),
            PathBuf::from("b.py"),
            PathBuf::from("c.py"),
        ];
        let outcome = read_workspace_sources_capped(&dir.0, &files, 1024, 100);
        assert_eq!(outcome.sources.len(), 1);
        assert_eq!(
            outcome.limits.len(),
            2,
            "every refused file is named, not just the first"
        );
        assert_eq!(outcome.limits[0].0, PathBuf::from("b.py"));
        assert_eq!(outcome.limits[1].0, PathBuf::from("c.py"));
        assert!(
            outcome
                .limits
                .iter()
                .all(|(_, err)| matches!(err, CappedReadError::OverWorkspaceBudget { .. })),
            "expected budget errors, got {:?}",
            outcome.limits
        );
    }

    #[test]
    fn capped_workspace_read_collects_io_failures_separately() {
        let dir = TempDir::new("workspace-io");
        dir.write("ok.py", b"def a():\n    return 1\n");
        dir.write("bad.py", &[0xff, 0xfe]);
        let files = vec![PathBuf::from("ok.py"), PathBuf::from("bad.py")];
        let outcome = read_workspace_sources_capped(&dir.0, &files, 1024, 4096);
        assert_eq!(outcome.sources.len(), 1);
        assert!(outcome.limits.is_empty(), "IO failures are not size limits");
        assert_eq!(outcome.io_failures.len(), 1);
        assert_eq!(outcome.io_failures[0].0, PathBuf::from("bad.py"));
    }

    #[test]
    fn byte_limit_env_parsing_matches_repo_conventions() {
        assert_eq!(
            python_byte_limit_from_env("RIPR_TEST_X", 16, Err(std::env::VarError::NotPresent)),
            Ok(16)
        );
        assert_eq!(
            python_byte_limit_from_env("RIPR_TEST_X", 16, Ok(" 64 ".to_string())),
            Ok(64)
        );
        assert!(
            python_byte_limit_from_env("RIPR_TEST_X", 16, Ok("0".to_string())).is_err(),
            "zero limit must be rejected"
        );
        assert!(
            python_byte_limit_from_env("RIPR_TEST_X", 16, Ok("nope".to_string())).is_err(),
            "non-numeric limit must be rejected"
        );
    }

    #[test]
    fn workspace_file_limit_env_parsing_matches_repo_conventions() {
        assert_eq!(
            python_workspace_file_limit_from_env(Err(std::env::VarError::NotPresent)),
            Ok(DEFAULT_PYTHON_MAX_WORKSPACE_FILES),
            "default admits a Django-sized workspace (#10441)"
        );
        assert_eq!(
            python_workspace_file_limit_from_env(Ok(" 1200 ".to_string())),
            Ok(1200)
        );
        assert!(
            python_workspace_file_limit_from_env(Ok("0".to_string())).is_err(),
            "zero limit must be rejected"
        );
        assert!(
            python_workspace_file_limit_from_env(Ok("many".to_string())).is_err(),
            "non-numeric limit must be rejected"
        );
    }

    fn select(
        files: &[PathBuf],
        max_files: usize,
        is_changed: impl Fn(&Path) -> bool,
    ) -> (Vec<PathBuf>, usize) {
        let mut selection = WorkspaceFileSelection::new(max_files, is_changed);
        for file in files {
            selection.push(file.clone());
        }
        selection.finish()
    }

    #[test]
    fn workspace_file_selection_keeps_sorted_prefix_and_counts_refusals() {
        let files: Vec<PathBuf> = (0..5)
            .map(|index| PathBuf::from(format!("f{index}.py")))
            .collect();
        let unchanged = |_: &Path| false;
        let (kept, refused) = select(&files, 3, unchanged);
        assert_eq!(kept, files[..3], "sorted prefix is retained");
        assert_eq!(refused, 2, "refused count is disclosed");

        let (kept, refused) = select(&files, 5, unchanged);
        assert_eq!(kept, files, "at-cap list is untouched");
        assert_eq!(refused, 0);

        let (kept, refused) = select(&[], 3, unchanged);
        assert!(kept.is_empty());
        assert_eq!(refused, 0);
    }

    #[test]
    fn workspace_file_selection_always_keeps_changed_files() {
        let files: Vec<PathBuf> = (0..5)
            .map(|index| PathBuf::from(format!("f{index}.py")))
            .collect();
        let last = Path::new("f4.py");
        let (kept, refused) = select(&files, 3, |file| file == last);
        assert_eq!(
            kept,
            vec![files[0].clone(), files[1].clone(), files[4].clone()],
            "a late-sorting changed file displaces context, not itself"
        );
        assert_eq!(refused, 2, "refused count is disclosed");

        let (kept, refused) = select(&files, 1, |file| file == Path::new("f3.py") || file == last);
        assert_eq!(
            kept,
            vec![files[3].clone(), files[4].clone()],
            "changed files are kept even when they alone exceed the cap"
        );
        assert_eq!(refused, 3);
    }

    #[test]
    fn workspace_file_selection_is_order_independent_and_holds_at_most_the_cap() {
        // #10594: discovery order is directory iteration order, so the
        // selection must not depend on it, and must not buffer every path.
        let sorted: Vec<PathBuf> = (0..2000)
            .map(|index| PathBuf::from(format!("pkg{:02}/m{index:04}.py", index % 37)))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        // The first and the last path in sorted order are the changed ones,
        // so the late-sorting one must survive a cap it sorts far past.
        let first = sorted[0].clone();
        let last = sorted[sorted.len() - 1].clone();
        let changed = |file: &Path| file == first || file == last;
        let expected = {
            let mut kept: Vec<PathBuf> = sorted
                .iter()
                .filter(|file| !changed(file))
                .take(3)
                .cloned()
                .collect();
            kept.extend(sorted.iter().filter(|file| changed(file)).cloned());
            kept.sort();
            kept
        };
        // A deterministic shuffle: stride through the list by a step coprime
        // to its length.
        let shuffled: Vec<PathBuf> = (0..sorted.len())
            .map(|index| sorted[(index * 7919) % sorted.len()].clone())
            .collect();
        for order in [&sorted, &shuffled] {
            let mut selection = WorkspaceFileSelection::new(5, changed);
            for file in order.iter() {
                selection.push(file.clone());
            }
            assert!(
                selection.peak_context <= 5,
                "context buffer grew past the cap: {}",
                selection.peak_context
            );
            let (kept, refused) = selection.finish();
            assert_eq!(kept, expected, "same subset whatever the discovery order");
            assert_eq!(refused, sorted.len() - 5);
        }

        // More changed files than the cap: every changed file, no context.
        let (kept, refused) = select(&shuffled, 1, |file| file.starts_with("pkg01"));
        let changed_count = sorted
            .iter()
            .filter(|file| file.starts_with("pkg01"))
            .count();
        assert_eq!(kept.len(), changed_count);
        assert!(kept.iter().all(|file| file.starts_with("pkg01")));
        assert!(
            kept.windows(2).all(|pair| pair[0] < pair[1]),
            "kept stays sorted"
        );
        assert_eq!(refused, sorted.len() - changed_count);
    }

    #[test]
    fn committed_history_overlay_supplies_head_bytes_under_the_same_caps() {
        use crate::analysis::committed_source::{CommittedSourceOverlay, with_overlay};
        let dir = TempDir::new("committed-overlay");
        dir.write("dirty.py", b"worktree bytes\n");
        dir.write("staged.py", b"staged only\n");
        dir.write("clean.py", b"clean bytes\n");
        dir.write("grown.py", b"small\n");
        let overlay = CommittedSourceOverlay::from_entries(
            &dir.0,
            [
                ("dirty.py", Some(&b"committed bytes\n"[..])),
                ("staged.py", None),
                ("grown.py", Some(&[b'g'; 200][..])),
            ],
        );
        let files = vec![
            PathBuf::from("dirty.py"),
            PathBuf::from("staged.py"),
            PathBuf::from("clean.py"),
            PathBuf::from("grown.py"),
        ];
        let outcome = with_overlay(Some(std::sync::Arc::new(overlay)), || {
            read_workspace_sources_capped(&dir.0, &files, 100, 1024)
        });
        assert_eq!(
            outcome
                .sources
                .get(&PathBuf::from("dirty.py"))
                .map(String::as_str),
            Some("committed bytes\n"),
            "a dirty python file reads its committed bytes"
        );
        assert!(
            !outcome.sources.contains_key(&PathBuf::from("staged.py")),
            "a path absent at HEAD is not committed source"
        );
        assert_eq!(
            outcome
                .sources
                .get(&PathBuf::from("clean.py"))
                .map(String::as_str),
            Some("clean bytes\n")
        );
        assert!(
            outcome
                .limits
                .iter()
                .any(|(path, err)| path == &PathBuf::from("grown.py")
                    && matches!(err, CappedReadError::OverFileLimit { .. })),
            "committed bytes obey the per-file cap: {:?}",
            outcome.limits
        );
        assert!(outcome.io_failures.is_empty(), "{:?}", outcome.io_failures);
    }
}
