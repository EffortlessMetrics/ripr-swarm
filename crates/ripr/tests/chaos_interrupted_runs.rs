//! Built-binary chaos runs: terminated, corrupted, unwritable, full and concurrent
//! environments must never change the analysis result, leave a cache entry or
//! temporary file that a later run trusts, or turn a write failure into a
//! clean-looking exit.
//!
//! Every scenario compares against one cold reference run on the same
//! fixture, so a drift in result bytes, not just a crash, fails the test.
//!
//! The fixture's `ripr check` writes only the file-facts cache layer; the
//! seam, compact-classified, fingerprint and sharded layers are not
//! exercised here (#6663 tracks widening this).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const CACHE_DIR_ENV: &str = "RIPR_CACHE_DIR";
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

const LIB_RS: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n}\n";
const TEST_RS: &str =
    "use fx::discount;\n#[test]\nfn small() {\n    assert!(discount(5, 2) > 0);\n}\n";
const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const DIFF_GE_12: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 12 { price * qty * 9 / 10 } else { price * qty }\n }\n";
const DIFF_GE_11: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 11 { price * qty * 9 / 10 } else { price * qty }\n }\n";

type Corruption = fn(&Path) -> std::io::Result<()>;

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    cache: PathBuf,
    preserve_on_drop: bool,
}

impl Fixture {
    fn new(label: &str) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let base = std::env::temp_dir().join(format!(
            "ripr-chaos-{label}-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        let root = base.join("fx");
        fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        fs::create_dir_all(root.join("tests")).map_err(|error| error.to_string())?;
        write(&root.join("Cargo.toml"), MANIFEST)?;
        write(&root.join("src/lib.rs"), LIB_RS)?;
        write(&root.join("tests/discount.rs"), TEST_RS)?;
        write(&base.join("ge12.diff"), DIFF_GE_12)?;
        write(&base.join("ge11.diff"), DIFF_GE_11)?;
        Ok(Self {
            cache: base.join("cache"),
            base,
            root,
            preserve_on_drop: false,
        })
    }

    fn command(&self, diff: &str, extra: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .args(["check", "--root"])
            .arg(&self.root)
            .arg("--diff")
            .arg(self.base.join(diff))
            .args(["--format", "json"])
            .args(extra)
            .env(CACHE_DIR_ENV, &self.cache);
        command
    }

    fn run(&self, diff: &str, extra: &[&str]) -> Result<Output, String> {
        self.command(diff, extra)
            .output()
            .map_err(|error| format!("run ripr check: {error}"))
    }

    /// Run to completion, require success, and return the result bytes with
    /// this fixture's temporary directory masked, so results from different
    /// fixtures are comparable.
    fn result(&self, diff: &str) -> Result<Vec<u8>, String> {
        let output = self.run(diff, &[])?;
        if !output.status.success() {
            return Err(format!(
                "ripr check failed: {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let text = String::from_utf8(output.stdout)
            .map_err(|error| format!("result is not UTF-8: {error}"))?;
        // The result is JSON, so mask the fixture path as the serializer
        // spells it (escaped backslashes on Windows, escaped quotes or
        // control characters anywhere), then the portable forward-slash
        // spelling normalized emitters use (`root` and other `display_path`
        // fields), then any raw spelling.
        let base = self.base.display().to_string();
        let escaped = serde_json::to_string(&base).map_err(|error| error.to_string())?;
        let escaped = escaped
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .unwrap_or(&escaped);
        let portable = base.replace('\\', "/");
        Ok(text
            .replace(escaped, "<fixture>")
            .replace(&portable, "<fixture>")
            .replace(&base, "<fixture>")
            .into_bytes())
    }

    fn cache_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![self.cache.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => stack.push(path),
                    Ok(_) => files.push(path),
                    Err(_) => {}
                }
            }
        }
        files.sort();
        files
    }

    /// Every cache file with its bytes, so a later run that rewrites or
    /// adds an entry is visible.
    fn cache_snapshot(&self) -> Result<Vec<(PathBuf, Vec<u8>)>, String> {
        self.cache_files()
            .into_iter()
            .map(|path| {
                let bytes =
                    fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
                Ok((path, bytes))
            })
            .collect()
    }

    /// Temporary files an interrupted atomic write could strand.
    fn stranded_temp_files(&self) -> Vec<PathBuf> {
        let mut files = self.cache_files();
        files.retain(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".tmp") || name.starts_with(".ripr-atomic"))
        });
        files
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if self.preserve_on_drop {
            eprintln!("retained failed lifecycle proof: {}", self.base.display());
            return;
        }
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

