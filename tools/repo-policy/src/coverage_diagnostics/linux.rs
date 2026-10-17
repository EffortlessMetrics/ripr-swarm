//! Linux-only capture machinery for the coverage failure diagnostic adapter.
//!
//! Gated by `cfg(target_os = "linux")` at the [`super::command`] entry: the
//! adapter refuses every other host before any item here is reachable, and
//! the gate keeps non-Linux hosts from compiling (and then dead-code
//! rejecting, #7624) an unreferenced copy of the adapter. Linux owns the
//! actual LLVM proof; unsupported OSes compile the explicit adapter refusal
//! control instead. Missing Linux prerequisites are failures.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::Result;

#[path = "binary_ids.rs"]
mod binary_ids;

const ENTRIES: usize = 8192;
const ROWS: usize = 512;
const INVENTORY_BYTES: usize = 65536;
const LIST_BYTES: usize = 8388608;
const LIST_ENTRIES: usize = 65536;
const PROFILES: usize = 8;
const PREFIX_BYTES: usize = 65536;
const STREAM_BYTES: usize = 16384;
const ARTIFACT_BYTES: usize = 1048576;
const ARTIFACT_FILES: usize = 16;
const MANIFEST_RESERVE: usize = 448 * 1024;
const REPLAY_PROFILE_LIMIT: usize = 8192;
const TOOL_KILL_GRACE: Duration = Duration::from_secs(1);

fn drain(mut pipe: impl Read) -> Result<(Vec<u8>, bool)> {
    let mut retained = Vec::new();
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let count = pipe.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        let keep = count.min(STREAM_BYTES.saturating_sub(retained.len()));
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep != count;
    }
    Ok((retained, truncated))
}

fn tool(args: &[String]) -> Result<Value> {
    tool_with_deadline(args, Duration::from_secs(30))
}

fn kill_group(pgid: u32) -> Result<bool> {
    let group = format!("-{pgid}");
    Command::new("kill")
        .args(["-KILL", "--", group.as_str()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .map_err(|error| error.to_string())
}

fn tool_with_deadline(args: &[String], deadline_after: Duration) -> Result<Value> {
    let (program, arguments) = args.split_first().ok_or("empty tool argv")?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let pgid = child.id();
    let stdout = child.stdout.take().ok_or("stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("stderr unavailable")?;
    let (tx, rx) = mpsc::channel();
    let out_tx = tx.clone();
    let out_thread = std::thread::spawn(move || {
        let _ = out_tx.send((true, drain(stdout)));
    });
    let err_thread = std::thread::spawn(move || {
        let _ = tx.send((false, drain(stderr)));
    });
    let deadline = Instant::now() + deadline_after;
    let mut kill_deadline = None;
    let mut status: Option<ExitStatus> = None;
    let mut stdout = None;
    let mut stderr = None;
    let mut timed_out = false;
    let mut group_killed = false;
    let mut cleanup_error = None;

    while status.is_none() || stdout.is_none() || stderr.is_none() {
        if status.is_none() {
            status = match child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    let _ = kill_group(pgid);
                    return Err(error.to_string());
                }
            };
            if status.is_some() && kill_deadline.is_none() {
                match kill_group(pgid) {
                    Ok(terminated) => group_killed = terminated,
                    Err(error) => cleanup_error = Some(error),
                }
                kill_deadline = Some(Instant::now() + TOOL_KILL_GRACE);
            }
        }
        while let Ok((is_stdout, result)) = rx.try_recv() {
            if is_stdout {
                stdout = Some(result);
            } else {
                stderr = Some(result);
            }
        }
        let now = Instant::now();
        if now >= deadline && !timed_out {
            timed_out = true;
            match kill_group(pgid) {
                Ok(terminated) => group_killed |= terminated,
                Err(error) => cleanup_error = Some(error),
            }
            kill_deadline = Some(now + TOOL_KILL_GRACE);
        }
        if kill_deadline.is_some_and(|grace| now >= grace)
            && (status.is_none() || stdout.is_none() || stderr.is_none())
        {
            if status.is_none() {
                let _ = child.kill();
                status = child.try_wait().ok().flatten();
            }
            break;
        }
        if status.is_some() && stdout.is_some() && stderr.is_some() {
            break;
        }
        let wake_at = kill_deadline.unwrap_or(deadline);
        let wait = wake_at
            .saturating_duration_since(now)
            .min(Duration::from_millis(10));
        if let Ok((is_stdout, result)) = rx.recv_timeout(wait) {
            if is_stdout {
                stdout = Some(result);
            } else {
                stderr = Some(result);
            }
        }
    }

    // If a process escaped its group, detach its drain thread rather than
    // allowing inherited pipes to extend the advertised deadline.
    if stdout.is_some() {
        let _ = out_thread.join();
    }
    if stderr.is_some() {
        let _ = err_thread.join();
    }
    let stdout = stdout.unwrap_or_else(|| Ok((Vec::new(), true)))?;
    let stderr = stderr.unwrap_or_else(|| Ok((Vec::new(), true)))?;
    let mut result = tool_result(
        status.and_then(|status| status.code()),
        &stdout.0,
        &stderr.0,
        stdout.1 || stderr.1,
        timed_out,
        group_killed,
    );
    result["cleanup_error"] = json!(cleanup_error);
    Ok(result)
}

fn display_bytes(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => character,
            _ if !character.is_control() => character,
            _ => '?',
        })
        .collect()
}

fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

fn tool_result(
    status: Option<i32>,
    stdout: &[u8],
    stderr: &[u8],
    truncated: bool,
    timed_out: bool,
    descendants_terminated: bool,
) -> Value {
    json!({"exit": status, "stdout": display_bytes(stdout), "stderr": display_bytes(stderr),
        "stdout_retained_prefix_utf8_valid": is_valid_utf8(stdout), "stderr_retained_prefix_utf8_valid": is_valid_utf8(stderr),
        "stdout_hex": hex(stdout), "stderr_hex": hex(stderr), "truncated": truncated,
        "timed_out": timed_out, "descendants_terminated": descendants_terminated})
}

// Decode only the bounded prefix retained by drain; display text is never a
// machine path source. The unread suffix has no UTF-8 validity claim.
fn retained_text(value: &Value, stream: &str) -> Result<String> {
    let encoded = field(value, &format!("{stream}_hex"));
    if encoded.len() > STREAM_BYTES * 2 || !encoded.len().is_multiple_of(2) {
        return Err("invalid retained stream encoding".to_owned());
    }
    let bytes = encoded
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
            u8::from_str_radix(pair, 16).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    String::from_utf8(bytes).map_err(|error| error.to_string())
}

fn invoke(run: &impl Fn(&[String]) -> Result<Value>, args: &[&str]) -> Value {
    let args = args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    run(&args).unwrap_or_else(|e| {
        json!({"exit": null, "stdout": "", "stderr": e,
        "stdout_retained_prefix_utf8_valid": true, "stderr_retained_prefix_utf8_valid": true})
    })
}

fn field<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

fn no_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        if ancestor.is_symlink() {
            return Err("diagnostic path contains a symlink".into());
        }
    }
    Ok(())
}

