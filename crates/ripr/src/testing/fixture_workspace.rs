//! Hermetic fixture workspace for the gate evaluation matrix.
//!
//! The gate tests resolve corpus inputs (`fixtures/...`) against an
//! explicit root. Rooting those evaluations at the live repository couples
//! outputs to ambient generated state: the default-path causal loads read
//! `<root>/target/ripr/pr/canonical-delta.json`, so a producer artifact
//! left in the live tree silently changes decisions and receipt fields
//! (#3742 class (b)). This helper materializes a private copy containing
//! exactly the current-worktree corpus trees the matrix reads, preserving layout
//! so relative input strings stay byte-identical. Expected-content reads
//! and documented re-bless writes intentionally stay repo-rooted (see
//! `assert_repo_fixture`); only evaluation roots move here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

/// Current-worktree corpus trees the gate matrix reads, relative to the workspace
/// root. Keep this list closed: every `fixtures/...` input the matrix
/// resolves must be covered, and nothing else is copied.
const GATE_CORPUS_TREES: &[&str] = &[
    "fixtures/boundary_gap/expected",
    "fixtures/gate_baseline_fallback_disclosure",
];

/// Share a read-only copy while callers own it, without extending its lifetime
/// to process exit. Nextest runs tests in separate processes; a static owning
/// path would leave a corpus copy behind for every such process.
static HERMETIC_ROOT: Mutex<Weak<HermeticGateFixture>> = Mutex::new(Weak::new());
static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

/// Keep this owner alive until every evaluation using its path has finished.
/// Planting tests must still use their own writable roots.
#[derive(Debug)]
pub(crate) struct HermeticGateFixture {
    path: PathBuf,
}