fn same_result(label: &str, expected: &[u8], actual: &[u8]) -> Result<(), String> {
    if expected == actual {
        return Ok(());
    }
    Err(format!(
        "{label}: result differs from the cold reference run ({} vs {} bytes)",
        expected.len(),
        actual.len()
    ))
}

#[test]
fn corrupt_cache_entries_never_change_the_result() -> Result<(), String> {
    let fixture = Fixture::new("corrupt")?;
    let cold = fixture.result("ge12.diff")?;
    same_result("warm", &cold, &fixture.result("ge12.diff")?)?;
    let entries = fixture.cache_files();
    assert!(
        !entries.is_empty(),
        "fixture must populate the cache before corruption is meaningful"
    );

    let corruptions: [(&str, Corruption); 4] = [
        ("truncated", |path| {
            let bytes = fs::read(path)?;
            fs::write(path, &bytes[..bytes.len() / 2])
        }),
        ("garbage", |path| fs::write(path, b"{\"schema\":")),
        ("empty", |path| fs::write(path, b"")),
        ("non-utf8", |path| fs::write(path, [0xff, 0xfe, 0x00, 0x80])),
    ];
    for (label, corrupt) in corruptions {
        // Re-warm so every mode starts from the same intact entries.
        fixture.result("ge12.diff")?;
        for path in fixture.cache_files() {
            corrupt(&path).map_err(|error| format!("{label}: {}: {error}", path.display()))?;
        }
        same_result(label, &cold, &fixture.result("ge12.diff")?)?;
        // The run after recovery must also agree: a repaired entry is not
        // allowed to differ from the one it replaced.
        same_result(label, &cold, &fixture.result("ge12.diff")?)?;
    }
    Ok(())
}

#[test]
fn directory_in_place_of_a_cache_entry_never_changes_the_result() -> Result<(), String> {
    let fixture = Fixture::new("dir-entry")?;
    let cold = fixture.result("ge12.diff")?;
    for path in fixture.cache_files() {
        fs::remove_file(&path).map_err(|error| error.to_string())?;
        fs::create_dir(&path).map_err(|error| error.to_string())?;
    }
    same_result("directory entries", &cold, &fixture.result("ge12.diff")?)
}

#[test]
fn unusable_cache_location_never_changes_the_result() -> Result<(), String> {
    let reference = Fixture::new("unusable-ref")?;
    let cold = reference.result("ge12.diff")?;

    // The cache base is a regular file, so no entry can ever be created
    // beneath it, on any platform and for any user.
    let blocked = Fixture::new("unusable")?;
    write(&blocked.cache, "not a directory")?;
    same_result("cache base is a file", &cold, &blocked.result("ge12.diff")?)?;
    // Failing to cache must not damage the blocker itself.
    assert_eq!(
        fs::read_to_string(&blocked.cache).map_err(|error| error.to_string())?,
        "not a directory"
    );
    Ok(())
}

