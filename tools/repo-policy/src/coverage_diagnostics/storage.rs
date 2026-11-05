//! Metadata-only observations around one fixed coverage command (#4978).
//! Observation limits never stop Cargo or change its environment or result.

use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use super::Result;
use super::process::{CARGO_ARGS, LINE_BYTES, coverage_command, run_observed, write_record};

const SAMPLE_ENTRIES: usize = 100_000;
const SAMPLE_DEPTH: usize = 32;
const SAMPLE_TIME: Duration = Duration::from_secs(5);
const SAMPLE_INTERVAL: Duration = Duration::from_mins(1);
const PERIODIC_SAMPLES: usize = 46;
const SNAPSHOT_BYTES: usize = 16 * 1024;
const PROFILE_ROWS: usize = 8;

pub(super) fn run() -> Result<i32> {
    let root = std::env::current_dir().map_err(|error| error.to_string())?;
    let roots = observed_roots(&root);
    let started = Instant::now();
    let mut sequence = 0;
    run_observed(
        coverage_command(),
        SAMPLE_INTERVAL,
        PERIODIC_SAMPLES,
        |event| {
            let snapshot = snapshot(&roots, Limits::default());
            if let Err(error) = emit_snapshot(
                &mut io::stdout().lock(),
                &event,
                &snapshot,
                sequence,
                started.elapsed().as_millis(),
            ) {
                let _ = writeln!(
                    io::stderr().lock(),
                    "coverage storage receipt unavailable: {}",
                    error.kind()
                );
            }
            sequence += 1;
        },
    )
}

#[derive(Clone, Copy)]
struct Limits {
    entries: usize,
    depth: usize,
    time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            entries: SAMPLE_ENTRIES,
            depth: SAMPLE_DEPTH,
            time: SAMPLE_TIME,
        }
    }
}

#[derive(Serialize, Default)]
struct Totals {
    entries: u64,
    unique_inodes: u64,
    regular_files: u64,
    directories: u64,
    symlinks: u64,
    other_entries: u64,
    logical_file_bytes: u64,
    allocated_bytes: u64,
    duplicate_inodes: u64,
}

#[derive(Serialize, Default)]
struct Snapshot {
    partial: bool,
    entries_inspected: usize,
    buckets: BTreeMap<&'static str, Totals>,
    roots: BTreeMap<&'static str, Value>,
    filesystems: BTreeMap<u64, Value>,
    errors: BTreeMap<String, u64>,
    limits_hit: BTreeSet<&'static str>,
    largest_profile_entries: Vec<ProfileEntry>,
}

#[derive(Serialize)]
struct ProfileEntry {
    name: String,
    name_truncated: bool,
    device: u64,
    inode: u64,
    logical_bytes: u64,
    allocated_bytes: u64,
}

struct Root {
    label: &'static str,
    path: PathBuf,
    target: bool,
}

fn observed_roots(workspace: &Path) -> Vec<Root> {
    let mut roots = vec![Root {
        label: "target",
        path: workspace.join("target"),
        target: true,
    }];
    roots.push(Root {
        label: "checkout_git",
        path: workspace.join(".git"),
        target: false,
    });
    for (label, variable, fallback) in [
        ("cargo_home", "CARGO_HOME", Some(".cargo")),
        ("rustup_home", "RUSTUP_HOME", Some(".rustup")),
        ("runner_temp", "RUNNER_TEMP", None),
    ] {
        let path = std::env::var_os(variable).map(PathBuf::from).or_else(|| {
            fallback.and_then(|suffix| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(suffix))
            })
        });
        if let Some(path) = path {
            roots.push(Root {
                label,
                path: workspace.join(path),
                target: false,
            });
        }
    }
    roots
}

fn bucket(root: &Root, relative: &Path) -> &'static str {
    if !root.target {
        return root.label;
    }
    if relative.starts_with("llvm-cov-target") {
        if relative.parent() == Some(Path::new("llvm-cov-target"))
            && relative.extension().is_some_and(|extension| extension == "profraw")
        {
            return "raw_profiles";
        }
        return "instrumented_build";
    }
    if relative.starts_with("debug") || relative.starts_with("release") {
        return "plain_build";
    }
    if relative.starts_with("ripr") {
        return "ripr_artifacts";
    }
    if relative
        .components()
        .any(|component| component == Component::Normal("native-target".as_ref()))
    {
        return "fixture_native_build";
    }
    "other_target"
}

