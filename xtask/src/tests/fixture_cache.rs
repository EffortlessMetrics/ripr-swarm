//! Real fixture ownership controls for #3054 and #7300.

use super::*;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::time::Instant;

const SOURCE: &str = "fixtures/all_no_path_disclosure";
const CHILD_PATH: &str = "RIPR_XTASK_FIXTURE_TEST_PATH";
const CHILD: &str = "tests::fixture_cache::fixture_cache_process_child";

/// Acquire only fresh paths. The parent owns them until every child is reaped,
/// including a runner failure, timeout or assertion unwind.
pub(super) struct Case {
    pub(super) fixture: PathBuf,
    pub(super) cache: PathBuf,
    pub(super) output: PathBuf,
    owned: Vec<PathBuf>,
}

impl Drop for Case {
    fn drop(&mut self) {
        for path in self.owned.iter().rev() {
            match fs::remove_dir_all(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    eprintln!("remove owned fixture test path {}: {error}", path.display())
                }
            }
        }
    }
}

fn repo_root() -> Result<PathBuf, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest must have a repository parent".to_string())
}

impl Case {
    pub(super) fn new(label: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let name = format!(
            "fixture-test-{label}-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        Self::create(&repo_root()?, &name)
    }

    fn create(repo: &Path, name: &str) -> Result<Self, String> {
        let fixture = Path::new("target/ripr/fixture-tests").join(name);
        let cache = repo.join("target/ripr/fixture-cache").join(name);
        let output = repo.join("target/ripr/fixtures").join(name);
        let mut case = Self {
            fixture,
            cache,
            output,
            owned: Vec::new(),
        };
        for path in [
            repo.join(&case.fixture),
            case.cache.clone(),
            case.output.clone(),
        ] {
            let parent = path.parent().ok_or("owned fixture path needs a parent")?;
            fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
            fs::create_dir(&path).map_err(|e| format!("acquire {}: {e}", path.display()))?;
            case.owned.push(path);
        }
        // Copy authored inputs only, never unknown input/target residue.
        for file in [
            "diff.patch",
            "input/Cargo.toml",
            "input/src/lib.rs",
            "expected/check.json",
            "expected/human.txt",
        ] {
            let target = repo.join(&case.fixture).join(file);
            let parent = target.parent().ok_or("fixture copy needs a parent")?;
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            fs::copy(repo.join(SOURCE).join(file), &target)
                .map_err(|e| format!("copy {}: {e}", target.display()))?;
        }
        let relative = crate::normalize_path(&case.fixture);
        case.rebind(
            repo,
            "expected/check.json",
            &format!("\"root\": \"{SOURCE}/input\""),
            &format!("\"root\": \"{relative}/input\""),
            1,
        )?;
        case.rebind(
            repo,
            "expected/human.txt",
            &format!("root: {SOURCE}/input"),
            &format!("root: {relative}/input"),
            1,
        )?;
        for suffix in ["input", "diff.patch"] {
            case.rebind(
                repo,
                "expected/human.txt",
                &format!("<cwd>/{SOURCE}/{suffix}"),
                &format!("<cwd>/{relative}/{suffix}"),
                2,
            )?;
        }
        Ok(case)
    }

    fn rebind(
        &self,
        repo: &Path,
        file: &str,
        from: &str,
        to: &str,
        count: usize,
    ) -> Result<(), String> {
        let path = repo.join(&self.fixture).join(file);
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        if text.matches(from).count() != count {
            return Err(format!(
                "{} must contain {count} exact authored path bindings for {from:?}",
                path.display()
            ));
        }
        fs::write(path, text.replace(from, to)).map_err(|e| e.to_string())
    }

    #[cfg(target_os = "linux")]
    fn seed_stale(&self) -> Result<PathBuf, String> {
        let stale = self.cache.join("repo-file-facts/0.2/stale-7300.json");
        fs::create_dir_all(stale.parent().ok_or("stale path needs parent")?)
            .map_err(|e| e.to_string())?;
        fs::write(&stale, b"{\"stale\":true}").map_err(|e| e.to_string())?;
        Ok(stale)
    }
}

fn snapshot(path: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, String> {
    fn visit(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<PathBuf, Vec<u8>>,
    ) -> Result<(), String> {
        for entry in fs::read_dir(path).map_err(|e| format!("read {}: {e}", path.display()))? {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                visit(root, &entry.path(), files)?;
            } else {
                files.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_path_buf(),
                    fs::read(entry.path()).map_err(|e| e.to_string())?,
                );
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(path, path, &mut files)?;
    Ok(files)
}

fn child(case: &Case, timeout: Duration) -> Result<crate::run::TimedOutput, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = crate::normalize_path(&case.fixture);
    let result = crate::run::capture_output_measured(
        &exe.to_string_lossy(),
        &[
            CHILD.to_string(),
            "--exact".to_string(),
            "--nocapture".to_string(),
        ],
        Some(&repo_root()?),
        &[(CHILD_PATH, &path)],
        timeout,
        "owned real fixture test process",
    )?;
    Ok(result.output)
}

fn assert_child(output: &crate::run::TimedOutput) {
    assert!(
        !output.timed_out && output.status.is_some_and(|status| status.success()),
        "{}",
        describe(output)
    );
    assert!(
        output.stdout.contains("running 1 test") && output.stdout.contains("1 passed; 0 failed"),
        "exact child selection must execute one test: {}",
        describe(output)
    );
}

fn describe(output: &crate::run::TimedOutput) -> String {
    format!(
        "status={:?}, timed_out={}, stdout={}, stderr={}",
        output.status, output.timed_out, output.stdout, output.stderr
    )
}

#[test]
fn fixture_cache_process_child() -> Result<(), String> {
    let Some(path) = std::env::var_os(CHILD_PATH) else {
        return Ok(());
    };
    let fixture = PathBuf::from(path);
    let name = fixture
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("child fixture needs name")?;
    let cache = crate::fixture_cache_dir(name)?;
    let run = crate::run_fixture(&fixture)?;
    assert!(
        run.comparisons_all_match(),
        "owned fixture must retain its authored goldens"
    );
    assert!(
        !snapshot(&cache)?.is_empty(),
        "real runner must populate owned facts"
    );
    assert!(
        !fixture.join("input/target").exists(),
        "cache must stay outside fixture workspace"
    );
    let check_json = Path::new("target/ripr/fixtures")
        .join(name)
        .join("check.json");
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(check_json).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    assert_eq!(
        json["findings"].as_array().map(Vec::len),
        Some(1),
        "must execute the real fixture subject"
    );
    Ok(())
}

// Linux Cargo's artifact lock uses flock, as does File::lock. The ownership
// repair and the two binding tests are platform independent; this specific
// native contention discriminator is Linux evidence.
#[cfg(target_os = "linux")]
#[test]
fn fixture_cache_processes_keep_owned_facts_and_outputs_during_cargo_contention()
-> Result<(), String> {
    let a = Case::new("concurrent-a")?;
    assert_child(&child(&a, crate::run::tool_build_timeout()?)?);
    let facts = snapshot(&a.cache)?;
    let outputs = snapshot(&a.output)?;
    assert!(!facts.is_empty());
    assert_eq!(outputs.len(), 3);
    assert!(outputs.values().all(|bytes| !bytes.is_empty()));
    let b = Case::new("concurrent-b")?;
    let stale = b.seed_stale()?;
    std::thread::scope(|scope| -> Result<(), String> {
        // Define after entering the scope: unwind releases the lock before
        // scope autojoin, and owners outside the scope clean only after reap.
        let binary = repo_root()?.join(crate::ripr_debug_binary());
        let lock_path = binary
            .parent()
            .ok_or("binary needs parent")?
            .join(".cargo-lock");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| format!("open {}: {e}", lock_path.display()))?;
        lock.lock()
            .map_err(|e| format!("lock {}: {e}", lock_path.display()))?;
        let peer = scope.spawn(|| child(&b, crate::run::tool_build_timeout()?));
        let deadline = Instant::now() + Duration::from_secs(20);
        while stale.exists() && !peer.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            !stale.exists(),
            "peer must reach the real runner's pre-build clear"
        );
        assert!(
            !peer.is_finished(),
            "peer must still be waiting under the native Cargo lock"
        );
        assert!(
            a.cache.is_dir(),
            "competing fixture process removed the first run's populated cache"
        );
        assert_eq!(
            snapshot(&a.cache)?,
            facts,
            "competing fixture process must preserve the first run's facts"
        );
        assert_eq!(
            snapshot(&a.output)?,
            outputs,
            "competing fixture process must preserve the first run's outputs"
        );
        assert!(
            !b.output.join("check.json").exists(),
            "peer may not publish before the current-binary build"
        );
        // Keep the native lock held long enough for Cargo to report its wait;
        // its captured stderr below verifies the wait rather than assuming it.
        std::thread::sleep(Duration::from_millis(200));
        lock.unlock().map_err(|e| e.to_string())?;
        let result = peer
            .join()
            .map_err(|_| "fixture process driver panicked")??;
        assert_child(&result);
        assert!(
            result
                .stderr
                .contains("Blocking waiting for file lock on artifact directory"),
            "native Cargo lock wait must be observed: {}",
            describe(&result)
        );
        assert!(!snapshot(&b.cache)?.is_empty());
        assert_eq!(snapshot(&a.cache)?, facts);
        assert_eq!(snapshot(&a.output)?, outputs);
        eprintln!(
            "#7300 native contention: both real processes exit 0; first facts={} outputs={}; Cargo lock wait observed; peer facts={}",
            facts.len(),
            outputs.len(),
            snapshot(&b.cache)?.len()
        );
        Ok(())
    })
}