#[test]
fn terminated_runs_leave_a_cache_every_later_run_agrees_with() -> Result<(), String> {
    let fixture = Fixture::new("terminated")?;
    let reference = Fixture::new("terminated-ref")?;
    let cold = reference.result("ge12.diff")?;

    let mut interrupted = 0usize;
    for delay_micros in [0u64, 300, 1_000, 3_000, 6_000, 10_000, 20_000, 40_000] {
        let _ = fs::remove_dir_all(&fixture.cache);
        let mut child = fixture
            .command("ge12.diff", &[])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("spawn ripr check: {error}"))?;
        std::thread::sleep(Duration::from_micros(delay_micros));
        // The child may already have exited; a failed kill is not an error.
        let _ = child.kill();
        let status = child.wait().map_err(|error| error.to_string())?;
        if !status.success() {
            interrupted += 1;
        }

        let label = format!("terminated after {delay_micros}us");
        // A kill between temp-file creation and rename can strand a
        // `.ripr-atomic-*.tmp` file; the #6343 sweep removes only ones older
        // than 10 minutes, so a fresh strand survives the rerun. The
        // guarantee checked here is the one readers rely on: such a file is
        // never taken for an entry, so later results still match.
        same_result(&label, &cold, &fixture.result("ge12.diff")?)?;
        same_result(
            &format!("{label}, second rerun"),
            &cold,
            &fixture.result("ge12.diff")?,
        )?;
    }
    assert!(
        interrupted > 0,
        "no run was interrupted before it exited; the sweep proved nothing"
    );
    Ok(())
}

#[test]
fn interrupted_artifact_write_keeps_the_previous_artifact_or_a_complete_new_one()
-> Result<(), String> {
    let fixture = Fixture::new("terminated-artifact")?;
    let artifact = fixture.base.join("check-artifact.json");
    let artifact_arg = artifact.display().to_string();

    let old = fixture.run("ge12.diff", &["--write-artifact", &artifact_arg])?;
    assert!(old.status.success(), "seed artifact write failed");
    let old_bytes = fs::read(&artifact).map_err(|error| error.to_string())?;

    // The complete new artifact, produced once without interruption by the
    // same fixture, root and artifact path the interrupted runs use: the
    // artifact records the canonical root and diff path, so bytes from any
    // other fixture could never match a run that finished before the kill.
    let new_run = fixture.run("ge11.diff", &["--write-artifact", &artifact_arg])?;
    assert!(new_run.status.success(), "reference artifact write failed");
    let new_bytes = fs::read(&artifact).map_err(|error| error.to_string())?;
    assert_ne!(
        old_bytes, new_bytes,
        "fixture must produce distinguishable artifacts"
    );
    fs::write(&artifact, &old_bytes).map_err(|error| error.to_string())?;

    let mut interrupted = 0usize;
    for delay_micros in [0u64, 500, 2_000, 5_000, 10_000, 20_000, 40_000] {
        let mut child = fixture
            .command("ge11.diff", &["--write-artifact", &artifact_arg])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("spawn ripr check: {error}"))?;
        std::thread::sleep(Duration::from_micros(delay_micros));
        let _ = child.kill();
        let status = child.wait().map_err(|error| error.to_string())?;
        if !status.success() {
            interrupted += 1;
        }

        let observed = fs::read(&artifact).map_err(|error| error.to_string())?;
        assert!(
            observed == old_bytes || observed == new_bytes,
            "terminated after {delay_micros}us: artifact is neither the previous nor a complete new artifact ({} bytes)",
            observed.len()
        );
        // Restore the previous artifact so each delay races the same swap.
        fs::write(&artifact, &old_bytes).map_err(|error| error.to_string())?;
    }
    assert!(
        interrupted > 0,
        "no run was interrupted before it exited; the sweep proved nothing"
    );
    Ok(())
}