struct Scan {
    result: Snapshot,
    seen: HashSet<(u64, u64)>,
    started: Instant,
    limits: Limits,
}

impl Scan {
    fn profile_entry(&mut self, name: &std::ffi::OsStr, metadata: &Metadata) {
        let mut name = name.to_string_lossy().into_owned();
        let name_truncated = name.len() > 128;
        if name_truncated {
            let mut end = 128;
            while !name.is_char_boundary(end) {
                end -= 1;
            }
            name.truncate(end);
        }
        self.result.largest_profile_entries.push(ProfileEntry {
            name,
            name_truncated,
            device: metadata.dev(),
            inode: metadata.ino(),
            logical_bytes: metadata.len(),
            allocated_bytes: metadata.blocks().saturating_mul(512),
        });
        self.result.largest_profile_entries.sort_by(|left, right| {
            right
                .allocated_bytes
                .cmp(&left.allocated_bytes)
                .then_with(|| right.logical_bytes.cmp(&left.logical_bytes))
                .then_with(|| left.name.cmp(&right.name))
        });
        self.result.largest_profile_entries.truncate(PROFILE_ROWS);
    }

    fn error(&mut self, operation: &str, error: &io::Error) {
        self.result.partial = true;
        let key = format!("{operation}:{}", error.kind());
        *self.result.errors.entry(key).or_default() += 1;
    }

    fn limited(&mut self, reason: &'static str) {
        self.result.partial = true;
        self.result.limits_hit.insert(reason);
    }

    fn available(&mut self) -> bool {
        if self.result.entries_inspected >= self.limits.entries {
            self.limited("entries");
            return false;
        }
        if self.started.elapsed() >= self.limits.time {
            self.limited("time");
            return false;
        }
        true
    }

    fn account(&mut self, label: &'static str, metadata: &Metadata) {
        let totals = self.result.buckets.entry(label).or_default();
        let mut overflow = false;
        totals.entries += 1;
        if metadata.is_file() {
            totals.regular_files += 1;
            overflow |= totals
                .logical_file_bytes
                .checked_add(metadata.len())
                .is_none();
            totals.logical_file_bytes = totals.logical_file_bytes.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            totals.directories += 1;
        } else if metadata.file_type().is_symlink() {
            totals.symlinks += 1;
        } else {
            totals.other_entries += 1;
        }
        if self.seen.insert((metadata.dev(), metadata.ino())) {
            totals.unique_inodes += 1;
            overflow |= metadata.blocks().checked_mul(512).is_none();
            overflow |= totals
                .allocated_bytes
                .checked_add(metadata.blocks().saturating_mul(512))
                .is_none();
            totals.allocated_bytes = totals
                .allocated_bytes
                .saturating_add(metadata.blocks().saturating_mul(512));
        } else {
            totals.duplicate_inodes += 1;
        }
        if overflow {
            self.limited("numeric_overflow");
        }
    }

    fn directory(&mut self, directory: &File, root: &Root, relative: &Path, depth: usize) {
        if depth >= self.limits.depth {
            self.limited("depth");
            return;
        }
        let entries = match fs::read_dir(descriptor_path(directory)) {
            Ok(entries) => entries,
            Err(error) => {
                self.error("read_directory", &error);
                return;
            }
        };
        for entry in entries {
            if !self.available() {
                return;
            }
            self.result.entries_inspected += 1;
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    self.error("directory_entry", &error);
                    continue;
                }
            };
            let path = entry.path();
            let relative = relative.join(entry.file_name());
            // Direct raw profiles were prioritized before the recursive walk.
            if bucket(root, &relative) == "raw_profiles" {
                continue;
            }
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) => {
                    self.error("metadata", &error);
                    continue;
                }
            };
            self.account(bucket(root, &relative), &metadata);
            if metadata.is_dir() {
                match open_directory(&path) {
                    Ok(child) => match child.metadata() {
                        Ok(opened) if same_file(&metadata, &opened) => {
                            self.directory(&child, root, &relative, depth + 1);
                        }
                        Ok(_) => self.limited("directory_identity_changed"),
                        Err(error) => self.error("opened_metadata", &error),
                    },
                    Err(error) => self.error("open_directory", &error),
                }
            }
        }
    }

    fn profiles(&mut self, root: &Root) {
        let directory = match open_chain(&root.path.join("llvm-cov-target")) {
            Ok(directory) => directory,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return,
            Err(error) => {
                self.error("profile_directory", &error);
                return;
            }
        };
        let entries = match fs::read_dir(descriptor_path(&directory)) {
            Ok(entries) => entries,
            Err(error) => {
                self.error("profile_entries", &error);
                return;
            }
        };
        for entry in entries {
            if !self.available() {
                return;
            }
            self.result.entries_inspected += 1;
            match entry {
                Ok(entry) if entry.path().extension().is_some_and(|ext| ext == "profraw") => {
                    match fs::symlink_metadata(entry.path()) {
                        Ok(metadata) => {
                            self.account("raw_profiles", &metadata);
                            if metadata.is_file() {
                                self.profile_entry(&entry.file_name(), &metadata);
                            } else {
                                self.limited("non_regular_profile_entry");
                            }
                        }
                        Err(error) => self.error("profile_metadata", &error),
                    }
                }
                Ok(_) => {}
                Err(error) => self.error("profile_entry", &error),
            }
        }
    }
}

