//! Same-worker RIPR libtest compile receipt and direct replay (#12543).
//!
//! Compile goes through the existing owned-process Cargo adapter. The test
//! executable is taken from Cargo JSON `compiler-artifact` identity, never from
//! `target/` glob, mtime, or basename. Replay invokes that bound path without
//! Cargo, rustc, or a compiler PATH.

use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::run::{
    ByteCaptureBudget, TimedBytesOutput, capture_bytes_in_dir_with_env_budget,
    capture_bytes_in_dir_with_timeout,
};

pub(crate) const RECEIPT_SCHEMA: &str = "ripr_native_libtest_build.v1";
pub(crate) const RIPR_LIBTEST_PACKAGE: &str = "ripr";
const COMPILE_ENV_REMOVE: [&str; 6] = [
    "CARGO_TARGET_DIR",
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_BUILD_RUSTC_WRAPPER",
    "CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER",
];
const REPLAY_ENV_REMOVE: [&str; 12] = [
    "CARGO_TARGET_DIR",
    "CARGO",
    "CARGO_HOME",
    "RUSTC",
    "RUSTC_WRAPPER",
    "RUSTC_WORKSPACE_WRAPPER",
    "CARGO_ENCODED_RUSTFLAGS",
    "RUSTFLAGS",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "http_proxy",
    "https_proxy",
];

/// First supported producer argv after `cargo` for the current RIPR default graph.
pub(crate) fn ripr_libtest_compile_argv() -> Vec<String> {
    vec![
        "test".to_string(),
        "--locked".to_string(),
        "-p".to_string(),
        RIPR_LIBTEST_PACKAGE.to_string(),
        "--lib".to_string(),
        "--no-run".to_string(),
        "--message-format=json".to_string(),
    ]
}

#[derive(Clone, Debug)]
pub(crate) struct CompileRequest<'a> {
    pub(crate) root: &'a Path,
    pub(crate) target_dir: &'a Path,
    pub(crate) package: &'a str,
    pub(crate) locked: bool,
    pub(crate) offline: bool,
    pub(crate) timeout: Duration,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct NativeLibtestReceipt {
    pub(crate) schema_version: String,
    pub(crate) worktree: String,
    pub(crate) source_digest: String,
    pub(crate) cargo_toml_sha256: String,
    pub(crate) cargo_lock_sha256: Option<String>,
    pub(crate) rustc_verbose_version: String,
    pub(crate) cargo_version: String,
    pub(crate) host_target: String,
    pub(crate) package: String,
    pub(crate) target_name: String,
    pub(crate) target_kind: Vec<String>,
    pub(crate) features: Vec<String>,
    pub(crate) profile_test: bool,
    pub(crate) rustflags: Option<String>,
    pub(crate) build_script_limitation: String,
    pub(crate) target_dir: String,
    pub(crate) cargo_argv: Vec<String>,
    pub(crate) cargo_exit_code: Option<i32>,
    pub(crate) cargo_timed_out: bool,
    pub(crate) cargo_stdout_sha256: String,
    pub(crate) cargo_stderr_sha256: String,
    pub(crate) json_complete: bool,
    pub(crate) executable: String,
    pub(crate) executable_sha256: String,
    pub(crate) executable_bytes: u64,
    pub(crate) executable_mode: String,
    pub(crate) runtime_dependency_limitation: String,
    pub(crate) test_list_exit_code: Option<i32>,
    pub(crate) test_inventory: Vec<String>,
    pub(crate) test_inventory_digest: String,
    pub(crate) resource_lane: String,
    pub(crate) cgroup: Option<String>,
    pub(crate) process_tree_settled: bool,
    pub(crate) compile_duration_ms: u128,
    pub(crate) list_duration_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectedLibtestArtifact {
    pub(crate) executable: PathBuf,
    pub(crate) target_name: String,
    pub(crate) target_kind: Vec<String>,
    pub(crate) features: Vec<String>,
    pub(crate) profile_test: bool,
    pub(crate) package_id: String,
    pub(crate) manifest_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReplayOutcome {
    pub(crate) argv: Vec<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) timed_out: bool,
    pub(crate) duration_ms: u128,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) ignored: usize,
    pub(crate) executed: usize,
    pub(crate) process_tree_settled: bool,
}

pub(crate) fn run(args: &[String]) -> Result<(), String> {
    let options = parse_run_args(args)?;
    let timeout = if options.package == RIPR_LIBTEST_PACKAGE {
        crate::run::tool_build_timeout()?
    } else {
        Duration::from_mins(3)
    };
    let request = CompileRequest {
        root: &options.root,
        target_dir: &options.target_dir,
        package: &options.package,
        locked: options.locked,
        offline: options.offline,
        timeout,
    };
    let receipt = compile_libtest(&request)?;
    let mut replays = Vec::new();
    replays.push(replay_list(&receipt, &options.root)?);
    for filter in &options.replay_filters {
        replays.push(replay_filter(&receipt, &options.root, filter)?);
    }
    let report = serde_json::json!({
        "receipt": receipt,
        "replays": replays.iter().map(|replay| {
            serde_json::json!({
                "argv": replay.argv,
                "exit_code": replay.exit_code,
                "timed_out": replay.timed_out,
                "duration_ms": replay.duration_ms,
                "passed": replay.passed,
                "failed": replay.failed,
                "ignored": replay.ignored,
                "executed": replay.executed,
                "process_tree_settled": replay.process_tree_settled,
            })
        }).collect::<Vec<_>>(),
    });
    let encoded = serde_json::to_string_pretty(&report)
        .map_err(|error| format!("encode native libtest receipt: {error}"))?;
    if let Some(path) = options.out {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "create native libtest receipt parent `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        fs::write(&path, format!("{encoded}\n")).map_err(|error| {
            format!("write native libtest receipt `{}`: {error}", path.display())
        })?;
        println!("{}", path.display());
    } else {
        println!("{encoded}");
    }
    Ok(())
}