#[test]
fn concurrent_runs_on_one_cache_agree_with_the_cold_result() -> Result<(), String> {
    let reference = Fixture::new("concurrent-ref")?;
    let cold = reference.result("ge12.diff")?;
    let cold_other = reference.result("ge11.diff")?;

    for round in 0..4 {
        let fixture = Fixture::new("concurrent")?;
        // Half the runs analyze a different diff over the same cache so
        // writers race on overlapping and distinct entries.
        let results: Vec<(bool, Result<Vec<u8>, String>)> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|index| {
                    let other = index % 2 == 1;
                    let fixture = &fixture;
                    let handle = scope.spawn(move || {
                        fixture.result(if other { "ge11.diff" } else { "ge12.diff" })
                    });
                    (other, handle)
                })
                .collect();
            handles
                .into_iter()
                .map(|(other, handle)| {
                    let result = handle
                        .join()
                        .unwrap_or_else(|_| Err("run thread panicked".to_string()));
                    (other, result)
                })
                .collect()
        });
        for (other, result) in results {
            let bytes = result.map_err(|error| format!("round {round}: {error}"))?;
            same_result(
                &format!("round {round}"),
                if other { &cold_other } else { &cold },
                &bytes,
            )?;
        }
        let stranded = fixture.stranded_temp_files();
        assert!(
            stranded.is_empty(),
            "round {round}: stranded temporary files: {stranded:?}"
        );
        // And the cache the race left behind is sound as left: snapshot it
        // before any rerun, because a rerun silently repairs a malformed or
        // missing entry. Warm reruns of both diffs must agree with the cold
        // results and leave every entry byte-identical.
        let raced = fixture.cache_snapshot()?;
        assert!(!raced.is_empty(), "round {round}: the race left no cache");
        same_result(
            &format!("round {round}, post-race rerun"),
            &cold,
            &fixture.result("ge12.diff")?,
        )?;
        same_result(
            &format!("round {round}, post-race rerun of the other diff"),
            &cold_other,
            &fixture.result("ge11.diff")?,
        )?;
        let rerun = fixture.cache_snapshot()?;
        let mut changed: Vec<&PathBuf> = raced
            .iter()
            .filter(|entry| !rerun.contains(entry))
            .map(|(path, _)| path)
            .chain(
                rerun
                    .iter()
                    .filter(|entry| !raced.contains(entry))
                    .map(|(path, _)| path),
            )
            .collect();
        changed.sort();
        changed.dedup();
        assert!(
            changed.is_empty(),
            "round {round}: a warm rerun rewrote or added cache entries the race left: {changed:?}"
        );
    }
    Ok(())
}

#[test]
fn artifact_path_that_cannot_be_written_fails_loudly_and_names_the_artifact() -> Result<(), String>
{
    let fixture = Fixture::new("artifact-blocked")?;
    // The artifact's parent is a regular file, so the write cannot succeed.
    let blocker = fixture.base.join("blocker");
    write(&blocker, "file")?;
    let target = blocker.join("artifact.json");
    let target_arg = target.display().to_string();

    let output = fixture.run("ge12.diff", &["--write-artifact", &target_arg])?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "unwritable artifact path must not exit success"
    );
    assert!(
        stderr.contains("artifact"),
        "error must say which write failed: {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "write failure must be an error, not a panic: {stderr}"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn full_device_on_stdout_is_a_nonzero_exit_with_a_stated_cause() -> Result<(), String> {
    let fixture = Fixture::new("stdout-full")?;
    let full = fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .map_err(|error| format!("open /dev/full: {error}"))?;
    let output = fixture
        .command("ge12.diff", &[])
        .stdout(Stdio::from(full))
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| format!("run ripr check: {error}"))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "a result that could not be delivered must not exit success"
    );
    assert!(
        stderr.contains("write to stdout failed") && stderr.contains("No space left on device"),
        "error must say what happened: {stderr}"
    );
    Ok(())
}