fn descriptor_path(directory: &File) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
}

fn open_directory(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
}

fn open_chain(path: &Path) -> io::Result<File> {
    if !path.is_absolute() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "relative root"));
    }
    let mut directory = open_directory(Path::new("/"))?;
    for (depth, component) in path.components().enumerate() {
        if depth >= 128 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "root depth"));
        }
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => {
                directory = open_directory(&descriptor_path(&directory).join(name))?;
            }
            _ => {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "parent root"));
            }
        }
    }
    Ok(directory)
}

fn same_file(left: &Metadata, right: &Metadata) -> bool {
    left.dev() == right.dev() && left.ino() == right.ino()
}

fn free_space(directory: &File) -> Value {
    match rustix::fs::fstatvfs(directory) {
        Ok(stats) => capacity_values(stats.f_bavail, stats.f_frsize, stats.f_favail),
        Err(error) => json!({
            "status": "unavailable",
            "error": "fstatvfs",
            "detail": error.to_string(),
        }),
    }
}

fn capacity_values(available_blocks: u64, fragment_bytes: u64, available_inodes: u64) -> Value {
    if fragment_bytes == 0 {
        return json!({"status": "unavailable", "error": "capacity_fragment_size"});
    }
    match available_blocks.checked_mul(fragment_bytes) {
        Some(bytes) => json!({
            "status": "available",
            "available_bytes": bytes,
            "available_inodes": available_inodes,
        }),
        None => json!({"status": "unavailable", "error": "capacity_byte_overflow"}),
    }
}

fn snapshot(roots: &[Root], limits: Limits) -> Snapshot {
    let mut scan = Scan {
        result: Snapshot::default(),
        seen: HashSet::new(),
        started: Instant::now(),
        limits,
    };
    // Capacity is sampled before expensive walks, once per observed device.
    for root in roots {
        match open_chain(&root.path) {
            Ok(directory) => match directory.metadata() {
                Ok(metadata) => {
                    let space = scan
                        .result
                        .filesystems
                        .entry(metadata.dev())
                        .or_insert_with(|| free_space(&directory));
                    if space["status"] != "available" {
                        scan.result.partial = true;
                    }
                    scan.result.roots.insert(
                        root.label,
                        json!({"status": "available", "device": metadata.dev()}),
                    );
                }
                Err(error) => {
                    scan.result
                        .roots
                        .insert(root.label, json!({"status": "unavailable"}));
                    scan.error("root_metadata", &error);
                }
            },
            Err(error) => {
                scan.result
                    .roots
                    .insert(root.label, json!({"status": "unavailable"}));
                scan.error("root_open", &error);
            }
        }
    }
    for root in roots.iter().filter(|root| root.target) {
        scan.profiles(root);
    }
    let mut walked: Vec<&Path> = Vec::new();
    for root in roots {
        if walked.iter().any(|prior| root.path.starts_with(prior)) {
            scan.result.roots.insert(
                root.label,
                json!({"status": "included_in_prior_root"}),
            );
            continue;
        }
        if walked.iter().any(|prior| prior.starts_with(&root.path)) {
            scan.result.roots.insert(
                root.label,
                json!({"status": "overlapping_root_not_scanned"}),
            );
            scan.limited("overlapping_root");
            continue;
        }
        if !scan.available() {
            scan.result
                .roots
                .insert(root.label, json!({"status": "not_scanned_limit"}));
            continue;
        }
        match open_chain(&root.path) {
            Ok(directory) => {
                match directory.metadata() {
                    Ok(metadata) => scan.account(bucket(root, Path::new("")), &metadata),
                    Err(error) => scan.error("root_metadata", &error),
                }
                scan.directory(&directory, root, Path::new(""), 0);
                walked.push(&root.path);
            }
            Err(error) => scan.error("root_open", &error),
        }
    }
    scan.result
}

