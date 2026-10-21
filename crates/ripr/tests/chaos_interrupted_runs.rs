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
/// The interrupted children are terminated at arbitrary points, including while an
/// instrumented process writes its coverage profile at exit. A terminated writer
/// leaves a truncated `.profraw` that makes `llvm-profdata merge` fail the whole
/// coverage run (#4978), so these children discard their profile. This keeps
/// these two tests from producing truncated profiles; other kill sites remain.
const TERMINATED_CHILD_PROFILE_ENV: &str = "LLVM_PROFILE_FILE";
const TERMINATED_CHILD_PROFILE: &str = "/dev/null";
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

const LIB_RS: &str = "pub fn discount(price: u32, qty: u32) -> u32 {\n    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n}\n";
const TEST_RS: &str =
    "use fx::discount;\n#[test]\nfn small() {\n    assert!(discount(5, 2) > 0);\n}\n";
const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
const DIFF_GE_12: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 12 { price * qty * 9 / 10 } else { price * qty }\n }\n";
const DIFF_GE_11: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1,3 +1,3 @@\n pub fn discount(price: u32, qty: u32) -> u32 {\n-    if qty >= 10 { price * qty * 9 / 10 } else { price * qty }\n+    if qty >= 11 { price * qty * 9 / 10 } else { price * qty }\n }\n";

type Corruption = fn(&Path) -> std::io::Result<()>;

// Exact product binary or this developer-owned test executable only. No
// repository-provided executable or command enters this construction site.
fn owned_test_command(executable: &Path) -> Command {
    Command::new(executable)
}

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
        let mut command = owned_test_command(Path::new(env!("CARGO_BIN_EXE_ripr")));
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
            eprintln!("retained lifecycle proof: {}", self.base.display());
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
            .env(TERMINATED_CHILD_PROFILE_ENV, TERMINATED_CHILD_PROFILE)
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
            .env(TERMINATED_CHILD_PROFILE_ENV, TERMINATED_CHILD_PROFILE)
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
    const SIDECAR: &str = "unique successful-export sidecar evidence\n";
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
    healthy|held) printf '%s\n' "$RIPR_LIFECYCLE_PACKET" > "$out" ;;
    sidecar)
        printf '%s\n' "$RIPR_LIFECYCLE_PACKET" > "$out"
        printf '%s' "$RIPR_LIFECYCLE_SIDECAR" > "${out%/*}/exporter-sidecar"
        ;;
    partial|nonzero|ignore)
        printf '%s' '{"interrupted_export":' > "$out"
        printf '%s' '{"interrupted_export":' > "${RIPR_LIFECYCLE_MARKER}.partial"
        ;;
    absent|directory) ;;
    *) exit 65 ;;
esac
[[ $RIPR_LIFECYCLE_MODE != ignore ]] || trap '' INT
printf '%s\n' "$BASHPID" > "${RIPR_LIFECYCLE_MARKER}.pid"
printf 'ready\n' > "${RIPR_LIFECYCLE_MARKER}.ready"
case $RIPR_LIFECYCLE_MODE in
    healthy|sidecar) exit 0 ;;
    nonzero) exit 17 ;;
esac
# Builtins only: there is no descendant process to outlive this producer.
# The independent safety exit bounds a broken termination control.
SECONDS=0
while (( SECONDS < 8 )); do
    if [[ $RIPR_LIFECYCLE_MODE == held && -f ${RIPR_LIFECYCLE_MARKER}.release ]]; then
        exit 0
    fi
