use super::*;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::os::unix::fs::symlink;
use std::process::{Output, Stdio};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self> {
        crate::test_support::scratch("coverage-storage").map(Self)
    }

    fn target(&self) -> Root {
        Root {
            label: "target",
            path: self.0.join("target"),
            target: true,
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("retain coverage storage fixture: {error}");
        }
    }
}

fn totals<'a>(snapshot: &'a Snapshot, label: &str) -> Result<&'a Totals> {
    snapshot
        .buckets
        .get(label)
        .ok_or_else(|| format!("missing bucket {label}"))
}

#[test]
fn allocation_buckets_distinguish_profiles_builds_and_native_fixture_storage() -> Result<()> {
    let fixture = Fixture::new()?;
    let target = fixture.0.join("target");
    for directory in [
        "llvm-cov-target/debug",
        "debug",
        "ripr/reports",
        "ripr-proptest-profile-1/native-target/debug",
    ] {
        fs::create_dir_all(target.join(directory)).map_err(|error| error.to_string())?;
    }
    let profile = target.join("llvm-cov-target/one.profraw");
    let mut file = File::create(&profile).map_err(|error| error.to_string())?;
    file.write_all(b"profile fixture")
        .map_err(|error| error.to_string())?;
    file.set_len(1024 * 1024).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    assert!(metadata.blocks() * 512 < metadata.len(), "sparse setup");
    fs::hard_link(&profile, target.join("llvm-cov-target/two.profraw"))
        .map_err(|error| error.to_string())?;
    fs::hard_link(&profile, target.join("debug/profile-link"))
        .map_err(|error| error.to_string())?;
    for path in [
        "llvm-cov-target/debug/fixture-binary",
        "ripr/reports/fixture.json",
        "ripr-proptest-profile-1/native-target/debug/native-output",
        "ripr-proptest-profile-1/source.rs",
    ] {
        fs::write(target.join(path), b"owned bytes").map_err(|error| error.to_string())?;
    }
    let outside = fixture.0.join("outside");
    fs::create_dir(&outside).map_err(|error| error.to_string())?;
    fs::write(outside.join("not-inspected"), vec![0; 8192]).map_err(|error| error.to_string())?;
    symlink(&outside, target.join("outside-link")).map_err(|error| error.to_string())?;
    symlink(&target, target.join("cycle")).map_err(|error| error.to_string())?;

    let observed = snapshot(&[fixture.target()], Limits::default());
    assert!(
        !observed.partial,
        "{}",
        serde_json::to_string(&observed).map_err(|error| error.to_string())?
    );
    let profiles = totals(&observed, "raw_profiles")?;
    assert_eq!(profiles.regular_files, 2);
    assert_eq!(profiles.unique_inodes, 1);
    assert_eq!(profiles.duplicate_inodes, 1);
    assert_eq!(profiles.logical_file_bytes, metadata.len() * 2);
    assert_eq!(profiles.allocated_bytes, metadata.blocks() * 512);
    let plain = totals(&observed, "plain_build")?;
    assert_eq!(plain.regular_files, 1);
    assert_eq!(plain.duplicate_inodes, 1);
    assert_eq!(plain.logical_file_bytes, metadata.len());
    assert_eq!(
        plain.allocated_bytes,
        fs::metadata(target.join("debug"))
            .map_err(|error| error.to_string())?
            .blocks()
            * 512
    );
    assert_eq!(totals(&observed, "instrumented_build")?.regular_files, 1);
    assert_eq!(totals(&observed, "fixture_native_build")?.regular_files, 1);
    assert_eq!(totals(&observed, "ripr_artifacts")?.regular_files, 1);
    assert_eq!(totals(&observed, "other_target")?.regular_files, 1);
    assert_eq!(totals(&observed, "other_target")?.symlinks, 2);
    assert_eq!(
        observed
            .buckets
            .values()
            .map(|bucket| bucket.regular_files)
            .sum::<u64>(),
        7
    );
    Ok(())
}

#[test]
fn scan_limits_unavailable_roots_and_overlap_are_explicit() -> Result<()> {
    let fixture = Fixture::new()?;
    fs::create_dir_all(fixture.0.join("target/deep/nested")).map_err(|error| error.to_string())?;
    fs::write(fixture.0.join("target/deep/nested/value"), b"retained")
        .map_err(|error| error.to_string())?;
    for (limits, reason) in [
        (
            Limits {
                entries: 0,
                ..Limits::default()
            },
            "entries",
        ),
        (
            Limits {
                depth: 0,
                ..Limits::default()
            },
            "depth",
        ),
        (
            Limits {
                time: Duration::ZERO,
                ..Limits::default()
            },
            "time",
        ),
    ] {
        let observed = snapshot(&[fixture.target()], limits);
        assert!(observed.partial);
        assert!(observed.limits_hit.contains(reason), "missing {reason}");
    }
    let ancestor = Root {
        label: "ancestor",
        path: fixture.0.clone(),
        target: false,
    };
    let observed = snapshot(&[fixture.target(), ancestor], Limits::default());
    assert!(observed.partial);
    assert_eq!(
        observed.roots["ancestor"]["status"],
        "overlapping_root_not_scanned"
    );
    assert!(!observed.buckets.contains_key("ancestor"));

    let linked = fixture.0.join("linked");
    symlink(fixture.0.join("target"), &linked).map_err(|error| error.to_string())?;
    assert!(open_chain(&linked).is_err());
    assert!(open_chain(&linked.join("deep")).is_err());
    let observed = snapshot(
        &[Root {
            label: "linked",
            path: linked,
            target: false,
        }],
        Limits::default(),
    );
    assert!(observed.partial);
    assert!(observed.buckets.is_empty());
    assert_eq!(observed.roots["linked"]["status"], "unavailable");
    Ok(())
}

