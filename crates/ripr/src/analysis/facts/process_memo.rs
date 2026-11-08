//! Process-lifetime memo of per-file parse facts for long-lived servers.
//!
//! Every index build looks each file up in the on-disk file-fact cache. A
//! warm lookup still reads the entry, decodes it and recomputes its payload
//! digest. On this repository that is about 2.5 s per build, and an editor
//! save runs two builds (the dependent-scope admission core and the main
//! index). `ripr lsp` keeps the facts of the last bytes it saw for each path
//! in memory, so a later save copies them from memory and only the saved
//! file misses.
//!
//! Stale facts are impossible by construction. A hit requires the looked-up
//! bytes to equal the remembered file's source text byte for byte, and parse
//! facts are a pure function of the path, the bytes and the running analyzer
//! build, which a process cannot change. No digest stands in for the
//! comparison. A file whose bytes differ from its decoded text (a BOM or
//! invalid UTF-8) is never remembered and always takes the disk path.
//!
//! The memo is off unless the process enables it: `ripr lsp` does, one-shot
//! commands and tests do not, so their cache statistics are unchanged.

use super::FileFacts;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Remembered source bytes above which the memo is cleared before the next
/// insert. The index already holds the same facts, so this bounds the extra
/// resident copy, not the analysis.
const MAX_SOURCE_BYTES: usize = 512 * 1024 * 1024;

/// Remembered paths above which the memo is cleared before the next insert.
const MAX_ENTRIES: usize = 50_000;

/// The latest parse facts per workspace-relative path.
pub(crate) struct ProcessFactMemo {
    state: Mutex<MemoState>,
}

struct MemoState {
    by_path: BTreeMap<PathBuf, Arc<FileFacts>>,
    source_bytes: usize,
}

impl ProcessFactMemo {
    pub(crate) const fn new() -> Self {
        Self {
            state: Mutex::new(MemoState {
                by_path: BTreeMap::new(),
                source_bytes: 0,
            }),
        }
    }

    /// The remembered facts for `path` when they were produced from exactly
    /// `bytes`.
    pub(crate) fn lookup(&self, path: &Path, bytes: &[u8]) -> Option<FileFacts> {
        let remembered = {
            let state = self.state.lock().ok()?;
            Arc::clone(state.by_path.get(path)?)
        };
        // The copy runs outside the lock so parallel lookups do not queue.
        (remembered.source.as_bytes() == bytes).then(|| FileFacts::clone(&remembered))
    }

    /// Remember `facts` as the parse of `bytes` at `path`, replacing any
    /// facts for an earlier version of the file.
    pub(crate) fn remember(&self, path: &Path, bytes: &[u8], facts: &FileFacts) {
        if facts.source.as_bytes() != bytes || facts.path != path {
            return;
        }
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if let Some(previous) = state.by_path.remove(path) {
            state.source_bytes = state.source_bytes.saturating_sub(previous.source.len());
        }
        if state.by_path.len() >= MAX_ENTRIES
            || state.source_bytes.saturating_add(bytes.len()) > MAX_SOURCE_BYTES
        {
            state.by_path.clear();
            state.source_bytes = 0;
        }
        state.source_bytes = state.source_bytes.saturating_add(bytes.len());
        state
            .by_path
            .insert(path.to_path_buf(), Arc::new(facts.clone()));
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.state.lock().map_or(0, |state| state.by_path.len())
    }
}

static PROCESS_FACT_MEMO: ProcessFactMemo = ProcessFactMemo::new();
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Turn on the process memo for the rest of this process. Called once by
/// `ripr lsp` before it serves.
pub(crate) fn enable_process_fact_memo() {
    ENABLED.store(true, Ordering::Release);
}

/// The process memo, when this process enabled it.
pub(super) fn process_fact_memo() -> Option<&'static ProcessFactMemo> {
    ENABLED
        .load(Ordering::Acquire)
        .then_some(&PROCESS_FACT_MEMO)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(path: &str, source: &str) -> FileFacts {
        FileFacts {
            path: PathBuf::from(path),
            source: Arc::from(source),
            ..FileFacts::default()
        }
    }

    #[test]
    fn hit_requires_the_exact_remembered_bytes() {
        let memo = ProcessFactMemo::new();
        let path = Path::new("src/lib.rs");
        let remembered = facts("src/lib.rs", "pub fn a() {}\n");
        memo.remember(path, b"pub fn a() {}\n", &remembered);

        assert_eq!(memo.lookup(path, b"pub fn a() {}\n"), Some(remembered));
        assert_eq!(memo.lookup(path, b"pub fn b() {}\n"), None);
        assert_eq!(
            memo.lookup(Path::new("src/other.rs"), b"pub fn a() {}\n"),
            None
        );
    }

    #[test]
    fn a_new_version_replaces_the_old_one() {
        let memo = ProcessFactMemo::new();
        let path = Path::new("src/lib.rs");
        memo.remember(
            path,
            b"pub fn a() {}\n",
            &facts("src/lib.rs", "pub fn a() {}\n"),
        );
        let second = facts("src/lib.rs", "pub fn a() { 1; }\n");
        memo.remember(path, b"pub fn a() { 1; }\n", &second);

        assert_eq!(memo.len(), 1);
        assert_eq!(memo.lookup(path, b"pub fn a() {}\n"), None);
        assert_eq!(memo.lookup(path, b"pub fn a() { 1; }\n"), Some(second));
    }

    #[test]
    fn bytes_that_differ_from_the_decoded_text_are_never_remembered() {
        let memo = ProcessFactMemo::new();
        let path = Path::new("src/lib.rs");
        // A BOM is stripped from the decoded source, so the bytes differ.
        memo.remember(
            path,
            b"\xEF\xBB\xBFpub fn a() {}\n",
            &facts("src/lib.rs", "pub fn a() {}\n"),
        );
        // Facts produced for another path are not remembered either.
        memo.remember(
            path,
            b"pub fn a() {}\n",
            &facts("src/other.rs", "pub fn a() {}\n"),
        );

        assert_eq!(memo.len(), 0);
        assert_eq!(memo.lookup(path, b"pub fn a() {}\n"), None);
        assert_eq!(memo.lookup(path, b"\xEF\xBB\xBFpub fn a() {}\n"), None);
    }
}
