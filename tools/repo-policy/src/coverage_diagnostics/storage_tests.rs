use super::*;
use std::os::unix::fs::symlink;

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
    if open_chain(&linked).is_ok() {
        return Err("open_chain accepted a symlink root".into());
    }
    if open_chain(&linked.join("deep")).is_ok() {
        return Err("open_chain traversed a symlink ancestor".into());
    }
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