#[test]
fn free_space_preserves_zero_and_rejects_invalid_or_overflowed_capacity() {
    assert_eq!(capacity_values(2, 1024, 17)["available_bytes"], 2048);
    assert_eq!(capacity_values(2, 1024, 17)["available_inodes"], 17);
    assert_eq!(capacity_values(0, 4096, 0)["status"], "available");
    assert_eq!(capacity_values(0, 4096, 0)["available_bytes"], 0);
    assert_eq!(capacity_values(0, 4096, 0)["available_inodes"], 0);
    for (blocks, fragment_bytes) in [(1, 0), (u64::MAX, 2)] {
        assert_eq!(
            capacity_values(blocks, fragment_bytes, 17)["status"],
            "unavailable"
        );
    }
}

#[derive(Default)]
struct Flushed {
    bytes: Vec<u8>,
    writes: usize,
    record_sizes: Vec<usize>,
    flushes: usize,
}

impl Write for Flushed {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(bytes);
        self.writes += 1;
        self.record_sizes.push(bytes.len());
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        Ok(())
    }
}

#[test]
fn receipts_flush_immediately_and_bound_even_oversized_input() -> Result<()> {
    let mut writer = Flushed::default();
    emit(
        &mut writer,
        &json!({"event": {"phase": "before"}, "count": 3}),
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(writer.flushes, 1);
    assert_eq!(writer.writes, 1);
    assert!(writer.bytes.starts_with(b"\nripr-coverage-storage {"));
    assert!(writer.bytes.ends_with(b"\n"));
    writer.bytes.clear();
    emit(
        &mut writer,
        &json!({
            "event": {"phase": "final", "exit": 23, "extra": "x".repeat(LINE_BYTES)},
            "extra": "y".repeat(LINE_BYTES),
        }),
    )
    .map_err(|error| error.to_string())?;
    assert!(writer.bytes.len() <= LINE_BYTES);
    let text = std::str::from_utf8(&writer.bytes).map_err(|error| error.to_string())?;
    let value: Value = serde_json::from_str(text.trim_start_matches("\nripr-coverage-storage "))
        .map_err(|error| error.to_string())?;
    assert_eq!(value["partial"], true);
    assert_eq!(value["exit"], 23);
    assert_eq!(value["error"], "receipt_byte_limit");
    assert_eq!(writer.flushes, 2);
    assert_eq!(writer.writes, 2);
    Ok(())
}

#[test]
fn fixed_command_preserves_full_population_and_inherited_environment() {
    let command = coverage_command();
    assert_eq!(command.get_program(), "cargo");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        [
            "llvm-cov",
            "--workspace",
            "--all-features",
            "--lcov",
            "--output-path",
            "lcov.info"
        ]
    );
    assert_eq!(command.get_envs().count(), 0);
    assert!(command.get_current_dir().is_none());
}

#[test]
fn observation_limit_preserves_output_profiles_and_native_failure() -> Result<()> {
    let fixture = Fixture::new()?;
    let stdout = fixture.0.join("stdout");
    let stderr = fixture.0.join("stderr");
    let bin = fixture.0.join("bin");
    fs::create_dir(&bin).map_err(|error| error.to_string())?;
    symlink("/bin/sh", bin.join("cargo")).map_err(|error| error.to_string())?;
    let home = std::env::var("HOME").map_err(|error| format!("inherited HOME setup: {error}"))?;
    assert!(!home.is_empty(), "nonempty inherited HOME setup");
    fs::write(fixture.0.join("expected-home"), format!("{home}\n"))
        .map_err(|error| error.to_string())?;
    // The executable is the stable system shell. Its ordinary input file is
    // named for the first fixed Cargo argument and is never made executable.
    fs::write(
        fixture.0.join("llvm-cov"),
        r#"test "$#" -eq 5 || exit 81
test "$1" = --workspace && test "$2" = --all-features || exit 82
test "$3" = --lcov && test "$4" = --output-path && test "$5" = lcov.info || exit 83
IFS= read -r expected_home < expected-home
test "$HOME" = "$expected_home" || exit 84
i=0
while [ ! -f release ] && [ "$i" -lt 100 ]; do
    i=$((i+1))
    /bin/sleep 0.01
done
test -f release || exit 99
printf '%s|%s\n' "$LLVM_PROFILE_FILE" "$CARGO_LLVM_COV"
printf 'stderr retained\n' >&2
/bin/sleep 0.05
printf 'after observation cap\n'
exit 23
"#,
    )
    .map_err(|error| error.to_string())?;
    let mut command = coverage_command();
    command
        .current_dir(&fixture.0)
        .env("PATH", &bin)
        .env("LLVM_PROFILE_FILE", "unchanged-profile-%p-%m.profraw")
        .env("CARGO_LLVM_COV", "1")
        .stdout(Stdio::from(
            File::create(&stdout).map_err(|error| error.to_string())?,
        ))
        .stderr(Stdio::from(
            File::create(&stderr).map_err(|error| error.to_string())?,
        ));
    let mut events = Vec::new();
    let mut write_error = None;
    let code = run_observed(command, Duration::from_millis(1), 1, |event| {
        if event["phase"] == "during"
            && let Err(error) = fs::write(fixture.0.join("release"), b"continue")
        {
            write_error = Some(error.to_string());
        }
        events.push(event);
    })?;
    if let Some(error) = write_error {
        return Err(error);
    }
    assert_eq!(code, 23);
    assert_eq!(
        events
            .iter()
            .map(|event| event["phase"].as_str())
            .collect::<Vec<_>>(),
        [Some("before"), Some("during"), Some("final")]
    );
    assert_eq!(events.last().ok_or("missing final receipt")?["exit"], 23);
    assert_eq!(
        fs::read_to_string(stdout).map_err(|error| error.to_string())?,
        "unchanged-profile-%p-%m.profraw|1\nafter observation cap\n"
    );
    assert_eq!(
        fs::read_to_string(stderr).map_err(|error| error.to_string())?,
        "stderr retained\n"
    );
    Ok(())
}

