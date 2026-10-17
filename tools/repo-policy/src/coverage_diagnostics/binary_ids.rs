//! Bounded Linux ELF build-ID association; never test or termination attribution.
use super::{Result, hex};
#[cfg(target_os = "linux")]
use super::{display_bytes, file_identity};
use serde_json::{Value, json};
use std::path::Path;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
use std::time::{Duration, Instant};

const IDS: usize = 32;
const ID_BYTES: usize = 128;
const HEADER_BYTES: usize = 128;
const ID_BLOCK_BYTES: usize = 8192;
const JSON_BYTES: usize = 32768;
const ROOTS: [&str; 4] = ["debug", "debug/deps", "release", "release/deps"];

// LLVM 22 raw64 version 10: 16 u64 header fields, followed by length-prefixed
// binary IDs padded to eight bytes. Unknown layouts stay explicitly unavailable.
// compiler-rt's Linux writer emits GNU NT_GNU_BUILD_ID descriptors from PT_NOTE.
pub(super) fn profile_ids(prefix: &[u8]) -> Value {
    let mut ids = Vec::new();
    let mut parse = || -> std::result::Result<(), &'static str> {
        if prefix.len() < HEADER_BYTES {
            return Err("truncated_header");
        }
        if prefix[..8] != [0x81, 0x72, 0x66, 0x6f, 0x72, 0x70, 0x6c, 0xff] {
            return Err("unsupported_magic");
        }
        // Keep the full version visible; only the known layout is admitted.
        if u64le(prefix, 8).map_err(|_error| "truncated_field")? != 10 {
            return Err("unsupported_raw_version");
        }
        let size = usize::try_from(u64le(prefix, 16).map_err(|_error| "truncated_field")?)
            .map_err(|_error| "id_block_limit")?;
        if size > ID_BLOCK_BYTES {
            return Err("id_block_limit");
        }
        let block = prefix
            .get(HEADER_BYTES..HEADER_BYTES.checked_add(size).ok_or("id_block_limit")?)
            .ok_or("truncated_id_block")?;
        let mut at = 0;
        while at < block.len() {
            if ids.len() == IDS {
                return Err("id_count_limit");
            }
            let size = usize::try_from(u64le(block, at).map_err(|_error| "invalid_id_length")?)
                .map_err(|_error| "invalid_id_length")?;
            if size == 0 || size > ID_BYTES {
                return Err("invalid_id_length");
            }
            at = at.checked_add(8).ok_or("invalid_id_length")?;
            let end = at.checked_add(size).ok_or("invalid_id_length")?;
            ids.push(hex(block.get(at..end).ok_or("truncated_id")?));
            at = end.checked_add(7).ok_or("invalid_id_length")? & !7;
            if at > block.len() {
                return Err("truncated_id_padding");
            }
        }
        Ok(())
    };
    match parse() {
        Ok(()) => {
            json!({"status": "available", "ids": ids, "layout": "llvm_raw64_le_v10"})
        }
        Err(reason) if ids.is_empty() => json!({
            "status": "unavailable",
            "truncated": reason.contains("limit") || reason.contains("truncated"),
            "reason": reason,
            "ids": ids
        }),
        Err(reason) => json!({
            "status": "partial",
            "truncated": reason.contains("limit") || reason.contains("truncated"),
            "reason": reason,
            "ids": ids,
            "layout": "llvm_raw64_le_v10"
        }),
    }
}

fn u64le(bytes: &[u8], at: usize) -> Result<u64> {
    let value: [u8; 8] = bytes
        .get(at..at.saturating_add(8))
        .ok_or("truncated_field")?
        .try_into()
        .map_err(|_error| "truncated_field")?;
    Ok(u64::from_le_bytes(value))
}
#[cfg(target_os = "linux")]
fn u32le(bytes: &[u8], at: usize) -> Result<u32> {
    let value: [u8; 4] = bytes
        .get(at..at.saturating_add(4))
        .ok_or("truncated_field")?
        .try_into()
        .map_err(|_error| "truncated_field")?;
    Ok(u32::from_le_bytes(value))
}
#[cfg(target_os = "linux")]
fn u16le(bytes: &[u8], at: usize) -> Result<u16> {
    let value: [u8; 2] = bytes
        .get(at..at.saturating_add(2))
        .ok_or("truncated_field")?
        .try_into()
        .map_err(|_error| "truncated_field")?;
    Ok(u16::from_le_bytes(value))
}