fn encoded_record(value: &Value) -> (String, bool) {
    let mut line = format!("\nripr-coverage-storage {value}\n");
    let partial = line.len() > LINE_BYTES;
    if partial {
        line = format!(
            "\nripr-coverage-storage {}\n",
            json!({
                "schema_version": "coverage_storage.v1",
                "sequence": value["sequence"].as_u64(),
                "kind": "partial_record",
                "phase": value["event"]["phase"].as_str().filter(|phase| {
                    matches!(*phase, "before" | "during" | "final")
                }),
                "exit": value["event"]["exit"].as_i64(),
                "signal": value["event"]["signal"].as_i64(),
                "partial": true,
                "error": "receipt_byte_limit",
            })
        );
    }
    (line, partial)
}

fn emit(writer: &mut impl Write, value: &Value) -> io::Result<()> {
    write_record(writer, &encoded_record(value).0)
}

fn emit_snapshot(
    writer: &mut impl Write,
    event: &Value,
    snapshot: &Snapshot,
    sequence: u64,
    elapsed_ms: u128,
) -> io::Result<()> {
    let header = json!({
        "schema_version": "coverage_storage.v1",
        "kind": "summary",
        "sequence": sequence,
        "elapsed_ms": elapsed_ms,
        "command": {"program": "cargo", "arguments": CARGO_ARGS},
        "non_atomic": true,
        "allocation_assignment": "first_seen_device_inode",
        "event": event,
        "partial": snapshot.partial,
        "entries_inspected": snapshot.entries_inspected,
        "roots": snapshot.roots,
        "filesystems": snapshot.filesystems,
        "errors": snapshot.errors,
        "limits_hit": snapshot.limits_hit,
    });
    let (line, mut output_partial) = encoded_record(&header);
    let mut written = line.len();
    write_record(writer, &line)?;
    let mut emitted = 0;
    for (label, totals) in &snapshot.buckets {
        let record = json!({
            "schema_version": "coverage_storage.v1",
            "kind": "bucket",
            "sequence": sequence,
            "bucket": label,
            "scope_partial": snapshot.partial,
            "totals": totals,
        });
        let (line, partial) = encoded_record(&record);
        // Reserve ample space for the mandatory completion/partiality record.
        if written + line.len() + 512 > SNAPSHOT_BYTES {
            output_partial = true;
            break;
        }
        write_record(writer, &line)?;
        written += line.len();
        if !partial {
            emitted += 1;
        }
        output_partial |= partial;
    }
    let mut profiles = 0;
    for profile in &snapshot.largest_profile_entries {
        let record = json!({
            "schema_version": "coverage_storage.v1",
            "kind": "profile_entry",
            "sequence": sequence,
            "scope_partial": snapshot.partial,
            "entry": profile,
        });
        let (line, partial) = encoded_record(&record);
        if written + line.len() + 512 > SNAPSHOT_BYTES {
            output_partial = true;
            break;
        }
        write_record(writer, &line)?;
        written += line.len();
        if !partial {
            profiles += 1;
        }
        output_partial |= partial;
    }
    emit(
        writer,
        &json!({
            "schema_version": "coverage_storage.v1",
            "kind": "end",
            "sequence": sequence,
            "partial": snapshot.partial || output_partial,
            "output_partial": output_partial,
            "emitted_buckets": emitted,
            "omitted_buckets": snapshot.buckets.len().saturating_sub(emitted),
            "emitted_profile_entries": profiles,
            "omitted_profile_entries": snapshot.largest_profile_entries.len().saturating_sub(profiles),
        }),
    )
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