done
exit 66
"#;

    struct Attempt {
        output: Output,
        tmp: PathBuf,
        marker: PathBuf,
    }

    struct RunningAttempt {
        child: ripr::process_owner::OwnedProcess,
        marker: PathBuf,
        mode: String,
        started: Instant,
        observed_live: bool,
        planted_directory: bool,
        settled: bool,
    }

    fn setup(label: &str) -> Result<Fixture, String> {
        let bash = fs::metadata("/bin/bash")
            .map_err(|error| format!("lifecycle fixture requires /bin/bash: {error}"))?;
        if !bash.is_file() || bash.permissions().mode() & 0o111 == 0 {
            return Err("lifecycle fixture requires executable /bin/bash".to_string());
        }
        let mut fixture = Fixture::new(label)?;
        fixture.preserve_on_drop = true;
        write(
            &fixture.root.join("src/lib.rs"),
            &LIB_RS.replace("qty >= 10", "qty >= 12"),
        )?;
        fs::create_dir(fixture.root.join("lib")).map_err(|error| error.to_string())?;
        write(
            &fixture.root.join("lib/Discount.pm"),
            "package Discount;\nsub discount { return 12; }\n1;\n",
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

    fn temporary_path(fixture: &Fixture, marker: &Path) -> Result<PathBuf, String> {
        let reported = PathBuf::from(
            fs::read_to_string(marker.with_extension("path"))
                .map_err(|error| error.to_string())?
                .trim_end(),
        );
        // The exporter runs in the fixture root and is handed a root-relative
        // output path, so a relative report names a file under that root; `join` keeps an
        // absolute report as is.
        let tmp = fixture.root.join(reported);
        let cache = fixture.root.join("managed-packets");
        let parent = tmp.parent().ok_or("packet temporary path has no parent")?;
        // Accept the old shared location so baseline/wrong-implementation
        // controls reach the behavior assertion rather than fail at setup.
        let confined = parent == cache
            || (parent.parent() == Some(cache.as_path())
                && parent
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(".ripr-perl-attempt-")));
        if !confined || tmp.extension().and_then(|value| value.to_str()) != Some("tmp") {
            return Err(format!(
                "exporter received an unexpected packet path: {tmp:?}"
            ));
        }
        Ok(tmp)
    }

    fn published_path(fixture: &Fixture, tmp: &Path) -> Result<PathBuf, String> {
        let name = tmp.file_stem().ok_or("packet temporary path has no leaf")?;
        Ok(fixture.root.join("managed-packets").join(name))
    }

    fn start_attempt(fixture: &Fixture, label: &str, mode: &str) -> Result<RunningAttempt, String> {
        start_attempt_with_stderr(fixture, label, mode, None)
    }

    fn start_attempt_with_stderr(
        fixture: &Fixture,
        label: &str,
        mode: &str,
        stderr: Option<Stdio>,
    ) -> Result<RunningAttempt, String> {
        let marker = fixture.base.join(format!("attempt-{label}"));
        let exclusive = |extension: &str| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(marker.with_extension(extension))
                .map_err(|error| format!("create {label} {extension} capture: {error}"))
        };
        let mut command = fixture.command("managed.diff", &[]);
        if stderr.is_some() {
            command.arg("--quiet");
        }
        let packet = if mode == "sidecar" {
            EMPTY_PACKET.replace("lifecycle-empty", "lifecycle-sidecar")
        } else {
            EMPTY_PACKET.to_string()
        };
        command
            .env("RIPR_ALLOW_REPO_PERL_EXECUTABLE", "1")
            .env("RIPR_LIFECYCLE_MODE", mode)
            .env("RIPR_LIFECYCLE_MARKER", &marker)
            .env("RIPR_LIFECYCLE_PACKET", packet)
            .env("RIPR_LIFECYCLE_SIDECAR", SIDECAR)
            .stdin(Stdio::null())
            .stdout(exclusive("stdout")?)
            .stderr(stderr.unwrap_or(Stdio::from(exclusive("stderr")?)))
            .process_group(0);
        let child = ripr::process_owner::OwnedProcess::spawn(command)
            .map_err(|error| format!("spawn owned lifecycle check: {error}"))?;
        Ok(RunningAttempt {
            child,
            marker,
            mode: mode.to_string(),
            started: Instant::now(),
            observed_live: false,
            planted_directory: false,
            settled: false,
        })
    }

    impl RunningAttempt {
        fn observe_live(&mut self, fixture: &Fixture) -> Result<Option<u32>, String> {
            let Some(pid) = marker_pid(&self.marker)? else {
                return Ok(None);
            };
            if !primary_exists(pid)? {
                return Ok(None);
            }
            self.observed_live = true;
            if self.mode == "directory" && !self.planted_directory {
                let tmp = temporary_path(fixture, &self.marker)?;
                fs::create_dir(&tmp).map_err(|error| format!("plant packet blocker: {error}"))?;
                write(
                    &tmp.join("unique-evidence"),
                    "directory must survive partial-file cleanup",
                )?;
                self.planted_directory = true;
            }
            Ok(Some(pid))
        }

        fn wait_ready(&mut self, fixture: &Fixture) -> Result<(u32, PathBuf), String> {
            loop {
                if let Some(pid) = self.observe_live(fixture)? {
                    return Ok((pid, temporary_path(fixture, &self.marker)?));
                }
                if self
                    .child
                    .try_wait()
                    .map_err(|error| error.to_string())?
                    .is_some()
                {
                    return Err("CLI exited before its exporter was observed live".to_string());
                }
                if self.started.elapsed() >= Duration::from_secs(20) {
                    return Err("exporter readiness exceeded the independent 20s bound".to_string());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn release(&self) -> Result<(), String> {
            write(&self.marker.with_extension("release"), "release\n")
        }

        fn finish(&mut self, fixture: &Fixture) -> Result<Attempt, String> {
            let status = loop {
                self.observe_live(fixture)?;
                if let Some(status) = self.child.try_wait().map_err(|error| error.to_string())? {
                    break status;
                }
                if self.started.elapsed() >= Duration::from_secs(20) {
                    let cleanup = self.child.terminate_tree();
                    return Err(format!(
                        "lifecycle check exceeded its independent 20s bound; primary cleanup: {cleanup:?}"
                    ));
                }
                std::thread::sleep(Duration::from_millis(2));
            };
            // The CLI has been reaped. Observe the independently marked
            // exporter before inspecting or disposing of packet artifacts.
            let pid =
                marker_pid(&self.marker)?.ok_or("exporter never reached its readiness marker")?;
            if primary_exists(pid)? {
                return Err(format!(
                    "exporter primary {pid} still exists after CLI exit"
                ));
            }
            self.settled = true;
            if matches!(
                self.mode.as_str(),
                "partial" | "absent" | "directory" | "held"
            ) && !self.observed_live
            {
                return Err(
                    "exporter was never observed live before its timeout/release".to_string(),
                );
            }
            let tmp = temporary_path(fixture, &self.marker)?;
            let read_capture = |extension: &str| {
                let path = self.marker.with_extension(extension);
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
                marker: self.marker.clone(),
            })
        }
    }

    impl Drop for RunningAttempt {
        fn drop(&mut self) {
            if self.settled {
                return;
            }
            // Release the healthy sibling even when an overlap assertion
            // failed, then observe/reap both owned processes before the
            // fixture's failure-preservation guard can leave its scratch.
            if self.mode == "held"
                && let Err(error) = self.release()
            {
                eprintln!("failed to release owned exporter gate: {error}");
            }
            loop {
                match self.child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) if self.started.elapsed() < Duration::from_secs(20) => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    observation => {
                        let cleanup = self.child.terminate_tree();
                        eprintln!(
                            "lifecycle emergency observation {observation:?}; owned CLI cleanup: {cleanup:?}"
                        );
                        break;
                    }
                }
            }
            let settlement_started = Instant::now();
            loop {
                match marker_pid(&self.marker).and_then(|pid| pid.map(primary_exists).transpose()) {
                    Ok(Some(false)) => break,
                    Ok(Some(true)) if settlement_started.elapsed() < Duration::from_secs(8) => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    observation => {
                        eprintln!(
                            "exporter emergency settlement not established: {observation:?}; retained marker {}",
                            self.marker.display()
                        );
                        break;
                    }
                }
            }
        }
    }

    fn attempt(fixture: &Fixture, label: &str, mode: &str) -> Result<Attempt, String> {
        start_attempt(fixture, label, mode)?.finish(fixture)
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
            || !report["preview_languages"].as_array().is_some_and(|runs| {
                runs.iter()
                    .any(|run| run["language"] == "perl" && run["analyzed"] == true)
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

    fn staging_removed(fixture: &Fixture, tmp: &Path) -> Result<(), String> {
        let parent = tmp.parent().ok_or("packet temporary path has no parent")?;
        if parent != fixture.root.join("managed-packets") && parent.exists() {
            return Err(format!(
                "settled attempt left its empty staging directory: {}",
                parent.display()
            ));
        }
        Ok(())
    }

    #[test]
    fn managed_exporter_timeout_discards_partial_preserves_published_packet_and_restarts()
    -> Result<(), String> {
        let mut fixture = setup("managed-timeout")?;
        let cold = attempt(&fixture, "cold", "healthy")?;
        healthy(&cold)?;
        let packet = published_path(&fixture, &cold.tmp)?;
        let published = fs::read(&packet).map_err(|error| error.to_string())?;
        let source =
            fs::read(fixture.root.join("src/lib.rs")).map_err(|error| error.to_string())?;
        let failed = attempt(&fixture, "timeout", "partial")?;
        degraded(&failed, "timed out")?;
        if published_path(&fixture, &failed.tmp)? != packet {
            return Err("timeout changed the same-config final packet/cache identity".to_string());
        }
        if failed.tmp == cold.tmp {
            return Err("separate exporter attempts shared a temporary packet path".to_string());
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
        staging_removed(&fixture, &failed.tmp)?;
        let restarted = attempt(&fixture, "restart", "healthy")?;
        healthy(&restarted)?;
        if published_path(&fixture, &restarted.tmp)? != packet {
            return Err("restart changed the same-config final packet/cache identity".to_string());
        }
        if restarted.tmp == cold.tmp || restarted.tmp == failed.tmp {
            return Err("restart reused another attempt's temporary packet path".to_string());
        }
        staging_removed(&fixture, &cold.tmp)?;
        staging_removed(&fixture, &restarted.tmp)?;
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
        if failed.tmp.exists() || published_path(&fixture, &failed.tmp)?.exists() {
            return Err("no-write timeout manufactured a packet".to_string());
        }
        staging_removed(&fixture, &failed.tmp)?;
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
        let packet = published_path(&fixture, &cold.tmp)?;
        let published = fs::read(&packet).map_err(|error| error.to_string())?;
        let failed = attempt(&fixture, "nonzero", "nonzero")?;
        degraded(&failed, "non-zero")?;
        if failed.tmp.exists() {
            return Err("nonzero exporter left its rejected partial packet".to_string());
        }
        if fs::read(&packet).map_err(|error| error.to_string())? != published {
            return Err("nonzero exporter changed the published packet".to_string());
        }
        staging_removed(&fixture, &failed.tmp)?;
        fixture.preserve_on_drop = false;
        Ok(())
    }

    #[test]
    fn managed_exporter_timeout_retains_directory_evidence_and_reports_cleanup_failure()
    -> Result<(), String> {
        let mut fixture = setup("managed-timeout-directory")?;
        let cold = attempt(&fixture, "cold", "healthy")?;
        healthy(&cold)?;
        let packet = published_path(&fixture, &cold.tmp)?;
        let published = fs::read(&packet).map_err(|error| error.to_string())?;
        let failed = attempt(&fixture, "timeout", "directory")?;
        degraded(&failed, "timed out")?;
        let evidence = failed.tmp.join("unique-evidence");
        if published_path(&fixture, &failed.tmp)? != packet
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
        // An independently staged retry succeeds with the blocker still in
        // place: no deletion or move is needed to manufacture recovery.
        let restarted = attempt(&fixture, "restart", "healthy")?;
        healthy(&restarted)?;
        if restarted.tmp == failed.tmp || published_path(&fixture, &restarted.tmp)? != packet {
            return Err(
                "retry reused blocked staging or changed the final packet identity".to_string(),
            );
        }
        same_result(
            "directory blocker recovery",
            &cold.output.stdout,
            &restarted.output.stdout,
        )?;
        if fs::read_to_string(&evidence).map_err(|error| error.to_string())?
            != "directory must survive partial-file cleanup"
        {
            return Err("healthy retry changed the retained blocker evidence".to_string());
        }
        // Keep this control's unique blocker witness for the proof owner.
        fixture.preserve_on_drop = true;
        Ok(())
    }

    #[test]
    fn managed_exporter_timeout_preserves_overlapping_live_sibling_packet() -> Result<(), String> {
        let mut fixture = setup("managed-overlap")?;
        let config = fixture.root.join("ripr.toml");
        let configured = fs::read_to_string(&config).map_err(|error| error.to_string())?;
        write(
            &config,
            &configured.replace("timeout_ms = 1000", "timeout_ms = 3000"),
        )?;
        let cold = attempt(&fixture, "cold", "healthy")?;
        healthy(&cold)?;
        let packet = published_path(&fixture, &cold.tmp)?;
        let published = fs::read(&packet).map_err(|error| error.to_string())?;

        let mut timed = start_attempt(&fixture, "timeout", "partial")?;
        let (_, timed_tmp) = timed.wait_ready(&fixture)?;
        std::thread::sleep(Duration::from_millis(1500));
        if timed
            .child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Err("first CLI finished before a sibling could overlap it".to_string());
        }
        let mut sibling = start_attempt(&fixture, "sibling", "held")?;
        let (sibling_pid, sibling_tmp) = sibling.wait_ready(&fixture)?;
        let sibling_bytes = fs::read(&sibling_tmp).map_err(|error| error.to_string())?;
        if sibling_bytes != format!("{EMPTY_PACKET}\n").as_bytes() {
            return Err("held sibling did not stage its independently valid packet".to_string());
        }

        // Every error in this block releases/reaps the sibling through its
        // guard before failure scratch can be retained or inspected later.
        let ownership = (|| {
            let failed = timed.finish(&fixture)?;
            degraded(&failed, "timed out")?;
            if timed_tmp == sibling_tmp {
                return Err(
                    "overlapping same-config exporters shared their temporary packet path"
                        .to_string(),
                );
            }
            if published_path(&fixture, &timed_tmp)? != packet
                || published_path(&fixture, &sibling_tmp)? != packet
            {
                return Err("attempt isolation changed the final packet/cache key".to_string());
            }
            if failed.tmp.exists() {
                return Err("timed-out owner retained its own partial packet".to_string());
            }
            staging_removed(&fixture, &failed.tmp)?;
            if !primary_exists(sibling_pid)?
                || sibling
                    .child
                    .try_wait()
                    .map_err(|error| error.to_string())?
                    .is_some()
            {
                return Err("sibling was not live after the other attempt timed out".to_string());
            }
            if fs::read(&sibling_tmp).map_err(|error| error.to_string())? != sibling_bytes {
                return Err(
                    "timeout cleanup deleted or changed the live sibling's packet".to_string(),
                );
            }
            if fs::read(&packet).map_err(|error| error.to_string())? != published {
                return Err(
                    "timeout or held sibling changed the previously published packet".to_string(),
                );
            }
            Ok(())
        })();
        // Release first, even if the ownership oracle failed. Finish both
        // process observations before returning that failure to the caller.
        let release = sibling.release();
        let sibling_result = sibling.finish(&fixture);
        let timed_settlement = if timed.settled {
            Ok(())
        } else {
            timed.finish(&fixture).map(|_| ())
        };
        ownership?;
        release?;
        timed_settlement?;
        let completed = sibling_result?;
        healthy(&completed)?;
        staging_removed(&fixture, &completed.tmp)?;
        same_result(
            "overlapping sibling recovery",
            &cold.output.stdout,
            &completed.output.stdout,
        )?;
        fixture.preserve_on_drop = false;
        Ok(())
    }

    #[test]
    fn managed_exporter_success_consumes_published_packet_when_sidecar_blocks_staging_cleanup()
    -> Result<(), String> {
        let mut fixture = setup("managed-success-sidecar")?;
        let cold = attempt(&fixture, "cold", "healthy")?;
        healthy(&cold)?;
        let packet = published_path(&fixture, &cold.tmp)?;
        let previous = fs::read(&packet).map_err(|error| error.to_string())?;
        let exported = attempt(&fixture, "sidecar", "sidecar")?;
        let expected_packet = format!(
            "{}\n",
            EMPTY_PACKET.replace("lifecycle-empty", "lifecycle-sidecar")
        );
        let published = fs::read(&packet).map_err(|error| error.to_string())?;
        if published != expected_packet.as_bytes() || published == previous {
            return Err(
                "successful sidecar exporter did not publish its distinguishable valid packet"
                    .to_string(),
            );
        }
        if exported.tmp.exists() {
            return Err("successful sidecar exporter retained its unpublished packet".to_string());
        }
        let staging = exported
            .tmp
            .parent()
            .ok_or("sidecar packet has no staging parent")?;
        let sidecar = staging.join("exporter-sidecar");
        if fs::read(&sidecar).map_err(|error| error.to_string())? != SIDECAR.as_bytes() {
            return Err(
                "successful publication changed or deleted unique sidecar evidence".to_string(),
            );
        }
        // Inspect publication and retained bytes before challenging the
        // rendered outcome: a cleanup error after commit must not report
        // producer failure while the new packet is already visible.
        healthy(&exported)?;
        let stderr = String::from_utf8_lossy(&exported.output.stderr);
        if !stderr.contains("published")
            || !stderr.contains("packet")
            || !stderr.contains(&packet.display().to_string())
            || !stderr.contains("staging cleanup failed")
            || !stderr.contains("retained")
            || !stderr.contains(&staging.display().to_string())
            || stderr.contains("Perl facts exporter failed:")
            || String::from_utf8_lossy(&exported.output.stdout).contains("\"producer_failure\"")
        {
            return Err(format!(
                "published packet/staging warning was not truthful: {stderr}"
            ));
        }

        let restarted = attempt(&fixture, "restart", "healthy")?;
        healthy(&restarted)?;
        if restarted.tmp == exported.tmp || published_path(&fixture, &restarted.tmp)? != packet {
            return Err(
                "healthy retry reused retained sidecar staging or changed final identity"
                    .to_string(),
            );
        }
        staging_removed(&fixture, &restarted.tmp)?;
        same_result(
            "successful sidecar retry",
            &cold.output.stdout,
            &restarted.output.stdout,
        )?;
        if fs::read(&sidecar).map_err(|error| error.to_string())? != SIDECAR.as_bytes() {
            return Err(
                "healthy retry changed retained successful-export sidecar evidence".to_string(),
            );
        }
        // Retain the unique sidecar witness for the proof owner on success.
        fixture.preserve_on_drop = true;
        Ok(())
    }
    // Linux observations distinguish stopped execution from application-owned
    // reaping. The test subreaper only settles failed-test witnesses, after
    // the assertion; it cannot supply the product's success observation.
    #[cfg(target_os = "linux")]
    mod interruption {
        use super::*;
        use nix::sys::signal::{Signal, kill, killpg, raise};
        use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
        use nix::unistd::Pid;
        use std::io::Read;
        use std::os::unix::process::ExitStatusExt;
        use std::sync::Mutex;

        static SUBREAPER: Mutex<()> = Mutex::new(());

        struct Reaper {
            prior: bool,
            producer: Option<Pid>,
        }

        impl Reaper {
            fn new() -> Result<Self, String> {
                let prior = nix::sys::prctl::get_child_subreaper().map_err(|e| e.to_string())?;
                nix::sys::prctl::set_child_subreaper(true).map_err(|e| e.to_string())?;
                Ok(Self {
                    prior,
                    producer: None,
                })
            }
        }

        impl Drop for Reaper {
            fn drop(&mut self) {
                if let Some(pid) = self.producer {
                    // StillAlive proves this exact PID is our adopted child.
                    // ECHILD means the CLI already reaped it; never signal an
                    // unowned PID merely because its integer was recorded.
                    match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
                        Ok(WaitStatus::StillAlive) => {
                            let _ = kill(pid, Signal::SIGKILL);
                            let _ = waitpid(pid, None);
                        }
                        Ok(_) | Err(nix::errno::Errno::ECHILD) => {}
                        Err(error) => eprintln!("owned failed-test witness reap: {error}"),
                    }
                }
                if let Err(error) = nix::sys::prctl::set_child_subreaper(self.prior) {
                    eprintln!("restore test subreaper: {error}");
                }
            }
        }

        fn pid(value: u32) -> Result<Pid, String> {
            i32::try_from(value)
                .map(Pid::from_raw)
                .map_err(|e| e.to_string())
        }

        fn normalized(fixture: &Fixture, bytes: &[u8]) -> Result<String, String> {
            String::from_utf8(bytes.to_vec())
                .map(|text| text.replace(&fixture.base.display().to_string(), "<fixture>"))
                .map_err(|e| e.to_string())
        }

        fn assert_sigint(mode: &str, cli_only: bool) -> Result<(), String> {
            let _serial = SUBREAPER.lock().map_err(|e| e.to_string())?;
            let mut reaper = Reaper::new()?;
            let fixture = setup(&format!("sigint-{mode}"))?;
            let config = fixture.root.join("ripr.toml");
            let text = fs::read_to_string(&config).map_err(|e| e.to_string())?;
            let long_timeout = text.replace("timeout_ms = 1000", "timeout_ms = 30000");
            if long_timeout == text || !long_timeout.contains("timeout_ms = 30000") {
                return Err("30s timeout setup did not take effect".to_string());
            }
            write(&config, &long_timeout)?;
            let seed = attempt(&fixture, "seed", "healthy")?;
            healthy(&seed)?;
            let published_path = published_path(&fixture, &seed.tmp)?;
            let published = fs::read(&published_path).map_err(|e| e.to_string())?;
            let source = fs::read(fixture.root.join("src/lib.rs")).map_err(|e| e.to_string())?;
            let prior = fixture
                .root
                .join("managed-packets/prior-staging/unique-evidence");
            fs::create_dir_all(prior.parent().ok_or("prior evidence parent")?)
                .map_err(|e| e.to_string())?;
            write(&prior, "prior user evidence")?;
            let mut running = start_attempt(&fixture, "interrupt", mode)?;
            let (producer, tmp) = running.wait_ready(&fixture)?;
            reaper.producer = Some(pid(producer)?);
            if mode != "directory"
                && fs::read(&tmp).map_err(|e| e.to_string())? != PARTIAL.as_bytes()
            {
                return Err("SIGINT setup did not produce real partial bytes".to_string());
            }
            if running
                .child
                .try_wait()
                .map_err(|e| e.to_string())?
                .is_some()
            {
                return Err("SIGINT setup CLI was not live".to_string());
            }
            if nix::unistd::getpgid(Some(pid(producer)?)).map_err(|e| e.to_string())?
                != pid(running.child.id())?
            {
                return Err("exporter escaped the owned CLI process group".to_string());
            }
            let signalled = Instant::now();
            if cli_only {
                // Leave the exporter live so only the CLI's cancellation
                // owner can stop and reap it. The existing group-signal
                // cases separately cover terminal-style SIGINT delivery.
                kill(pid(running.child.id())?, Signal::SIGINT).map_err(|e| e.to_string())?;
            } else {
                killpg(pid(running.child.id())?, Signal::SIGINT).map_err(|e| e.to_string())?;
            }
            let status = loop {
                if let Some(status) = running.child.try_wait().map_err(|e| e.to_string())? {
                    break status;
                }
                if signalled.elapsed() >= Duration::from_secs(5) {
                    return Err(
                        "first SIGINT exceeded 5s, below 30s timeout and 8s safety exit"
                            .to_string(),
                    );
                }
                std::thread::sleep(Duration::from_millis(2));
            };
            // Observe before any test-side waitpid or emergency cleanup.
            let producer_exists = primary_exists(producer)?;
            running.settled = true;
            let stdout =
                fs::read(running.marker.with_extension("stdout")).map_err(|e| e.to_string())?;
            let stderr = fs::read_to_string(running.marker.with_extension("stderr"))
                .map_err(|e| e.to_string())?;
            eprintln!(
                "SIGINT receipt mode={mode} target={} cli={} producer={producer} status={status:?} producer_exists={producer_exists} elapsed_ms={} tmp={} fixture={}",
                if cli_only { "cli" } else { "group" },
                running.child.id(),
                signalled.elapsed().as_millis(),
                tmp.display(),
                fixture.base.display()
            );
            if producer_exists {
                return Err(format!(
                    "producer {producer} still exists, including zombie, after CLI exit; application reaping absent"
                ));
            }
            if status.code() != Some(130) || !stdout.is_empty() {
                return Err(format!(
                    "interruption status/stdout dishonest: {status:?}, {} bytes",
                    stdout.len()
                ));
            }
            if !stderr.contains("check interrupted")
                || !stderr.contains("rerun the same command")
                || !stderr.contains("staging")
                || !stderr.contains("analysis cancelled: Cancelled")
                || stderr.contains("exited with status")
                || stderr.contains("timed out")
                || stderr.contains("other languages continue")
                || stderr.contains("non-zero")
            {
                return Err(format!(
                    "interruption recovery guidance/degradation: {stderr}"
                ));
            }
            if mode == "directory" {
                if !stderr.contains(&tmp.display().to_string())
                    || fs::read_to_string(tmp.join("unique-evidence")).map_err(|e| e.to_string())?
                        != "directory must survive partial-file cleanup"
                    || !stderr.contains("cleanup")
                {
                    return Err("cleanup blocker was not preserved and named".to_string());
                }
            } else {
                if tmp.exists() {
                    return Err("own interrupted partial was retained".to_string());
                }
                staging_removed(&fixture, &tmp)?;
            }
            if fs::read(&published_path).map_err(|e| e.to_string())? != published
                || fs::read_to_string(&prior).map_err(|e| e.to_string())? != "prior user evidence"
                || fs::read(fixture.root.join("src/lib.rs")).map_err(|e| e.to_string())? != source
            {
                return Err(
                    "interruption changed prior packet, evidence, or user source".to_string(),
                );
            }
            let repeat = attempt(&fixture, "repeat", "healthy")?;
            healthy(&repeat)?;
            if repeat.output.stdout != seed.output.stdout {
                return Err("repeat differs from same-root healthy seed".to_string());
            }
            if fs::read(&published_path).map_err(|e| e.to_string())? != published
                || fs::read_to_string(&prior).map_err(|e| e.to_string())? != "prior user evidence"
                || fs::read(fixture.root.join("src/lib.rs")).map_err(|e| e.to_string())? != source
            {
                return Err("healthy repeat changed prior packet, evidence, or source".to_string());
            }
            if mode == "directory"
                && fs::read_to_string(tmp.join("unique-evidence")).map_err(|e| e.to_string())?
                    != "directory must survive partial-file cleanup"
            {
                return Err("healthy repeat changed cleanup blocker evidence".to_string());
            }
            let cold = setup(&format!("sigint-cold-{mode}"))?;
            let cold_config = cold.root.join("ripr.toml");
            // Same trusted exporter and exact config; root/cache/output are
            // independently created, so the comparison cannot reuse packets.
            write(&cold_config, &long_timeout)?;
            let cold_output = attempt(&cold, "cold", "healthy")?;
            healthy(&cold_output)?;
            if normalized(&fixture, &repeat.output.stdout)?
                != normalized(&cold, &cold_output.output.stdout)?
            {
                return Err(
                    "healthy repeat differs from independent cold JSON after root normalization"
                        .to_string(),
                );
            }
            Ok(())
        }

        #[test]
        fn managed_sigint_reaps_partial_and_repeats() -> Result<(), String> {
            assert_sigint("partial", false)
        }
        #[test]
        fn managed_sigint_reaps_ignoring_exporter() -> Result<(), String> {
            assert_sigint("ignore", false)
        }
        #[test]
        fn managed_sigint_names_retained_directory() -> Result<(), String> {
            assert_sigint("directory", false)
        }
        #[test]
        fn cli_sigint_reaps_ignoring_exporter() -> Result<(), String> {
            assert_sigint("ignore", true)
        }
        #[test]
        fn cli_sigint_names_retained_directory() -> Result<(), String> {
            assert_sigint("directory", true)
        }

        /// Kernel wait channel of every thread of `pid`. A thread that exits
        /// between the directory listing and the read is skipped; an
        /// unreadable instrument is an error, not an empty observation.
        fn task_wchans(pid: u32) -> Result<Vec<String>, String> {
            let mut waits = Vec::new();
            for entry in fs::read_dir(format!("/proc/{pid}/task"))
                .map_err(|e| format!("observe Linux blocked-I/O instrument: {e}"))?
            {
                let entry = entry.map_err(|e| e.to_string())?;
                match fs::read_to_string(entry.path().join("wchan")) {
                    Ok(wait) => waits.push(wait),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("observe Linux blocked-I/O instrument: {e}")),
                }
            }
            if waits.is_empty() {
                return Err(
                    "observe Linux blocked-I/O instrument: no thread wchan readable".to_string(),
                );
            }
            Ok(waits)
        }

        fn wait_blocked(
            child: &mut ripr::process_owner::OwnedProcess,
            operation: &str,
        ) -> Result<(), String> {
            let started = Instant::now();
            loop {
                if child.try_wait().map_err(|e| e.to_string())?.is_some() {
                    return Err("CLI exited before blocked-I/O observation".to_string());
                }
                // The budgeted check runs on a worker thread (#8846), so the main
                // thread waits on a futex while a sibling thread holds the blocking
                // I/O. Observe every thread of the CLI, not only the leader.
                let wait = task_wchans(child.id())?;
                if wait.iter().any(|w| w.contains(operation)) {
                    return Ok(());
                }
                if started.elapsed() > Duration::from_secs(5) {
                    return Err(format!(
                        "CLI did not reach {operation}; last wchan per thread={wait:?}"
                    ));
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn native_interrupt(child: &mut ripr::process_owner::OwnedProcess) -> Result<(), String> {
            kill(pid(child.id())?, Signal::SIGINT).map_err(|e| e.to_string())?;
            let started = Instant::now();
            loop {
                if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                    if status.signal() != Some(2) {
                        return Err(format!("expected native first SIGINT, got {status:?}"));
                    }
                    return Ok(());
                }
                if started.elapsed() > Duration::from_secs(5) {
                    child.terminate_tree()?;
                    return Err("first SIGINT swallowed during blocking I/O".to_string());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }

        fn held_input(fifo: bool) -> Result<(), String> {
            let mut fixture = Fixture::new("native-input")?;
            fixture.preserve_on_drop = true;
            let input = fixture.base.join(if fifo { "held.fifo" } else { "-" });
            if fifo {
                nix::unistd::mkfifo(
                    &input,
                    nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
                )
                .map_err(|e| e.to_string())?;
            }
            let capture = fixture.base.join("native-stdout");
            let mut command = owned_test_command(Path::new(env!("CARGO_BIN_EXE_ripr")));
            command
                .args(["check", "--root"])
                .arg(&fixture.root)
                .arg("--diff")
                .arg(if fifo {
                    input.as_os_str()
                } else {
                    std::ffi::OsStr::new("-")
                })
                .args(["--format", "json"])
                .stdin(Stdio::piped())
                .stdout(fs::File::create(&capture).map_err(|e| e.to_string())?)
                .stderr(Stdio::null());
            let mut child =
                ripr::process_owner::OwnedProcess::spawn(command).map_err(|e| e.to_string())?;
            let _writer = child
                .stdin_pipe()
                .take()
                .ok_or("missing held stdin writer")?;
            wait_blocked(
                &mut child,
                if fifo {
                    "wait_for_partner"
                } else {
                    "pipe_read"
                },
            )?;
            native_interrupt(&mut child)?;
            if fs::metadata(&capture).map_err(|e| e.to_string())?.len() != 0
                || fs::read_to_string(fixture.root.join("src/lib.rs")).map_err(|e| e.to_string())?
                    != LIB_RS
            {
                return Err("native input interrupt changed output/source".to_string());
            }
            Ok(())
        }
        #[test]
        fn native_sigint_interrupts_held_stdin() -> Result<(), String> {
            held_input(false)
        }
        #[test]
        fn native_sigint_interrupts_unopened_fifo() -> Result<(), String> {
            held_input(true)
        }

        #[test]
        fn native_sigint_interrupts_unread_stdout() -> Result<(), String> {
            let mut fixture = Fixture::new("native-stdout")?;
            fixture.preserve_on_drop = true;
            let full = fixture.result("ge12.diff")?;
            let (read, writer) = nix::unistd::pipe().map_err(|e| e.to_string())?;
            let capacity = nix::fcntl::fcntl(&writer, nix::fcntl::FcntlArg::F_SETPIPE_SZ(4096))
                .map_err(|e| e.to_string())?;
            if full.len() <= usize::try_from(capacity).map_err(|e| e.to_string())? {
                return Err("stdout fixture does not exceed pipe capacity".to_string());
            }
            let mut command = fixture.command("ge12.diff", &[]);
            command
                .stdin(Stdio::null())
                .stdout(fs::File::from(writer))
                .stderr(Stdio::null());
            let mut child =
                ripr::process_owner::OwnedProcess::spawn(command).map_err(|e| e.to_string())?;
            wait_blocked(&mut child, "pipe_write")?;
            native_interrupt(&mut child)?;
            let mut partial = Vec::new();
            fs::File::from(read)
                .read_to_end(&mut partial)
                .map_err(|e| e.to_string())?;
            if partial.is_empty() || partial.len() >= full.len() {
                return Err("stdout was drained or never reached before interruption".to_string());
            }
            Ok(())
        }

        #[test]
        fn second_sigint_escapes_blocked_interruption_diagnostics() -> Result<(), String> {
            use std::io::Write;
            let _serial = SUBREAPER.lock().map_err(|e| e.to_string())?;
            let mut reaper = Reaper::new()?;
            let fixture = setup("second-sigint")?;
            let config = fixture.root.join("ripr.toml");
            let text = fs::read_to_string(&config).map_err(|e| e.to_string())?;
            write(
                &config,
                &text.replace("timeout_ms = 1000", "timeout_ms = 30000"),
            )?;
            let (reader, writer) = nix::unistd::pipe().map_err(|e| e.to_string())?;
            let capacity = nix::fcntl::fcntl(&writer, nix::fcntl::FcntlArg::F_SETPIPE_SZ(4096))
                .map_err(|e| e.to_string())?;
            let mut writer = fs::File::from(writer);
            writer
                .write_all(&vec![
                    b'x';
                    usize::try_from(capacity).map_err(|e| e.to_string())?
                ])
                .map_err(|e| e.to_string())?;
            let _unread = reader;
            let mut running =
                start_attempt_with_stderr(&fixture, "force", "partial", Some(Stdio::from(writer)))?;
            let (producer, tmp) = running.wait_ready(&fixture)?;
            reaper.producer = Some(pid(producer)?);
            if fs::read(&tmp).map_err(|e| e.to_string())? != PARTIAL.as_bytes() {
                return Err("force control missing partial witness".to_string());
            }
            if nix::unistd::getpgid(Some(pid(producer)?)).map_err(|e| e.to_string())?
                != pid(running.child.id())?
            {
                return Err("force exporter group mismatch".to_string());
            }
            killpg(pid(running.child.id())?, Signal::SIGINT).map_err(|e| e.to_string())?;
            wait_blocked(&mut running.child, "pipe_write")?;
            if primary_exists(producer)? {
                return Err(
                    "first SIGINT did not reap producer before diagnostic block".to_string()
                );
            }
            native_interrupt(&mut running.child)?;
            running.settled = true;
            if fs::metadata(running.marker.with_extension("stdout"))
                .map_err(|e| e.to_string())?
                .len()
                != 0
            {
                return Err("forced interruption emitted stdout".to_string());
            }
            Ok(())
        }

        #[test]
        fn library_cli_retains_native_signal_policy() -> Result<(), String> {
            const CHILD: &str = "RIPR_7618_LIBRARY_SIGNAL_CHILD";
            if let Ok(mode) = std::env::var(CHILD) {
                let observed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let handler = if mode == "caller" {
                    Some(
                        signal_hook::flag::register(signal_hook::consts::SIGINT, observed.clone())
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    None
                };
                let fixture = Fixture::new("library-signal-policy")?;
                ripr::cli::run(vec![
                    "ripr".to_string(),
                    "check".to_string(),
                    "--root".to_string(),
                    fixture.root.display().to_string(),
                    "--diff".to_string(),
                    fixture.base.join("ge12.diff").display().to_string(),
                    "--format".to_string(),
                    "json".to_string(),
                ])
                .map_err(|e| e.to_string())?;
                // Isolated wrong-policy control: unregistering a signal-hook
                // flag leaves an empty OS handler, unlike the default policy.
                if mode == "swallowed" {
                    let bad =
                        signal_hook::flag::register(signal_hook::consts::SIGINT, observed.clone())
                            .map_err(|e| e.to_string())?;
                    signal_hook::low_level::unregister(bad);
                }
                raise(Signal::SIGINT).map_err(|e| e.to_string())?;
                let Some(handler) = handler else {
                    return Err(
                        "default native SIGINT was swallowed after ordinary cli::run".to_string(),
                    );
                };
                signal_hook::low_level::unregister(handler);
                if !observed.load(Ordering::SeqCst) {
                    return Err("library CLI displaced the caller's signal policy".to_string());
                }
                return Ok(());
            }
            let test = "managed_producer::interruption::library_cli_retains_native_signal_policy";
            for mode in ["caller", "native"] {
                let executable = std::env::current_exe().map_err(|e| e.to_string())?;
                let mut command = owned_test_command(&executable);
                let fixture = Fixture::new("library-signal-controller")?;
                let capture = fixture.base.join("child.stderr");
                command
                    .args(["--exact", test, "--nocapture"])
                    .env(CHILD, mode)
                    .stdout(
                        fs::File::create(fixture.base.join("child.stdout"))
                            .map_err(|e| e.to_string())?,
                    )
                    .stderr(fs::File::create(&capture).map_err(|e| e.to_string())?);
                let mut child =
                    ripr::process_owner::OwnedProcess::spawn(command).map_err(|e| e.to_string())?;
                let started = Instant::now();
                let status = loop {
                    if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                        break status;
                    }
                    if started.elapsed() > Duration::from_secs(5) {
                        child.terminate_tree()?;
                        return Err("library signal child exceeded5s".to_string());
                    }
                    std::thread::sleep(Duration::from_millis(2));
                };
                if (mode == "caller" && !status.success())
                    || (mode == "native" && status.signal() != Some(2))
                {
                    return Err(format!(
                        "library signal child displaced caller SIGINT policy: {status:?}, {}",
                        fs::read_to_string(&capture).map_err(|e| e.to_string())?
                    ));
                }
            }
            Ok(())
        }
    }
}