#[test]
fn native_success_and_signal_survive_observation() -> Result<()> {
    for (script, expected, signal) in [
        ("exit 0", 0, None),
        ("kill -TERM $$", 143, Some(15)),
    ] {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        let mut events = Vec::new();
        let code = run_observed(command, Duration::from_secs(60), 0, |event| {
            events.push(event);
        })?;
        assert_eq!(code, expected);
        let final_event = events.last().ok_or("missing final receipt")?;
        assert_eq!(final_event["signal"], json!(signal));
        assert_eq!(final_event["cleanup_error"], Value::Null);
    }
    Ok(())
}

#[test]
fn child_cleanup_retains_custody_and_refuses_external_reaping() -> Result<()> {
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30"]);
    let mut child = CoverageChild::spawn(command)?;
    assert_ne!(child.pid, rustix::process::getpgrp());
    assert!(!child.exited().map_err(|error| error.to_string())?);
    let (status, error) = child.cleanup();
    assert_eq!(error, None);
    assert_eq!(status.ok_or("missing cleanup status")?.signal(), Some(9));
    assert!(child.closed);
    // A deliberately wrong external reap must disarm signalling.
    let mut command = Command::new("sh");
    command.args(["-c", "exit 0"]);
    let mut child = CoverageChild::spawn(command)?;
    assert!(
        child
            .child
            .wait()
            .map_err(|error| error.to_string())?
            .success()
    );
    assert!(child.exited().is_err());
    assert!(child.closed);
    assert_eq!(child.cleanup().1, Some("owned_child_custody_unavailable"));
    Ok(())
}

#[test]
fn dropped_owner_terminates_and_reaps_its_child() -> Result<()> {
    use rustix::process::{WaitId, WaitIdOptions, waitid};
    let mut command = Command::new("sh");
    command.args(["-c", "sleep 30"]);
    let child = CoverageChild::spawn(command)?;
    let pid = child.pid;
    drop(child);
    assert!(matches!(
        waitid(WaitId::Pid(pid), WaitIdOptions::EXITED | WaitIdOptions::NOHANG),
        Err(rustix::io::Errno::CHILD)
    ));
    Ok(())
}

#[test]
fn held_directory_capacity_remains_numeric_after_rename() -> Result<()> {
    let fixture = Fixture::new()?;
    let path = fixture.0.join("directory");
    fs::create_dir(&path).map_err(|error| error.to_string())?;
    let directory = open_chain(&path).map_err(|error| error.to_string())?;
    fs::rename(&path, fixture.0.join("renamed")).map_err(|error| error.to_string())?;
    assert!(!path.exists(), "capacity must use the held directory");
    let space = free_space(&directory);
    assert_eq!(space["status"], "available", "{space}");
    assert!(space["available_bytes"].as_u64().is_some());
    assert!(space["available_inodes"].as_u64().is_some());
    Ok(())
}