impl HermeticGateFixture {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for HermeticGateFixture {
    fn drop(&mut self) {
        match std::fs::remove_dir_all(&self.path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => eprintln!(
                "remove owned gate corpus {} failed: {err}",
                self.path.display()
            ),
        }
    }
}

/// Returns an owning handle to the hermetic gate corpus. Only the two selected
/// trees are copied, including local edits within them; workspace `target/`
/// state cannot leak into evaluations. The last handle removes the copy.
pub(crate) fn hermetic_gate_fixture_root() -> Result<Arc<HermeticGateFixture>, String> {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .ok_or_else(|| "hermetic fixture root needs a workspace root".to_string())?;
    shared_root(&HERMETIC_ROOT, &workspace_root)
}

fn shared_root(
    cache: &Mutex<Weak<HermeticGateFixture>>,
    workspace_root: &Path,
) -> Result<Arc<HermeticGateFixture>, String> {
    let mut cached = cache
        .lock()
        .map_err(|err| format!("lock gate corpus cache failed: {err}"))?;
    if let Some(root) = cached.upgrade() {
        return Ok(root);
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|err| format!("system time before unix epoch: {err}"))?
        .as_nanos();
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent).map_err(|err| {
        format!(
            "create gate corpus parent {} failed: {err}",
            parent.display()
        )
    })?;
    let root = parent.join(format!(
        "ripr-gate-corpus-{}-{stamp}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    let root = Arc::new(materialize_hermetic_root_at(workspace_root, root)?);
    *cached = Arc::downgrade(&root);
    Ok(root)
}

fn materialize_hermetic_root_at(
    workspace_root: &Path,
    path: PathBuf,
) -> Result<HermeticGateFixture, String> {
    // Cleanup authority begins only after exclusive creation. Install it before
    // copying so partial materialization is removed on every error/unwind path.
    std::fs::create_dir(&path)
        .map_err(|err| format!("claim gate corpus {} failed: {err}", path.display()))?;
    let root = HermeticGateFixture { path };
    for tree in GATE_CORPUS_TREES {
        copy_tree(&workspace_root.join(tree), &root.path.join(tree))?;
    }
    Ok(root)
}

fn copy_tree(source: &Path, dest: &Path) -> Result<(), String> {
    let entries = std::fs::read_dir(source)
        .map_err(|err| format!("read {} failed: {err}", source.display()))?;
    std::fs::create_dir_all(dest)
        .map_err(|err| format!("create {} failed: {err}", dest.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read entry failed: {err}"))?;
        let from = entry.path();
        let to = dest.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(|err| format!("copy {} failed: {err}", from.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            if let Ok(()) = std::fs::remove_dir_all(&self.0) {}
        }
    }

    fn scratch() -> Result<Scratch, String> {
        let parent = std::env::temp_dir();
        std::fs::create_dir_all(&parent).map_err(|err| err.to_string())?;
        let path = parent.join(format!(
            "ripr-gate-corpus-control-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|err| err.to_string())?;
        Ok(Scratch(path))
    }

    fn seed(workspace: &Path) -> Result<(), String> {
        for tree in GATE_CORPUS_TREES {
            let dir = workspace.join(tree);
            std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
            std::fs::write(dir.join("local.json"), b"current worktree bytes\n")
                .map_err(|err| err.to_string())?;
        }
        // An ambient producer outside the two selected trees must not travel.
        std::fs::create_dir_all(workspace.join("target/ripr/pr")).map_err(|err| err.to_string())?;
        std::fs::write(workspace.join("target/ripr/pr/canonical-delta.json"), b"{}")
            .map_err(|err| err.to_string())
    }

    fn tree_bytes(root: &Path) -> Result<std::collections::BTreeMap<PathBuf, Vec<u8>>, String> {
        let mut files = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(root).map_err(|err| err.to_string())? {
            let entry = entry.map_err(|err| err.to_string())?;
            let name = PathBuf::from(entry.file_name());
            if entry.file_type().map_err(|err| err.to_string())?.is_dir() {
                for (relative, bytes) in tree_bytes(&entry.path())? {
                    files.insert(name.join(relative), bytes);
                }
            } else {
                files.insert(
                    name,
                    std::fs::read(entry.path()).map_err(|err| err.to_string())?,
                );
            }
        }
        Ok(files)
    }

    #[test]
    fn hermetic_copy_carries_corpus_without_generated_state() -> Result<(), String> {
        let root = hermetic_gate_fixture_root()?;
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for tree in GATE_CORPUS_TREES {
            assert!(
                root.path().join(tree).is_dir(),
                "corpus tree missing from hermetic copy: {tree}"
            );
            let original = tree_bytes(&workspace.join(tree))?;
            assert!(!original.is_empty(), "empty corpus tree: {tree}");
            assert_eq!(tree_bytes(&root.path().join(tree))?, original, "{tree}");
        }
        assert!(
            root.path()
                .join("fixtures/boundary_gap/expected/pr-guidance/exact-line/comments.json")
                .is_file(),
            "matrix guidance input missing from hermetic copy"
        );
        assert!(
            !root.path().join("target").exists(),
            "hermetic copy must not carry generated state"
        );
        Ok(())
    }

    #[test]
    fn live_handles_share_bytes_until_the_last_drop() -> Result<(), String> {
        let workspace = scratch()?;
        seed(&workspace.0)?;
        let cache = Mutex::new(Weak::new());
        let first = shared_root(&cache, &workspace.0)?;
        let path = first.path().to_path_buf();
        let second = shared_root(&cache, &workspace.0)?;
        assert_eq!(first.path(), second.path());
        assert!(!path.join("target").exists());
        drop(first);
        for tree in GATE_CORPUS_TREES {
            assert_eq!(
                std::fs::read(second.path().join(tree).join("local.json"))
                    .map_err(|err| err.to_string())?,
                b"current worktree bytes\n"
            );
        }
        drop(second);
        assert!(
            !path.exists(),
            "last caller left the corpus at {}",
            path.display()
        );
        let next = shared_root(&cache, &workspace.0)?;
        assert_ne!(
            next.path(),
            path,
            "expired cache must materialize a new owned root"
        );
        Ok(())
    }

    #[test]
    fn concurrent_readers_keep_one_copy_alive() -> Result<(), String> {
        let workspace = scratch()?;
        seed(&workspace.0)?;
        let cache = Mutex::new(Weak::new());
        let barrier = std::sync::Barrier::new(8);
        let path = std::thread::scope(|scope| -> Result<PathBuf, String> {
            let mut readers = Vec::new();
            for _ in 0..8 {
                readers.push(scope.spawn(|| -> Result<PathBuf, String> {
                    let root = shared_root(&cache, &workspace.0);
                    barrier.wait();
                    let root = root?;
                    let path = root.path().to_path_buf();
                    assert!(path.join(GATE_CORPUS_TREES[0]).join("local.json").is_file());
                    Ok(path)
                }));
            }
            let mut paths = Vec::new();
            for reader in readers {
                paths.push(
                    reader
                        .join()
                        .map_err(|payload| format!("corpus reader panicked: {payload:?}"))??,
                );
            }
            assert_eq!(paths.len(), 8);
            assert!(paths.iter().all(|path| path == &paths[0]));
            Ok(paths.remove(0))
        })?;
        assert!(!path.exists(), "joined readers left their copy behind");
        Ok(())
    }

    #[test]
    fn partial_copy_failure_removes_only_the_claimed_root() -> Result<(), String> {
        let workspace = scratch()?;
        let first_tree = workspace.0.join(GATE_CORPUS_TREES[0]);
        std::fs::create_dir_all(&first_tree).map_err(|err| err.to_string())?;
        std::fs::write(first_tree.join("keep.json"), b"source survives")
            .map_err(|err| err.to_string())?;
        let path = workspace.0.join("partial");
        let error = materialize_hermetic_root_at(&workspace.0, path.clone())
            .err()
            .ok_or("missing second tree must fail materialization")?;
        assert!(error.contains(GATE_CORPUS_TREES[1]), "{error}");
        assert!(
            !path.exists(),
            "failed materialization leaked a partial copy"
        );
        assert_eq!(
            std::fs::read(first_tree.join("keep.json")).map_err(|err| err.to_string())?,
            b"source survives"
        );
        Ok(())
    }

    #[test]
    fn occupied_path_is_never_adopted_or_removed() -> Result<(), String> {
        let workspace = scratch()?;
        let occupied = workspace.0.join("occupied");
        std::fs::create_dir(&occupied).map_err(|err| err.to_string())?;
        std::fs::write(occupied.join("owner"), b"another owner").map_err(|err| err.to_string())?;
        if let Ok(root) = materialize_hermetic_root_at(&workspace.0, occupied.clone()) {
            return Err(format!("adopted occupied root {}", root.path().display()));
        }
        assert_eq!(
            std::fs::read(occupied.join("owner")).map_err(|err| err.to_string())?,
            b"another owner"
        );
        Ok(())
    }

    #[test]
    fn evaluation_error_releases_the_corpus() -> Result<(), String> {
        let workspace = scratch()?;
        seed(&workspace.0)?;
        let cache = Mutex::new(Weak::new());
        let mut path = PathBuf::new();
        let result = (|| -> Result<(), String> {
            let fixture = shared_root(&cache, &workspace.0)?;
            path = fixture.path().to_path_buf();
            assert!(path.is_dir());
            Err("evaluation failed".to_string())
        })();
        assert_eq!(result, Err("evaluation failed".to_string()));
        assert!(!path.exists(), "evaluation error leaked its corpus");
        Ok(())
    }

    #[test]
    fn evaluation_unwind_releases_the_corpus() -> Result<(), String> {
        let workspace = scratch()?;
        seed(&workspace.0)?;
        let root = shared_root(&Mutex::new(Weak::new()), &workspace.0)?;
        let path = root.path().to_path_buf();
        assert!(path.is_dir());
        let result = std::panic::catch_unwind(move || {
            let _fixture = root;
            std::panic::resume_unwind(Box::new(()));
        });
        assert!(result.is_err(), "unwind control did not execute");
        assert!(!path.exists(), "evaluation unwind leaked its corpus");
        Ok(())
    }
}