#[test]
fn fixture_cache_owner_preserves_unknown_paths_and_cleans_failed_setup() -> Result<(), String> {
    let anchor = Case::new("cleanup-anchor")?;
    let repo = repo_root()?;
    let name = anchor
        .fixture
        .file_name()
        .and_then(|p| p.to_str())
        .ok_or("anchor needs name")?;
    let sentinel = repo.join(&anchor.fixture).join("unknown");
    fs::write(&sentinel, b"retain unknown state").map_err(|e| e.to_string())?;
    assert!(Case::create(&repo, name).is_err());
    assert_eq!(
        fs::read(&sentinel).map_err(|e| e.to_string())?,
        b"retain unknown state"
    );
    let fake_repo = repo.join(&anchor.fixture).join("acquisition-controls");
    for (index, occupied) in ["fixture-tests", "fixture-cache", "fixtures"]
        .iter()
        .enumerate()
    {
        let name = format!("refuse-{index}");
        let unknown = fake_repo.join("target/ripr").join(occupied).join(&name);
        fs::create_dir_all(&unknown).map_err(|e| e.to_string())?;
        fs::write(unknown.join("sentinel"), b"unknown").map_err(|e| e.to_string())?;
        assert!(Case::create(&fake_repo, &name).is_err());
        assert_eq!(
            fs::read(unknown.join("sentinel")).map_err(|e| e.to_string())?,
            b"unknown"
        );
        for acquired in ["fixture-tests", "fixture-cache", "fixtures"] {
            let path = fake_repo.join("target/ripr").join(acquired).join(&name);
            if path != unknown {
                assert!(
                    !path.exists(),
                    "failed acquisition must release only its own path: {}",
                    path.display()
                );
            }
        }
    }
    assert!(Case::create(&fake_repo, "missing-template").is_err());
    for dir in ["fixture-tests", "fixture-cache", "fixtures"] {
        assert!(
            !fake_repo
                .join("target/ripr")
                .join(dir)
                .join("missing-template")
                .exists(),
            "failed copy must clean acquired paths"
        );
    }
    let paths = {
        let failed = Case::new("runner-failure")?;
        let paths = failed.owned.clone();
        fs::remove_file(repo.join(&failed.fixture).join("diff.patch"))
            .map_err(|e| e.to_string())?;
        let result = child(&failed, Duration::from_secs(30))?;
        assert!(
            !result.timed_out && result.status.is_some_and(|status| !status.success()),
            "{}",
            describe(&result)
        );
        assert!(
            format!("{}{}", result.stdout, result.stderr).contains("missing diff.patch"),
            "must fail at actual runner setup: {}",
            describe(&result)
        );
        paths
    };
    assert!(
        paths.iter().all(|p| !p.exists()),
        "failed-run owner must clean only acquired paths"
    );
    let unwind = Case::new("unwind")?;
    let paths = unwind.owned.clone();
    assert!(
        std::panic::catch_unwind(move || {
            let owned = unwind;
            assert!(owned.owned.is_empty(), "controlled owner unwind");
        })
        .is_err()
    );
    assert!(paths.iter().all(|p| !p.exists()));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn fixture_cache_aborted_process_is_reaped_before_owned_cleanup_and_recovers() -> Result<(), String>
{
    // Warm the real binary first, so abort concerns native lock contention,
    // rather than a slow cold build or missing producer.
    let warm = Case::new("abort-warm")?;
    assert_child(&child(&warm, crate::run::tool_build_timeout()?)?);
    let facts = snapshot(&warm.cache)?;
    let output = snapshot(&warm.output)?;
    let paths = {
        let aborted = Case::new("abort")?;
        let stale = aborted.seed_stale()?;
        let paths = aborted.owned.clone();
        let binary = repo_root()?.join(crate::ripr_debug_binary());
        let lock_path = binary
            .parent()
            .ok_or("binary needs parent")?
            .join(".cargo-lock");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
            .map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        // The existing timed runner terminates/reaps its owned process group
        // before returning. Parent path ownership outlives that return.
        let result = child(&aborted, Duration::from_secs(5))?;
        assert!(
            result.timed_out && result.status.is_some_and(|status| !status.success()),
            "controlled abort must be native timeout: {}",
            describe(&result)
        );
        assert!(
            !stale.exists(),
            "aborted process must have reached the real runner clear"
        );
        assert!(
            result
                .stderr
                .contains("Blocking waiting for file lock on artifact directory"),
            "abort must reach native Cargo contention: {}",
            describe(&result)
        );
        assert!(!aborted.output.join("check.json").exists());
        assert_eq!(snapshot(&warm.cache)?, facts);
        assert_eq!(snapshot(&warm.output)?, output);
        lock.unlock().map_err(|e| e.to_string())?;
        // Reuse only this owner's paths after reaping: a failed/aborted run
        // must remain recoverable through the same real runner.
        assert_child(&child(&aborted, crate::run::tool_build_timeout()?)?);
        assert!(!snapshot(&aborted.cache)?.is_empty());
        paths
    };
    assert!(
        paths.iter().all(|p| !p.exists()),
        "reaped abort/recovery owner must clean its own paths"
    );
    Ok(())
}