#[test]
fn full_schema_keeps_all_buckets_and_escaped_profile_entries_in_atomic_records() -> Result<()> {
    let mut sample = Snapshot::default();
    for label in [
        "raw_profiles",
        "instrumented_build",
        "plain_build",
        "ripr_artifacts",
        "fixture_native_build",
        "other_target",
        "checkout_git",
        "cargo_home",
        "rustup_home",
        "runner_temp",
    ] {
        sample.buckets.insert(
            label,
            Totals {
                entries: 10_000,
                unique_inodes: 9877,
                regular_files: 8234,
                directories: 1448,
                symlinks: 318,
                other_entries: 0,
                logical_file_bytes: u64::MAX,
                allocated_bytes: u64::MAX,
                duplicate_inodes: 123,
            },
        );
    }
    for label in [
        "target",
        "checkout_git",
        "cargo_home",
        "rustup_home",
        "runner_temp",
    ] {
        sample
            .roots
            .insert(label, json!({"status": "available", "device": 2049}));
    }
    sample.filesystems.insert(
        2049,
        json!({
            "status": "available", "available_bytes": 9_000_000, "available_inodes": 100_000,
        }),
    );
    sample.errors.insert("metadata:entity not found".into(), 72);
    sample.partial = true;
    sample.entries_inspected = SAMPLE_ENTRIES;
    sample.limits_hit.insert("entries");
    for inode in 1..=8 {
        sample.largest_profile_entries.push(ProfileEntry {
            // Escaping expands each retained byte sixfold. It must not drop the
            // aggregate raw-profile bucket or misreport omitted bucket counts.
            name: "\u{1}".repeat(128),
            name_truncated: true,
            device: 2049,
            inode,
            logical_bytes: u64::MAX,
            allocated_bytes: u64::MAX,
        });
    }
    let mut writer = Flushed::default();
    emit_snapshot(&mut writer, &json!({"phase": "during"}), &sample, 7, 60_000)
        .map_err(|error| error.to_string())?;
    assert!(writer.bytes.len() <= SNAPSHOT_BYTES);
    assert!(writer.record_sizes.iter().all(|size| *size <= LINE_BYTES));
    let text = std::str::from_utf8(&writer.bytes).map_err(|error| error.to_string())?;
    let records: Vec<Value> = text
        .lines()
        .filter_map(|line| line.strip_prefix("ripr-coverage-storage "))
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()
        .map_err(|error| error.to_string())?;
    assert_eq!(writer.writes, records.len());
    assert_eq!(writer.flushes, records.len());
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "bucket")
            .count(),
        10
    );
    assert_eq!(
        records
            .iter()
            .filter(|record| record["kind"] == "profile_entry")
            .count(),
        8
    );
    let end = records.last().ok_or("missing complete sample")?;
    assert_eq!(end["kind"], "end");
    assert_eq!(end["output_partial"], false);
    assert_eq!(end["emitted_buckets"], 10);
    assert_eq!(end["omitted_buckets"], 0);
    assert_eq!(end["emitted_profile_entries"], 8);
    assert_eq!(end["omitted_profile_entries"], 0);
    assert!(records.iter().all(|record| record["sequence"] == 7));
    Ok(())
}

fn process_is_running(pid: u32) -> Result<bool> {
    let read = (|| -> io::Result<String> {
        let file = File::open(format!("/proc/{pid}/status"))?;
        let mut text = String::new();
        file.take(4096).read_to_string(&mut text)?;
        Ok(text)
    })();
    process_status_is_running(read)
}

fn process_status_is_running(read: io::Result<String>) -> Result<bool> {
    let text = match read {
        Ok(text) => text,
        // Match the existing exit observer: disappearance can race either the
        // open or the read, but permission, I/O and malformed data stay errors.
        Err(error)
            if error.kind() == io::ErrorKind::NotFound
                || error.raw_os_error() == Some(libc::ESRCH) =>
        {
            return Ok(false);
        }
        Err(error) => return Err(error.to_string()),
    };
    let state = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("State:")
                .and_then(|rest| rest.split_whitespace().next())
        })
        .ok_or("missing process state")?;
    Ok(!matches!(state, "Z" | "X"))
}

#[test]
fn descendant_status_accepts_only_vanished_tasks_and_preserves_other_errors() {
    for errno in [libc::ENOENT, libc::ESRCH] {
        assert_eq!(
            process_status_is_running(Err(io::Error::from_raw_os_error(errno))),
            Ok(false)
        );
    }
    for errno in [libc::EACCES, libc::EIO] {
        let error = io::Error::from_raw_os_error(errno);
        let expected = error.to_string();
        assert_eq!(process_status_is_running(Err(error)), Err(expected));
    }
    assert_eq!(
        process_status_is_running(Ok("malformed status".into())),
        Err("missing process state".into())
    );
    for (state, expected) in [("R", true), ("S", true), ("Z", false), ("X", false)] {
        assert_eq!(
            process_status_is_running(Ok(format!("Name:\tfixture\nState:\t{state}\n"))),
            Ok(expected)
        );
    }
}

