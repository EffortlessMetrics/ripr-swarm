//! Watched-file events for sources the committed snapshot already analyzed
//! (#14099).
//!
//! A file that is not open has no document-sync path: when it is deleted,
//! renamed or rewritten outside the editor (a terminal, a formatter, a coding
//! agent) the only signal is `workspace/didChangeWatchedFiles`. This module
//! decides whether such an event moved a committed input, so the caller can
//! schedule one diagnostics refresh instead of leaving rows on a file that is
//! gone or on evidence that predates the rewrite.
//!
//! It makes no analysis claim. The consumed-source carrier stays the owner of
//! "do these bytes still match" (`ConsumedRustSources::first_drift`); this
//! file only routes events to it.

use super::uri::{
    file_uri_relative_to_root, file_uris_match_on, path_from_file_uri, paths_equal_or_below_on,
};
use crate::analysis::consumed_source::ConsumedRustSources;
use crate::output::path::{host_paths_case_insensitive, path_identity_key_on};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tower_lsp_server::ls_types::{FileChangeType, FileEvent, LSPAny, Uri};

/// `WatchKind.Change`: a rewrite of a source file. Creations are never drift,
/// so the client is not asked to report them.
const WATCH_CHANGE: u32 = 2;
/// `WatchKind.Delete`.
const WATCH_DELETE: u32 = 4;

/// Root-anchored watchers for what a committed snapshot can refer to.
/// Registered with the root diagnostics inputs so they follow root
/// transitions. `absolute_root` is the glob-safe display form the caller
/// already validated; it is used only when the client cannot anchor a
/// `RelativePattern`.
///
/// Two registrations, so a source file never reaches the server twice for one
/// deletion: source files are watched for changes, and every path under the
/// root is watched for deletion. The deletion glob is not limited to source
/// extensions because a folder deleted as one entry (`rm -rf src/nested`, a
/// branch switch) is reported as the folder, not as the files it held, and a
/// glob that matches only `*.rs` never sees it (#14099).
pub(super) fn committed_source_watchers(
    root_uri: Option<&Uri>,
    absolute_root: &str,
) -> Vec<LSPAny> {
    let extensions = ["rs", "py", "pm", "pl", "t"]
        .iter()
        .chain(crate::analysis::TYPESCRIPT_SOURCE_EXTENSIONS)
        .chain(crate::analysis::JAVASCRIPT_SOURCE_EXTENSIONS)
        .copied()
        .collect::<Vec<_>>()
        .join(",");
    let pattern = format!("**/*.{{{extensions}}}");
    let glob = |pattern: &str| match root_uri {
        Some(base) => serde_json::json!({"baseUri": base.as_str(), "pattern": pattern}),
        None => serde_json::json!(format!("{absolute_root}/{pattern}")),
    };
    vec![
        serde_json::json!({"globPattern": glob(&pattern), "kind": WATCH_CHANGE}),
        serde_json::json!({"globPattern": glob("**/*"), "kind": WATCH_DELETE}),
    ]
}

/// The committed inputs one batch of watched-file events is checked against.
pub(super) struct CommittedSources<'a> {
    pub(super) root: &'a Path,
    pub(super) consumed: &'a ConsumedRustSources,
    /// Every URI the committed snapshot carries diagnostics for.
    pub(super) diagnostic_uris: Vec<&'a Uri>,
    /// Root-relative files that committed findings list as related tests.
    pub(super) related_test_files: BTreeSet<PathBuf>,
}

/// The root-relative files that committed findings list as related tests. A
/// file outside the root or an unnormalized path is dropped: it cannot be
/// matched against a watched-file event under the root.
pub(super) fn related_test_files<'a>(
    root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
) -> BTreeSet<PathBuf> {
    related_test_files_on(root, files, host_paths_case_insensitive())
}

