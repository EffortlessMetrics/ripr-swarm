//! Real fixture ownership controls for #3054 and #7300.

use super::*;
use std::collections::BTreeMap;
#[cfg(target_os = "linux")]
use std::os::fd::AsRawFd;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(target_os = "linux")]
use std::time::Instant;

const SOURCE: &str = "fixtures/all_no_path_disclosure";
const CHILD_PATH: &str = "RIPR_XTASK_FIXTURE_TEST_PATH";
#[cfg(target_os = "linux")]
const CHILD_PID: &str = "RIPR_XTASK_FIXTURE_TEST_PID";
#[cfg(target_os = "linux")]
const CHILD_START_GATE: &str = "RIPR_XTASK_FIXTURE_TEST_START_GATE";
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
    #[cfg(not(target_os = "linux"))]
    let envs = [(CHILD_PATH, path.as_str())];
    #[cfg(target_os = "linux")]
    let marker = crate::normalize_path(&child_pid_path(case)?);
    #[cfg(target_os = "linux")]
    let envs = [(CHILD_PATH, path.as_str()), (CHILD_PID, marker.as_str())];
    let result = crate::run::capture_output_measured(
        &exe.to_string_lossy(),
        &[
            CHILD.to_string(),
            "--exact".to_string(),
            "--nocapture".to_string(),
        ],
        Some(&repo_root()?),
        &envs,
        timeout,
        "owned real fixture test process",
    )?;
    Ok(result.output)
}