#[test]
fn normal_exit_cleanup_stops_a_confirmed_separate_descendant() -> Result<()> {
    let fixture = Fixture::new()?;
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            r#"/bin/sh -c '
while IFS=": " read -r field value; do
    if [ "$field" = Pid ]; then
        printf "%s\n" "$value" > "$1/descendant"
        printf "ready\n" > "$1/descendant-ready"
        break
    fi
done < /proc/self/status
i=0
while [ "$i" -lt 500 ]; do i=$((i+1)); /bin/sleep 0.01; done
' descendant "$1" &
i=0
while [ ! -f "$1/primary-exit" ] && [ "$i" -lt 300 ]; do
    i=$((i+1))
    /bin/sleep 0.01
done
test -f "$1/primary-exit" || exit 97
exit 0
"#,
            "fixture",
        ])
        .arg(&fixture.0);
    let mut descendant = None;
    let mut setup_error = None;
    let code = run_observed(command, Duration::from_millis(1), 200, |event| {
        if event["phase"] != "during" || descendant.is_some() || setup_error.is_some() {
            return;
        }
        if !fixture.0.join("descendant-ready").is_file() {
            return;
        }
        let text = match fs::read_to_string(fixture.0.join("descendant")) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                setup_error = Some(error.to_string());
                return;
            }
        };
        let result = (|| -> Result<u32> {
            let pid = text
                .trim()
                .parse::<u32>()
                .map_err(|error| error.to_string())?;
            if pid == 0 || !process_is_running(pid)? {
                return Err("descendant was not alive at setup".into());
            }
            fs::write(fixture.0.join("primary-exit"), b"exit")
                .map_err(|error| error.to_string())?;
            Ok(pid)
        })();
        match result {
            Ok(pid) => descendant = Some(pid),
            Err(error) => setup_error = Some(error),
        }
    })?;
    if let Some(error) = setup_error {
        return Err(error);
    }
    assert_eq!(code, 0);
    let pid = descendant.ok_or("no confirmed descendant")?;
    let started = Instant::now();
    while process_is_running(pid)? && started.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !process_is_running(pid)?,
        "owned descendant survived normal exit cleanup"
    );
    Ok(())
}

const NATIVE_OUTPUT_BYTES: u64 = 256 * 1024;
const STRIP_PROBE: &str = "probe_runs_the_hot_branch";
const STRIP_ACK: &str = "ripr-strip-probe: asserted hot branch";
const STRIP_SOURCE: &str = r#"#[inline(never)]
pub fn score(flag: bool) -> u32 {
    if flag {
        17
    } else {
        23
    }
}

#[test]
fn probe_runs_the_hot_branch() {
    assert_eq!(score(std::hint::black_box(true)), 17);
    println!("ripr-strip-probe: asserted hot branch");
}
"#;

struct StripFixture {
    root: PathBuf,
    removed: bool,
}

impl StripFixture {
    fn new() -> Result<Self> {
        Ok(Self {
            root: crate::test_support::scratch("coverage-strip")?,
            removed: false,
        })
    }

    fn remove(&mut self) -> Result<()> {
        fs::remove_dir_all(&self.root).map_err(|error| error.to_string())?;
        self.removed = true;
        Ok(())
    }
}

impl Drop for StripFixture {
    fn drop(&mut self) {
        // Any error or panic may leave uncertain child custody. Retain its
        // files; only explicit success after confirmed cleanup removes them.
        if !self.removed {
            let _ = writeln!(
                io::stderr().lock(),
                "retain native strip fixture: {}",
                self.root.display()
            );
        }
    }
}

fn native_arguments(arguments: &[&str]) -> Vec<OsString> {
    arguments.iter().map(|value| OsString::from(*value)).collect()
}