fn read_prefix(path: &Path, limit: usize) -> Result<Vec<u8>> {
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .is_file()
    {
        return Err("profile input is not a regular file".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Use the target libc values; Linux open flag numbers vary by ABI.
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("profile input changed to a nonregular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn profile_list_source_recheck(listing: &Path, original: Option<&[u8]>) -> Value {
    let Some(original) = original else {
        return json!({"status": "not_attempted"});
    };
    match read_prefix(listing, LIST_BYTES + 1) {
        Ok(current) if current == original => json!({"status": "unchanged"}),
        Ok(_) => json!({"status": "changed"}),
        Err(error) => json!({"status": "unavailable", "error": short_error(error)}),
    }
}

struct Budget {
    root: PathBuf,
    bytes: usize,
    files: usize,
}
impl Budget {
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let data_limit = ARTIFACT_BYTES - MANIFEST_RESERVE;
        if self.files >= ARTIFACT_FILES - 1 || self.bytes + bytes.len() > data_limit {
            return Err("diagnostic artifact budget exceeded".into());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(name))
            .map_err(|e| e.to_string())?;
        // Reserve the full attempted write so a partial I/O failure cannot
        // make a later artifact exceed either aggregate budget.
        self.bytes += bytes.len();
        self.files += 1;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        Ok(())
    }

    fn write_manifest(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() > MANIFEST_RESERVE
            || self.files >= ARTIFACT_FILES
            || self.bytes + bytes.len() > ARTIFACT_BYTES
        {
            return Err("reserved diagnostic manifest budget exceeded".into());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join("manifest.json"))
            .map_err(|error| error.to_string())?;
        self.bytes += bytes.len();
        self.files += 1;
        file.write_all(bytes).map_err(|error| error.to_string())?;
        Ok(())
    }
}

fn owned_profile_path(path: &Path, profiles: &Path) -> bool {
    path.extension().is_some_and(|s| s == "profraw")
        && path.parent().and_then(|p| p.canonicalize().ok()) == profiles.canonicalize().ok()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
    bytes: u64,
    modified_seconds: i64,
    modified_nanoseconds: i64,
}

fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        FileIdentity {
            device: metadata.dev(),
            inode: metadata.ino(),
            bytes: metadata.len(),
            modified_seconds: metadata.mtime(),
            modified_nanoseconds: metadata.mtime_nsec(),
        }
    }
    #[cfg(not(unix))]
    {
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok());
        FileIdentity {
            device: 0,
            inode: 0,
            bytes: metadata.len(),
            modified_seconds: modified.map_or(0, |time| time.as_secs() as i64),
            modified_nanoseconds: modified.map_or(0, |time| time.subsec_nanos() as i64),
        }
    }
}

struct ValidatedInput {
    source: PathBuf,
    identity: FileIdentity,
}

struct ReplayEntry {
    source: PathBuf,
    pinned: PathBuf,
    identity: FileIdentity,
}

struct ReplaySnapshot {
    root: PathBuf,
    listing: PathBuf,
    entries: Vec<ReplayEntry>,
    listing_sha256: String,
}

impl Drop for ReplaySnapshot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn validate_profile_list(bytes: &[u8], profiles: &Path) -> Result<Vec<ValidatedInput>> {
    if bytes.len() > LIST_BYTES {
        return Err("profile list exceeds byte limit".into());
    }
    let text = std::str::from_utf8(bytes).map_err(|_error| "profile list is not UTF-8")?;
    if text.lines().count() > LIST_ENTRIES {
        return Err("profile list exceeds entry limit".into());
    }
    let mut inputs = Vec::new();
    for line in text.lines() {
        let source = PathBuf::from(line);
        if !owned_profile_path(&source, profiles) {
            return Err("profile list contains an outside or non-profile path".into());
        }
        let metadata = fs::symlink_metadata(&source).map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("profile list contains a non-regular input".into());
        }
        inputs.push(ValidatedInput {
            source,
            identity: file_identity(&metadata),
        });
    }
    Ok(inputs)
}

fn prepare_replay_snapshot(
    inputs: Vec<ValidatedInput>,
    output_parent: &Path,
) -> Result<ReplaySnapshot> {
    if inputs.len() > REPLAY_PROFILE_LIMIT {
        return Err("profile list exceeds replay identity limit".into());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let root = output_parent.join(format!(".coverage-replay-{}-{nonce}", std::process::id()));
    fs::create_dir(&root).map_err(|error| error.to_string())?;
    let root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let listing = root.join("validated-profraw-list");
    let mut snapshot = ReplaySnapshot {
        root: root.clone(),
        listing: listing.clone(),
        entries: Vec::with_capacity(inputs.len()),
        listing_sha256: String::new(),
    };
    let mut listing_bytes = Vec::new();
    for (index, input) in inputs.into_iter().enumerate() {
        let before = fs::symlink_metadata(&input.source).map_err(|error| error.to_string())?;
        if !before.is_file() || file_identity(&before) != input.identity {
            return Err("profile identity changed after list validation".into());
        }
        let pinned = root.join(format!("input-{index:05}.profraw"));
        fs::hard_link(&input.source, &pinned).map_err(|error| error.to_string())?;
        let linked = fs::symlink_metadata(&pinned).map_err(|error| error.to_string())?;
        if !linked.is_file() || file_identity(&linked) != input.identity {
            return Err("could not pin validated profile identity".into());
        }
        listing_bytes.extend_from_slice(pinned.to_string_lossy().as_bytes());
        listing_bytes.push(b'\n');
        snapshot.entries.push(ReplayEntry {
            source: input.source,
            pinned,
            identity: input.identity,
        });
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&listing)
        .map_err(|error| error.to_string())?;
    file.write_all(&listing_bytes)
        .map_err(|error| error.to_string())?;
    drop(file);
    let mut permissions = fs::metadata(&listing)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&listing, permissions).map_err(|error| error.to_string())?;
    snapshot.listing_sha256 = format!("{:x}", Sha256::digest(&listing_bytes));
    Ok(snapshot)
}

fn replay_inputs_stable(snapshot: &ReplaySnapshot) -> bool {
    snapshot.entries.iter().all(|entry| {
        fs::symlink_metadata(&entry.pinned)
            .is_ok_and(|metadata| metadata.is_file() && file_identity(&metadata) == entry.identity)
    })
}

fn replay_sources_stable(snapshot: &ReplaySnapshot) -> bool {
    snapshot.entries.iter().all(|entry| {
        fs::symlink_metadata(&entry.source)
            .is_ok_and(|metadata| metadata.is_file() && file_identity(&metadata) == entry.identity)
    })
}