/// [`related_test_files`] with host case-insensitivity explicit, mirroring
/// `file_uris_match_on`: on a case-insensitive host a file recorded under a
/// differently cased root still strips to its root-relative form.
pub(super) fn related_test_files_on<'a>(
    root: &Path,
    files: impl IntoIterator<Item = &'a Path>,
    case_insensitive: bool,
) -> BTreeSet<PathBuf> {
    files
        .into_iter()
        .filter_map(|file| {
            let relative = strip_root_prefix_on(root, file, case_insensitive);
            relative
                .components()
                .all(|part| matches!(part, std::path::Component::Normal(_)))
                .then(|| relative.to_path_buf())
        })
        .collect()
}

fn strip_root_prefix_on(root: &Path, file: &Path, case_insensitive: bool) -> PathBuf {
    if let Ok(relative) = file.strip_prefix(root) {
        return relative.to_path_buf();
    }
    if case_insensitive && paths_equal_or_below_on(root, file, true) {
        file.components().skip(root.components().count()).collect()
    } else {
        file.to_path_buf()
    }
}

fn paths_match_on(left: &Path, right: &Path, case_insensitive: bool) -> bool {
    // The identity key compares raw text, so first restore the component
    // normalization the former PathBuf equality applied: a recorded
    // `tests/./a.rs` (or a repeated separator) names the same file as
    // `tests/a.rs`.
    let normalized = |path: &Path| -> PathBuf {
        path.components()
            .filter(|part| !matches!(part, std::path::Component::CurDir))
            .collect()
    };
    path_identity_key_on(&normalized(left), cfg!(windows), case_insensitive)
        == path_identity_key_on(&normalized(right), cfg!(windows), case_insensitive)
}

/// Whether any event names a committed source whose saved bytes moved.
///
/// - `Created` is never drift: no committed evidence refers to a new file.
/// - An open document is skipped: document sync owns it (#1970, #11153), and
///   the watcher echo of a save the editor already reported must not analyze
///   the same bytes twice.
/// - A path with a captured commitment drifts only when its current bytes
///   hash differently or cannot be read, so a `touch` schedules nothing.
/// - A path without a commitment (a non-Rust source) drifts when it carries
///   committed diagnostics: `Changed` has nothing to compare against, and
///   `Deleted` must be confirmed by the file actually being gone.
/// - A deleted directory drifts when it is gone and covered a committed
///   source or a diagnostics URI. The registered deletion glob covers every
///   path under the root, so a folder deleted as one entry reaches here.
/// - A test file that a committed finding lists as a related test, but whose
///   bytes the snapshot never captured (a test the analysis matched without
///   loading it as a consumed source), has nothing to compare against: a
///   change drifts, and a deletion drifts once the file is confirmed gone.
///   This is the repair loop where an agent rewrites a test the developer
///   does not have open.
/// - A committed source replaced by a link that resolves outside the root
///   drifts without its new target being read.
pub(super) fn events_drift_committed_sources(
    committed: &CommittedSources<'_>,
    changes: &[FileEvent],
    is_open: &dyn Fn(&Uri) -> bool,
) -> bool {
    events_drift_committed_sources_on(committed, changes, is_open, host_paths_case_insensitive())
}

/// [`events_drift_committed_sources`] with host case-insensitivity explicit,
/// mirroring `file_uris_match_on`.
pub(super) fn events_drift_committed_sources_on(
    committed: &CommittedSources<'_>,
    changes: &[FileEvent],
    is_open: &dyn Fn(&Uri) -> bool,
    case_insensitive: bool,
) -> bool {
    changes
        .iter()
        .any(|event| event_drifts_committed_source(committed, event, is_open, case_insensitive))
}