fn read_native_output(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(NATIVE_OUTPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > NATIVE_OUTPUT_BYTES {
        return Err("native strip control exceeded its output budget".into());
    }
    Ok(bytes)
}

fn native_output(
    root: &Path,
    label: &str,
    program: &OsStr,
    arguments: &[OsString],
    environment: &[(&str, OsString)],
) -> Result<Output> {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    let capture = |path: &Path| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| error.to_string())
    };
    let mut command = Command::new(program);
    command
        .current_dir(root)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(capture(&stdout)?)
        .stderr(capture(&stderr)?);
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = CoverageChild::spawn(command)?;
    let started = Instant::now();
    loop {
        let exited = child.exited().map_err(|error| error.to_string())?;
        let oversized = [&stdout, &stderr].iter().try_fold(false, |large, path| {
            fs::metadata(path)
                .map(|metadata| large || metadata.len() > NATIVE_OUTPUT_BYTES)
                .map_err(|error| error.to_string())
        })?;
        if oversized || started.elapsed() >= Duration::from_secs(30) {
            let (status, cleanup) = child.cleanup();
            return Err(format!(
                "native strip control {label} exceeded output/time bound: {status:?}; {cleanup:?}"
            ));
        }
        if exited {
            let (status, cleanup) = child.cleanup();
            if let Some(error) = cleanup {
                return Err(format!("native strip control {label}: {error}"));
            }
            let output = Output {
                status: status.ok_or("native strip control status unavailable")?,
                stdout: read_native_output(&stdout)?,
                stderr: read_native_output(&stderr)?,
            };
            if !output.status.success() {
                let prefix = |bytes: &[u8]| {
                    String::from_utf8_lossy(&bytes[..bytes.len().min(8192)]).into_owned()
                };
                return Err(format!(
                    "native strip control {label}: {}; stdout: {}; stderr: {}",
                    output.status,
                    prefix(&output.stdout),
                    prefix(&output.stderr)
                ));
            }
            return Ok(output);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn native_text(output: &Output) -> Result<&str> {
    std::str::from_utf8(&output.stdout).map_err(|error| error.to_string())
}

struct StripTools {
    rustc: PathBuf,
    cargo: PathBuf,
    profdata: PathBuf,
    cov: PathBuf,
    readobj: PathBuf,
    versions: BTreeMap<&'static str, String>,
}

fn strip_tools(root: &Path) -> Result<StripTools> {
    // As in the existing required LLVM control, Linux tool discovery failure
    // is a proof failure. There is no success/ignore fallback for missing tools.
    let sysroot = native_output(
        root,
        "sysroot",
        OsStr::new("rustc"),
        &native_arguments(&["--print", "sysroot"]),
        &[],
    )?;
    let sysroot = PathBuf::from(native_text(&sysroot)?.trim());
    if !sysroot.is_absolute() {
        return Err("native strip control requires an absolute compiler sysroot".into());
    }
    let rustc = sysroot.join("bin/rustc");
    let cargo = sysroot.join("bin/cargo");
    let identity = native_output(
        root,
        "rustc-version",
        rustc.as_os_str(),
        &native_arguments(&["-vV"]),
        &[],
    )?;
    let identity = native_text(&identity)?.trim().to_owned();
    let host = identity
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("native strip control compiler host missing")?;
    let llvm = sysroot.join("lib/rustlib").join(host).join("bin");
    let mut tools = StripTools {
        rustc,
        cargo,
        profdata: llvm.join("llvm-profdata"),
        cov: llvm.join("llvm-cov"),
        readobj: llvm.join("llvm-readobj"),
        versions: BTreeMap::from([("rustc", identity)]),
    };
    for (name, path) in [
        ("cargo", &tools.cargo),
        ("llvm-profdata", &tools.profdata),
        ("llvm-cov", &tools.cov),
        ("llvm-readobj", &tools.readobj),
    ] {
        let output = native_output(
            root,
            &format!("{name}-version"),
            path.as_os_str(),
            &native_arguments(&["--version"]),
            &[],
        )?;
        tools.versions.insert(name, native_text(&output)?.trim().into());
    }
    Ok(tools)
}

fn strip_executable(root: &Path, tools: &StripTools, mode: &str) -> Result<PathBuf> {
    let target = root.join(format!("target-{mode}"));
    let environment = [
        ("CARGO_TARGET_DIR", target.as_os_str().to_owned()),
        ("CARGO_BUILD_JOBS", OsString::from("1")),
        ("CARGO_INCREMENTAL", OsString::from("0")),
        ("CARGO_TERM_COLOR", OsString::from("never")),
        ("CARGO_PROFILE_TEST_STRIP", OsString::from(mode)),
        ("RUSTC", tools.rustc.as_os_str().to_owned()),
        ("RUSTC_WRAPPER", OsString::new()),
        ("RUSTC_WORKSPACE_WRAPPER", OsString::new()),
        ("RUSTFLAGS", OsString::new()),
        (
            "CARGO_ENCODED_RUSTFLAGS",
            OsString::from("-C\u{1f}instrument-coverage\u{1f}-C\u{1f}link-arg=-Wl,--build-id=sha1"),
        ),
    ];
    let output = native_output(
        root,
        &format!("{mode}-build"),
        tools.cargo.as_os_str(),
        &native_arguments(&[
            "test",
            "--offline",
            "--lib",
            "--no-run",
            "-vv",
            "--message-format=json",
        ]),
        &environment,
    )?;
    let compiler = std::str::from_utf8(&output.stderr).map_err(|error| error.to_string())?;
    let invocations: Vec<_> = compiler
        .lines()
        .filter(|line| {
            line.contains("Running `")
                && line.contains("--crate-name ripr_coverage_strip_fixture ")
                && line.contains("--test")
        })
        .collect();
    assert_eq!(invocations.len(), 1, "one actual Cargo test compilation");
    let invocation = invocations[0];
    assert!(invocation.contains("-C instrument-coverage"));
    assert!(invocation.contains("-C debuginfo=line-tables-only"));
    assert!(!invocation.contains("-C strip=symbols"));
    assert_eq!(invocation.contains("-C strip=debuginfo"), mode == "debuginfo");

    let mut executables = Vec::new();
    for line in native_text(&output)?.lines() {
        let artifact: Value = serde_json::from_str(line).map_err(|error| error.to_string())?;
        if artifact["reason"] == "compiler-artifact"
            && artifact["target"]["name"] == "ripr_coverage_strip_fixture"
            && artifact["profile"]["test"] == true
            && let Some(path) = artifact["executable"].as_str()
        {
            executables.push(PathBuf::from(path));
        }
    }
    assert_eq!(executables.len(), 1, "one intended Cargo test executable");
    let executable = fs::canonicalize(&executables[0]).map_err(|error| error.to_string())?;
    let target = fs::canonicalize(target).map_err(|error| error.to_string())?;
    assert!(
        executable.starts_with(target),
        "artifact belongs to this variant"
    );
    Ok(executable)
}

fn one_build_id<'a>(ids: impl Iterator<Item = &'a str>) -> Result<String> {
    let ids: Vec<_> = ids.filter(|id| !id.is_empty()).collect();
    if ids.len() != 1
        || ids[0].len() != 40
        || !ids[0].bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!(
            "native strip control requires one SHA-1 build ID: {ids:?}"
        ));
    }
    Ok(ids[0].to_ascii_lowercase())
}