fn candidate_record(
    budget: &mut Budget,
    read_path: &Path,
    source_name: &str,
    error_family: &str,
    index: usize,
) -> Value {
    let name = source_name.chars().take(240).collect::<String>();
    let attempt = (|| -> Result<Value> {
        let info = fs::symlink_metadata(read_path).map_err(|e| e.to_string())?;
        if !info.is_file() {
            return Err("profile input is not a regular file".into());
        }
        let data = read_prefix(read_path, PREFIX_BYTES)?;
        let artifact = format!("profile-{index:02}.prefix.profraw");
        budget.write(&artifact, &data)?;
        let mut parts = name.trim_end_matches(".profraw").rsplitn(3, '-');
        let module = parts.next();
        let pid = parts.next().and_then(|p| p.parse::<u64>().ok());
        Ok(json!({"name": name, "artifact": artifact,
            "original_bytes": info.len(), "retained_bytes": data.len(), "complete": data.len() as u64 == info.len(),
            "prefix_sha256": format!("{:x}", Sha256::digest(&data)), "first_256_hex": hex(&data[..data.len().min(256)]),
            "pid_hint": pid, "module_hint": module, "error_family": error_family,
            "binary_ids": binary_ids::profile_ids(&data)}))
    })();
    attempt.unwrap_or_else(
        |error| json!({"name": name, "error": short_error(error), "error_family": error_family}),
    )
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn profile_warning(line: &str) -> Option<(PathBuf, &'static str)> {
    let warning = line.strip_prefix("warning: ")?;
    [
        (
            ": invalid instrumentation profile data",
            "invalid_instrumentation_profile_data",
        ),
        (
            ": malformed instrumentation profile data",
            "malformed_instrumentation_profile_data",
        ),
        (
            ": unrecognized instrumentation profile encoding format",
            "unrecognized_instrumentation_profile_encoding_format",
        ),
    ]
    .into_iter()
    .find_map(|(marker, family)| {
        warning
            .find(marker)
            .map(|index| (PathBuf::from(&warning[..index]), family))
    })
}

fn short_error(error: impl AsRef<str>) -> String {
    error.as_ref().chars().take(240).collect()
}

fn capture(
    profiles: &Path,
    listing: &Path,
    output: &Path,
    run: impl Fn(&[String]) -> Result<Value>,
) -> Result<Value> {
    no_links(profiles)?;
    no_links(output)?;
    fs::create_dir_all(output.parent().ok_or("output parent")?).map_err(|e| e.to_string())?;
    fs::create_dir(output).map_err(|e| e.to_string())?;
    let mut budget = Budget {
        root: output.to_owned(),
        bytes: 0,
        files: 0,
    };
    let mut report = json!({"schema": 1, "original_coverage_outcome": "failure",
        "producer": "unresolved; filename PID/module are hints, not executable attribution",
        "limits": {"directory_entries": ENTRIES, "inventory_rows": ROWS,
            "inventory_json_bytes": INVENTORY_BYTES, "profile_list_bytes": LIST_BYTES,
            "profile_list_entries": LIST_ENTRIES, "replay_profile_entries": REPLAY_PROFILE_LIMIT,
            "malformed_prefixes": PROFILES,
            "profile_bytes_each": PREFIX_BYTES, "tool_bytes_each_stream": STREAM_BYTES,
            "tool_deadline_seconds": 30, "tool_kill_grace_seconds": TOOL_KILL_GRACE.as_secs(),
            "artifact_bytes": ARTIFACT_BYTES, "artifact_files": ARTIFACT_FILES,
            "manifest_reserve_bytes": MANIFEST_RESERVE},
        "inventory_truncated": false, "inventory_errors": 0, "inventory_error_samples": [],
        "profile_list_replay_safe": false,
        "captured": []});
    let mut rows = Vec::new();
    let mut row_bytes = 0;
    let mut inventory_errors = 0usize;
    let mut inventory_error_samples = Vec::new();
    let mut inventory_error = |error: String| {
        inventory_errors += 1;
        if inventory_error_samples.len() < 8 {
            inventory_error_samples.push(short_error(error));
        }
    };
    match fs::read_dir(profiles) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                if index >= ENTRIES {
                    report["inventory_truncated"] = json!(true);
                    break;
                }
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        inventory_error(error.to_string());
                        continue;
                    }
                };
                let path = entry.path();
                if path.extension().is_none_or(|s| s != "profraw") {
                    continue;
                }
                let info = match fs::symlink_metadata(&path) {
                    Ok(info) => info,
                    Err(error) => {
                        inventory_error(error.to_string());
                        continue;
                    }
                };
                if !info.is_file() {
                    continue;
                }
                let row = json!({"name": entry.file_name().to_string_lossy().chars().take(240).collect::<String>(),
                "bytes": info.len()});
                let size = serde_json::to_vec(&row).map_err(|e| e.to_string())?.len();
                let serialized_array_size = row_bytes + size + rows.len() + 2;
                if rows.len() >= ROWS || serialized_array_size > INVENTORY_BYTES {
                    report["inventory_truncated"] = json!(true);
                    continue;
                }
                row_bytes += size;
                rows.push(row);
            }
        }
        Err(error) => report["profiles_unavailable"] = json!(error.to_string()),
    }
    report["inventory_errors"] = json!(inventory_errors);
    report["inventory_error_samples"] = json!(inventory_error_samples);
    report["inventory"] = json!(rows);
    report["inventory_json_bytes"] = json!(
        serde_json::to_vec(&report["inventory"])
            .map_err(|error| error.to_string())?
            .len()
    );
    let mut replay_snapshot = None;
    let mut original_list_bytes = None;
    match read_prefix(listing, LIST_BYTES + 1) {
        Err(error) => report["profile_list_error"] = json!(short_error(error)),
        Ok(bytes) => {
            report["profile_list_bytes"] = json!(bytes.len());
            if bytes.len() > LIST_BYTES {
                report["profile_list_error"] = json!("profile list exceeds byte limit");
            } else if listing
                .parent()
                .and_then(|parent| parent.canonicalize().ok())
                != profiles.canonicalize().ok()
            {
                report["profile_list_error"] =
                    json!("profile list is outside the profile directory");
            } else {
                match validate_profile_list(&bytes, profiles).and_then(|inputs| {
                    prepare_replay_snapshot(inputs, output.parent().unwrap_or(output))
                }) {
                    Ok(snapshot) => {
                        report["profile_list_replay_safe"] = json!(true);
                        report["profile_list_sha256"] =
                            json!(format!("{:x}", Sha256::digest(&bytes)));
                        report["profile_list_snapshot_sha256"] =
                            json!(snapshot.listing_sha256.clone());
                        let prefix = &bytes[..bytes.len().min(PREFIX_BYTES)];
                        if let Err(error) = budget.write("profile-list-prefix.txt", prefix) {
                            report["profile_list_prefix_error"] = json!(short_error(error));
                        }
                        original_list_bytes = Some(bytes);
                        replay_snapshot = Some(snapshot);
                    }
                    Err(error) => {
                        report["profile_list_error"] = json!(short_error(error));
                    }
                }
            }
        }
    }
    let version = invoke(&run, &["rustc", "-vV"]);
    let sysroot = invoke(&run, &["rustc", "--print", "sysroot"]);
    report["rustc"] = version.clone();
    let commit = invoke(&run, &["git", "rev-parse", "HEAD"]);
    let sha = field(&commit, "stdout").trim();
    report["checkout_sha"] = json!(
        (commit["exit"] == 0 && sha.len() == 40 && sha.bytes().all(|c| c.is_ascii_hexdigit()))
            .then_some(sha)
    );
    let host = field(&version, "stdout")
        .lines()
        .find_map(|line| line.strip_prefix("host: "));
    let mut replay = json!({"exit": null, "stdout": "", "stderr": "validated profile snapshot or matching LLVM tool unavailable",
        "stdout_retained_prefix_utf8_valid": true, "stderr_retained_prefix_utf8_valid": true});
    if let Some(snapshot) = replay_snapshot.as_ref()
        && version["exit"] == 0
        && sysroot["exit"] == 0
        && let Some(host) = host.filter(|h| {
            h.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        })
    {
        let tool = Path::new(field(&sysroot, "stdout").trim())
            .join("lib/rustlib")
            .join(host)
            .join("bin/llvm-profdata");
        if let (Some(tool), Some(replay_list)) = (tool.to_str(), snapshot.listing.to_str()) {
            report["llvm_profdata"] = invoke(&run, &[tool, "--version"]);
            // The snapshot list names hard links to the exact validated profile
            // identities. Later changes to the caller's list or profile paths
            // cannot redirect LLVM to another file.
            replay = invoke(
                &run,
                &[
                    tool,
                    "merge",
                    "-sparse",
                    "-f",
                    replay_list,
                    "-o",
                    "/dev/null",
                ],
            );
        } else {
            report["profile_list_error"] = json!("snapshot or LLVM tool path is not UTF-8");
        }
    }
    report["profile_inputs_identity_stable"] =
        json!(replay_snapshot.as_ref().is_some_and(replay_inputs_stable));
    report["profile_source_paths_stable"] =
        json!(replay_snapshot.as_ref().is_some_and(replay_sources_stable));
    report["profile_list_source_recheck"] =
        profile_list_source_recheck(listing, original_list_bytes.as_deref());
    let mut candidates: Vec<(usize, &'static str)> = Vec::new();
    let mut unclassified_profile_warnings = 0usize;
    let exact_stderr = retained_text(&replay, "stderr");
    if exact_stderr.is_err() {
        report["profile_warning_parse_refusal"] = json!(
            "retained replay stderr prefix is not valid UTF-8; exact warning path matching refused"
        );
        unclassified_profile_warnings = field(&replay, "stderr")
            .lines()
            .filter(|line| line.starts_with("warning: "))
            .count();
    } else {
        for line in exact_stderr.as_deref().unwrap_or_default().split('\n') {
            if !line.starts_with("warning: ") {
                continue;
            }
            let Some((path, family)) = profile_warning(line) else {
                unclassified_profile_warnings += 1;
                continue;
            };
            let Some(snapshot) = replay_snapshot.as_ref() else {
                unclassified_profile_warnings += 1;
                continue;
            };
            let Some(index) = snapshot
                .entries
                .iter()
                .position(|entry| entry.pinned == path)
            else {
                unclassified_profile_warnings += 1;
                continue;
            };
            if !candidates.iter().any(|(candidate, _)| *candidate == index) {
                candidates.push((index, family));
            }
        }
    }
    report["unclassified_profile_warnings"] = json!(unclassified_profile_warnings);
    report["failed_profile_capture_truncated"] = json!(candidates.len() > PROFILES);
    let mut captured = Vec::new();
    if let Some(snapshot) = replay_snapshot.as_ref() {
        for (index, (entry_index, family)) in candidates.iter().take(PROFILES).enumerate() {
            if let Some(entry) = snapshot.entries.get(*entry_index) {
                let name = entry
                    .source
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "<unknown profile>".into());
                captured.push(candidate_record(
                    &mut budget,
                    &entry.pinned,
                    &name,
                    family,
                    index,
                ));
            }
        }
        for entry in &snapshot.entries {
            let Some(pinned) = entry.pinned.to_str() else {
                continue;
            };
            let original = entry
                .source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "<unknown profile>".into());
            let stderr = field(&replay, "stderr").replace(pinned, &original);
            replay["stderr"] = json!(stderr);
        }
    }
    if candidates.is_empty() {
        report["profile_capture_note"] = json!(
            "No supported malformed-profile diagnostic identified a validated replay input; no profile bytes were retained."
        );
    }
    report["executable_mapping"] = binary_ids::associate(profiles, &captured);
    report["captured"] = json!(captured);
    report["merge_replay"] = replay.clone();
    report["merge_replay_scope"] =
        json!("diagnostic only; original failed coverage step remains failed");
    report["artifact_bytes_before_manifest"] = json!(budget.bytes);
    let initial_manifest_bytes = serde_json::to_vec_pretty(&report)
        .map_err(|e| e.to_string())?
        .len();
    report["manifest_bytes"] = json!(initial_manifest_bytes);
    report["artifact_bytes_total"] = json!(budget.bytes + initial_manifest_bytes);
    for _ in 0..3 {
        let next_len = serde_json::to_vec_pretty(&report)
            .map_err(|e| e.to_string())?
            .len();
        report["manifest_bytes"] = json!(next_len);
        report["artifact_bytes_total"] = json!(budget.bytes + next_len);
    }
    let mut manifest = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    if manifest.len() > MANIFEST_RESERVE {
        report["manifest_truncated"] = json!(true);
        report["inventory"] = json!([]);
        report["inventory_error_samples"] = json!([]);
        report["rustc"] = json!({"exit": version["exit"], "truncated": version["truncated"]});
        report["merge_replay"] = json!({"exit": replay["exit"], "timed_out": replay["timed_out"],
            "truncated": replay["truncated"], "stderr": "manifest exceeded reserved space; full tool streams omitted"});
        for candidate in report["captured"].as_array_mut().into_iter().flatten() {
            if candidate.get("error").is_none() {
                candidate["first_256_hex"] = json!("");
                // Keep the retained profile hash even when optional display
                // detail must be compacted to protect the original manifest.
            }
        }
        manifest = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    }
    for _ in 0..2 {
        report["manifest_bytes"] = json!(manifest.len());
        report["artifact_bytes_total"] = json!(budget.bytes + manifest.len());
        manifest = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
    }
    budget.write_manifest(&manifest)?;
    drop(replay_snapshot);
    Ok(report)
}