#[derive(Debug)]
struct RunOptions {
    root: PathBuf,
    target_dir: PathBuf,
    package: String,
    locked: bool,
    offline: bool,
    replay_filters: Vec<String>,
    out: Option<PathBuf>,
}

fn parse_run_args(args: &[String]) -> Result<RunOptions, String> {
    let mut root = None;
    let mut target_dir = None;
    let mut package = None;
    let mut locked = false;
    let mut offline = false;
    let mut replay_filters = Vec::new();
    let mut out = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                root = Some(PathBuf::from(required_value(args, index, "--root")?));
                index += 2;
            }
            "--package" => {
                package = Some(required_value(args, index, "--package")?.to_string());
                index += 2;
            }
            "--target-dir" => {
                target_dir = Some(PathBuf::from(required_value(args, index, "--target-dir")?));
                index += 2;
            }
            "--replay-filter" => {
                replay_filters.push(required_value(args, index, "--replay-filter")?.to_string());
                index += 2;
            }
            "--out" => {
                out = Some(PathBuf::from(required_value(args, index, "--out")?));
                index += 2;
            }
            "--locked" => {
                locked = true;
                index += 1;
            }
            "--offline" => {
                offline = true;
                index += 1;
            }
            other => {
                return Err(format!(
                    "unknown native-libtest argument `{other}`; require --root and --package"
                ));
            }
        }
    }
    let root = root.ok_or_else(|| {
        "native-libtest requires --root <path> and --package <name> (no implied ripr compile)"
            .to_string()
    })?;
    let package = package.ok_or_else(|| {
        "native-libtest requires --root <path> and --package <name> (no implied ripr compile)"
            .to_string()
    })?;
    let target_dir = target_dir.unwrap_or_else(|| root.join("target/ripr-native-libtest"));
    Ok(RunOptions {
        root,
        target_dir,
        package,
        locked,
        offline,
        replay_filters,
        out,
    })
}

fn required_value<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    args.get(index + 1)
        .map(String::as_str)
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
        .ok_or_else(|| format!("{flag} requires a value"))
}

pub(crate) fn compile_argv_for(request: &CompileRequest<'_>) -> Result<Vec<String>, String> {
    if request.package == RIPR_LIBTEST_PACKAGE && request.locked {
        let mut args = ripr_libtest_compile_argv();
        if request.offline {
            args.insert(2, "--offline".to_string());
        }
        refuse_compile_filter(&args)?;
        return Ok(args);
    }
    let mut args = vec!["test".to_string()];
    if request.locked {
        args.push("--locked".to_string());
    }
    if request.offline {
        args.push("--offline".to_string());
    }
    args.extend([
        "-p".to_string(),
        request.package.to_string(),
        "--lib".to_string(),
        "--no-run".to_string(),
        "--message-format=json".to_string(),
    ]);
    refuse_compile_filter(&args)?;
    Ok(args)
}