#[derive(Clone, Copy)]
struct Limits {
    entries: usize,
    files: usize,
    bytes: usize,
    matches: usize,
    elapsed: Duration,
}
const LIMITS: Limits = Limits {
    entries: 1024,
    files: 256,
    bytes: 2 * 1024 * 1024,
    matches: 16,
    elapsed: Duration::from_secs(2),
};

fn base_report(ids: &[String]) -> Value {
    json!({"status": "unavailable", "scope": ROOTS,
        "association": "exact GNU build ID plus LLVM profile sections; no test or cause attribution",
        "ids": ids.iter().map(|id| json!({"id": id, "status": "unavailable", "matches": []})).collect::<Vec<_>>(),
        "candidates": [], "non_candidates": 0, "truncated": false,
        "profile_ids_truncated": false, "profile_ids_incomplete": false,
        "limits": {"entries": LIMITS.entries, "files": LIMITS.files, "read_bytes": LIMITS.bytes,
            "bytes_per_file": 65536, "matches": LIMITS.matches, "json_bytes": JSON_BYTES,
            "elapsed_ms": LIMITS.elapsed.as_millis()}})
}

pub(super) fn associate(build: &Path, captured: &[Value]) -> Value {
    let mut ids = Vec::new();
    let mut ids_truncated = false;
    let mut profile_incomplete = false;
    for profile in captured {
        for id in profile["binary_ids"]["ids"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            if !ids.iter().any(|known| known == id) {
                if ids.len() == IDS {
                    ids_truncated = true;
                } else {
                    ids.push(id.to_owned());
                }
            }
        }
        profile_incomplete |= profile["binary_ids"]["status"] != "available";
    }
    let mut report = associate_with_limits(build, &ids, LIMITS);
    report["profile_ids_truncated"] = json!(ids_truncated);
    report["profile_ids_incomplete"] = json!(profile_incomplete);
    if ids_truncated || profile_incomplete {
        report["status"] = json!("partial");
        for row in report["ids"].as_array_mut().into_iter().flatten() {
            if row["status"] == "not_found_within_inspected_scope" {
                row["status"] = json!("unavailable_within_partial_scope");
            }
        }
    }
    report
}