#[test]
fn doctor_says_when_the_cache_cannot_be_written() -> Result<(), String> {
    let fixture = Fixture::new("doctor-cache")?;
    write(&fixture.cache, "not a directory")?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["doctor", "--root"])
        .arg(&fixture.root)
        .env(CACHE_DIR_ENV, &fixture.cache)
        .output()
        .map_err(|error| format!("run ripr doctor: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The cache is optional: an unwritable one is a warning, not a failure.
    assert!(
        output.status.success(),
        "doctor must not fail on an unwritable cache: {}\nstdout: {stdout}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("! Cache not writable") && stdout.contains("RIPR_CACHE_DIR"),
        "doctor must name the unwritable cache and the remedy: {stdout}"
    );
    Ok(())
}

// These controls qualify one single-process exporter's partial-packet
// disposition. They do not qualify Unix descendant containment, external
// SIGINT handling, or any cache-key/reuse policy.
#[cfg(all(target_os = "linux", feature = "lang-perl", feature = "lang-rust"))]
mod managed_producer {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::CommandExt;
    use std::time::Instant;

    const PARTIAL: &str = "{\"interrupted_export\":";
    // Empty identity-bearing arrays have the independent SHA-256(empty)
    // fingerprint. This packet carries no invented Perl analysis facts;
    // the real Rust adapter supplies the nonempty workload witness.
    const EMPTY_PACKET: &str = r#"{"schema_version":"ripr-perl-facts-v1","packet_id":"lifecycle-empty","packet_status":"complete","packet_fingerprint":"sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","producer":{"name":"perl-ripr-facts","version":"lifecycle-fixture","capabilities":["syntax","workspace","test_facts"]},"root":{"repo_relative":".","vcs_head":null,"path_style":"repo_relative"},"input":{"base":null,"head":"HEAD","diff_id":null,"requested_fact_classes":["owners","changes","tests","oracles"]},"files":[],"owners":[],"changes":[],"tests":[],"oracles":[],"relations":[],"dynamic_boundaries":[],"verify_commands":[],"limitations":[],"provenance":[]}"#;
    const EXPORTER: &str = r#"#!/bin/bash
set -eu
out=
while (( $# > 0 )); do
    if [[ $1 == --out ]]; then out=$2; shift; fi
    shift
done
[[ -n $out ]] || exit 64
printf '%s\n' "$out" > "${RIPR_LIFECYCLE_MARKER}.path"
case $RIPR_LIFECYCLE_MODE in
    healthy) printf '%s\n' "$RIPR_LIFECYCLE_PACKET" > "$out" ;;
    partial|nonzero)
        printf '%s' '{"interrupted_export":' > "$out"
        printf '%s' '{"interrupted_export":' > "${RIPR_LIFECYCLE_MARKER}.partial"
        ;;
    absent|directory) ;;
    *) exit 65 ;;
esac
printf '%s\n' "$BASHPID" > "${RIPR_LIFECYCLE_MARKER}.pid"
printf 'ready\n' > "${RIPR_LIFECYCLE_MARKER}.ready"
case $RIPR_LIFECYCLE_MODE in
    healthy) exit 0 ;;
    nonzero) exit 17 ;;
