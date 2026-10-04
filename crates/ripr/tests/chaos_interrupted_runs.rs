//! Built-binary chaos runs: terminated, corrupted, unwritable, full and concurrent
//! environments must never change the analysis result, leave a cache entry or
//! temporary file that a later run trusts, or turn a write failure into a
//! clean-looking exit.
//!
//! Every scenario compares against one cold reference run on the same
//! fixture, so a drift in result bytes, not just a crash, fails the test.

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
        Ok(text
            .replace(&self.base.display().to_string(), "<fixture>")
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
        child.wait().map_err(|error| error.to_string())?;

        let label = format!("terminated after {delay_micros}us");
        // An interrupted atomic write must not be left where a reader or
        // a later writer could mistake it for an entry.
        let stranded = fixture.stranded_temp_files();
        assert!(
            stranded.is_empty(),
            "{label}: stranded temporary files: {stranded:?}"
        );
        same_result(&label, &cold, &fixture.result("ge12.diff")?)?;
        same_result(
            &format!("{label}, second rerun"),
            &cold,
            &fixture.result("ge12.diff")?,
        )?;
    }
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

    // The complete new artifact, produced once without interruption.
    let reference = Fixture::new("terminated-artifact-ref")?;
    let new_artifact = reference.base.join("new.json");
    let new_arg = new_artifact.display().to_string();
    let new_run = reference.run("ge11.diff", &["--write-artifact", &new_arg])?;
    assert!(new_run.status.success(), "reference artifact write failed");
    let new_bytes = fs::read(&new_artifact).map_err(|error| error.to_string())?;
    assert_ne!(
        old_bytes, new_bytes,
        "fixture must produce distinguishable artifacts"
    );

    for delay_micros in [0u64, 500, 2_000, 5_000, 10_000, 20_000, 40_000] {
        let mut child = fixture
            .command("ge11.diff", &["--write-artifact", &artifact_arg])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("spawn ripr check: {error}"))?;
        std::thread::sleep(Duration::from_micros(delay_micros));
        let _ = child.kill();
        child.wait().map_err(|error| error.to_string())?;

        let observed = fs::read(&artifact).map_err(|error| error.to_string())?;
        assert!(
            observed == old_bytes || observed == new_bytes,
            "terminated after {delay_micros}us: artifact is neither the previous nor a complete new artifact ({} bytes)",
            observed.len()
        );
        // Restore the previous artifact so each delay races the same swap.
        fs::write(&artifact, &old_bytes).map_err(|error| error.to_string())?;
    }
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
        // And the cache the race left behind is still sound.
        same_result(
            &format!("round {round}, post-race rerun"),
            &cold,
            &fixture.result("ge12.diff")?,
        )?;
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