fn owned_lcov_lines(text: &str, source: &Path) -> Result<BTreeMap<u64, u64>> {
    let mut records = text.split("end_of_record").filter(|record| {
        record.lines().any(|line| {
            line.strip_prefix("SF:")
                .is_some_and(|path| Path::new(path) == source)
        })
    });
    let record = records.next().ok_or("owned source missing from LCOV")?;
    if records.next().is_some() {
        return Err("duplicate owned source in LCOV".into());
    }
    let mut lines = BTreeMap::new();
    for line in record.lines().filter_map(|line| line.strip_prefix("DA:")) {
        let mut fields = line.split(',');
        let number = fields.next().ok_or("LCOV line missing")?;
        let count = fields.next().ok_or("LCOV count missing")?;
        let number = number.parse::<u64>().map_err(|error| error.to_string())?;
        let count = count.parse::<u64>().map_err(|error| error.to_string())?;
        if lines.insert(number, count).is_some() {
            return Err("duplicate LCOV source line".into());
        }
    }
    if lines.is_empty() {
        return Err("owned LCOV source has no line counters".into());
    }
    Ok(lines)
}

fn owned_coverage(export: &Value, source: &Path) -> Result<Value> {
    let data = export["data"].as_array().ok_or("coverage data missing")?;
    if data.len() != 1 {
        return Err("expected one binary coverage export".into());
    }
    let files = data[0]["files"].as_array().ok_or("coverage files missing")?;
    let files: Vec<_> = files
        .iter()
        .filter(|file| {
            file["filename"]
                .as_str()
                .is_some_and(|path| Path::new(path) == source)
        })
        .collect();
    if files.len() != 1 {
        return Err("expected exactly one owned source coverage record".into());
    }
    let functions = data[0]["functions"]
        .as_array()
        .ok_or("coverage functions missing")?;
    let mut owned = BTreeMap::new();
    for name in ["score", STRIP_PROBE] {
        let functions: Vec<_> = functions
            .iter()
            .filter(|function| {
                function["name"]
                    .as_str()
                    .is_some_and(|symbol| symbol.contains(name))
                    && function["filenames"].as_array().is_some_and(|paths| {
                        paths.iter().any(|path| {
                            path.as_str().is_some_and(|path| Path::new(path) == source)
                        })
                    })
            })
            .collect();
        if functions.len() != 1 {
            return Err(format!("expected exactly one owned {name} function"));
        }
        let mut function = (*functions[0]).clone();
        assert_eq!(function["count"], 2, "{name}: two real invocations");
        assert!(
            function["regions"]
                .as_array()
                .is_some_and(|regions| !regions.is_empty())
        );
        // Cargo may change the mangled crate hash between profile settings.
        // Retain every source filename, region and counter in the comparison.
        function
            .as_object_mut()
            .ok_or("function record is not an object")?
            .remove("name");
        owned.insert(name, function);
    }
    Ok(json!({"file": files[0], "functions": owned}))
}

struct StripObservation {
    bytes: u64,
    build_id: String,
    lines: BTreeMap<u64, u64>,
    coverage: Value,
}

