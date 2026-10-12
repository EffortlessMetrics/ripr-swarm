//! Failure-only, bounded read-only evidence. Producer attribution stays unresolved.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

type Result<T> = std::result::Result<T, String>;

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
    // GNU timeout owns the group and force-kills it after its grace period.
    // Fixed local tool argv only; no shell, network or environment inspection.
    let mut child = Command::new("timeout")
        .args(["--kill-after=1s", "30s"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let stdout = child.stdout.take().ok_or("stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("stderr unavailable")?;
    let out = std::thread::spawn(move || drain(stdout));
    let err = std::thread::spawn(move || drain(stderr));
    let status = child.wait().map_err(|e| e.to_string())?;
    let (stdout, out_cut) = out.join().map_err(|_panic| "stdout drain failed")??;
    let (stderr, err_cut) = err.join().map_err(|_panic| "stderr drain failed")??;
    Ok(
        json!({"exit": status.code(), "stdout": String::from_utf8_lossy(&stdout),
        "stderr": String::from_utf8_lossy(&stderr), "truncated": out_cut || err_cut,
        "timed_out": status.code() == Some(124)}),
    )
}

fn invoke(run: &impl Fn(&[String]) -> Result<Value>, args: &[&str]) -> Value {
    let args = args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    run(&args).unwrap_or_else(|e| json!({"exit": null, "stdout": "", "stderr": e}))
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
        // Linux O_NOFOLLOW | O_NONBLOCK: reject link/FIFO replacement races.
        options.custom_flags(0x20000 | 0x800);
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

struct Budget {
    root: PathBuf,
    bytes: usize,
    files: usize,
}
impl Budget {
    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        if self.files >= ARTIFACT_FILES || self.bytes + bytes.len() > ARTIFACT_BYTES {
            return Err("diagnostic artifact budget exceeded".into());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(name))
            .map_err(|e| e.to_string())?;
        file.write_all(bytes).map_err(|e| e.to_string())?;
        self.bytes += bytes.len();
        self.files += 1;
        Ok(())
    }
}

fn safe_input(path: &Path, profiles: &Path) -> bool {
    path.extension().is_some_and(|s| s == "profraw")
        && path.parent().and_then(|p| p.canonicalize().ok()) == profiles.canonicalize().ok()
        && fs::symlink_metadata(path).is_ok_and(|m| m.is_file())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
            "profile_list_entries": LIST_ENTRIES, "malformed_prefixes": PROFILES,
            "profile_bytes_each": PREFIX_BYTES, "tool_bytes_each_stream": STREAM_BYTES,
            "tool_deadline_seconds": 30, "artifact_bytes": ARTIFACT_BYTES, "artifact_files": ARTIFACT_FILES},
        "inventory_truncated": false, "captured": []});
    let mut rows = Vec::new();
    let mut row_bytes = 0;
    match fs::read_dir(profiles) {
        Ok(entries) => {
            for (index, entry) in entries.enumerate() {
                if index >= ENTRIES {
                    report["inventory_truncated"] = json!(true);
                    break;
                }
                let entry = entry.map_err(|e| e.to_string())?;
                let path = entry.path();
                if path.extension().is_none_or(|s| s != "profraw") {
                    continue;
                }
                let info = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
                if !info.is_file() {
                    continue;
                }
                let row = json!({"name": entry.file_name().to_string_lossy().chars().take(240).collect::<String>(),
                "bytes": info.len()});
                let size = serde_json::to_vec(&row).map_err(|e| e.to_string())?.len();
                if rows.len() >= ROWS || row_bytes + size > INVENTORY_BYTES {
                    report["inventory_truncated"] = json!(true);
                    continue;
                }
                row_bytes += size;
                rows.push(row);
            }
        }
        Err(error) => report["profiles_unavailable"] = json!(error.to_string()),
    }
    report["inventory"] = json!(rows);
    let version = invoke(&run, &["rustc", "-vV"]);
    let sysroot = invoke(&run, &["rustc", "--print", "sysroot"]);
    report["rustc"] = version.clone();
    let commit = invoke(&run, &["git", "rev-parse", "HEAD"]);
    let sha = field(&commit, "stdout").trim();
    report["checkout_sha"] = json!(
        (commit["exit"] == 0 && sha.len() == 40 && sha.bytes().all(|c| c.is_ascii_hexdigit()))
            .then_some(sha)
    );
    let contents = read_prefix(listing, LIST_BYTES + 1);
    let safe_list = contents.as_ref().is_ok_and(|bytes| {
        bytes.len() <= LIST_BYTES
            && listing.parent().and_then(|p| p.canonicalize().ok()) == profiles.canonicalize().ok()
            && std::str::from_utf8(bytes).is_ok_and(|text| {
                text.lines().count() <= LIST_ENTRIES
                    && text
                        .lines()
                        .all(|line| safe_input(Path::new(line), profiles))
            })
    });
    report["profile_list_replay_safe"] = json!(safe_list);
    let host = field(&version, "stdout")
        .lines()
        .find_map(|line| line.strip_prefix("host: "));
    let mut replay = json!({"exit": null, "stdout": "", "stderr": "safe profile list or matching LLVM tool unavailable"});
    if safe_list
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
        let tool = tool.to_str().ok_or("LLVM tool path is not UTF-8")?;
        report["llvm_profdata"] = invoke(&run, &[tool, "--version"]);
        let data = contents.map_err(|e| e.to_string())?;
        report["profile_list_bytes"] = json!(data.len());
        budget.write(
            "profile-list-prefix.txt",
            &data[..data.len().min(PREFIX_BYTES)],
        )?;
        // Original full list, no filtering. Discard only the new diagnostic
        // OUTPUT; original profiles/profdata/LCOV and failed step are untouched.
        replay = invoke(
            &run,
            &[
                tool,
                "merge",
                "-sparse",
                "-f",
                listing.to_str().ok_or("profile list path is not UTF-8")?,
                "-o",
                "/dev/null",
            ],
        );
    }
    let mut candidates = Vec::new();
    for line in field(&replay, "stderr").lines() {
        if let Some(path) = line.strip_prefix("warning: ").and_then(|line| {
            line.split_once(": invalid instrumentation profile data")
                .map(|(path, _)| PathBuf::from(path))
        }) && safe_input(&path, profiles)
            && !candidates.contains(&path)
        {
            candidates.push(path);
        }
    }
    report["failed_profile_capture_truncated"] = json!(candidates.len() > PROFILES);
    let mut captured = Vec::new();
    for (index, path) in candidates.iter().take(PROFILES).enumerate() {
        let info = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        let data = read_prefix(path, PREFIX_BYTES)?;
        let name = format!("profile-{index:02}.prefix.profraw");
        budget.write(&name, &data)?;
        let original = path
            .file_name()
            .ok_or("profile filename")?
            .to_string_lossy();
        let mut parts = original.trim_end_matches(".profraw").rsplitn(3, '-');
        let module = parts.next();
        let pid = parts.next().and_then(|p| p.parse::<u64>().ok());
        captured.push(json!({"name": original.chars().take(240).collect::<String>(), "artifact": name,
            "original_bytes": info.len(), "retained_bytes": data.len(), "complete": data.len() as u64 == info.len(),
            "prefix_sha256": format!("{:x}", Sha256::digest(&data)), "first_256_hex": hex(&data[..data.len().min(256)]),
            "pid_hint": pid, "module_hint": module}));
    }
    report["captured"] = json!(captured);
    report["merge_replay"] = replay;
    report["merge_replay_scope"] =
        json!("diagnostic only; original failed coverage step remains failed");
    report["artifact_bytes_before_manifest"] = json!(budget.bytes);
    budget.write(
        "manifest.json",
        &serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )?;
    Ok(report)
}

pub fn command() -> Result<()> {
    #[cfg(not(target_os = "linux"))]
    return Err("coverage diagnostic capture is a Linux workflow adapter".into());
    #[cfg(target_os = "linux")]
    {
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
            return Ok(
                json!({"exit": if bad.is_some() { 1 } else { 0 }, "stdout": "", "stderr":
                bad.map(|p| format!("warning: {}: invalid instrumentation profile data (file header is corrupt)\nerror: no profile can be merged\n", p.display())).unwrap_or_default()}),
            );
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
        assert_eq!(fs::read(file).map_err(|e| e.to_string())?, b"profile");
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
        }
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
        assert_eq!(report["captured"][0]["complete"], false);
        assert_eq!(report["captured"][0]["retained_bytes"], PREFIX_BYTES);
        let sizes = fs::read_dir(out)
            .map_err(|e| e.to_string())?
            .map(|e| e.and_then(|e| e.metadata()).map(|m| m.len()))
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        assert!(sizes.len() <= ARTIFACT_FILES);
        assert!(sizes.iter().sum::<u64>() <= ARTIFACT_BYTES as u64);
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
        }
        Ok(())
    }
}