esac
# Builtins only: there is no descendant process to outlive this producer.
# The independent safety exit bounds a broken termination control.
SECONDS=0
while (( SECONDS < 8 )); do :; done
exit 66
"#;

    struct Attempt {
        output: Output,
        tmp: PathBuf,
        marker: PathBuf,
    }

    fn setup(label: &str) -> Result<Fixture, String> {
        let bash = fs::metadata("/bin/bash")
            .map_err(|error| format!("lifecycle fixture requires /bin/bash: {error}"))?;
        if !bash.is_file() || bash.permissions().mode() & 0o111 == 0 {
            return Err("lifecycle fixture requires executable /bin/bash".to_string());
        }
        let mut fixture = Fixture::new(label)?;
        fixture.preserve_on_drop = true;
        fs::create_dir(fixture.root.join("lib")).map_err(|error| error.to_string())?;
        write(
            &fixture.root.join("lib/Discount.pm"),
            "package Discount;\nsub discount { return 10; }\n1;\n",
        )?;
        write(
            &fixture.base.join("managed.diff"),
            &format!(
                "{DIFF_GE_12}diff --git a/lib/Discount.pm b/lib/Discount.pm\n--- a/lib/Discount.pm\n+++ b/lib/Discount.pm\n@@ -1,3 +1,3 @@\n package Discount;\n-sub discount {{ return 10; }}\n+sub discount {{ return 12; }}\n 1;\n"
            ),
        )?;
        let exporter = fixture.base.join("exporter");
        write(&exporter, EXPORTER)?;
        fs::set_permissions(&exporter, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        let quoted_exporter = serde_json::to_string(&exporter.display().to_string())
            .map_err(|error| error.to_string())?;
        write(
            &fixture.root.join("ripr.toml"),
            &format!(
                "[perl]\nproducer = \"perl-ripr-facts\"\nexecutable = {quoted_exporter}\ntimeout_ms = 1000\ncache_dir = \"managed-packets\"\n"
            ),
        )?;
        Ok(fixture)
    }

    fn marker_pid(marker: &Path) -> Result<Option<u32>, String> {
        match fs::read_to_string(marker.with_extension("ready")) {
            Ok(text) if text == "ready\n" => {}
            Ok(_) => return Ok(None),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("read exporter readiness marker: {error}")),
        }
        match fs::read_to_string(marker.with_extension("pid")) {
            Ok(text) => text
                .trim()
                .parse::<u32>()
                .map(Some)
                .map_err(|error| format!("invalid exporter PID marker: {error}")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("read exporter PID marker: {error}")),
        }
    }

    fn primary_exists(pid: u32) -> Result<bool, String> {
        if pid == 0 || pid == std::process::id() {
            return Err(format!("invalid exporter primary PID: {pid}"));
        }
        match fs::metadata(format!("/proc/{pid}")) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("observe owned exporter PID {pid}: {error}")),
        }
    }

    fn attempt(fixture: &Fixture, label: &str, mode: &str) -> Result<Attempt, String> {
        let marker = fixture.base.join(format!("attempt-{label}"));
        let exclusive = |extension: &str| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(marker.with_extension(extension))
                .map_err(|error| format!("create {label} {extension} capture: {error}"))
        };
        let mut command = fixture.command("managed.diff", &[]);
        command
            .env("RIPR_ALLOW_REPO_PERL_EXECUTABLE", "1")
            .env("RIPR_LIFECYCLE_MODE", mode)
            .env("RIPR_LIFECYCLE_MARKER", &marker)
            .env("RIPR_LIFECYCLE_PACKET", EMPTY_PACKET)
            .stdin(Stdio::null())
            .stdout(exclusive("stdout")?)
            .stderr(exclusive("stderr")?)
            .process_group(0);
        let mut child = ripr::process_owner::OwnedProcess::spawn(command)
            .map_err(|error| format!("spawn owned lifecycle check: {error}"))?;
        let start = Instant::now();
        let mut observed_live = false;
        let status = loop {
            if let Some(pid) = marker_pid(&marker)? {
                if primary_exists(pid)? {
                    match fs::read_to_string(format!("/proc/{pid}/task/{pid}/children")) {
                        Ok(children) if children.trim().is_empty() => observed_live = true,
                        Ok(children) => {
                            return Err(format!(
                                "single-process exporter spawned descendants: {children}"
                            ));
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => return Err(format!("observe exporter children: {error}")),
                    }
                }
            }
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                break status;
            }
            if start.elapsed() >= Duration::from_secs(20) {
                let cleanup = child.terminate_tree();
                return Err(format!(
                    "lifecycle check exceeded its independent 20s bound; primary cleanup: {cleanup:?}"
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        // The CLI has been reaped. Observe the independently marked exporter
        // before inspecting, moving or deleting any packet/artifact path.
        let pid = marker_pid(&marker)?.ok_or("exporter never reached its readiness marker")?;
        if primary_exists(pid)? {
            return Err(format!(
                "exporter primary {pid} still exists after CLI exit"
            ));
        }
        if matches!(mode, "partial" | "absent" | "directory") && !observed_live {
            return Err("exporter was never observed live before its timeout".to_string());
        }
        let tmp = PathBuf::from(
            fs::read_to_string(marker.with_extension("path"))
                .map_err(|error| error.to_string())?
                .trim_end(),
        );
        if tmp.parent() != Some(fixture.root.join("managed-packets").as_path())
            || tmp.extension().and_then(|value| value.to_str()) != Some("tmp")
        {
            return Err(format!(
                "exporter received an unexpected packet path: {tmp:?}"
            ));
        }
        let read_capture = |extension: &str| {
            let path = marker.with_extension(extension);
            if fs::metadata(&path)
                .map_err(|error| error.to_string())?
                .len()
                > 4 * 1024 * 1024
            {
                return Err(format!("lifecycle {extension} capture exceeds 4 MiB"));
            }
            fs::read(&path).map_err(|error| error.to_string())
        };
        Ok(Attempt {
            output: Output {
                status,
                stdout: read_capture("stdout")?,
                stderr: read_capture("stderr")?,
            },
            tmp,
            marker,
        })
    }

    fn report(output: &Output) -> Result<serde_json::Value, String> {
        if output.status.code() != Some(0) {
            return Err(format!(
                "other languages must continue after exporter failure: {}\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let report: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("CLI did not emit report JSON: {error}"))?;
        if !report["findings"]
            .as_array()
            .is_some_and(|findings| findings.iter().any(|finding| finding["language"] == "rust"))
        {
            return Err("real Rust analysis produced no Rust findings".to_string());
        }
        Ok(report)
    }

    fn degraded(attempt: &Attempt, expected_cause: &str) -> Result<(), String> {
        let report = report(&attempt.output)?;
        let outcome = &report["analysis_outcome"];
        if outcome["analysis_complete"] != false
            || outcome["outcome"]["kind"] != "partial_with_limitations"
            || !outcome["outcome"]["limitations"]
                .as_array()
                .is_some_and(|limits| {
                    limits
                        .iter()
                        .any(|limit| limit["kind"] == "producer_failure")
                })
            || !report["language_runs"].as_array().is_some_and(|runs| {
                runs.iter().any(|run| {
                    run["language"] == "perl"
                        && run["status"] == "failed"
                        && run["reason"]
                            .as_str()
                            .is_some_and(|reason| reason.contains(expected_cause))
                })
            })
        {
            return Err(format!(
                "exporter failure was not truthfully degraded: {report}"
            ));
        }
        let stderr = String::from_utf8_lossy(&attempt.output.stderr);
        if !stderr.contains(expected_cause) || stderr.contains("panicked") {
            return Err(format!("stderr lost the exporter failure: {stderr}"));
        }
        Ok(())
    }

    fn healthy(attempt: &Attempt) -> Result<(), String> {
        let report = report(&attempt.output)?;
        if report["analysis_outcome"]["analysis_complete"] != true
            || !report["language_runs"].as_array().is_some_and(|runs| {
                runs.iter()
                    .any(|run| run["language"] == "perl" && run["status"] == "ok")
            })
        {
            return Err(format!(
                "healthy exporter was not consumed successfully: {report}"
            ));
        }
        if attempt.tmp.exists() {
            return Err("healthy publication left its temporary packet".to_string());
        }
        Ok(())
    }

    #[test]
    fn managed_exporter_timeout_discards_partial_preserves_published_packet_and_restarts()
    -> Result<(), String> {
        let mut fixture = setup("managed-timeout")?;
        let cold = attempt(&fixture, "cold", "healthy")?;
        healthy(&cold)?;
        let packet = cold.tmp.with_extension("");
        let published = fs::read(&packet).map_err(|error| error.to_string())?;
        let source =
            fs::read(fixture.root.join("src/lib.rs")).map_err(|error| error.to_string())?;
        let failed = attempt(&fixture, "timeout", "partial")?;
        degraded(&failed, "timed out")?;
        if failed.tmp != cold.tmp {
            return Err("timeout changed the same-config packet/cache identity".to_string());
        }
        if fs::read(failed.marker.with_extension("partial")).map_err(|error| error.to_string())?
            != PARTIAL.as_bytes()
        {
            return Err("producer did not retain its original partial-write witness".to_string());
        }
        if fs::read(&packet).map_err(|error| error.to_string())? != published {
            return Err("timeout changed the previously published packet".to_string());
        }
        if failed.tmp.exists() {
            return Err(format!(
                "settled exporter timeout left unpublished partial packet: {}",
                failed.tmp.display()
            ));
        }
        let restarted = attempt(&fixture, "restart", "healthy")?;
        healthy(&restarted)?;
        if restarted.tmp != cold.tmp {
            return Err("restart changed the same-config packet/cache identity".to_string());
        }
        same_result(
            "managed timeout restart",
            &cold.output.stdout,
            &restarted.output.stdout,
        )?;
        if fs::read(fixture.root.join("src/lib.rs")).map_err(|error| error.to_string())? != source {
            return Err("timeout/restart changed the analyzed source".to_string());
        }
        fixture.preserve_on_drop = false;
        Ok(())
    }

    #[test]
    fn managed_exporter_timeout_without_a_partial_packet_is_a_cleanup_noop() -> Result<(), String> {
        let mut fixture = setup("managed-timeout-no-packet")?;
        let failed = attempt(&fixture, "timeout", "absent")?;
        degraded(&failed, "timed out")?;
        if failed.tmp.exists() || failed.tmp.with_extension("").exists() {
            return Err("no-write timeout manufactured a packet".to_string());
        }
        if String::from_utf8_lossy(&failed.output.stderr).contains("cleanup failed") {
            return Err("absent temporary packet was treated as a cleanup failure".to_string());
        }
        fixture.preserve_on_drop = false;
        Ok(())
    }

    #[test]
    fn managed_exporter_nonzero_exit_rejects_partial_and_preserves_published_packet()
    -> Result<(), String> {
        let mut fixture = setup("managed-nonzero")?;
        let cold = attempt(&fixture, "cold", "healthy")?;
        healthy(&cold)?;
        let packet = cold.tmp.with_extension("");
        let published = fs::read(&packet).map_err(|error| error.to_string())?;
        let failed = attempt(&fixture, "nonzero", "nonzero")?;
        degraded(&failed, "non-zero")?;
        if failed.tmp.exists() {
            return Err("nonzero exporter left its rejected partial packet".to_string());
        }
        if fs::read(&packet).map_err(|error| error.to_string())? != published {
            return Err("nonzero exporter changed the published packet".to_string());
        }
        fixture.preserve_on_drop = false;
        Ok(())
    }

    #[test]
    fn managed_exporter_timeout_retains_directory_evidence_and_reports_cleanup_failure()
    -> Result<(), String> {
        let mut fixture = setup("managed-timeout-directory")?;
        let cold = attempt(&fixture, "cold", "healthy")?;
        healthy(&cold)?;
        let packet = cold.tmp.with_extension("");
        let published = fs::read(&packet).map_err(|error| error.to_string())?;
        fs::create_dir(&cold.tmp).map_err(|error| error.to_string())?;
        let evidence = cold.tmp.join("unique-evidence");
        write(&evidence, "directory must survive partial-file cleanup")?;
        let failed = attempt(&fixture, "timeout", "directory")?;
        degraded(&failed, "timed out")?;
        if failed.tmp != cold.tmp
            || !failed.tmp.is_dir()
            || fs::read_to_string(&evidence).map_err(|error| error.to_string())?
                != "directory must survive partial-file cleanup"
            || fs::read(&packet).map_err(|error| error.to_string())? != published
        {
            return Err("cleanup destroyed directory evidence or the published packet".to_string());
        }
        let stderr = String::from_utf8_lossy(&failed.output.stderr);
        if !stderr.contains("partial")
            || !stderr.contains("packet")
            || !(stderr.contains("cleanup failed") || stderr.contains("failed to remove"))
            || !stderr.contains(&failed.tmp.display().to_string())
        {
            return Err(format!(
                "partial-packet cleanup failure was not reported: {stderr}"
            ));
        }
        // Preserve the original blocker as evidence while making a retry
        // possible; do not recursively delete it to manufacture recovery.
        fs::rename(
            &failed.tmp,
            fixture.base.join("retained-directory-evidence"),
        )
        .map_err(|error| error.to_string())?;
        let restarted = attempt(&fixture, "restart", "healthy")?;
        healthy(&restarted)?;
        same_result(
            "directory blocker recovery",
            &cold.output.stdout,
            &restarted.output.stdout,
        )?;
        fixture.preserve_on_drop = false;
        Ok(())
    }
}