fn observe_strip_variant(root: &Path, tools: &StripTools, mode: &str) -> Result<StripObservation> {
    let executable = strip_executable(root, tools, mode)?;
    let mut profiles = Vec::new();
    for run in 0..2 {
        let profile = root.join(format!("{mode}-{run}.profraw"));
        assert!(!profile.exists(), "each invocation starts without a profile");
        let output = native_output(
            root,
            &format!("{mode}-run-{run}"),
            executable.as_os_str(),
            &native_arguments(&["--exact", STRIP_PROBE, "--nocapture"]),
            &[("LLVM_PROFILE_FILE", profile.as_os_str().to_owned())],
        )?;
        let text = native_text(&output)?;
        assert_eq!(
            text.matches(STRIP_ACK).count(),
            1,
            "post-assertion acknowledgement"
        );
        let passed =
            "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";
        assert_eq!(
            text.matches(passed).count(),
            1,
            "exact nonempty libtest selection"
        );
        assert!(
            output.stderr.is_empty(),
            "profile write diagnostics are not success"
        );
        let bytes = fs::metadata(&profile)
            .map_err(|error| error.to_string())?
            .len();
        assert!(
            (1..=1024 * 1024).contains(&bytes),
            "nonempty bounded raw profile"
        );
        profiles.push(profile.into_os_string());
    }
    let merged = root.join(format!("{mode}.profdata"));
    let mut arguments = native_arguments(&["merge", "-sparse"]);
    arguments.extend(profiles);
    arguments.push("-o".into());
    arguments.push(merged.as_os_str().to_owned());
    let merge = native_output(
        root,
        &format!("{mode}-merge"),
        tools.profdata.as_os_str(),
        &arguments,
        &[],
    )?;
    assert!(
        merge.stderr.is_empty(),
        "strict merge has no discarded-profile warning"
    );

    let image = native_output(
        root,
        &format!("{mode}-elf"),
        tools.readobj.as_os_str(),
        &[
            OsString::from("--sections"),
            OsString::from("--notes"),
            OsString::from("--elf-output-style=GNU"),
            executable.as_os_str().to_owned(),
        ],
        &[],
    )?;
    assert!(
        image.stderr.is_empty(),
        "ELF inspection warnings are not success"
    );
    let image = native_text(&image)?;
    let debug_sections: Vec<_> = image
        .split_whitespace()
        .filter(|word| word.starts_with(".debug_") || word.starts_with(".zdebug_"))
        .collect();
    if mode == "none" {
        assert!(
            debug_sections.contains(&".debug_line"),
            "baseline has actual line tables"
        );
    } else {
        assert!(
            debug_sections.is_empty(),
            "candidate removed all debug sections"
        );
    }
    let build_id = one_build_id(
        image
            .lines()
            .filter_map(|line| line.trim().strip_prefix("Build ID: ")),
    )?;
    let ids = native_output(
        root,
        &format!("{mode}-ids"),
        tools.profdata.as_os_str(),
        &[
            OsString::from("show"),
            OsString::from("--binary-ids"),
            merged.as_os_str().to_owned(),
        ],
        &[],
    )?;
    assert!(ids.stderr.is_empty(), "profile ID warnings are not success");
    let mut ids = native_text(&ids)?.lines();
    if !ids.any(|line| line.trim() == "Binary IDs:") {
        return Err("merged profile binary IDs missing".into());
    }
    assert_eq!(one_build_id(ids.map(str::trim))?, build_id);

    let source = fs::canonicalize(root.join("src/lib.rs")).map_err(|error| error.to_string())?;
    let arguments = [
        OsString::from("export"),
        executable.as_os_str().to_owned(),
        OsString::from("-instr-profile"),
        merged.as_os_str().to_owned(),
    ];
    let export = native_output(
        root,
        &format!("{mode}-json"),
        tools.cov.as_os_str(),
        &arguments,
        &[],
    )?;
    assert!(
        export.stderr.is_empty(),
        "coverage export warnings are not success"
    );
    let export: Value = serde_json::from_slice(&export.stdout).map_err(|error| error.to_string())?;
    let coverage = owned_coverage(&export, &source)?;
    let mut arguments = arguments.to_vec();
    arguments.push("-format=lcov".into());
    let lcov = native_output(
        root,
        &format!("{mode}-lcov"),
        tools.cov.as_os_str(),
        &arguments,
        &[],
    )?;
    assert!(lcov.stderr.is_empty(), "LCOV export warnings are not success");
    let lines = owned_lcov_lines(native_text(&lcov)?, &source)?;
    assert_eq!(lines.get(&4), Some(&2), "the hot return executed twice");
    assert_eq!(lines.get(&6), Some(&0), "the cold return never executed");
    Ok(StripObservation {
        bytes: fs::metadata(executable)
            .map_err(|error| error.to_string())?
            .len(),
        build_id,
        lines,
        coverage,
    })
}

#[test]
fn native_test_profile_strip_preserves_coverage_and_reduces_fixture_size() -> Result<()> {
    let mut fixture = StripFixture::new()?;
    fs::create_dir(fixture.root.join("src")).map_err(|error| error.to_string())?;
    fs::write(fixture.root.join("src/lib.rs"), STRIP_SOURCE).map_err(|error| error.to_string())?;
    fs::write(
        fixture.root.join("Cargo.toml"),
        concat!(
            "[package]\nname = \"ripr_coverage_strip_fixture\"\n",
            "version = \"0.0.0\"\nedition = \"2024\"\n",
            "[workspace]\n[profile.dev]\ndebug = \"line-tables-only\"\n",
        ),
    )
    .map_err(|error| error.to_string())?;
    let tools = strip_tools(&fixture.root)?;
    let baseline = observe_strip_variant(&fixture.root, &tools, "none")?;
    let stripped = observe_strip_variant(&fixture.root, &tools, "debuginfo")?;
    assert_eq!(baseline.lines, stripped.lines, "all owned source line counts");
    assert_eq!(
        baseline.coverage, stripped.coverage,
        "source regions and function counts"
    );
    assert!(
        stripped.bytes < baseline.bytes,
        "the fixture must actually shrink"
    );
    // Direct bounded output remains visible in the mandatory libtest preflight.
    // Fixture savings do not predict the full workspace's allocation reduction.
    let record = format!(
        "\nripr-coverage-strip {}\n",
        json!({
            "native_test_profile_strip": "passed",
            "tools": tools.versions,
            "baseline_bytes": baseline.bytes,
            "stripped_bytes": stripped.bytes,
            "baseline_build_id": baseline.build_id,
            "stripped_build_id": stripped.build_id,
            "invocations_per_variant": 2,
            "hot_line_count": 2,
            "cold_line_count": 0,
        })
    );
    assert!(record.len() <= LINE_BYTES);
    fixture.remove()?;
    write_record(&mut io::stdout().lock(), &record).map_err(|error| error.to_string())?;
    Ok(())
}