pub(crate) fn refuse_compile_filter(args: &[String]) -> Result<(), String> {
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "test" | "--locked" | "--offline" | "--lib" | "--no-run" | "--message-format=json" => {
                index += 1;
            }
            "-p" | "--package" | "--message-format" => {
                if args.get(index + 1).is_none() {
                    return Err("compile argv is missing a flag value".to_string());
                }
                index += 2;
            }
            "--" => {
                return Err(
                    "a focused filter is not a smaller compilation graph; compile argv cannot name tests"
                        .to_string(),
                );
            }
            other if other.starts_with('-') => {
                return Err(format!(
                    "unsupported compile flag `{other}`; compile argv cannot name tests"
                ));
            }
            _ => {
                return Err(
                    "a focused filter is not a smaller compilation graph; compile argv cannot name tests"
                        .to_string(),
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn select_libtest_executable(
    cargo_json: &[u8],
    package: &str,
) -> Result<SelectedLibtestArtifact, String> {
    let messages = cargo_json_messages(cargo_json)?;
    require_complete_build_finished(&messages)?;
    let mut matches = Vec::new();
    for message in &messages {
        if message.get("reason").and_then(Value::as_str) != Some("compiler-artifact") {
            continue;
        }
        let Some(target) = message.get("target") else {
            continue;
        };
        let name = target.get("name").and_then(Value::as_str).unwrap_or("");
        let kind = json_string_list(target.get("kind"));
        let profile_test = message
            .get("profile")
            .and_then(|profile| profile.get("test"))
            .and_then(Value::as_bool)
            == Some(true);
        let Some(executable) = message.get("executable").and_then(Value::as_str) else {
            continue;
        };
        if executable.is_empty()
            || name != package
            || !kind.iter().any(|item| item == "lib")
            || !profile_test
        {
            continue;
        }
        matches.push(SelectedLibtestArtifact {
            executable: PathBuf::from(executable),
            target_name: name.to_string(),
            target_kind: kind,
            features: json_string_list(message.get("features")),
            profile_test,
            package_id: message
                .get("package_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            manifest_path: message
                .get("manifest_path")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
        });
    }
    match matches.as_slice() {
        [artifact] => {
            if artifact.package_id.is_empty() || artifact.manifest_path.is_empty() {
                return Err(
                    "selected libtest artifact is missing package or manifest identity".to_string(),
                );
            }
            Ok(artifact.clone())
        }
        [] => Err(
            "Cargo JSON has no exact libtest executable for the requested package; zero exit is not success"
                .to_string(),
        ),
        many => Err(format!(
            "Cargo JSON has {} matching libtest executables; refusing ambiguous artifact selection",
            many.len()
        )),
    }
}

pub(crate) fn compile_libtest(
    request: &CompileRequest<'_>,
) -> Result<NativeLibtestReceipt, String> {
    let args = compile_argv_for(request)?;
    fs::create_dir_all(request.target_dir).map_err(|error| {
        format!(
            "create private libtest target `{}`: {error}",
            request.target_dir.display()
        )
    })?;
    let target_dir = std::path::absolute(request.target_dir).map_err(|error| {
        format!(
            "resolve private libtest target `{}`: {error}",
            request.target_dir.display()
        )
    })?;
    let rustc_verbose_version = observe_tool(request.root, "rustc", &["-vV"])?;
    let cargo_version = observe_tool(request.root, "cargo", &["-vV"])?;
    let host_target = rustc_verbose_version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or_else(|| "rustc -vV did not report a host target".to_string())?
        .to_string();
    let cargo_toml_sha256 = sha256_file(&request.root.join("Cargo.toml"))?;
    let rustflags = std::env::var("RUSTFLAGS")
        .ok()
        .filter(|value| !value.is_empty());
    let target_dir_value = target_dir.to_string_lossy().into_owned();
    let output = capture_bytes_in_dir_with_timeout(
        Path::new("cargo"),
        &args,
        request.root,
        &[
            ("CARGO_TARGET_DIR", target_dir_value.as_str()),
            ("CARGO_TERM_COLOR", "never"),
            ("CARGO_INCREMENTAL", "0"),
            (
                "CARGO_NET_OFFLINE",
                if request.offline { "true" } else { "false" },
            ),
        ],
        &COMPILE_ENV_REMOVE,
        request.timeout,
        "native libtest --no-run compile",
    )?;
    if output.timed_out {
        return Err("native libtest compile exceeded the owned process deadline".to_string());
    }
    let cargo_exit = output.status.and_then(|status| status.code());
    if !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "native libtest compile failed with status {cargo_exit:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let selected = select_libtest_executable(&output.stdout, request.package)?;
    let executable = std::fs::canonicalize(&selected.executable).map_err(|error| {
        format!(
            "resolve Cargo libtest executable `{}`: {error}",
            selected.executable.display()
        )
    })?;
    if !executable.starts_with(&target_dir) {
        return Err(
            "selected libtest executable escapes the private compile target directory".to_string(),
        );
    }
    let metadata = fs::metadata(&executable).map_err(|error| {
        format!(
            "stat Cargo libtest executable `{}`: {error}",
            executable.display()
        )
    })?;
    if !metadata.is_file() {
        return Err("selected libtest executable is not a regular file".to_string());
    }
    let list = invoke_bound_binary(
        &executable,
        &["--list".to_string()],
        request.root,
        Duration::from_secs(30),
        "native libtest --list",
        None,
    )?;
    let inventory = parse_libtest_list(&list.stdout)?;
    if inventory.is_empty() {
        return Err(
            "libtest listing is missing or truncated; empty inventory cannot succeed".to_string(),
        );
    }
    let source_digest = source_digest(request.root, &[&target_dir])?;
    Ok(NativeLibtestReceipt {
        schema_version: RECEIPT_SCHEMA.to_string(),
        worktree: std::path::absolute(request.root)
            .map_err(|error| format!("resolve libtest worktree: {error}"))?
            .to_string_lossy()
            .into_owned(),
        source_digest,
        cargo_toml_sha256,
        cargo_lock_sha256: match request.root.join("Cargo.lock") {
            path if path.is_file() => Some(sha256_file(&path)?),
            _ => None,
        },
        rustc_verbose_version,
        cargo_version,
        host_target,
        package: request.package.to_string(),
        target_name: selected.target_name,
        target_kind: selected.target_kind,
        features: selected.features,
        profile_test: selected.profile_test,
        rustflags,
        build_script_limitation:
            "build-script and proc-macro outputs are admitted only through this compile receipt; they are not independently portable"
                .to_string(),
        target_dir: target_dir_value,
        cargo_argv: std::iter::once("cargo".to_string()).chain(args).collect(),
        cargo_exit_code: cargo_exit,
        cargo_timed_out: output.timed_out,
        cargo_stdout_sha256: sha256_bytes(&output.stdout),
        cargo_stderr_sha256: sha256_bytes(&output.stderr),
        json_complete: true,
        executable: executable.to_string_lossy().into_owned(),
        executable_sha256: sha256_file(&executable)?,
        executable_bytes: metadata.len(),
        executable_mode: executable_mode(&metadata),
        runtime_dependency_limitation:
            "same-worker dynamic loader and source-tree inputs only; foreign replay is out of scope"
                .to_string(),
        test_list_exit_code: list.status.and_then(|status| status.code()),
        test_inventory_digest: inventory_digest(&inventory),
        test_inventory: inventory,
        resource_lane: "same-worker-private-target".to_string(),
        cgroup: read_cgroup(),
        process_tree_settled: !output.timed_out && !list.timed_out,
        compile_duration_ms: output.duration.as_millis(),
        list_duration_ms: list.duration.as_millis(),
    })
}

pub(crate) fn replay_list(
    receipt: &NativeLibtestReceipt,
    root: &Path,
) -> Result<ReplayOutcome, String> {
    refuse_if_inputs_changed(receipt, root)?;
    let output = invoke_bound_binary(
        Path::new(&receipt.executable),
        &["--list".to_string()],
        root,
        Duration::from_secs(30),
        "native libtest same-worker --list",
        Some(&receipt.executable),
    )?;
    let inventory = parse_libtest_list(&output.stdout)?;
    if inventory_digest(&inventory) != receipt.test_inventory_digest {
        return Err("same-worker listing digest diverged from the compile receipt".to_string());
    }
    Ok(ReplayOutcome {
        argv: vec![receipt.executable.clone(), "--list".to_string()],
        exit_code: output.status.and_then(|status| status.code()),
        timed_out: output.timed_out,
        duration_ms: output.duration.as_millis(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        passed: 0,
        failed: 0,
        ignored: 0,
        executed: inventory.len(),
        process_tree_settled: !output.timed_out,
    })
}

pub(crate) fn replay_filter(
    receipt: &NativeLibtestReceipt,
    root: &Path,
    filter: &str,
) -> Result<ReplayOutcome, String> {
    refuse_if_inputs_changed(receipt, root)?;
    if filter.trim().is_empty() {
        return Err(
            "a zero-test filter cannot discharge a nonempty libtest obligation".to_string(),
        );
    }
    let args = vec![filter.to_string(), "--exact".to_string()];
    let output = invoke_bound_binary(
        Path::new(&receipt.executable),
        &args,
        root,
        Duration::from_secs(30),
        "native libtest same-worker filter",
        Some(&receipt.executable),
    )?;
    if output.timed_out {
        return Err("same-worker libtest filter exceeded the owned process deadline".to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let counts = parse_libtest_result(&stdout)?;
    let executed = counts.passed + counts.failed;
    if executed == 0 {
        return Err(
            "a zero-match, ignored-only, or empty filter cannot discharge a nonempty libtest obligation"
                .to_string(),
        );
    }
    if counts.failed > 0 || !output.status.is_some_and(|status| status.success()) {
        return Err(format!(
            "same-worker libtest replay failed: passed={} failed={} ignored={} exit={:?}",
            counts.passed,
            counts.failed,
            counts.ignored,
            output.status.and_then(|status| status.code())
        ));
    }
    Ok(ReplayOutcome {
        argv: std::iter::once(receipt.executable.clone())
            .chain(args)
            .collect(),
        exit_code: output.status.and_then(|status| status.code()),
        timed_out: output.timed_out,
        duration_ms: output.duration.as_millis(),
        stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        passed: counts.passed,
        failed: counts.failed,
        ignored: counts.ignored,
        executed,
        process_tree_settled: true,
    })
}

pub(crate) fn refuse_if_inputs_changed(
    receipt: &NativeLibtestReceipt,
    root: &Path,
) -> Result<(), String> {
    if receipt.schema_version != RECEIPT_SCHEMA {
        return Err("libtest receipt schema is not ripr_native_libtest_build.v1".to_string());
    }
    let skip = PathBuf::from(&receipt.target_dir);
    let current = source_digest(root, &[&skip])?;
    if current != receipt.source_digest {
        return Err(
            "source, lock, features, profile, rustflags, or build inputs moved after compile"
                .to_string(),
        );
    }
    let lock = match root.join("Cargo.lock") {
        path if path.is_file() => Some(sha256_file(&path)?),
        _ => None,
    };
    if lock != receipt.cargo_lock_sha256 {
        return Err(
            "source, lock, features, profile, rustflags, or build inputs moved after compile"
                .to_string(),
        );
    }
    let rustflags = std::env::var("RUSTFLAGS")
        .ok()
        .filter(|value| !value.is_empty());
    if rustflags != receipt.rustflags {
        return Err("rustflags moved after compile; replay is not admitted".to_string());
    }
    refuse_if_executable_replaced(receipt)
}

pub(crate) fn refuse_if_executable_replaced(receipt: &NativeLibtestReceipt) -> Result<(), String> {
    let path = Path::new(&receipt.executable);
    let digest = sha256_file(path)?;
    if digest != receipt.executable_sha256 {
        return Err(
            "bound libtest executable was replaced after compile; replay is not admitted"
                .to_string(),
        );
    }
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "stat bound libtest executable `{}`: {error}",
            path.display()
        )
    })?;
    if metadata.len() != receipt.executable_bytes {
        return Err(
            "bound libtest executable size diverged after compile; replay is not admitted"
                .to_string(),
        );
    }
    Ok(())
}

pub(crate) fn cargo_json_messages(output: &[u8]) -> Result<Vec<Value>, String> {
    let text =
        std::str::from_utf8(output).map_err(|error| format!("Cargo JSON is not UTF-8: {error}"))?;
    if text.trim().is_empty() {
        return Err("Cargo JSON output is empty or truncated".to_string());
    }
    let mut messages = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.chars().all(char::is_whitespace) {
            continue;
        }
        let message = serde_json::from_str::<Value>(line).map_err(|error| {
            format!(
                "Cargo JSON line {} is truncated or malformed: {error}",
                index + 1
            )
        })?;
        messages.push(message);
    }
    if messages.is_empty() {
        return Err("Cargo JSON output is empty or truncated".to_string());
    }
    Ok(messages)
}

pub(crate) fn parse_libtest_list(stdout: &[u8]) -> Result<Vec<String>, String> {
    let text = std::str::from_utf8(stdout)
        .map_err(|error| format!("libtest listing is not UTF-8: {error}"))?;
    if text.is_empty() || !text.ends_with('\n') {
        return Err(
            "libtest listing is missing or truncated; empty inventory cannot succeed".to_string(),
        );
    }
    let mut names = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || is_libtest_list_summary(trimmed) {
            continue;
        }
        let Some((name, kind)) = trimmed.rsplit_once(": ") else {
            return Err(format!(
                "libtest listing is truncated or malformed at `{trimmed}`"
            ));
        };
        if kind != "test" && kind != "bench" {
            continue;
        }
        if name.is_empty() {
            return Err("libtest listing contains an empty test name".to_string());
        }
        names.push(name.to_string());
    }
    names.sort();
    names.dedup();
    Ok(names)
}

struct ResultCounts {
    passed: usize,
    failed: usize,
    ignored: usize,
}

fn parse_libtest_result(stdout: &str) -> Result<ResultCounts, String> {
    for line in stdout.lines() {
        let Some(rest) = line.trim().strip_prefix("test result: ") else {
            continue;
        };
        let Some((_, rest)) = rest.split_once(". ") else {
            continue;
        };
        let mut remaining = rest;
        let mut passed = None;
        let mut failed = None;
        let mut ignored = None;
        for (slot, label) in [
            (&mut passed, " passed; "),
            (&mut failed, " failed; "),
            (&mut ignored, " ignored; "),
        ] {
            let Some((count, rest)) = remaining.split_once(label) else {
                return Err("libtest result line is truncated".to_string());
            };
            *slot = Some(parse_count(count)?);
            remaining = rest;
        }
        return Ok(ResultCounts {
            passed: passed.ok_or_else(|| "libtest result is missing passed count".to_string())?,
            failed: failed.ok_or_else(|| "libtest result is missing failed count".to_string())?,
            ignored: ignored
                .ok_or_else(|| "libtest result is missing ignored count".to_string())?,
        });
    }
    Err("libtest output is missing a test result line".to_string())
}

fn is_libtest_list_summary(line: &str) -> bool {
    let Some((tests, benchmarks)) = line.split_once(", ") else {
        return false;
    };
    is_count_label(tests, "test") && is_count_label(benchmarks, "benchmark")
}

fn is_count_label(value: &str, singular: &str) -> bool {
    let Some((count, label)) = value.split_once(' ') else {
        return false;
    };
    let plural = match singular {
        "test" => "tests",
        "benchmark" => "benchmarks",
        _ => return false,
    };
    !count.is_empty()
        && count.bytes().all(|byte| byte.is_ascii_digit())
        && (label == singular || label == plural)
}

fn parse_count(value: &str) -> Result<usize, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("libtest count `{value}` is not a number"));
    }
    value
        .parse()
        .map_err(|error| format!("libtest count `{value}` overflowed: {error}"))
}