fn event_drifts_committed_source(
    committed: &CommittedSources<'_>,
    event: &FileEvent,
    is_open: &dyn Fn(&Uri) -> bool,
    case_insensitive: bool,
) -> bool {
    let deleted = event.typ == FileChangeType::DELETED;
    if !deleted && event.typ != FileChangeType::CHANGED {
        return false;
    }
    if is_open(&event.uri) {
        return false;
    }
    let Some(path) = path_from_file_uri(&event.uri) else {
        return false;
    };
    let Some(relative) = file_uri_relative_to_root(committed.root, &event.uri) else {
        // The entry no longer resolves under the root. If its lexical place
        // still names a committed source, that source was replaced by a link
        // pointing outside: drift, without reading the new target.
        return paths_equal_or_below_on(committed.root, &path, case_insensitive)
            && committed
                .consumed
                .digest(&strip_root_prefix_on(
                    committed.root,
                    &path,
                    case_insensitive,
                ))
                .is_some();
    };
    if committed.consumed.digest(&relative).is_some() {
        return committed
            .consumed
            .first_drift([relative.as_path()], |relative| {
                std::fs::read(committed.root.join(relative)).ok()
            })
            .is_some();
    }
    let carries_diagnostics = committed
        .diagnostic_uris
        .iter()
        .any(|uri| file_uris_match_on(uri, &event.uri, case_insensitive));
    let is_related_test = committed
        .related_test_files
        .iter()
        .any(|file| paths_match_on(file, &relative, case_insensitive));
    if !deleted {
        return carries_diagnostics || is_related_test;
    }
    // The delete watcher covers every path under the root, so most events
    // (build output, editor scratch files) name nothing committed: answer that
    // from memory first and touch the filesystem only for a covered path.
    let covers_committed_input = carries_diagnostics
        || is_related_test
        || committed
            .related_test_files
            .iter()
            .any(|file| paths_equal_or_below_on(&relative, file, case_insensitive))
        || committed.consumed.any_captured_below(&relative)
        || committed.diagnostic_uris.iter().any(|uri| {
            path_from_file_uri(uri).is_some_and(|stored| {
                !paths_match_on(&stored, &path, case_insensitive)
                    && paths_equal_or_below_on(&path, &stored, case_insensitive)
            })
        });
    // `symlink_metadata` so a dangling link left behind still reads as present
    // (the event named the entry, not its target).
    covers_committed_input && std::fs::symlink_metadata(&path).is_err()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::uri::{file_uri_for_path, file_uris_match};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

    struct TempRoot(PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            let _removed = std::fs::remove_dir_all(&self.0);
        }
    }

    fn temp_root(name: &str) -> Result<TempRoot, String> {
        let path = std::env::temp_dir().join(format!(
            "ripr-watched-drift-{name}-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("src")).map_err(|err| format!("create root: {err}"))?;
        // Resolve the temp alias once so root-relative keys are stable.
        let path = std::fs::canonicalize(&path).map_err(|err| format!("canonicalize: {err}"))?;
        // Windows canonicalize returns a verbatim `\\?\` spelling, which the
        // URI encoder refuses; keep the resolved path in its plain spelling.
        let path = match path.to_string_lossy().strip_prefix(r"\\?\") {
            Some(plain) => PathBuf::from(plain),
            None => path,
        };
        Ok(TempRoot(path))
    }

    fn write(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::write(root.join(relative), bytes).map_err(|err| format!("write {relative}: {err}"))
    }

    fn event(root: &Path, relative: &str, typ: FileChangeType) -> Result<FileEvent, String> {
        Ok(FileEvent {
            uri: file_uri_for_path(&root.join(relative))?,
            typ,
        })
    }

    fn consumed(entries: &[(&str, &[u8])]) -> ConsumedRustSources {
        let mut sources = ConsumedRustSources::default();
        for (relative, bytes) in entries {
            sources.record(Path::new(relative), Some(bytes));
        }
        sources
    }

    fn drifts(
        root: &Path,
        consumed: &ConsumedRustSources,
        diagnostic_uris: &[Uri],
        changes: &[FileEvent],
        open: &[Uri],
    ) -> bool {
        drifts_with_tests(root, consumed, diagnostic_uris, &[], changes, open)
    }

    fn drifts_with_tests(
        root: &Path,
        consumed: &ConsumedRustSources,
        diagnostic_uris: &[Uri],
        related_tests: &[&str],
        changes: &[FileEvent],
        open: &[Uri],
    ) -> bool {
        let committed = CommittedSources {
            root,
            consumed,
            diagnostic_uris: diagnostic_uris.iter().collect(),
            related_test_files: related_tests.iter().map(PathBuf::from).collect(),
        };
        events_drift_committed_sources(&committed, changes, &|uri| {
            open.iter().any(|open| file_uris_match(open, uri))
        })
    }

    fn drifts_with_tests_on(
        root: &Path,
        consumed: &ConsumedRustSources,
        related_test_files: BTreeSet<PathBuf>,
        changes: &[FileEvent],
        case_insensitive: bool,
    ) -> bool {
        let committed = CommittedSources {
            root,
            consumed,
            diagnostic_uris: Vec::new(),
            related_test_files,
        };
        events_drift_committed_sources_on(&committed, changes, &|_| false, case_insensitive)
    }

    #[test]
    fn rewritten_committed_source_drifts_and_an_unchanged_one_does_not() -> Result<(), String> {
        let root = temp_root("rewrite")?;
        let analyzed = b"pub fn f() -> bool { true }\n";
        write(&root.0, "src/lib.rs", analyzed)?;
        let sources = consumed(&[("src/lib.rs", analyzed)]);
        let changed = [event(&root.0, "src/lib.rs", FileChangeType::CHANGED)?];

        // A `touch` or a save echo: same bytes, nothing to refresh.
        assert!(!drifts(&root.0, &sources, &[], &changed, &[]));

        write(&root.0, "src/lib.rs", b"pub fn f() -> bool { false }\n")?;
        assert!(drifts(&root.0, &sources, &[], &changed, &[]));
        Ok(())
    }

    #[test]
    fn deleted_committed_source_drifts() -> Result<(), String> {
        let root = temp_root("delete")?;
        let analyzed = b"pub fn f() {}\n";
        let sources = consumed(&[("src/lib.rs", analyzed)]);
        let deleted = [event(&root.0, "src/lib.rs", FileChangeType::DELETED)?];

        // The file was never written: the committed bytes are unreadable.
        assert!(drifts(&root.0, &sources, &[], &deleted, &[]));

        // A delete event for a file that is back with the analyzed bytes
        // (delete-and-recreate by an atomic save) is not drift.
        write(&root.0, "src/lib.rs", analyzed)?;
        assert!(!drifts(&root.0, &sources, &[], &deleted, &[]));
        Ok(())
    }

    #[test]
    fn open_documents_and_created_files_are_left_to_their_owners() -> Result<(), String> {
        let root = temp_root("owners")?;
        write(&root.0, "src/lib.rs", b"rewritten\n")?;
        let sources = consumed(&[("src/lib.rs", b"analyzed\n")]);
        let changed = event(&root.0, "src/lib.rs", FileChangeType::CHANGED)?;

        // Control: the same event drifts while the document is closed.
        assert!(drifts(
            &root.0,
            &sources,
            &[],
            std::slice::from_ref(&changed),
            &[]
        ));
        // Open: document sync owns the buffer and its saves.
        assert!(!drifts(
            &root.0,
            &sources,
            &[],
            std::slice::from_ref(&changed),
            std::slice::from_ref(&changed.uri),
        ));
        // Created: no committed evidence refers to it.
        let created = [event(&root.0, "src/lib.rs", FileChangeType::CREATED)?];
        assert!(!drifts(&root.0, &sources, &[], &created, &[]));
        Ok(())
    }

    #[test]
    fn uncommitted_paths_drift_only_when_they_carry_diagnostics() -> Result<(), String> {
        let root = temp_root("uncommitted")?;
        write(&root.0, "src/app.py", b"x = 1\n")?;
        write(&root.0, "src/other.py", b"y = 1\n")?;
        let sources = ConsumedRustSources::default();
        let carried = [file_uri_for_path(&root.0.join("src/app.py"))?];

        let changed_carrier = [event(&root.0, "src/app.py", FileChangeType::CHANGED)?];
        let changed_other = [event(&root.0, "src/other.py", FileChangeType::CHANGED)?];
        assert!(drifts(&root.0, &sources, &carried, &changed_carrier, &[]));
        assert!(!drifts(&root.0, &sources, &carried, &changed_other, &[]));

        // A delete event is confirmed against the filesystem.
        let deleted_carrier = [event(&root.0, "src/app.py", FileChangeType::DELETED)?];
        assert!(!drifts(&root.0, &sources, &carried, &deleted_carrier, &[]));
        std::fs::remove_file(root.0.join("src/app.py")).map_err(|err| format!("remove: {err}"))?;
        assert!(drifts(&root.0, &sources, &carried, &deleted_carrier, &[]));
        Ok(())
    }

    #[test]
    fn deleted_directory_drifts_only_when_it_covered_a_committed_source() -> Result<(), String> {
        let root = temp_root("directory")?;
        let sources = consumed(&[("src/nested/a.rs", b"pub fn a() {}\n")]);
        let carried = [file_uri_for_path(&root.0.join("pkg/app.py"))?];

        // Gone directories covering a committed source or a diagnostics URI.
        let nested = [event(&root.0, "src/nested", FileChangeType::DELETED)?];
        let pkg = [event(&root.0, "pkg", FileChangeType::DELETED)?];
        assert!(drifts(&root.0, &sources, &[], &nested, &[]));
        assert!(drifts(&root.0, &sources, &carried, &pkg, &[]));

        // A gone directory covering neither, and a sibling whose name merely
        // shares the prefix (`src/nest` vs `src/nested`).
        let unrelated = [event(&root.0, "docs", FileChangeType::DELETED)?];
        let prefix = [event(&root.0, "src/nest", FileChangeType::DELETED)?];
        assert!(!drifts(&root.0, &sources, &carried, &unrelated, &[]));
        assert!(!drifts(&root.0, &sources, &carried, &prefix, &[]));

        // A directory that still exists is not a deletion.
        std::fs::create_dir_all(root.0.join("src/nested"))
            .map_err(|err| format!("mkdir: {err}"))?;
        std::fs::create_dir_all(root.0.join("pkg")).map_err(|err| format!("mkdir: {err}"))?;
        assert!(!drifts(&root.0, &sources, &carried, &pkg, &[]));
        Ok(())
    }

    #[test]
    fn related_test_without_a_commitment_drifts_on_change_and_confirmed_delete()
    -> Result<(), String> {
        let root = temp_root("related-test")?;
        std::fs::create_dir_all(root.0.join("tests")).map_err(|err| format!("mkdir: {err}"))?;
        write(&root.0, "tests/end_to_end.rs", b"#[test] fn t() {}\n")?;
        write(&root.0, "tests/other.rs", b"#[test] fn o() {}\n")?;
        let sources = consumed(&[("src/lib.rs", b"pub fn f() {}\n")]);
        let related = ["tests/end_to_end.rs"];
        let changed = [event(
            &root.0,
            "tests/end_to_end.rs",
            FileChangeType::CHANGED,
        )?];
        let unrelated = [event(&root.0, "tests/other.rs", FileChangeType::CHANGED)?];

        // The snapshot captured no bytes for the test, so a change cannot be
        // compared and refreshes; the same event for a test no committed
        // finding lists does not, and neither does a creation.
        assert!(drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &related,
            &changed,
            &[]
        ));
        assert!(!drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &related,
            &unrelated,
            &[]
        ));
        // A related test recorded with an internal dot segment or a repeated
        // separator names the same file as the watch event's spelling.
        for dotted in ["tests/./end_to_end.rs", "tests//end_to_end.rs"] {
            assert!(
                drifts_with_tests(&root.0, &sources, &[], &[dotted], &changed, &[]),
                "{dotted}"
            );
        }
        assert!(!drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &[],
            &changed,
            &[]
        ));
        let created = [event(
            &root.0,
            "tests/end_to_end.rs",
            FileChangeType::CREATED,
        )?];
        assert!(!drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &related,
            &created,
            &[]
        ));
        // An open document stays with document sync.
        assert!(!drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &related,
            &changed,
            std::slice::from_ref(&changed[0].uri),
        ));

        // Differently cased root and test path (#14729): the analyzer recorded
        // the test under an upper-cased root, the editor reports the real
        // spelling. On a case-insensitive host they are the same file.
        let cased_root = PathBuf::from(root.0.to_string_lossy().to_uppercase());
        let cased_files = related_test_files_on(
            &cased_root,
            [cased_root.join("TESTS/END_TO_END.RS").as_path()],
            true,
        );
        assert!(drifts_with_tests_on(
            &root.0,
            &sources,
            cased_files.clone(),
            &changed,
            true
        ));
        // Case-sensitive control: the same differently cased path does not drift.
        assert!(!drifts_with_tests_on(
            &root.0,
            &sources,
            cased_files.clone(),
            &changed,
            false
        ));

        // A delete event is confirmed against the filesystem, and a deleted
        // folder covers the related tests below it.
        let deleted = [event(
            &root.0,
            "tests/end_to_end.rs",
            FileChangeType::DELETED,
        )?];
        let folder = [event(&root.0, "tests", FileChangeType::DELETED)?];
        assert!(!drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &related,
            &deleted,
            &[]
        ));
        std::fs::remove_dir_all(root.0.join("tests")).map_err(|err| format!("remove: {err}"))?;
        assert!(drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &related,
            &deleted,
            &[]
        ));
        assert!(drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &related,
            &folder,
            &[]
        ));
        assert!(!drifts_with_tests(
            &root.0,
            &sources,
            &[],
            &[],
            &folder,
            &[]
        ));
        // Differently cased test path: a confirmed delete and a deleted
        // folder both drift on a case-insensitive host, neither does when
        // case sensitivity is on.
        let cased_folder = [event(&root.0, "TESTS", FileChangeType::DELETED)?];
        assert!(drifts_with_tests_on(
            &root.0,
            &sources,
            cased_files.clone(),
            &deleted,
            true
        ));
        assert!(drifts_with_tests_on(
            &root.0,
            &sources,
            cased_files.clone(),
            &folder,
            true
        ));
        assert!(drifts_with_tests_on(
            &root.0,
            &sources,
            cased_files.clone(),
            &cased_folder,
            true
        ));
        assert!(!drifts_with_tests_on(
            &root.0,
            &sources,
            cased_files.clone(),
            &deleted,
            false
        ));
        assert!(!drifts_with_tests_on(
            &root.0,
            &sources,
            cased_files,
            &folder,
            false
        ));
        Ok(())
    }

    #[test]
    fn related_test_files_are_root_relative_and_drop_escaping_paths() {
        let root = Path::new("/work/repo");
        let listed = [
            "tests/a.rs",
            "/work/repo/tests/b.rs",
            "../c.rs",
            "/else/d.rs",
        ];
        let files = related_test_files(root, listed.iter().map(Path::new));
        let files: Vec<_> = files.iter().map(|file| file.to_string_lossy()).collect();
        assert_eq!(files, ["tests/a.rs", "tests/b.rs"]);

        // Differently cased root (#14729): on a case-insensitive host the
        // absolute file still strips, keeping its own spelling below the root.
        let cased_root = Path::new("/WORK/REPO");
        let cased = related_test_files_on(
            cased_root,
            [Path::new("/work/repo/Tests/B.rs"), Path::new("/else/d.rs")],
            true,
        );
        let cased: Vec<_> = cased
            .iter()
            .map(|file| file.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(cased, ["Tests/B.rs"]);
        // Case-sensitive control: the differently cased absolute path is not
        // under the root, so it is dropped like any escaping path.
        let sensitive =
            related_test_files_on(cased_root, [Path::new("/work/repo/Tests/B.rs")], false);
        assert!(sensitive.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn committed_source_replaced_by_an_outside_link_drifts() -> Result<(), String> {
        let root = temp_root("retarget")?;
        let outside = temp_root("retarget-outside")?;
        let analyzed = b"pub fn f() {}\n";
        write(&outside.0, "src/other.rs", analyzed)?;
        let sources = consumed(&[("src/lib.rs", analyzed)]);
        let changed = [event(&root.0, "src/lib.rs", FileChangeType::CHANGED)?];
        let uncommitted = [event(&root.0, "src/extra.rs", FileChangeType::CHANGED)?];
        let link = |name: &str| {
            std::os::unix::fs::symlink(outside.0.join("src/other.rs"), root.0.join(name))
                .map_err(|err| format!("symlink {name}: {err}"))
        };
        link("src/lib.rs")?;
        link("src/extra.rs")?;

        // Same bytes behind the link, but the committed entry left the root.
        assert!(drifts(&root.0, &sources, &[], &changed, &[]));
        // An outside link at a path with no commitment stays excluded.
        assert!(!drifts(&root.0, &sources, &[], &uncommitted, &[]));
        Ok(())
    }

    #[test]
    fn source_watcher_reports_changes_and_deletions_under_the_root_only() -> Result<(), String> {
        let root = std::env::temp_dir().join("ripr-watched-drift-watcher-root");
        let base = file_uri_for_path(&root)?;
        let anchored = committed_source_watchers(Some(&base), "/unused");
        assert_eq!(anchored.len(), 2);
        assert_eq!(anchored[0]["kind"], serde_json::json!(2));
        assert_eq!(
            anchored[0]["globPattern"]["baseUri"],
            serde_json::json!(base.as_str())
        );
        let pattern = anchored[0]["globPattern"]["pattern"]
            .as_str()
            .ok_or("anchored pattern must be a string")?;
        assert!(pattern.starts_with("**/*.{rs,py,pm,pl,t"), "{pattern}");

        // Deletions are watched on every path under the root, so a folder
        // deleted as one entry is reported (#14099). Nothing is watched twice
        // for the same kind of event.
        assert_eq!(anchored[1]["kind"], serde_json::json!(4));
        assert_eq!(
            anchored[1]["globPattern"]["baseUri"],
            serde_json::json!(base.as_str())
        );
        assert_eq!(
            anchored[1]["globPattern"]["pattern"],
            serde_json::json!("**/*")
        );

        let plain = committed_source_watchers(None, "/work/repo");
        assert_eq!(plain.len(), 2);
        assert_eq!(plain[0]["kind"], serde_json::json!(2));
        assert_eq!(
            plain[0]["globPattern"],
            serde_json::json!(format!("/work/repo/{pattern}"))
        );
        assert_eq!(plain[1]["kind"], serde_json::json!(4));
        assert_eq!(
            plain[1]["globPattern"],
            serde_json::json!("/work/repo/**/*")
        );
        Ok(())
    }

    #[test]
    fn events_outside_the_root_or_without_a_file_path_are_ignored() -> Result<(), String> {
        let root = temp_root("outside")?;
        let outside = temp_root("outside-sibling")?;
        let sources = consumed(&[("src/lib.rs", b"analyzed\n")]);
        let foreign = [event(&outside.0, "src/lib.rs", FileChangeType::DELETED)?];
        assert!(!drifts(&root.0, &sources, &[], &foreign, &[]));

        let not_a_file = [FileEvent {
            uri: "untitled:Untitled-1"
                .parse()
                .map_err(|err| format!("parse uri: {err:?}"))?,
            typ: FileChangeType::DELETED,
        }];
        assert!(!drifts(&root.0, &sources, &[], &not_a_file, &[]));
        Ok(())
    }
}