#[cfg(target_os = "linux")]
fn child_after_readiness(
    case: &Case,
    gate: &Path,
    deadline: crate::run::ReadinessDeadline<'_>,
) -> Result<crate::run::TimedOutput, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = crate::normalize_path(&case.fixture);
    let marker = crate::normalize_path(&child_pid_path(case)?);
    let gate = crate::normalize_path(gate);
    let result = crate::run::capture_output_measured_after_readiness(
        &exe.to_string_lossy(),
        &[
            CHILD.to_string(),
            "--exact".to_string(),
            "--nocapture".to_string(),
        ],
        Some(&repo_root()?),
        &[
            (CHILD_PATH, path.as_str()),
            (CHILD_PID, marker.as_str()),
            (CHILD_START_GATE, gate.as_str()),
        ],
        deadline,
        "owned real fixture readiness process",
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
    #[cfg(target_os = "linux")]
    {
        let marker =
            PathBuf::from(std::env::var_os(CHILD_PID).ok_or("child needs owned PID marker")?);
        let temporary = marker.with_extension("tmp");
        fs::write(&temporary, std::process::id().to_string()).map_err(|e| e.to_string())?;
        fs::rename(temporary, marker).map_err(|e| e.to_string())?;
        if let Some(gate) = std::env::var_os(CHILD_START_GATE) {
            while !Path::new(&gate).try_exists().map_err(|e| e.to_string())? {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
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

#[cfg(target_os = "linux")]
fn child_pid_path(case: &Case) -> Result<PathBuf, String> {
    // Immutable acquisition root, not the runner path: the shared-path mutant
    // must never publish its marker into the authored corpus.
    case.owned
        .first()
        .map(|root| root.join("child.pid"))
        .ok_or_else(|| "fixture process needs an acquired marker root".to_string())
}

#[cfg(target_os = "linux")]
fn held_lock_key(lock: &fs::File) -> Result<String, String> {
    let path = format!("/proc/self/fdinfo/{}", lock.as_raw_fd());
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("observe held native Cargo lock {path}: {error}"))?;
    text.lines()
        .find_map(|line| {
            let fields: Vec<_> = line.split_ascii_whitespace().collect();
            if fields.first() == Some(&"lock:")
                && fields.get(2) == Some(&"FLOCK")
                && fields.get(3) == Some(&"ADVISORY")
                && fields.get(4) == Some(&"WRITE")
            {
                fields.get(6).map(|key| key.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| format!("held Cargo FLOCK identity unavailable in {path}"))
}

#[cfg(target_os = "linux")]
fn proc_waiter_gone(error: &std::io::Error) -> bool {
    // Match run.rs's vanished-process handling: ESRCH is Linux error 3.
    // Disappearance never contributes a successful waiter observation.
    error.kind() == std::io::ErrorKind::NotFound || error.raw_os_error() == Some(3)
}

#[cfg(target_os = "linux")]
fn owned_cargo_waiter(key: &str, marker: &Path) -> Result<bool, String> {
    let pid = match fs::read_to_string(marker) {
        Ok(text) => text
            .trim()
            .parse::<u32>()
            .map_err(|error| format!("read owned child PID {}: {error}", marker.display()))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "read owned child PID {}: {error}",
                marker.display()
            ));
        }
    };
    let locks = fs::read_to_string("/proc/locks")
        .map_err(|error| format!("observe native Cargo waiters /proc/locks: {error}"))?;
    for line in locks.lines() {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        if fields.get(1) != Some(&"->")
            || fields.get(2) != Some(&"FLOCK")
            || fields.get(3) != Some(&"ADVISORY")
            || fields.get(4) != Some(&"WRITE")
            || fields.get(6) != Some(&key)
        {
            continue;
        }
        let waiter = fields
            .get(5)
            .ok_or("native waiter needs PID")?
            .parse::<u32>()
            .map_err(|error| format!("read native Cargo waiter PID: {error}"))?;
        let stat_path = format!("/proc/{waiter}/stat");
        let stat = match fs::read_to_string(&stat_path) {
            Ok(stat) => stat,
            Err(error) if proc_waiter_gone(&error) => continue,
            Err(error) => return Err(format!("observe native Cargo waiter {stat_path}: {error}")),
        };
        let group = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_ascii_whitespace().nth(2))
            .ok_or_else(|| format!("native waiter process group unavailable in {stat_path}"))?
            .parse::<u32>()
            .map_err(|error| format!("read native Cargo waiter group: {error}"))?;
        // configure_timed_child_command makes the captured adapter its group
        // leader. The runner's Cargo child inherits that owned group.
        if group != pid {
            continue;
        }
        let cmd_path = format!("/proc/{waiter}/cmdline");
        let cmd = match fs::read(&cmd_path) {
            Ok(cmd) => cmd,
            Err(error) if proc_waiter_gone(&error) => continue,
            Err(error) => return Err(format!("observe native Cargo command {cmd_path}: {error}")),
        };
        let mut args = cmd.split(|byte| *byte == 0).filter(|arg| !arg.is_empty());
        let cargo = args
            .next()
            .and_then(|exe| exe.rsplit(|byte| *byte == b'/').next());
        if cargo == Some(b"cargo".as_slice())
            && args.next() == Some(b"build".as_slice())
            && args.next() == Some(b"-p".as_slice())
            && args.next() == Some(b"ripr".as_slice())
            && args.next().is_none()
        {
            return Ok(true);
        }
    }
    Ok(false)
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
        let key = held_lock_key(&lock)?;
        let marker = child_pid_path(&b)?;
        let peer = scope.spawn(|| child(&b, crate::run::tool_build_timeout()?));
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut blocked = false;
        while !peer.is_finished() && Instant::now() < deadline {
            if !stale.exists() && owned_cargo_waiter(&key, &marker)? {
                blocked = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            blocked,
            "owned Cargo process must reach its native artifact FLOCK wait"
        );
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
        // The owned native Cargo waiter is observed before snapshots/unlock;
        // captured stderr below independently retains Cargo's diagnostic.
        lock.unlock().map_err(|e| e.to_string())?;
        let result = peer
            .join()
            .map_err(|error| format!("fixture process driver panicked: {error:?}"))??;
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
    // Warm the producer, then deliberately delay this distinct child beyond
    // the abort budget. Only its observed native waiter can arm that budget.
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
        let key = held_lock_key(&lock)?;
        let marker = child_pid_path(&aborted)?;
        let gate = marker.with_file_name("start-gate");
        let mut gated_since = None;
        let mut released = false;
        let mut ready_at = None;
        let mut probe = || -> Result<bool, String> {
            if !released {
                if !marker.try_exists().map_err(|e| e.to_string())? {
                    return Ok(false);
                }
                let since = *gated_since.get_or_insert_with(|| {
                    eprintln!("observed owned child PID before delaying runner startup");
                    Instant::now()
                });
                if since.elapsed() < Duration::from_secs(6) {
                    return Ok(false);
                }
                if !stale.exists() {
                    return Err("gated child reached runner before gate release".into());
                }
                fs::write(&gate, b"start").map_err(|e| e.to_string())?;
                released = true;
                eprintln!("owned child startup gate held for {:?}", since.elapsed());
            }
            let ready = !stale.exists() && owned_cargo_waiter(&key, &marker)?;
            if ready {
                ready_at = Some(Instant::now());
                eprintln!("abort phase armed by cleared cache and owned native Cargo waiter");
            }
            Ok(ready)
        };
        // The existing timed runner terminates/reaps its owned process group
        // before returning. Parent path ownership outlives that return.
        let result = child_after_readiness(
            &aborted,
            &gate,
            crate::run::ReadinessDeadline::new(
                Duration::from_secs(20),
                Duration::from_secs(5),
                &mut probe,
            ),
        )?;
        assert!(released, "must release the observed child's startup gate");
        assert!(
            ready_at.is_some_and(|ready| ready.elapsed() >= Duration::from_secs(5)),
            "startup may not consume the five-second owned-waiter abort phase"
        );
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

#[cfg(target_os = "linux")]
#[test]
fn fixture_cache_absent_readiness_reaps_gated_child_before_owned_cleanup() -> Result<(), String> {
    let warm = Case::new("absent-readiness-warm")?;
    assert_child(&child(&warm, crate::run::tool_build_timeout()?)?);
    let facts = snapshot(&warm.cache)?;
    let output = snapshot(&warm.output)?;
    let paths = {
        let case = Case::new("absent-readiness")?;
        let stale = case.seed_stale()?;
        let paths = case.owned.clone();
        let marker = child_pid_path(&case)?;
        let gate = marker.with_file_name("unreleased-start-gate");
        let mut gated_child_observed = false;
        let mut probe = || -> Result<bool, String> {
            if marker.try_exists().map_err(|e| e.to_string())? {
                gated_child_observed = true;
            }
            Ok(false)
        };
        let error = match child_after_readiness(
            &case,
            &gate,
            crate::run::ReadinessDeadline::new(
                Duration::from_secs(20),
                Duration::from_secs(5),
                &mut probe,
            ),
        ) {
            Err(error) => error,
            Ok(output) => {
                return Err(format!(
                    "absent readiness unexpectedly completed: {}",
                    describe(&output)
                ));
            }
        };
        assert!(
            gated_child_observed,
            "must observe actual adapter at its unreleased gate"
        );
        assert!(
            error.contains("readiness not observed within startup budget"),
            "{error}"
        );
        assert!(
            error.contains("reaped status=") && error.contains("timed_out=true"),
            "{error}"
        );
        assert!(
            stale.exists(),
            "gated child may not reach the real runner clear"
        );
        assert!(snapshot(&case.output)?.is_empty());
        assert_eq!(snapshot(&warm.cache)?, facts);
        assert_eq!(snapshot(&warm.output)?, output);
        assert_child(&child(&case, crate::run::tool_build_timeout()?)?);
        assert!(!snapshot(&case.cache)?.is_empty());
        paths
    };
    assert!(paths.iter().all(|p| !p.exists()));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn fixture_cache_wrong_waiter_owner_fails_closed_then_recovers() -> Result<(), String> {
    let warm = Case::new("wrong-owner-warm")?;
    assert_child(&child(&warm, crate::run::tool_build_timeout()?)?);
    let facts = snapshot(&warm.cache)?;
    let output = snapshot(&warm.output)?;
    let paths = {
        let case = Case::new("wrong-waiter-owner")?;
        let stale = case.seed_stale()?;
        let paths = case.owned.clone();
        let binary = repo_root()?.join(crate::ripr_debug_binary());
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(
                binary
                    .parent()
                    .ok_or("binary needs parent")?
                    .join(".cargo-lock"),
            )
            .map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        let key = held_lock_key(&lock)?;
        let marker = child_pid_path(&case)?;
        let actual_marker = marker.with_file_name("actual-child.pid");
        let gate = marker.with_file_name("start-gate");
        let mut replaced = false;
        let mut actual_waiter_observed = false;
        let mut probe = || -> Result<bool, String> {
            if !replaced {
                match fs::read(&marker) {
                    Ok(pid) => fs::write(&actual_marker, pid).map_err(|e| e.to_string())?,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
                    Err(e) => return Err(e.to_string()),
                }
                // The adapter published atomically before waiting on this
                // gate, so it cannot subsequently overwrite the wrong owner.
                fs::write(&marker, std::process::id().to_string()).map_err(|e| e.to_string())?;
                fs::write(&gate, b"start").map_err(|e| e.to_string())?;
                replaced = true;
            }
            if !stale.exists() && owned_cargo_waiter(&key, &actual_marker)? {
                actual_waiter_observed = true;
                if owned_cargo_waiter(&key, &marker)? {
                    return Ok(true);
                }
                return Err("actual native Cargo waiter rejected for wrong process owner".into());
            }
            Ok(false)
        };
        let error = match child_after_readiness(
            &case,
            &gate,
            crate::run::ReadinessDeadline::new(
                Duration::from_secs(20),
                Duration::from_millis(100),
                &mut probe,
            ),
        ) {
            Err(error) => error,
            Ok(output) => {
                return Err(format!(
                    "wrong waiter ownership unexpectedly completed: {}",
                    describe(&output)
                ));
            }
        };
        assert!(
            actual_waiter_observed,
            "must independently observe the real owned waiter"
        );
        assert!(error.contains("readiness observation failed: actual native Cargo waiter rejected for wrong process owner"), "{error}");
        assert!(
            error.contains("reaped status=") && error.contains("timed_out=true"),
            "{error}"
        );
        assert!(
            error.contains("Blocking waiting for file lock on artifact directory"),
            "{error}"
        );
        assert!(
            !owned_cargo_waiter(&key, &actual_marker)?,
            "reaped capture group may not retain Cargo waiter"
        );
        assert!(snapshot(&case.output)?.is_empty());
        assert_eq!(snapshot(&warm.cache)?, facts);
        assert_eq!(snapshot(&warm.output)?, output);
        lock.unlock().map_err(|e| e.to_string())?;
        assert_child(&child(&case, crate::run::tool_build_timeout()?)?);
        assert!(!snapshot(&case.cache)?.is_empty());
        paths
    };
    assert!(paths.iter().all(|p| !p.exists()));
    Ok(())
}