fn require_complete_build_finished(messages: &[Value]) -> Result<(), String> {
    let finished = messages
        .iter()
        .filter(|message| message.get("reason").and_then(Value::as_str) == Some("build-finished"))
        .collect::<Vec<_>>();
    match finished.as_slice() {
        [message] if message.get("success") == Some(&Value::Bool(true)) => Ok(()),
        [message] => Err(format!(
            "Cargo JSON build-finished is not success: {message}"
        )),
        [] => Err(
            "Cargo JSON lacks terminal build-finished; output is incomplete even if cargo exited 0"
                .to_string(),
        ),
        _ => Err("Cargo JSON repeats build-finished; refusing incomplete capture".to_string()),
    }
}

fn json_string_list(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn observe_tool(root: &Path, program: &str, args: &[&str]) -> Result<String, String> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = capture_bytes_in_dir_with_timeout(
        Path::new(program),
        &args,
        root,
        &[],
        &[],
        Duration::from_secs(30),
        &format!("observe {program}"),
    )?;
    if output.timed_out || !output.status.is_some_and(|status| status.success()) {
        return Err(format!("{program} observation failed or timed out"));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} observation is not UTF-8: {error}"))?;
    if text.trim().is_empty() {
        return Err(format!("{program} observation is empty"));
    }
    Ok(text)
}