pub(super) fn capture_failure() -> Result<()> {
    let profiles = PathBuf::from("target/llvm-cov-target");
    let report = capture(
        &profiles,
        &profiles.join("ripr-swarm-profraw-list"),
        Path::new("target/ripr/reports/coverage-diagnostics"),
        tool,
    )?;
    println!(
        "Bounded diagnostics retained; original coverage failure remains unchanged. Replay: {}",
        report["merge_replay"]["exit"]
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Drop for Temp {
        fn drop(&mut self) {
            if let Ok(()) = fs::remove_dir_all(&self.0) {}
        }
    }
    fn temp() -> Result<Temp> {
        let root = std::env::temp_dir().join(format!(
            "coverage-capture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        Ok(Temp(root))
    }
    fn runner(args: &[String], bad: Option<&Path>) -> Result<Value> {
        let stdout = if args == ["rustc", "-vV"] {
            "rustc test\nhost: x86_64-unknown-linux-gnu\n"
        } else if args == ["rustc", "--print", "sysroot"] {
            "/test/sysroot\n"
        } else {
            "LLVM test\n"
        };
        if args.iter().any(|s| s == "merge") {
            assert_eq!(args.last().map(String::as_str), Some("/dev/null"));
            let listing = args
                .windows(2)
                .find(|pair| pair[0] == "-f")
                .map(|pair| PathBuf::from(&pair[1]))
                .ok_or("merge list missing")?;
            let pinned = fs::read_to_string(listing)
                .map_err(|error| error.to_string())?
                .lines()
                .next()
                .map(PathBuf::from)
                .ok_or("merge list empty")?;
            let stderr = bad
                .map(|_| format!("warning: {}: invalid instrumentation profile data (file header is corrupt)\nerror: no profile can be merged\n", pinned.display()))
                .unwrap_or_default();
            return Ok(tool_result(
                if bad.is_some() { Some(1) } else { Some(0) },
                b"",
                stderr.as_bytes(),
                false,
                false,
                false,
            ));
        }
        Ok(json!({"exit": 0, "stdout": stdout, "stderr": ""}))
    }
    fn seed(root: &Path, bytes: &[u8]) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let profiles = root.join("profiles");
        fs::create_dir(&profiles).map_err(|e| e.to_string())?;
        let profile = profiles.join("workspace-123-456_0.profraw");
        fs::write(&profile, bytes).map_err(|e| e.to_string())?;
        let list = profiles.join("workspace-profraw-list");
        fs::write(&list, format!("{}\n", profile.display())).map_err(|e| e.to_string())?;
        Ok((profiles, profile, list))
    }
    #[test]
    fn successful_replay_does_not_reset_original_failure() -> Result<()> {
        let t = temp()?;
        let (dir, file, list) = seed(&t.0, b"profile")?;
        let report = capture(&dir, &list, &t.0.join("evidence"), |a| runner(a, None))?;
        assert_eq!(report["original_coverage_outcome"], "failure");
        assert_eq!(report["merge_replay"]["exit"], 0);
        assert_eq!(report["profile_list_source_recheck"]["status"], "unchanged");
        assert_eq!(fs::read(file).map_err(|e| e.to_string())?, b"profile");
        Ok(())
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_profile_open_uses_target_abi_constants() {
        assert_ne!(libc::O_NOFOLLOW, 0);
        assert_ne!(libc::O_NONBLOCK, 0);
    }
    #[test]
    fn malformed_profile_under_unicode_root_is_retained() -> Result<()> {
        let t = temp()?;
        let root = t.0.join("zoë");
        fs::create_dir(&root).map_err(|error| error.to_string())?;
        let (dir, file, list) = seed(&root, b"malformed profile bytes")?;
        let out = root.join("evidence");
        let report = capture(&dir, &list, &out, |args| runner(args, Some(&file)))?;

        assert_eq!(report["merge_replay"]["exit"], 1);
        assert_eq!(report["captured"].as_array().map(Vec::len), Some(1));
        assert!(report["captured"][0]["complete"].as_bool().unwrap_or(false));
        assert_eq!(
            fs::read(out.join("profile-00.prefix.profraw")).map_err(|error| error.to_string())?,
            b"malformed profile bytes"
        );
        assert_eq!(
            fs::read(file).map_err(|error| error.to_string())?,
            b"malformed profile bytes"
        );
        Ok(())
    }
    #[cfg(target_os = "linux")]
    fn required_llvm_tool(run: &impl Fn(&[String]) -> Result<Value>) -> Result<PathBuf> {
        let sysroot = invoke(run, &["rustc", "--print", "sysroot"]);
        if sysroot["exit"] != 0 {
            return Err("required LLVM proof: rustc sysroot discovery failed".to_owned());
        }
        let version = invoke(run, &["rustc", "-vV"]);
        if version["exit"] != 0 {
            return Err("required LLVM proof: rustc host discovery failed".to_owned());
        }
        let host = field(&version, "stdout")
            .lines()
            .find_map(|line| line.strip_prefix("host: "))
            .ok_or("required LLVM proof: rustc host triple missing")?;
        let path = Path::new(field(&sysroot, "stdout").trim())
            .join("lib/rustlib")
            .join(host)
            .join("bin/llvm-profdata");
        let path_text = path
            .to_str()
            .ok_or("required LLVM proof: non-UTF-8 tool path")?;
        if invoke(run, &[path_text, "--version"])["exit"] != 0 {
            return Err("required LLVM proof: matching llvm-profdata unavailable".to_owned());
        }
        Ok(path)
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn required_llvm_discovery_failure_is_an_error() {
        for failing_command in ["sysroot", "-vV", "--version"] {
            let result = required_llvm_tool(&|args| {
                if args.iter().any(|arg| arg == failing_command) {
                    return Err("forced discovery failure".to_owned());
                }
                runner(args, None)
            });
            assert!(matches!(result, Err(error) if error.starts_with("required LLVM proof:")));
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn real_llvm_malformed_profile_under_unicode_root_is_retained() -> Result<()> {
        required_llvm_tool(&tool)?;
        for root_name in ["zoë", "zoë\u{0085}"] {
            let t = temp()?;
            let root = t.0.join(root_name);
            let profiles = root.join("profiles");
            fs::create_dir_all(&profiles).map_err(|error| error.to_string())?;
            let profile = profiles.join("worker-123456_0.profraw");
            let malformed = vec![0; 512];
            fs::write(&profile, &malformed).map_err(|error| error.to_string())?;
            let listing = profiles.join("ripr-swarm-profraw-list");
            fs::write(&listing, format!("{}\n", profile.display()))
                .map_err(|error| error.to_string())?;
            let output = root.join("coverage-diagnostics");

            let report = capture(&profiles, &listing, &output, tool)?;

            assert_eq!(report["merge_replay"]["exit"], 1);
            assert!(
                report["merge_replay"]["stderr_retained_prefix_utf8_valid"]
                    .as_bool()
                    .unwrap_or(false)
            );
            assert_eq!(report["profile_list_source_recheck"]["status"], "unchanged");
            assert_eq!(report["captured"].as_array().map(Vec::len), Some(1));
            assert_eq!(
                report["captured"][0]["error_family"],
                "unrecognized_instrumentation_profile_encoding_format"
            );
            assert_eq!(report["captured"][0]["retained_bytes"], 512);
            assert_eq!(
                fs::read(output.join("profile-00.prefix.profraw"))
                    .map_err(|error| error.to_string())?,
                malformed
            );
            assert_eq!(
                fs::read(profile).map_err(|error| error.to_string())?,
                malformed
            );
            assert!(output.join("manifest.json").is_file());
        }
        Ok(())
    }
    #[test]
    fn invalid_utf8_warning_is_lossless_and_refused_for_path_matching() -> Result<()> {
        let text = "warning: profile at zoë";
        let result = tool_result(Some(1), text.as_bytes(), &[0xff], false, false, false);
        assert_eq!(result["stdout"], text);
        assert_eq!(result["stdout_retained_prefix_utf8_valid"], true);
        assert_eq!(result["stderr_retained_prefix_utf8_valid"], false);
        assert_eq!(result["stderr_hex"], "ff");

        let t = temp()?;
        let (dir, file, list) = seed(&t.0, b"malformed profile bytes")?;
        let report = capture(&dir, &list, &t.0.join("evidence"), |args| {
            if args.iter().any(|arg| arg == "merge") {
                let listing = args
                    .windows(2)
                    .find(|pair| pair[0] == "-f")
                    .map(|pair| PathBuf::from(&pair[1]))
                    .ok_or("merge list missing")?;
                let pinned = fs::read_to_string(listing)
                    .map_err(|error| error.to_string())?
                    .lines()
                    .next()
                    .ok_or("merge input missing")?
                    .as_bytes()
                    .to_vec();
                let mut stderr = b"warning: ".to_vec();
                let split = pinned.len() / 2;
                stderr.extend_from_slice(&pinned[..split]);
                stderr.push(0xff);
                stderr.extend_from_slice(&pinned[split..]);
                stderr.extend_from_slice(b": invalid instrumentation profile data\n");
                return Ok(tool_result(Some(1), b"", &stderr, false, false, false));
            }
            runner(args, Some(&file))
        })?;

        assert_eq!(report["unclassified_profile_warnings"], 1);
        assert_eq!(report["captured"].as_array().map(Vec::len), Some(0));
        assert_eq!(
            report["profile_warning_parse_refusal"],
            "retained replay stderr prefix is not valid UTF-8; exact warning path matching refused"
        );
        assert!(field(&report["merge_replay"], "stderr_hex").contains("ff"));
        Ok(())
    }
    #[test]
    fn invalid_byte_after_retained_stderr_prefix_does_not_refuse_complete_warning() -> Result<()> {
        let t = temp()?;
        let (dir, file, list) = seed(&t.0, b"malformed profile bytes")?;
        let out = t.0.join("evidence");
        let report = capture(&dir, &list, &out, |args| {
            let result = runner(args, Some(&file))?;
            if !args.iter().any(|arg| arg == "merge") {
                return Ok(result);
            }
            let mut bytes = retained_text(&result, "stderr")?.into_bytes();
            assert!(bytes.len() < STREAM_BYTES);
            bytes.resize(STREAM_BYTES, b' ');
            bytes.push(0xff);
            let (prefix, truncated) = drain(std::io::Cursor::new(bytes))?;
            assert_eq!(prefix.len(), STREAM_BYTES);
            assert!(truncated);
            Ok(tool_result(Some(1), b"", &prefix, truncated, false, false))
        })?;
        assert_eq!(
            report["merge_replay"]["stderr_retained_prefix_utf8_valid"],
            true
        );
        assert_eq!(report["merge_replay"]["truncated"], true);
        assert_eq!(
            field(&report["merge_replay"], "stderr_hex").len(),
            STREAM_BYTES * 2
        );
        assert_eq!(report["unclassified_profile_warnings"], 0);
        assert_eq!(report["captured"].as_array().map(Vec::len), Some(1));
        assert_eq!(report["original_coverage_outcome"], "failure");
        assert_eq!(
            fs::read(out.join("profile-00.prefix.profraw")).map_err(|e| e.to_string())?,
            b"malformed profile bytes"
        );
        Ok(())
    }
    #[test]
    fn unavailable_source_list_recheck_is_distinct_and_manifest_survives() -> Result<()> {
        for replace_with_directory in [false, true] {
            let t = temp()?;
            let (dir, file, list) = seed(&t.0, b"malformed profile bytes")?;
            let out = t.0.join("evidence");
            let report = capture(&dir, &list, &out, |args| {
                if args.iter().any(|arg| arg == "merge") {
                    let result = runner(args, Some(&file))?;
                    fs::remove_file(&list).map_err(|error| error.to_string())?;
                    if replace_with_directory {
                        fs::create_dir(&list).map_err(|error| error.to_string())?;
                    }
                    return Ok(result);
                }
                runner(args, Some(&file))
            })?;

            assert_eq!(
                report["profile_list_source_recheck"]["status"],
                "unavailable"
            );
            assert!(report["profile_list_source_recheck"]["error"].is_string());
            assert_eq!(report["profile_list_replay_safe"], true);
            assert_eq!(report["captured"][0]["complete"], true);
            assert!(out.join("manifest.json").is_file());
        }
        Ok(())
    }
    #[test]
    fn corrupt_and_truncated_inputs_are_retained_unfiltered() -> Result<()> {
        for data in [b"bad".to_vec(), vec![0; 512]] {
            let t = temp()?;
            let (dir, file, list) = seed(&t.0, &data)?;
            let before = fs::read(&list).map_err(|e| e.to_string())?;
            let out = t.0.join("evidence");
            let report = capture(&dir, &list, &out, |a| {
                if a.iter().any(|s| s == "merge") {
                    assert_eq!(fs::read(&list).map_err(|e| e.to_string())?, before);
                }
                runner(a, Some(&file))
            })?;
            assert_eq!(report["merge_replay"]["exit"], 1);
            assert_eq!(report["captured"][0]["complete"], true);
            assert_eq!(
                fs::read(out.join("profile-00.prefix.profraw")).map_err(|e| e.to_string())?,
                data
            );
            assert_eq!(fs::read(&file).map_err(|e| e.to_string())?, data);
            assert_eq!(report["captured"][0]["pid_hint"], 123);
            assert_eq!(
                report["captured"][0]["error_family"],
                "invalid_instrumentation_profile_data"
            );
        }
        Ok(())
    }
    #[test]
    fn replay_uses_the_validated_list_and_pinned_profile_after_source_swap() -> Result<()> {
        let t = temp()?;
        let (dir, source, list) = seed(&t.0, b"allowed original profile")?;
        let outside = t.0.join("outside.profraw");
        fs::write(&outside, b"outside secret profile").map_err(|error| error.to_string())?;
        let saved = dir.join("original.saved");
        let out = t.0.join("evidence");
        let swapped = std::cell::Cell::new(false);
        let report = capture(&dir, &list, &out, |args| {
            if args.last().is_some_and(|arg| arg == "--version") {
                fs::write(&list, format!("{}\n", outside.display()))
                    .map_err(|error| error.to_string())?;
                fs::rename(&source, &saved).map_err(|error| error.to_string())?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(&outside, &source).map_err(|error| error.to_string())?;
                swapped.set(true);
            }
            if args.iter().any(|arg| arg == "merge") {
                let replay_list = args
                    .windows(2)
                    .find(|pair| pair[0] == "-f")
                    .map(|pair| PathBuf::from(&pair[1]))
                    .ok_or("replay list missing")?;
                let snapshot =
                    fs::read_to_string(replay_list).map_err(|error| error.to_string())?;
                assert!(!snapshot.contains(outside.to_string_lossy().as_ref()));
                assert!(snapshot.contains(".coverage-replay-"));
            }
            runner(args, Some(&source))
        })?;
        assert!(swapped.get());
        assert!(
            report["profile_list_replay_safe"]
                .as_bool()
                .unwrap_or(false)
        );
        assert_eq!(report["profile_list_source_recheck"]["status"], "changed");
        assert_eq!(report["profile_source_paths_stable"], false);
        assert_eq!(report["profile_inputs_identity_stable"], true);
        assert_eq!(report["captured"][0]["complete"], true);
        assert_eq!(
            fs::read(out.join("profile-00.prefix.profraw")).map_err(|e| e.to_string())?,
            b"allowed original profile"
        );
        assert_eq!(
            fs::read(&outside).map_err(|e| e.to_string())?,
            b"outside secret profile"
        );
        Ok(())
    }
    #[test]
    fn unavailable_warned_candidates_are_explicit_and_manifest_survives() -> Result<()> {
        let t = temp()?;
        let (dir, first, list) = seed(&t.0, b"first profile")?;
        let second = dir.join("workspace-124-456_0.profraw");
        fs::write(&second, b"second profile").map_err(|e| e.to_string())?;
        fs::write(
            &list,
            format!("{}\n{}\n", first.display(), second.display()),
        )
        .map_err(|e| e.to_string())?;
        let out = t.0.join("evidence");
        let report = capture(&dir, &list, &out, |args| {
            if args.iter().any(|arg| arg == "merge") {
                let snapshot_list = args
                    .windows(2)
                    .find(|pair| pair[0] == "-f")
                    .map(|pair| PathBuf::from(&pair[1]))
                    .ok_or("snapshot list missing")?;
                let pinned = fs::read_to_string(snapshot_list)
                    .map_err(|error| error.to_string())?
                    .lines()
                    .map(PathBuf::from)
                    .collect::<Vec<_>>();
                fs::remove_file(&pinned[0]).map_err(|error| error.to_string())?;
                fs::remove_file(&pinned[1]).map_err(|error| error.to_string())?;
                fs::create_dir(&pinned[1]).map_err(|error| error.to_string())?;
                let stderr = format!(
                    "warning: {}: invalid instrumentation profile data\nwarning: {}: invalid instrumentation profile data\nerror: no profile can be merged\n",
                    pinned[0].display(),
                    pinned[1].display()
                );
                return Ok(tool_result(
                    Some(1),
                    b"",
                    stderr.as_bytes(),
                    false,
                    false,
                    false,
                ));
            }
            runner(args, None)
        })?;
        assert_eq!(report["merge_replay"]["exit"], 1);
        assert!(field(&report["captured"][0], "error").contains("No such file"));
        assert_eq!(
            report["captured"][1]["error"],
            "profile input is not a regular file"
        );
        assert_eq!(report["profile_inputs_identity_stable"], false);
        assert_eq!(report["profile_source_paths_stable"], true);
        assert_eq!(
            fs::read(first).map_err(|error| error.to_string())?,
            b"first profile"
        );
        assert_eq!(
            fs::read(second).map_err(|error| error.to_string())?,
            b"second profile"
        );
        assert!(out.join("manifest.json").is_file());
        Ok(())
    }
    #[test]
    fn candidate_budget_error_is_a_row_and_later_candidates_continue() -> Result<()> {
        let t = temp()?;
        let (dir, file, _) = seed(&t.0, b"profile")?;
        let missing = dir.join("missing-456-789_0.profraw");
        let mut budget = Budget {
            root: t.0.join("evidence"),
            bytes: ARTIFACT_BYTES,
            files: 0,
        };
        fs::create_dir(&budget.root).map_err(|e| e.to_string())?;
        let unavailable = candidate_record(
            &mut budget,
            &missing,
            "missing-456-789_0.profraw",
            "invalid_instrumentation_profile_data",
            0,
        );
        let over_budget = candidate_record(
            &mut budget,
            &file,
            "workspace-123-456_0.profraw",
            "invalid_instrumentation_profile_data",
            1,
        );
        assert!(field(&unavailable, "error").contains("No such file"));
        assert_eq!(over_budget["error"], "diagnostic artifact budget exceeded");
        assert_eq!(budget.bytes, ARTIFACT_BYTES);
        assert_eq!(budget.files, 0);
        assert_eq!(
            fs::read_dir(&budget.root)
                .map_err(|e| e.to_string())?
                .count(),
            0
        );
        Ok(())
    }
    #[test]
    fn missing_inputs_are_explicit() -> Result<()> {
        let t = temp()?;
        let report = capture(
            &t.0.join("absent"),
            &t.0.join("absent/list"),
            &t.0.join("evidence"),
            |a| runner(a, None),
        )?;
        assert!(report.get("profiles_unavailable").is_some());
        assert!(report["merge_replay"]["exit"].is_null());
        Ok(())
    }
    #[test]
    fn inventory_and_prefix_are_bounded() -> Result<()> {
        let t = temp()?;
        let (dir, file, list) = seed(&t.0, &vec![1; PREFIX_BYTES + 99])?;
        for i in 0..ROWS + 2 {
            fs::write(dir.join(format!("extra-{i}.profraw")), b"x").map_err(|e| e.to_string())?;
        }
        let out = t.0.join("evidence");
        let report = capture(&dir, &list, &out, |a| runner(a, Some(&file)))?;
        assert_eq!(report["inventory_truncated"], true);
        assert!(
            report["inventory_json_bytes"].as_u64().unwrap_or(u64::MAX) <= INVENTORY_BYTES as u64
        );
        assert_eq!(report["captured"][0]["complete"], false);
        assert_eq!(report["captured"][0]["retained_bytes"], PREFIX_BYTES);
        let sizes = fs::read_dir(out)
            .map_err(|e| e.to_string())?
            .map(|e| e.and_then(|e| e.metadata()).map(|m| m.len()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        assert!(sizes.len() <= ARTIFACT_FILES);
        assert!(sizes.iter().sum::<u64>() <= ARTIFACT_BYTES as u64);
        assert!(
            report["artifact_bytes_total"].as_u64().unwrap_or(u64::MAX) <= ARTIFACT_BYTES as u64
        );
        assert!(report["manifest_bytes"].as_u64().unwrap_or(u64::MAX) <= MANIFEST_RESERVE as u64);
        Ok(())
    }
    #[test]
    fn inventory_json_limit_includes_array_punctuation() -> Result<()> {
        let t = temp()?;
        let (dir, _, list) = seed(&t.0, b"x")?;
        for index in 0..ROWS {
            let name = format!("item-{index:04}-{}.profraw", "x".repeat(190));
            fs::write(dir.join(name), b"x").map_err(|error| error.to_string())?;
        }
        let report = capture(&dir, &list, &t.0.join("evidence"), |args| {
            runner(args, None)
        })?;
        let inventory_bytes = serde_json::to_vec(&report["inventory"])
            .map_err(|error| error.to_string())?
            .len();
        assert!(inventory_bytes <= INVENTORY_BYTES);
        assert_eq!(report["inventory_json_bytes"], inventory_bytes);
        assert_eq!(report["inventory_truncated"], true);
        Ok(())
    }
    #[test]
    fn maximum_profile_and_control_stream_capture_keeps_reserved_manifest() -> Result<()> {
        let t = temp()?;
        let dir = t.0.join("profiles");
        fs::create_dir(&dir).map_err(|error| error.to_string())?;
        let mut profiles = Vec::new();
        for index in 0..PROFILES {
            let path = dir.join(format!("worker-{index:03}-123456_0.profraw"));
            fs::write(&path, vec![0x01; PREFIX_BYTES]).map_err(|error| error.to_string())?;
            profiles.push(path);
        }
        let list = dir.join("ripr-swarm-profraw-list");
        fs::write(
            &list,
            profiles
                .iter()
                .map(|path| format!("{}\n", path.display()))
                .collect::<String>(),
        )
        .map_err(|error| error.to_string())?;
        let control = vec![0x01; STREAM_BYTES];
        let out = t.0.join("evidence");
        let report = capture(&dir, &list, &out, |args| {
            if args == ["rustc", "-vV"] {
                let mut stdout = b"rustc test\nhost: x86_64-unknown-linux-gnu\n".to_vec();
                stdout.extend_from_slice(&control[..STREAM_BYTES - stdout.len()]);
                return Ok(tool_result(Some(0), &stdout, &control, false, false, false));
            }
            if args == ["rustc", "--print", "sysroot"] {
                return Ok(tool_result(
                    Some(0),
                    b"/test/sysroot\n",
                    &control,
                    false,
                    false,
                    false,
                ));
            }
            if args == ["git", "rev-parse", "HEAD"] {
                return Ok(tool_result(
                    Some(0),
                    b"0000000000000000000000000000000000000000\n",
                    &control,
                    false,
                    false,
                    false,
                ));
            }
            if args.last().is_some_and(|arg| arg == "--version") {
                let mut stdout = b"LLVM test\n".to_vec();
                stdout.extend_from_slice(&control[..STREAM_BYTES - stdout.len()]);
                return Ok(tool_result(Some(0), &stdout, &control, false, false, false));
            }
            let snapshot_list = args
                .windows(2)
                .find(|pair| pair[0] == "-f")
                .map(|pair| PathBuf::from(&pair[1]))
                .ok_or("merge snapshot list missing")?;
            let names = fs::read_to_string(snapshot_list).map_err(|error| error.to_string())?;
            let mut stderr = Vec::new();
            for (index, path) in names.lines().enumerate() {
                let family = match index % 3 {
                    0 => "invalid instrumentation profile data (file header is corrupt)",
                    1 => "malformed instrumentation profile data: binary id length is 0",
                    _ => "unrecognized instrumentation profile encoding format",
                };
                stderr.extend_from_slice(format!("warning: {path}: {family}\n").as_bytes());
            }
            stderr.extend_from_slice(b"error: no profile can be merged\n");
            stderr.resize(STREAM_BYTES, 0x01);
            Ok(tool_result(Some(1), &control, &stderr, false, false, false))
        })?;
        assert_eq!(report["original_coverage_outcome"], "failure");
        assert_eq!(report["merge_replay"]["exit"], 1);
        assert_eq!(report["captured"].as_array().map(Vec::len), Some(PROFILES));
        assert!(
            report["captured"]
                .as_array()
                .is_some_and(|rows| rows.iter().all(|row| row["complete"] == true))
        );
        assert!(report["manifest_bytes"].as_u64().unwrap_or(u64::MAX) <= MANIFEST_RESERVE as u64);
        assert!(
            report["artifact_bytes_total"].as_u64().unwrap_or(u64::MAX) <= ARTIFACT_BYTES as u64
        );
        let manifest = fs::read(out.join("manifest.json")).map_err(|error| error.to_string())?;
        assert_eq!(
            manifest.len() as u64,
            report["manifest_bytes"].as_u64().unwrap_or(0)
        );
        assert!(
            fs::read_dir(out)
                .map_err(|error| error.to_string())?
                .count()
                <= ARTIFACT_FILES
        );
        Ok(())
    }
    #[test]
    fn outside_and_symlink_inputs_are_not_read() -> Result<()> {
        let t = temp()?;
        let (dir, _, list) = seed(&t.0, b"x")?;
        let outside = t.0.join("outside.profraw");
        fs::write(&outside, b"not diagnostic data").map_err(|e| e.to_string())?;
        fs::write(&list, format!("{}\n", outside.display())).map_err(|e| e.to_string())?;
        let report = capture(&dir, &list, &t.0.join("evidence"), |a| {
            runner(a, Some(&outside))
        })?;
        assert_eq!(report["profile_list_replay_safe"], false);
        assert_eq!(report["captured"], json!([]));
        #[cfg(unix)]
        {
            let link = t.0.join("link");
            std::os::unix::fs::symlink(&dir, &link).map_err(|e| e.to_string())?;
            assert_eq!(
                capture(&link, &list, &t.0.join("second"), |a| runner(a, None))
                    .err()
                    .as_deref(),
                Some("diagnostic path contains a symlink")
            );
        }
        Ok(())
    }
    #[test]
    fn owned_tool_streams_are_bounded_and_drained() -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            let report = invoke(
                &tool,
                &[
                    "python3",
                    "-c",
                    "import sys; sys.stdout.write('x'*100000); sys.stderr.write('y'*100000)",
                ],
            );
            assert_eq!(report["exit"], 0);
            assert_eq!(report["truncated"], true);
            assert_eq!(field(&report, "stdout").len(), STREAM_BYTES);
            assert_eq!(field(&report, "stderr").len(), STREAM_BYTES);
            assert_eq!(field(&report, "stdout_hex").len(), STREAM_BYTES * 2);
            assert_eq!(field(&report, "stderr_hex").len(), STREAM_BYTES * 2);
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn process_is_running(pid: u32) -> Result<bool> {
        process_stat_is_running(fs::read_to_string(format!("/proc/{pid}/stat")))
    }

    #[cfg(target_os = "linux")]
    fn process_stat_is_running(stat: std::io::Result<String>) -> Result<bool> {
        let stat = match stat {
            Ok(stat) => stat,
            // A process can exit after its proc descriptor is opened but before
            // the stat read; Linux reports ESRCH for that vanished task.
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(libc::ESRCH) =>
            {
                return Ok(false);
            }
            Err(error) => return Err(error.to_string()),
        };
        let state = stat
            .rsplit_once(')')
            .and_then(|(_, tail)| tail.split_whitespace().next())
            .ok_or("process state unavailable")?;
        Ok(!matches!(state, "Z" | "X"))
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn process_exit_observer_accepts_only_missing_process_errors() {
        for errno in [libc::ENOENT, libc::ESRCH] {
            assert_eq!(
                process_stat_is_running(Err(std::io::Error::from_raw_os_error(errno))),
                Ok(false),
                "exited process observation errno {errno}"
            );
        }
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn process_exit_observer_preserves_unrelated_errors_and_malformed_stat() {
        for errno in [libc::EACCES, libc::EIO] {
            let error = std::io::Error::from_raw_os_error(errno);
            let expected = error.to_string();
            assert_eq!(process_stat_is_running(Err(error)), Err(expected));
        }
        assert_eq!(
            process_stat_is_running(Ok("malformed stat".to_string())),
            Err("process state unavailable".to_string())
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn process_exit_observer_distinguishes_live_and_terminal_states() {
        for (state, running) in [("R", true), ("S", true), ("Z", false), ("X", false)] {
            assert_eq!(
                process_stat_is_running(Ok(format!("123 (child (name)) {state} 1 2 3"))),
                Ok(running),
                "process state {state}"
            );
        }
    }

    #[cfg(target_os = "linux")]
    fn assert_process_stopped(pid: u32) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        while process_is_running(pid)? {
            if Instant::now() >= deadline {
                return Err(format!("owned descendant {pid} remained live"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    #[test]
    fn owned_deadline_and_early_parent_exit_reap_process_group_descendants() -> Result<()> {
        #[cfg(target_os = "linux")]
        for (parent_sleep, redirect, expect_timeout) in [
            (false, false, false),
            (false, true, false),
            (true, false, true),
        ] {
            let t = temp()?;
            let marker = t.0.join("descendant.pid");
            let redirect_statement = if redirect {
                ", stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL"
            } else {
                ""
            };
            let parent_statement = if parent_sleep { "; time.sleep(60)" } else { "" };
            let script = [
                "import subprocess,sys,time; p=subprocess.",
                "Popen(['sleep','60']",
                redirect_statement,
                "); open(sys.argv[1],'w').write(str(p.pid))",
                parent_statement,
            ]
            .concat();
            let args = vec![
                "python3".to_string(),
                "-c".to_string(),
                script,
                marker.to_string_lossy().into_owned(),
            ];
            // The deadline only has to separate a parent that exits on its own
            // from one that sleeps 60 s. A 250 ms deadline also raced Python
            // startup on a loaded runner (#7551): the early-exit rows timed
            // out, and the sleeping row could be terminated before writing its
            // marker. Both bounds stay far below the 60 s sleeps.
            let started = Instant::now();
            let report = tool_with_deadline(&args, Duration::from_secs(2))?;
            assert!(started.elapsed() < Duration::from_secs(5));
            assert_eq!(report["timed_out"], expect_timeout);
            assert_eq!(report["descendants_terminated"], true);
            let pid = fs::read_to_string(marker)
                .map_err(|error| error.to_string())?
                .parse::<u32>()
                .map_err(|error| error.to_string())?;
            assert_process_stopped(pid)?;
        }
        Ok(())
    }
}