fn associate_with_limits(build: &Path, ids: &[String], limits: Limits) -> Value {
    let mut report = base_report(ids);
    if ids.is_empty() {
        report["reason"] = json!("no_available_profile_binary_ids");
        return report;
    }
    if !cfg!(target_os = "linux") {
        report["reason"] = json!("unsupported_platform");
        return report;
    }
    if limits.elapsed.is_zero() {
        report["reason"] = json!("deadline");
        report["truncated"] = json!(true);
        return report;
    }
    let build = build.to_owned();
    let ids = ids.to_vec();
    let (tx, rx) = std::sync::mpsc::channel();
    let deadline = Instant::now() + limits.elapsed;
    if std::thread::Builder::new()
        .name("coverage-elf-ids".into())
        .spawn(move || {
            let result = scan(&build, &ids, limits, deadline);
            let _ = tx.send(result);
        })
        .is_err()
    {
        report["reason"] = json!("mapping_worker_unavailable");
        return report;
    }
    // Like bounded tool-pipe draining, a blocked filesystem read cannot hold
    // profile retention hostage. The worker owns only bounded read resources;
    // it writes nothing and stops at its deadline after any blocked call returns.
    match rx.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) => result,
        Err(_) => {
            report["reason"] = json!("deadline_or_worker_unavailable");
            report["truncated"] = json!(true);
            report
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn scan(_build: &Path, ids: &[String], _limits: Limits, _deadline: Instant) -> Value {
    base_report(ids)
}

#[cfg(target_os = "linux")]
mod elf {
    use super::*;
    use std::fs::{self, File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::os::unix::{
        ffi::OsStrExt,
        fs::{FileExt, OpenOptionsExt, PermissionsExt},
    };
    use std::path::Component;

    // Open every directory component without following links. Children are
    // opened relative to these retained directory descriptors, not mutable paths.
    pub(super) fn directory(path: &Path) -> Result<File> {
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else {
            std::env::current_dir()
                .map_err(|_error| "cwd_unavailable")?
                .join(path)
        };
        if absolute.components().count() > 128 {
            return Err("root_depth_limit".into());
        }
        let mut directory = File::open("/").map_err(|_error| "root_unavailable")?;
        for component in absolute.components() {
            match component {
                Component::RootDir => (),
                Component::Normal(name) => {
                    let anchored =
                        PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
                            .join(name);
                    directory = OpenOptions::new()
                        .read(true)
                        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
                        .open(anchored)
                        .map_err(|_error| "directory_unavailable_or_symlink")?;
                }
                _ => return Err("outside_or_non_normal_root".into()),
            }
        }
        Ok(directory)
    }

    pub(super) struct Budget {
        pub bytes: usize,
        pub deadline: Instant,
        pub limit: usize,
    }
    impl Budget {
        fn read(
            &mut self,
            file: &File,
            offset: u64,
            count: usize,
            size: u64,
            local: &mut usize,
        ) -> Result<Vec<u8>> {
            if Instant::now() >= self.deadline {
                return Err("deadline".into());
            }
            if count > 65536_usize.saturating_sub(*local)
                || count > self.limit.saturating_sub(self.bytes)
            {
                return Err("read_limit".into());
            }
            if offset
                .checked_add(count as u64)
                .is_none_or(|end| end > size)
            {
                return Err("extent_outside_file".into());
            }
            self.bytes += count;
            *local += count;
            let mut bytes = vec![0; count];
            file.read_exact_at(&mut bytes, offset)
                .map_err(|_error| "read_unavailable")?;
            Ok(bytes)
        }
    }

    pub(super) fn inspect(
        path: &Path,
        expected: &fs::Metadata,
        budget: &mut Budget,
    ) -> Result<(Vec<String>, u16)> {
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|_error| "file_unavailable_or_symlink")?;
        let opened = file.metadata().map_err(|_error| "metadata_unavailable")?;
        if !opened.is_file() || file_identity(&opened) != file_identity(expected) {
            return Err("identity_changed_or_nonregular".into());
        }
        let result = parse(&file, opened.len(), budget);
        let after = file.metadata().map_err(|_error| "metadata_unavailable")?;
        let source = fs::symlink_metadata(path).map_err(|_error| "source_unavailable")?;
        if file_identity(&after) != file_identity(expected)
            || !source.is_file()
            || file_identity(&source) != file_identity(expected)
        {
            return Err("identity_changed".into());
        }
        result
    }

    fn parse(file: &File, size: u64, budget: &mut Budget) -> Result<(Vec<String>, u16)> {
        let mut local = 0;
        let header = budget.read(file, 0, 64, size, &mut local)?;
        if header[..7] != [0x7f, b'E', b'L', b'F', 2, 1, 1] {
            return Err("unsupported_elf_layout".into());
        }
        if u32le(&header, 20)? != 1 {
            return Err("unsupported_elf_version".into());
        }
        if !matches!(u16le(&header, 16)?, 2 | 3) {
            return Err("not_executable_elf".into());
        }
        let machine = u16le(&header, 18)?;
        let segments = usize::from(u16le(&header, 56)?);
        if u16le(&header, 54)? != 56 || segments == 0 || segments > 64 {
            return Err("program_layout_or_limit".into());
        }
        let programs = budget.read(file, u64le(&header, 32)?, segments * 56, size, &mut local)?;
        let sections = usize::from(u16le(&header, 60)?);
        let strings_index = usize::from(u16le(&header, 62)?);
        if u16le(&header, 52)? != 64
            || u16le(&header, 58)? != 64
            || sections == 0
            || sections > 256
            || strings_index >= sections
        {
            return Err("section_layout_or_limit".into());
        }
        let table = budget.read(file, u64le(&header, 40)?, sections * 64, size, &mut local)?;
        let strings = &table[strings_index * 64..(strings_index + 1) * 64];
        if u32le(strings, 4)? != 3 {
            return Err("invalid_section_names".into());
        }
        let names_size = usize::try_from(u64le(strings, 32)?).map_err(|_error| "name_limit")?;
        if names_size > 32768 {
            return Err("name_limit".into());
        }
        let names = budget.read(file, u64le(strings, 24)?, names_size, size, &mut local)?;
        let mut data = false;
        let mut counters = false;
        let mut note = None;
        for section in table.chunks_exact(64) {
            let at =
                usize::try_from(u32le(section, 0)?).map_err(|_error| "invalid_section_name")?;
            let tail = names.get(at..).ok_or("invalid_section_name")?;
            let name = &tail[..tail
                .iter()
                .position(|b| *b == 0)
                .ok_or("invalid_section_name")?];
            let length = u64le(section, 32)?;
            let valid_profile_section = u32le(section, 4)? == 1
                && u64le(section, 8)? & 3 == 3
                && length > 0
                && u64le(section, 24)?
                    .checked_add(length)
                    .is_some_and(|end| end <= size);
            if name == b"__llvm_prf_data" || name == b".llvm_prf_data" {
                data |= valid_profile_section;
            }
            if name == b"__llvm_prf_cnts" || name == b".llvm_prf_cnts" {
                counters |= valid_profile_section;
            }
            if name == b".note.gnu.build-id" {
                if note.is_some() || u32le(section, 4)? != 7 {
                    return Err("ambiguous_build_id_section".into());
                }
                note = Some((u64le(section, 24)?, u64le(section, 32)?));
            }
        }
        if !data || !counters {
            return Err("not_profile_instrumented".into());
        }
        let (offset, length) = note.ok_or("build_id_unavailable")?;
        if !programs.chunks_exact(56).any(|segment| {
            u32le(segment, 0).ok() == Some(4)
                && u64le(segment, 8).ok().is_some_and(|start| start <= offset)
                && u64le(segment, 8)
                    .ok()
                    .zip(u64le(segment, 32).ok())
                    .and_then(|(start, count)| start.checked_add(count))
                    .zip(offset.checked_add(length))
                    .is_some_and(|(segment_end, note_end)| note_end <= segment_end)
        }) {
            return Err("build_id_not_in_note_segment".into());
        }
        let length = usize::try_from(length).map_err(|_error| "note_limit")?;
        if length > 4096 {
            return Err("note_limit".into());
        }
        let notes = budget.read(file, offset, length, size, &mut local)?;
        let mut ids = Vec::new();
        let mut at = 0;
        while at < notes.len() {
            let name_len = usize::try_from(u32le(&notes, at)?).map_err(|_error| "invalid_note")?;
            let id_len =
                usize::try_from(u32le(&notes, at + 4)?).map_err(|_error| "invalid_note")?;
            let kind = u32le(&notes, at + 8)?;
            let name_at = at.checked_add(12).ok_or("invalid_note")?;
            let id_at = name_at
                .checked_add(name_len.checked_add(3).ok_or("invalid_note")? & !3)
                .ok_or("invalid_note")?;
            let end = id_at.checked_add(id_len).ok_or("invalid_note")?;
            if kind == 3 && name_len == 4 && notes.get(name_at..name_at + 4) == Some(b"GNU\0") {
                if id_len == 0 || id_len > ID_BYTES || ids.len() == IDS {
                    return Err("invalid_build_id_or_limit".into());
                }
                ids.push(hex(notes.get(id_at..end).ok_or("invalid_note")?));
            }
            at = end.checked_add(3).ok_or("invalid_note")? & !3;
            if at > notes.len() {
                return Err("invalid_note".into());
            }
        }
        if ids.is_empty() {
            return Err("build_id_unavailable".into());
        }
        Ok((ids, machine))
    }

    pub(super) fn executable(metadata: &fs::Metadata) -> bool {
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    pub(super) fn relative_bytes(path: &Path) -> &[u8] {
        path.as_os_str().as_bytes()
    }
}

#[cfg(target_os = "linux")]
fn scan(build: &Path, ids: &[String], limits: Limits, deadline: Instant) -> Value {
    use std::{fs, os::fd::AsRawFd};
    let mut report = base_report(ids);
    let build_directory = match elf::directory(build) {
        Ok(directory) => directory,
        Err(reason) => {
            report["reason"] = json!(reason);
            return report;
        }
    };
    let mut budget = elf::Budget {
        bytes: 0,
        deadline,
        limit: limits.bytes,
    };
    let mut entries = 0;
    let mut inspected = 0;
    let mut unavailable = 0;
    let mut non_candidates = 0;
    let mut root_errors = Vec::new();
    let mut errors = Vec::new();
    let mut matches = Vec::new();
    let mut matched_identities: Vec<(u64, u64)> = Vec::new();
    let mut truncated = false;
    'roots: for root in ROOTS {
        if Instant::now() >= deadline || entries >= limits.entries || inspected >= limits.files {
            truncated = true;
            break;
        }
        let mut current = build_directory.try_clone().ok();
        for part in root.split('/') {
            current = current.and_then(|parent| {
                use std::os::unix::fs::OpenOptionsExt;
                fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
                    .open(PathBuf::from(format!("/proc/self/fd/{}", parent.as_raw_fd())).join(part))
                    .ok()
            });
        }
        let directory = match current {
            Some(directory) => directory,
            None => {
                root_errors.push(root);
                continue;
            }
        };
        let anchor = PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()));
        let listing = match fs::read_dir(&anchor) {
            Ok(listing) => listing,
            Err(_) => {
                root_errors.push(root);
                continue;
            }
        };
        let mut names = Vec::new();
        for item in listing {
            if entries == limits.entries {
                truncated = true;
                break;
            }
            entries += 1;
            match item {
                Ok(entry) => names.push(entry.file_name()),
                Err(_) => unavailable += 1,
            }
        }
        names.sort();
        for name in names {
            if Instant::now() >= deadline || inspected == limits.files {
                truncated = true;
                break 'roots;
            }
            let path = anchor.join(&name);
            let relative = Path::new(root).join(&name);
            let raw = elf::relative_bytes(&relative);
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => {
                    unavailable += 1;
                    continue;
                }
            };
            if metadata.is_dir() {
                continue;
            }
            if metadata.is_symlink() || !metadata.is_file() {
                unavailable += 1;
                continue;
            }
            if !elf::executable(&metadata) {
                continue;
            }
            if raw.len() > 160 {
                unavailable += 1;
                continue;
            }
            inspected += 1;
            match elf::inspect(&path, &metadata, &mut budget) {
                Ok((found, machine)) if found.iter().any(|id| ids.contains(id)) => {
                    let identity = file_identity(&metadata);
                    // Cargo hard-links debug/<name> to debug/deps/<name>-<hash>
                    // (likewise under release), so one executable can appear as
                    // direct entries in two scanned roots. One distinct
                    // executable is one candidate; only genuinely different
                    // files with the same ID stay ambiguous.
                    if matched_identities.contains(&(identity.device, identity.inode)) {
                        continue;
                    }
                    if matches.len() == limits.matches {
                        truncated = true;
                        break 'roots;
                    }
                    matched_identities.push((identity.device, identity.inode));
                    matches.push(json!({"relative_path": display_bytes(raw), "relative_path_hex": hex(raw),
                        "gnu_build_ids": found, "llvm_profile_sections": true, "elf_machine": machine,
                        "bytes": metadata.len(), "device": identity.device,
                        "inode": identity.inode}));
                }
                Ok(_) => (),
                Err(reason) => {
                    if reason == "not_profile_instrumented" {
                        non_candidates += 1;
                        continue;
                    }
                    unavailable += 1;
                    if errors.len() < 8 {
                        errors.push(reason.clone());
                    }
                    if matches!(reason.as_str(), "read_limit" | "deadline") {
                        truncated = true;
                        break 'roots;
                    }
                }
            }
        }
        if truncated {
            break;
        }
    }
    let partial = truncated || unavailable > 0 || !root_errors.is_empty();
    report["status"] = json!(if partial {
        "partial"
    } else {
        "inspected_scope"
    });
    report["truncated"] = json!(truncated);
    report["roots_unavailable"] = json!(root_errors);
    report["entries_seen"] = json!(entries);
    report["files_inspected"] = json!(inspected);
    report["read_bytes"] = json!(budget.bytes);
    report["candidates_unavailable"] = json!(unavailable);
    report["non_candidates"] = json!(non_candidates);
    report["error_samples"] = json!(errors);
    report["ids"] = json!(ids.iter().map(|id| {
        let matched = matches.iter().enumerate().filter_map(|(index, row)|
            row["gnu_build_ids"].as_array().is_some_and(|values| values.iter().any(|value| value == id)).then_some(index)).collect::<Vec<_>>();
        json!({"id": id, "status": if matched.is_empty() {
            if partial { "unavailable_within_partial_scope" } else { "not_found_within_inspected_scope" }
        } else { "matched" }, "ambiguous": matched.len() > 1, "matches": matched})
    }).collect::<Vec<_>>());
    report["candidates"] = json!(matches);
    if serde_json::to_vec_pretty(&report).map_or(true, |bytes| bytes.len() > JSON_BYTES) {
        let mut limited = base_report(ids);
        limited["reason"] = json!("json_limit");
        limited["truncated"] = json!(true);
        return limited;
    }
    report
}

#[cfg(all(test, target_os = "linux"))]
#[path = "binary_ids_tests.rs"]
mod tests;