fn invoke_bound_binary(
    program: &Path,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
    context: &str,
    replay_executable: Option<&str>,
) -> Result<TimedBytesOutput, String> {
    if !program.is_absolute() {
        return Err(format!("{context} requires an absolute executable path"));
    }
    let mut envs = Vec::new();
    let replay_path;
    if let Some(executable) = replay_executable {
        replay_path = replay_path_without_compilers(executable)?;
        envs.push(("PATH", replay_path.as_str()));
        envs.push(("CARGO_NET_OFFLINE", "true"));
    }
    let output = capture_bytes_in_dir_with_env_budget(
        program,
        args,
        cwd,
        (
            &envs,
            if replay_executable.is_some() {
                &REPLAY_ENV_REMOVE[..]
            } else {
                &[]
            },
        ),
        ByteCaptureBudget {
            timeout,
            stdout_bytes: 8 * 1024 * 1024,
            stderr_bytes: 1024 * 1024,
        },
        context,
    )?;
    if output.timed_out {
        return Err(format!("{context} exceeded the owned process deadline"));
    }
    if !output.status.is_some_and(|status| status.success()) && replay_executable.is_none() {
        return Err(format!(
            "{context} failed with status {:?}: {}",
            output.status.and_then(|status| status.code()),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output)
}

fn replay_path_without_compilers(executable: &str) -> Result<String, String> {
    let parent = Path::new(executable)
        .parent()
        .ok_or_else(|| "libtest executable has no parent directory".to_string())?;
    parent
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| "libtest executable parent is not UTF-8".to_string())
}

fn source_digest(root: &Path, skip_dirs: &[&Path]) -> Result<String, String> {
    let skip = skip_dirs
        .iter()
        .filter_map(|path| std::path::absolute(path).ok())
        .collect::<Vec<_>>();
    let mut files = BTreeMap::new();
    collect_source_files(root, root, &skip, &mut files)?;
    if files.is_empty() {
        return Err("libtest worktree has no hashed source files".to_string());
    }
    let mut joined = String::new();
    for (path, digest) in files {
        joined.push_str(&path);
        joined.push('\t');
        joined.push_str(&digest);
        joined.push('\n');
    }
    Ok(sha256_bytes(joined.as_bytes()))
}

fn collect_source_files(
    root: &Path,
    dir: &Path,
    skip_dirs: &[PathBuf],
    files: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|error| format!("read libtest source `{}`: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry
            .map_err(|error| format!("read libtest source entry `{}`: {error}", dir.display()))?;
        let path = entry.path();
        let name = entry.file_name();
        let absolute = std::path::absolute(&path).ok();
        if name == ".git"
            || name == "Cargo.lock"
            || (name == "target" && dir.join("Cargo.toml").is_file())
            || absolute.as_ref().is_some_and(|candidate| {
                skip_dirs
                    .iter()
                    .any(|skip| candidate == skip || candidate.starts_with(skip))
            })
        {
            continue;
        }
        let file_type = entry
            .file_type()
            .map_err(|error| format!("stat libtest source `{}`: {error}", path.display()))?;
        if file_type.is_dir() {
            collect_source_files(root, &path, skip_dirs, files)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_error| "libtest source escaped the worktree".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        files.insert(relative, sha256_file(&path)?);
    }
    Ok(())
}

fn inventory_digest(names: &[String]) -> String {
    sha256_bytes(names.join("\n").as_bytes())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("read digest input `{}`: {error}", path.display()))?;
    Ok(sha256_bytes(&bytes))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn executable_mode(metadata: &fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        format!("{:o}", metadata.permissions().mode())
    }
    #[cfg(not(unix))]
    {
        let _ = metadata;
        "windows-file".to_string()
    }
}

fn read_cgroup() -> Option<String> {
    fs::read_to_string("/proc/self/cgroup")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn artifact_json(package: &str, kind: &str, test: bool, executable: Option<&str>) -> String {
        let executable = match executable {
            Some(path) => format!("\"{path}\""),
            None => "null".to_string(),
        };
        format!(
            r#"{{"reason":"compiler-artifact","package_id":"{package} 0.1.0 (path+file:///tmp/{package})","manifest_path":"/tmp/{package}/Cargo.toml","target":{{"kind":["{kind}"],"name":"{package}","src_path":"/tmp/{package}/src/lib.rs"}},"profile":{{"test":{test}}},"features":[],"executable":{executable}}}"#
        )
    }

    fn finished(success: bool) -> String {
        format!(r#"{{"reason":"build-finished","success":{success}}}"#)
    }

    fn jsonl(lines: &[&str]) -> Vec<u8> {
        let mut out = lines.join("\n");
        out.push('\n');
        out.into_bytes()
    }

    fn temp_root(label: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "ripr-native-libtest-{label}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        Ok(dir)
    }

    fn write_fixture(root: &Path, token: &str) -> Result<(), String> {
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"harness\", \"support\"]\nresolver = \"2\"\n",
        )
        .map_err(|error| error.to_string())?;
        fs::create_dir_all(root.join("support/src")).map_err(|error| error.to_string())?;
        fs::write(
            root.join("support/Cargo.toml"),
            "[package]\nname = \"native_libtest_support\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            root.join("support/src/lib.rs"),
            "pub fn marker() -> &'static str { \"support\" }\n",
        )
        .map_err(|error| error.to_string())?;
        fs::create_dir_all(root.join("harness/src")).map_err(|error| error.to_string())?;
        fs::write(
            root.join("harness/Cargo.toml"),
            "[package]\nname = \"native_libtest_harness\"\nversion = \"0.1.0\"\nedition = \"2021\"\nbuild = \"build.rs\"\n\n[dependencies]\nnative_libtest_support = { path = \"../support\" }\n",
        )
        .map_err(|error| error.to_string())?;
        fs::write(root.join("harness/input.txt"), format!("{token}\n"))
            .map_err(|error| error.to_string())?;
        fs::write(
            root.join("harness/build.rs"),
            r#"fn main() {
    println!("cargo:rerun-if-changed=input.txt");
    let token = std::fs::read_to_string("input.txt").unwrap_or_default();
    let out = std::env::var("OUT_DIR").unwrap_or_default();
    let path = std::path::Path::new(&out).join("token.rs");
    let body = format!("pub const TOKEN: &str = {:?};\n", token.trim());
    let _ = std::fs::write(path, body);
    let _ = std::fs::write("generated_by_build.txt", token.trim());
}
"#,
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            root.join("harness/src/lib.rs"),
            r#"include!(concat!(env!("OUT_DIR"), "/token.rs"));

pub fn token() -> &'static str {
    TOKEN
}

pub fn support_marker() -> &'static str {
    native_libtest_support::marker()
}

#[cfg(test)]
mod tests {
    #[test]
    fn alpha() {
        assert_eq!(crate::support_marker(), "support");
    }

    #[test]
    fn beta() {
        assert_eq!(crate::token(), "build-token");
    }

    #[test]
    fn gamma_fail() {
        panic!("intentional native-libtest failure");
    }

    #[test]
    #[ignore]
    fn delta_ignored() {}
}
"#,
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn compile_fixture(root: &Path) -> Result<NativeLibtestReceipt, String> {
        compile_libtest(&CompileRequest {
            root,
            target_dir: &root.join("target-private"),
            package: "native_libtest_harness",
            locked: false,
            offline: true,
            timeout: Duration::from_mins(3),
        })
    }

    fn must_fail<T: std::fmt::Debug>(
        result: Result<T, String>,
        why: &str,
    ) -> Result<String, String> {
        match result {
            Err(error) => Ok(error),
            Ok(value) => Err(format!("{why}: unexpectedly succeeded with {value:?}")),
        }
    }

    #[test]
    fn ripr_compile_argv_is_the_locked_lib_no_run_json_request() -> Result<(), String> {
        assert_eq!(
            ripr_libtest_compile_argv(),
            vec![
                "test",
                "--locked",
                "-p",
                "ripr",
                "--lib",
                "--no-run",
                "--message-format=json",
            ]
        );
        refuse_compile_filter(&ripr_libtest_compile_argv())?;
        let root = Path::new(".");
        let target_dir = Path::new("target");
        let args = compile_argv_for(&CompileRequest {
            root,
            target_dir,
            package: RIPR_LIBTEST_PACKAGE,
            locked: true,
            offline: false,
            timeout: Duration::from_secs(1),
        })?;
        assert_eq!(args, ripr_libtest_compile_argv());
        Ok(())
    }

    #[test]
    fn focused_filter_is_not_a_smaller_compile_graph() -> Result<(), String> {
        let mut args = ripr_libtest_compile_argv();
        args.extend(["--".to_string(), "alpha".to_string()]);
        let error = must_fail(refuse_compile_filter(&args), "filter must be refused")?;
        if !error.contains("focused filter") {
            return Err(format!("unexpected refusal: {error}"));
        }
        Ok(())
    }

    #[test]
    fn missing_libtest_executable_is_not_success() -> Result<(), String> {
        let output = jsonl(&[
            &artifact_json("native_libtest_harness", "lib", false, None),
            &finished(true),
        ]);
        let error = must_fail(
            select_libtest_executable(&output, "native_libtest_harness"),
            "missing executable must refuse",
        )?;
        if !error.contains("no exact libtest executable") {
            return Err(format!("unexpected refusal: {error}"));
        }
        Ok(())
    }

    #[test]
    fn duplicate_libtest_artifacts_are_ambiguous() -> Result<(), String> {
        let output = jsonl(&[
            &artifact_json(
                "native_libtest_harness",
                "lib",
                true,
                Some("/tmp/old-harness"),
            ),
            &artifact_json(
                "native_libtest_harness",
                "lib",
                true,
                Some("/tmp/new-harness"),
            ),
            &finished(true),
        ]);
        let error = must_fail(
            select_libtest_executable(&output, "native_libtest_harness"),
            "duplicates must refuse",
        )?;
        if !error.contains("ambiguous") {
            return Err(format!("unexpected refusal: {error}"));
        }
        Ok(())
    }

    #[test]
    fn wrong_target_and_newer_neighbor_are_not_selected() -> Result<(), String> {
        let output = jsonl(&[
            &artifact_json(
                "native_libtest_support",
                "lib",
                true,
                Some("/tmp/newer-support"),
            ),
            &artifact_json(
                "native_libtest_harness",
                "bin",
                true,
                Some("/tmp/newer-bin"),
            ),
            &artifact_json(
                "native_libtest_harness",
                "lib",
                true,
                Some("/tmp/exact-harness"),
            ),
            &finished(true),
        ]);
        let selected = select_libtest_executable(&output, "native_libtest_harness")?;
        assert_eq!(selected.executable, PathBuf::from("/tmp/exact-harness"));
        assert_eq!(selected.target_kind, vec!["lib".to_string()]);
        assert!(selected.profile_test);
        Ok(())
    }

    #[test]
    fn truncated_json_and_missing_build_finished_are_incomplete() -> Result<(), String> {
        let truncated = br#"{"reason":"compiler-artifact","package_id":"native_libtest_harness"#;
        let error = must_fail(
            select_libtest_executable(truncated, "native_libtest_harness"),
            "truncated JSON must refuse",
        )?;
        if !(error.contains("truncated") || error.contains("malformed") || error.contains("UTF-8"))
        {
            return Err(format!("unexpected refusal: {error}"));
        }

        let missing_finished = jsonl(&[&artifact_json(
            "native_libtest_harness",
            "lib",
            true,
            Some("/tmp/exact-harness"),
        )]);
        let error = must_fail(
            select_libtest_executable(&missing_finished, "native_libtest_harness"),
            "missing build-finished must refuse",
        )?;
        if !error.contains("build-finished") {
            return Err(format!("unexpected refusal: {error}"));
        }
        Ok(())
    }

    #[test]
    fn truncated_listing_is_not_zero_inventory_success() -> Result<(), String> {
        let error = must_fail(parse_libtest_list(b""), "empty listing")?;
        if !(error.contains("truncated") || error.contains("missing")) {
            return Err(format!("unexpected empty listing refusal: {error}"));
        }
        let error = must_fail(parse_libtest_list(b"tests::alpha: tes"), "cut listing")?;
        if !(error.contains("truncated") || error.contains("malformed")) {
            return Err(format!("unexpected cut listing refusal: {error}"));
        }
        let names = parse_libtest_list(
            b"tests::beta: test\ntests::alpha: test\n\n2 tests, 0 benchmarks\n",
        )?;
        assert_eq!(names, vec!["tests::alpha", "tests::beta"]);
        Ok(())
    }

    #[test]
    fn command_refuses_missing_root_and_package_without_compiling() -> Result<(), String> {
        let error = must_fail(parse_run_args(&[]), "missing args")?;
        if !(error.contains("--root")
            && error.contains("--package")
            && error.contains("no implied ripr compile"))
        {
            return Err(format!("unexpected missing-args refusal: {error}"));
        }
        Ok(())
    }

    #[test]
    fn zero_match_filter_cannot_pass() -> Result<(), String> {
        let stdout = "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.00s\n";
        let counts = parse_libtest_result(stdout)?;
        assert_eq!(counts.passed + counts.failed + counts.ignored, 0);
        Ok(())
    }

    #[test]
    fn fixture_compile_identifies_json_executable_and_replays_without_cargo() -> Result<(), String>
    {
        let root = temp_root("live")?;
        write_fixture(&root, "build-token")?;
        let receipt = compile_fixture(&root)?;
        assert_eq!(receipt.schema_version, RECEIPT_SCHEMA);
        assert_eq!(receipt.package, "native_libtest_harness");
        assert_eq!(receipt.target_kind, vec!["lib".to_string()]);
        assert!(receipt.profile_test);
        assert!(receipt.json_complete);
        assert!(receipt.process_tree_settled);
        assert!(
            Path::new(&receipt.executable).starts_with(&receipt.target_dir),
            "executable {} escaped {}",
            receipt.executable,
            receipt.target_dir
        );
        assert!(
            receipt
                .test_inventory
                .iter()
                .any(|name| name.ends_with("alpha")),
            "inventory: {:?}",
            receipt.test_inventory
        );
        assert!(
            receipt
                .test_inventory
                .iter()
                .any(|name| name.ends_with("beta")),
            "inventory: {:?}",
            receipt.test_inventory
        );

        let decoy = Path::new(&receipt.executable)
            .parent()
            .ok_or_else(|| "missing executable parent".to_string())?
            .join("newer-decoy-binary");
        fs::write(&decoy, b"not-the-libtest").map_err(|error| error.to_string())?;
        let later = SystemTime::now()
            .checked_add(Duration::from_mins(1))
            .ok_or_else(|| "mtime overflow".to_string())?;
        filetime_or_ignore(&decoy, later)?;
        assert_ne!(
            decoy.to_string_lossy().as_ref(),
            receipt.executable.as_str()
        );

        let listed = replay_list(&receipt, &root)?;
        assert_eq!(listed.executed, receipt.test_inventory.len());
        assert!(listed.process_tree_settled);

        let alpha = receipt
            .test_inventory
            .iter()
            .find(|name| name.ends_with("alpha"))
            .ok_or_else(|| "alpha test missing".to_string())?;
        let beta = receipt
            .test_inventory
            .iter()
            .find(|name| name.ends_with("beta"))
            .ok_or_else(|| "beta test missing".to_string())?;
        let first = replay_filter(&receipt, &root, alpha)?;
        let second = replay_filter(&receipt, &root, beta)?;
        assert_eq!(
            first.argv.first().map(String::as_str),
            Some(receipt.executable.as_str())
        );
        assert!(
            first
                .argv
                .iter()
                .all(|arg| !arg.contains("cargo") && !arg.contains("rustc"))
        );
        assert_eq!(first.executed, 1);
        assert_eq!(first.passed, 1);
        assert_eq!(second.executed, 1);
        assert_eq!(second.passed, 1);
        assert!(first.duration_ms > 0 || second.duration_ms > 0 || receipt.compile_duration_ms > 0);
        assert_ne!(receipt.compile_duration_ms, 0);

        let zero = must_fail(
            replay_filter(&receipt, &root, "tests::no_such_libtest_case"),
            "zero-match filter",
        )?;
        if !(zero.contains("zero-match") || zero.contains("zero-test")) {
            return Err(format!("unexpected zero-match refusal: {zero}"));
        }

        let ignored = receipt
            .test_inventory
            .iter()
            .find(|name| name.ends_with("delta_ignored"))
            .ok_or_else(|| "ignored test missing".to_string())?;
        let ignored_err = must_fail(
            replay_filter(&receipt, &root, ignored),
            "ignored-only filter",
        )?;
        if !ignored_err.contains("ignored-only") {
            return Err(format!("unexpected ignored-only refusal: {ignored_err}"));
        }

        let failing = receipt
            .test_inventory
            .iter()
            .find(|name| name.ends_with("gamma_fail"))
            .ok_or_else(|| "failing test missing".to_string())?;
        let failed_err = must_fail(replay_filter(&receipt, &root, failing), "failing filter")?;
        if !failed_err.contains("replay failed") {
            return Err(format!("unexpected failing-filter refusal: {failed_err}"));
        }

        let nested = root.join("harness/src/target");
        fs::create_dir_all(&nested).map_err(|error| error.to_string())?;
        fs::write(nested.join("mod.rs"), "pub fn nested() {}\n")
            .map_err(|error| error.to_string())?;
        let nested_err = must_fail(
            refuse_if_inputs_changed(&receipt, &root),
            "nested src/target source must be hashed",
        )?;
        if !(nested_err.contains("build inputs moved") || nested_err.contains("source")) {
            return Err(format!("unexpected nested-target refusal: {nested_err}"));
        }
        fs::remove_dir_all(&nested).map_err(|error| error.to_string())?;
        refuse_if_inputs_changed(&receipt, &root)?;

        fs::write(&receipt.executable, b"replaced-libtest-bytes")
            .map_err(|error| error.to_string())?;
        let replaced = must_fail(
            replay_list(&receipt, &root),
            "replaced executable must refuse replay",
        )?;
        if !replaced.contains("replaced after compile") {
            return Err(format!(
                "unexpected replaced-executable refusal: {replaced}"
            ));
        }

        fs::write(root.join("harness/input.txt"), "changed-token\n")
            .map_err(|error| error.to_string())?;
        let changed = must_fail(
            refuse_if_inputs_changed(&receipt, &root),
            "changed build input must refuse replay",
        )?;
        if !(changed.contains("build inputs moved")
            || changed.contains("source")
            || changed.contains("replaced after compile"))
        {
            return Err(format!("unexpected changed-input refusal: {changed}"));
        }
        Ok(())
    }

    fn filetime_or_ignore(path: &Path, later: SystemTime) -> Result<(), String> {
        fs::File::open(path)
            .and_then(|file| file.set_modified(later))
            .map_err(|error| error.to_string())
    }
}
