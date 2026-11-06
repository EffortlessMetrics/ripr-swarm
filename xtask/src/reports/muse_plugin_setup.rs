//! `cargo xtask muse-plugin-setup` (#13184) — deterministic typed-setup
//! producer/evaluator seam for native Muse trials of the RIPR plugin.
//!
//! This slice deliberately stops at setup: no model-dependent journeys, no
//! natural-request runs, no scoreboard aggregation. The producer attempts the
//! real native setup against the actual Muse entry point (`muse.cmd` on the
//! Windows host, or any explicit `--muse` path) under a controlled PATH, and
//! retains the actual evidence bytes of every step: argv, stdout/stderr files
//! with digests, exit status, spawn errors, termination reason, and
//! observation timestamps. The evaluator never trusts the producer's claim:
//! it re-derives every observation from the retained bundle bytes, recomputes
//! artifact digests, and REFUSES missing, substituted, or mismatched bytes.
//!
//! Two deterministic setups are exercised for the `missing-or-unapproved-server`
//! scenario:
//!
//! - `missing-ripr`: every PATH entry that exposes a `ripr` binary is removed,
//!   so the typed setup must stop at `server_unavailable` with
//!   `environment.ripr_binary` `missing` (or `predates_mcp` when a stub binary
//!   that cannot serve `ripr mcp --stdio` is exposed instead).
//! - `unapproved-capability`: the plugin is installed through the real Muse
//!   CLI but its MCP capability `plugin:ripr:mcp_server:core` is left
//!   unapproved, so the typed setup stops at `approval_required` when the
//!   binary on the controlled PATH is genuinely MCP-capable.
//!
//! An unavailable prerequisite (missing Muse entry point, missing plugin
//! directory) is itself a typed setup outcome, never a panic and never a
//! fabricated success. A process exit 0 is never treated as tools-loaded;
//! this slice records `mcp.tools_discovered` as empty because no session tool
//! listing is part of a setup-only trial.
//!
//! The emitted record conforms to the canonical `muse-plugin-eval-v1` schema
//! owned by the shared evaluation consumer ([#13165]); the schema bytes are
//! not vendored on this branch, so `structural_validate_record` pins the
//! emitted wire shape (required keys, enum memberships, closed key sets) and
//! the tests prove both acceptance and refusal on that shape.
//!
//! Trial bundles are large, transient, host-specific evidence and live under
//! `target/ripr/muse-plugin/` (out of the repository, consistent with the
//! `benchmarks/agent-discrimination` precedent of run-owned scratch). The
//! suite's `receipts/` Markdown files stay historical and are never rewritten
//! by this harness.

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::run::env_timeout_secs;

const BUNDLE_SCHEMA: &str = "ripr-muse-plugin-setup-bundle-v1";
const RECORD_SCHEMA: &str = "muse-plugin-eval-v1";
const RECORD_FILE: &str = "muse-plugin-eval-v1.json";
const MANIFEST_FILE: &str = "manifest.json";
const SETUP_SCENARIO: &str = "missing-or-unapproved-server";
const CAPABILITY_ID: &str = "plugin:ripr:mcp_server:core";
const EVALUATOR_VERSION: &str = "xtask muse-plugin-setup 1";
const DEFAULT_SUITE_DIR: &str = "benchmarks/muse-plugin";
const DEFAULT_PLUGIN_DIR: &str = "plugins/muse/ripr";
const STEP_TIMEOUT_ENV: &str = "RIPR_MUSE_STEP_TIMEOUT_SECS";
const PROBE_TIMEOUT_ENV: &str = "RIPR_MUSE_PROBE_TIMEOUT_SECS";
const DEFAULT_STEP_TIMEOUT_SECS: u64 = 120;
const DEFAULT_PROBE_TIMEOUT_SECS: u64 = 30;
const EXECUTED_ABSENT_VALUES: [&str; 4] = ["not_applicable", "unknown", "not_run", "not_executed"];

const USAGE: &str = "usage: cargo xtask muse-plugin-setup \
[--scenario <id>] [--setup <missing-ripr|unapproved-capability>] \
[--muse <path>] [--suite <dir>] [--plugin-dir <dir>] [--expose-ripr <dir>] \
[--bundle-dir <dir>] | --evaluate-only <bundle-dir>\n\n\
Produces a typed native Muse setup trial for the muse-plugin suite: it \
materializes a private disposable fixture copy, controls PATH, runs the real \
Muse plugin install/inspect sequence plus a bounded `ripr mcp --stdio` probe, \
retains every evidence byte under the bundle directory, then re-derives the \
typed setup outcome from those bytes and writes one `muse-plugin-eval-v1` \
record per trial. `--evaluate-only` re-runs the evaluator (digest verification \
plus re-derivation) over an existing bundle and refuses tampered evidence.";

pub(crate) fn muse_plugin_setup(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{USAGE}");
        return Ok(());
    }
    let options = Options::parse(args)?;
    if let Some(bundle) = options.evaluate_only {
        let record = evaluate_bundle(&bundle)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&record).map_err(|err| err.to_string())?
        );
        println!(
            "verified bundle {} and re-derived {RECORD_FILE}",
            bundle.display()
        );
        return Ok(());
    }
    let bundle_dir = produce_trial(&options)?;
    let record = evaluate_bundle(&bundle_dir)?;
    let outcome = record
        .pointer("/mcp/setup_outcome")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let ripr_binary = record
        .pointer("/environment/ripr_binary")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    println!("typed setup outcome: {outcome} (environment.ripr_binary: {ripr_binary})");
    println!("wrote {}", bundle_dir.join(RECORD_FILE).display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq)]
struct Options {
    scenario: String,
    setup: SetupKind,
    muse: Option<PathBuf>,
    suite_dir: PathBuf,
    plugin_dir: PathBuf,
    expose_ripr: Option<PathBuf>,
    bundle_dir: Option<PathBuf>,
    evaluate_only: Option<PathBuf>,
    read_only: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SetupKind {
    MissingRipr,
    UnapprovedCapability,
}

impl SetupKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::MissingRipr => "missing-ripr",
            Self::UnapprovedCapability => "unapproved-capability",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "missing-ripr" => Some(Self::MissingRipr),
            "unapproved-capability" => Some(Self::UnapprovedCapability),
            _ => None,
        }
    }
}

impl Options {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut scenario = SETUP_SCENARIO.to_string();
        let mut setup = SetupKind::MissingRipr;
        let mut muse = None;
        let mut suite_dir = PathBuf::from(DEFAULT_SUITE_DIR);
        let mut plugin_dir = PathBuf::from(DEFAULT_PLUGIN_DIR);
        let mut expose_ripr = None;
        let mut bundle_dir = None;
        let mut evaluate_only = None;
        let mut read_only = false;
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].as_str();
            let value = |index: &mut usize| -> Result<String, String> {
                *index += 1;
                args.get(*index)
                    .cloned()
                    .ok_or_else(|| format!("{flag} requires a value\n{USAGE}"))
            };
            match flag {
                "--scenario" => scenario = value(&mut index)?,
                "--setup" => {
                    let raw = value(&mut index)?;
                    setup = SetupKind::parse(&raw)
                        .ok_or_else(|| format!("unknown setup {raw:?}\n{USAGE}"))?;
                }
                "--muse" => muse = Some(PathBuf::from(value(&mut index)?)),
                "--suite" => suite_dir = PathBuf::from(value(&mut index)?),
                "--plugin-dir" => plugin_dir = PathBuf::from(value(&mut index)?),
                "--expose-ripr" => expose_ripr = Some(PathBuf::from(value(&mut index)?)),
                "--bundle-dir" => bundle_dir = Some(PathBuf::from(value(&mut index)?)),
                "--evaluate-only" => evaluate_only = Some(PathBuf::from(value(&mut index)?)),
                "--read-only" => read_only = true,
                other => return Err(format!("unknown flag {other:?}\n{USAGE}")),
            }
            index += 1;
        }
        if evaluate_only.is_some() && args.len() > 2 {
            return Err(format!("--evaluate-only takes no other flags\n{USAGE}"));
        }
        Ok(Self {
            scenario,
            setup,
            muse,
            suite_dir,
            plugin_dir,
            expose_ripr,
            bundle_dir,
            evaluate_only,
            read_only,
        })
    }
}

// ---------------------------------------------------------------------------
// Suite model
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Deserialize)]
struct ScenarioFile {
    scenario: Vec<Scenario>,
}

#[derive(Clone, Debug, Deserialize)]
struct Scenario {
    id: String,
    fixture: String,
    kind: String,
    expected: String,
    summary: String,
}

fn load_scenario(suite_dir: &Path, scenario_id: &str) -> Result<Scenario, String> {
    let path = suite_dir.join("scenarios.toml");
    let text =
        fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let file: ScenarioFile =
        toml::from_str(&text).map_err(|err| format!("parse {}: {err}", path.display()))?;
    file.scenario
        .into_iter()
        .find(|scenario| scenario.id == scenario_id)
        .ok_or_else(|| {
            format!(
                "scenario {scenario_id:?} is not in {}; this slice exercises only kind = \"setup\" scenarios",
                path.display()
            )
        })
}

// ---------------------------------------------------------------------------
// Digests and tree snapshots
// ---------------------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(digest.len() * 2);
    for byte in digest {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn unix_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(9_007_199_254_740_991) as u64)
        .map_err(|err| format!("clock before Unix epoch: {err}"))
}

/// One retained file: its bundle-relative path, byte length, and digest.
#[derive(Clone, Debug, serde::Serialize, Deserialize)]
struct FileRef {
    path: String,
    len: u64,
    sha256: String,
}

/// Deterministic digest of one file list: sorted relative paths bound to their
/// length and content bytes.
fn digest_files(files: &[FileRef]) -> String {
    let mut sorted: Vec<&FileRef> = files.iter().collect();
    sorted.sort_by(|left, right| left.path.cmp(&right.path));
    let mut hasher = Sha256::new();
    for file in sorted {
        hasher.update(file.path.as_bytes());
        hasher.update([0]);
        hasher.update(file.len.to_string().as_bytes());
        hasher.update([0]);
        hasher.update(file.sha256.as_bytes());
        hasher.update([0xff]);
    }
    format!("sha256:{}", sha256_hex(&hasher.finalize()))
}

fn hash_file(path: &Path) -> Result<FileRef, String> {
    let bytes = fs::read(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    Ok(FileRef {
        path: String::new(),
        len: bytes.len() as u64,
        sha256: sha256_hex(&bytes),
    })
}

/// Recursively collect regular files under `root`, storing bundle-relative
/// slash-separated paths. Symlinks and non-regular files fail closed.
fn collect_tree_files(root: &Path) -> Result<Vec<FileRef>, String> {
    fn walk(root: &Path, dir: &Path, output: &mut Vec<FileRef>) -> Result<(), String> {
        let mut entries = fs::read_dir(dir)
            .map_err(|err| format!("read {}: {err}", dir.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|err| format!("read {}: {err}", dir.display()))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|err| format!("relativize {}: {err}", path.display()))?;
            let key = relative.to_string_lossy().replace('\\', "/");
            let file_type = entry
                .file_type()
                .map_err(|err| format!("stat {}: {err}", path.display()))?;
            if file_type.is_dir() {
                walk(root, &path, output)?;
            } else if file_type.is_file() {
                let file = hash_file(&path)?;
                output.push(FileRef {
                    path: key,
                    len: file.len,
                    sha256: file.sha256,
                });
            } else {
                return Err(format!(
                    "refusing non-regular evidence file {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }
    let mut output = Vec::new();
    walk(root, root, &mut output)?;
    Ok(output)
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    let mut entries = fs::read_dir(source)
        .map_err(|err| format!("read {}: {err}", source.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("read {}: {err}", source.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let from = entry.path();
        let to = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|err| format!("stat {}: {err}", from.display()))?;
        if file_type.is_dir() {
            fs::create_dir_all(&to).map_err(|err| format!("create {}: {err}", to.display()))?;
            copy_tree(&from, &to)?;
        } else if file_type.is_file() {
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)
                    .map_err(|err| format!("create {}: {err}", parent.display()))?;
            }
            fs::copy(&from, &to)
                .map_err(|err| format!("copy {} to {}: {err}", from.display(), to.display()))?;
        } else {
            return Err(format!("fixture source {} is not a file", from.display()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Step runner: bounded Command with file-captured evidence
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, Deserialize)]
struct StepRecord {
    id: String,
    argv: Vec<String>,
    start_unix_ms: u64,
    end_unix_ms: u64,
    spawn_error: Option<String>,
    exit_code: Option<i32>,
    terminated: Option<String>,
    stdout: FileRef,
    stderr: FileRef,
}

struct StepResult {
    record: StepRecord,
    stdout_bytes: Vec<u8>,
}

/// Run one bounded step. stdout/stderr are redirected straight to bundle
/// files, so a terminated grandchild can never hold a pipe open against the
/// harness; on timeout the direct child is terminated and reaped and the step is
/// marked `terminated: "timeout"` with whatever bytes reached the files.
fn run_step(
    steps_dir: &Path,
    id: &str,
    mut command: Command,
    path_env: Option<&std::ffi::OsStr>,
    timeout: Duration,
) -> Result<StepResult, String> {
    fs::create_dir_all(steps_dir)
        .map_err(|err| format!("create {}: {err}", steps_dir.display()))?;
    let stdout_path = steps_dir.join(format!("{id}.stdout"));
    let stderr_path = steps_dir.join(format!("{id}.stderr"));
    let stdout_file = fs::File::create(&stdout_path)
        .map_err(|err| format!("create {}: {err}", stdout_path.display()))?;
    let stderr_file = fs::File::create(&stderr_path)
        .map_err(|err| format!("create {}: {err}", stderr_path.display()))?;
    if let Some(path_env) = path_env {
        command.env("PATH", path_env);
    }
    command.stdout(Stdio::from(stdout_file));
    command.stderr(Stdio::from(stderr_file));
    let start = unix_ms()?;
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            // The launch itself is evidence: an absent Muse entry point or an
            // absent ripr binary is a typed observation, not a harness error.
            let end = unix_ms()?;
            let empty = FileRef {
                path: String::new(),
                len: 0,
                sha256: sha256_hex(&[]),
            };
            return Ok(StepResult {
                record: StepRecord {
                    id: id.to_string(),
                    argv: argv_of(&command),
                    start_unix_ms: start,
                    end_unix_ms: end,
                    spawn_error: Some(error.to_string()),
                    exit_code: None,
                    terminated: None,
                    stdout: empty.clone(),
                    stderr: empty,
                },
                stdout_bytes: Vec::new(),
            });
        }
    };
    let deadline = std::time::Instant::now() + timeout;
    let mut terminated = None;
    loop {
        match child
            .try_wait()
            .map_err(|err| format!("poll {id}: {err}"))?
        {
            Some(_status) => break,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                terminated = Some("timeout".to_string());
                break;
            }
            None => std::thread::sleep(Duration::from_millis(25)),
        }
    }
    let status = child.wait().ok();
    let end = unix_ms()?;
    let stderr_bytes =
        fs::read(&stderr_path).map_err(|err| format!("read {}: {err}", stderr_path.display()))?;
    let rel = |name: &str| format!("steps/{id}.{name}");
    let stdout_bytes =
        fs::read(&stdout_path).map_err(|err| format!("read {}: {err}", stdout_path.display()))?;
    Ok(StepResult {
        record: StepRecord {
            id: id.to_string(),
            argv: argv_of(&command),
            start_unix_ms: start,
            end_unix_ms: end,
            spawn_error: None,
            exit_code: status.and_then(|status| status.code()),
            terminated,
            stdout: FileRef {
                path: rel("stdout"),
                len: stdout_bytes.len() as u64,
                sha256: sha256_hex(&stdout_bytes),
            },
            stderr: FileRef {
                path: rel("stderr"),
                len: stderr_bytes.len() as u64,
                sha256: sha256_hex(&stderr_bytes),
            },
        },
        stdout_bytes,
    })
}

fn argv_of(command: &Command) -> Vec<String> {
    let mut argv = vec![command.get_program().to_string_lossy().into_owned()];
    argv.extend(
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned()),
    );
    argv
}

/// Spawn the Muse entry point. Windows `.cmd`/`.bat` wrappers are not
/// executable images, so they run through `cmd /d /s /c`; anything else is
/// spawned directly.
fn muse_command(muse: &Path, args: &[String]) -> Command {
    let is_batch = muse
        .extension()
        .map(|extension| {
            let lower = extension.to_string_lossy().to_ascii_lowercase();
            lower == "cmd" || lower == "bat"
        })
        .unwrap_or(false);
    let mut command = if is_batch {
        let mut command = Command::new("cmd");
        command.arg("/d").arg("/s").arg("/c").arg(muse);
        command
    } else {
        Command::new(muse)
    };
    command.args(args);
    command
}

// ---------------------------------------------------------------------------
// Classification (pure: every rule is testable over captured bytes)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RiprBinaryClass {
    Missing,
    PredatesMcp,
    McpCapable,
    Unknown,
}

impl RiprBinaryClass {
    fn recorded(self, setup_failed: bool) -> &'static str {
        // A failed setup must never claim an MCP-capable environment: the
        // capability was not observed in use. Map it to `unknown` instead.
        match (self, setup_failed) {
            (Self::Missing, _) => "missing",
            (Self::PredatesMcp, _) => "predates_mcp",
            (Self::McpCapable, false) => "mcp_capable",
            (Self::McpCapable, true) | (Self::Unknown, _) => "unknown",
        }
    }
}

/// Classify the bounded `ripr mcp --stdio` probe from its raw observation.
/// `spawn_error` is Some when the binary could not be launched at all.
fn classify_ripr_probe(
    spawn_error: Option<&str>,
    exit_code: Option<i32>,
    terminated: Option<&str>,
    output: &[u8],
    got_initialize_reply: bool,
) -> RiprBinaryClass {
    if spawn_error.is_some() {
        return RiprBinaryClass::Missing;
    }
    if got_initialize_reply {
        return RiprBinaryClass::McpCapable;
    }
    if terminated.is_some() {
        return RiprBinaryClass::Unknown;
    }
    let text = String::from_utf8_lossy(output).to_ascii_lowercase();
    let exited_nonzero = exit_code.is_some_and(|code| code != 0);
    let names_unknown_subcommand = [
        "unknown command",
        "unrecognized",
        "not recognized",
        "invalid subcommand",
        "no such subcommand",
        "unexpected argument",
    ]
    .iter()
    .any(|needle| text.contains(needle));
    if names_unknown_subcommand || (!exited_nonzero && !text.is_empty()) {
        // A released pre-MCP binary prints `unknown command "mcp"`; anything
        // that answered the MCP argv with ordinary CLI output is not an MCP
        // server either way.
        if names_unknown_subcommand {
            return RiprBinaryClass::PredatesMcp;
        }
        return RiprBinaryClass::Unknown;
    }
    RiprBinaryClass::Unknown
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ApprovalState {
    TrustedEnabled,
    ReviewNeeded,
    Modified,
    NotInstalled,
    Unknown,
}

/// Derive the capability trust state from the retained `plugins inspect`
/// bytes. The capability identity must be present; otherwise the plugin is
/// treated as not installed regardless of exit code.
fn classify_approval(inspect_stdout: &[u8]) -> ApprovalState {
    let text = String::from_utf8_lossy(inspect_stdout);
    if !text.contains(CAPABILITY_ID) {
        return ApprovalState::NotInstalled;
    }
    if text.contains("trusted_enabled") {
        return ApprovalState::TrustedEnabled;
    }
    if text.contains("review_needed") {
        return ApprovalState::ReviewNeeded;
    }
    if text.contains("modified") {
        return ApprovalState::Modified;
    }
    ApprovalState::Unknown
}

const SETUP_OUTCOME_READY: &str = "ready";
const SETUP_OUTCOME_APPROVAL: &str = "approval_required";
const SETUP_OUTCOME_UNAVAILABLE: &str = "server_unavailable";

/// Derive the typed setup outcome from the classified observations. Every
/// branch names its reason; the result feeds both the record and the
/// human-readable judgment.
fn derive_setup_outcome(
    muse_available: bool,
    ripr: RiprBinaryClass,
    approval: ApprovalState,
) -> (&'static str, String) {
    if !muse_available {
        return (
            SETUP_OUTCOME_UNAVAILABLE,
            "the Muse entry point could not be launched; the plugin server cannot be established on this host".to_string(),
        );
    }
    match ripr {
        RiprBinaryClass::Missing => (
            SETUP_OUTCOME_UNAVAILABLE,
            "no ripr binary is visible on the controlled PATH; `ripr mcp --stdio` cannot be served".to_string(),
        ),
        RiprBinaryClass::PredatesMcp => (
            SETUP_OUTCOME_UNAVAILABLE,
            "the ripr binary on the controlled PATH predates the `mcp` subcommand; an MCP-capable ripr build is the version floor for the plugin server".to_string(),
        ),
        RiprBinaryClass::Unknown => (
            SETUP_OUTCOME_UNAVAILABLE,
            "the ripr binary on the controlled PATH did not complete an MCP initialize handshake within the probe budget".to_string(),
        ),
        RiprBinaryClass::McpCapable => match approval {
            ApprovalState::TrustedEnabled => (
                SETUP_OUTCOME_READY,
                "the MCP capability handshake succeeded and the installed capability is trusted_enabled".to_string(),
            ),
            ApprovalState::ReviewNeeded | ApprovalState::Modified => (
                SETUP_OUTCOME_APPROVAL,
                format!("the installed capability {CAPABILITY_ID} is not approved for the current package; approval is the remaining blocker"),
            ),
            ApprovalState::NotInstalled => (
                SETUP_OUTCOME_UNAVAILABLE,
                "the plugin is not installed or the inspect output does not name the declared MCP capability".to_string(),
            ),
            ApprovalState::Unknown => (
                SETUP_OUTCOME_UNAVAILABLE,
                "the installed capability's trust state could not be read from the inspect bytes".to_string(),
            ),
        },
    }
}

/// Extract a `name version` label from a `--version` first line, e.g.
/// `Muse Code 1.4.14 (1.4.14-R7309.1)` -> `Muse Code 1.4.14`, `ripr 0.10.0`
/// -> `ripr 0.10.0`. Returns None when no line carries a digit.
fn version_label(stdout: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.chars().any(|ch| ch.is_ascii_digit()))
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// Bundle manifest
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, serde::Serialize, Deserialize)]
struct BundleManifest {
    schema_version: String,
    trial_id: String,
    scenario_id: String,
    setup_kind: String,
    suite_dir: String,
    plugin_dir: String,
    plugin_package_sha256: String,
    muse_entry: String,
    created_unix_ms: u64,
    controlled_path: ControlledPath,
    install_executed: bool,
    fixture_digest: String,
    fixture_source_digest: String,
    files: Vec<FileRef>,
    steps: Vec<StepRecord>,
    producer_claim: ProducerClaim,
}

#[derive(Clone, Debug, serde::Serialize, Deserialize)]
struct ControlledPath {
    setup: String,
    ripr_filter_removed_entries: usize,
    exposed_ripr_dir: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, Deserialize)]
struct ProducerClaim {
    setup_outcome: String,
    ripr_binary: String,
}

/// Build the digest-verified file list for one bundle: every regular file
/// under `bundle_dir` except the manifest and the emitted record, with
/// bundle-relative paths.
fn bundle_files(bundle_dir: &Path) -> Result<Vec<FileRef>, String> {
    let files = collect_tree_files(bundle_dir)?
        .into_iter()
        .filter(|file| file.path != MANIFEST_FILE && file.path != RECORD_FILE)
        .collect();
    Ok(files)
}

/// Refuse when any retained byte is missing, substituted, or mismatched:
/// every manifest-listed file must exist with its recorded length and digest,
/// and every file present must be listed (unknown extra evidence fails closed
/// too, so a substitution cannot hide as an addition).
fn verify_bundle_bytes(manifest: &BundleManifest, bundle_dir: &Path) -> Result<(), String> {
    let mut listed: Vec<&FileRef> = manifest.files.iter().collect();
    listed.sort_by(|left, right| left.path.cmp(&right.path));
    for file in &listed {
        let path = bundle_dir.join(&file.path);
        let bytes = fs::read(&path).map_err(|err| {
            format!(
                "bundle is missing retained evidence {}: {err}; refusing to evaluate",
                file.path
            )
        })?;
        let digest = sha256_hex(&bytes);
        if digest != file.sha256 || bytes.len() as u64 != file.len {
            return Err(format!(
                "retained evidence {} failed digest verification (recorded {} bytes {}, recomputed {} bytes {}); refusing to evaluate a substituted or tampered bundle",
                file.path,
                file.len,
                &file.sha256[..8.min(file.sha256.len())],
                bytes.len(),
                &digest[..8],
            ));
        }
    }
    let on_disk = bundle_files(bundle_dir)?;
    for file in &on_disk {
        if !listed.iter().any(|listed| listed.path == file.path) {
            return Err(format!(
                "bundle contains unlisted evidence {}; refusing to evaluate bytes the manifest does not own",
                file.path
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Producer
// ---------------------------------------------------------------------------

fn produce_trial(options: &Options) -> Result<PathBuf, String> {
    let scenario = load_scenario(&options.suite_dir, &options.scenario)?;
    if scenario.kind != "setup" {
        return Err(format!(
            "scenario {:?} has kind {:?}; the typed-setup producer only runs kind = \"setup\" scenarios",
            scenario.id, scenario.kind
        ));
    }
    let fixture_source = options.suite_dir.join(&scenario.fixture);
    if !fixture_source.is_dir() {
        return Err(format!(
            "fixture {} for scenario {} is missing; an unavailable prerequisite is reported by the harness, not fabricated",
            fixture_source.display(),
            scenario.id
        ));
    }
    if !options
        .plugin_dir
        .join(".muse-plugin/plugin.json")
        .is_file()
    {
        return Err(format!(
            "plugin manifest {} is missing; install prerequisites must exist before a trial can start",
            options.plugin_dir.display()
        ));
    }
    let plugin_package = plugin_manifest_digest(&options.plugin_dir)?;

    let stamp = unix_ms()?;
    let bundle_dir = options.bundle_dir.clone().unwrap_or_else(|| {
        PathBuf::from("target/ripr/muse-plugin").join(format!("{}-{stamp}", scenario.id))
    });
    if bundle_dir.exists() {
        return Err(format!(
            "bundle directory {} already exists; refusing to overwrite retained evidence",
            bundle_dir.display()
        ));
    }
    let fixture_dir = bundle_dir.join("fixture");
    fs::create_dir_all(&fixture_dir)
        .map_err(|err| format!("create {}: {err}", fixture_dir.display()))?;
    copy_tree(&fixture_source, &fixture_dir)?;
    let fixture_source_digest = digest_files(&collect_tree_files(&fixture_source)?);
    let fixture_digest = digest_files(&collect_tree_files(&fixture_dir)?);
    if fixture_source_digest != fixture_digest {
        return Err(
            "the materialized fixture copy does not match the suite fixture bytes; refusing to run a trial on substituted inputs"
                .to_string(),
        );
    }

    // Controlled PATH: drop every entry that exposes a ripr binary (the
    // missing/pre-MCP setups), then optionally prepend one explicit dir back
    // (the unapproved-capability setup needs a genuinely MCP-capable ripr so
    // approval is the only remaining blocker).
    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut kept: Vec<PathBuf> = Vec::new();
    let mut removed = 0_usize;
    for entry in std::env::split_paths(&original_path) {
        if dir_exposes_ripr(&entry) {
            removed += 1;
        } else {
            kept.push(entry);
        }
    }
    if let Some(expose) = &options.expose_ripr {
        if options.setup == SetupKind::MissingRipr {
            return Err(
                "--expose-ripr contradicts --setup missing-ripr; the missing setup must not expose any ripr binary"
                    .to_string(),
            );
        }
        kept.insert(0, expose.clone());
    }
    // Resolve ripr explicitly against only the controlled PATH dirs. Relying
    // on the OS executable search would be host-dependent: on Windows the
    // application directory (for example the cargo test deps dir, which holds
    // a dev ripr.exe) is searched before PATH, which would let an MCP-capable
    // build leak into a trial that must observe `missing`.
    let ripr_target = resolve_ripr_on_path(&kept).unwrap_or_else(|| {
        kept.first()
            .map(|dir| dir.join("ripr-not-present.exe"))
            .unwrap_or_else(|| PathBuf::from("ripr-not-present.exe"))
    });
    let controlled_path = std::env::join_paths(kept.iter().map(PathBuf::as_path))
        .map_err(|err| format!("build the controlled PATH: {err}"))?;

    let steps_dir = bundle_dir.join("steps");
    let step_timeout = env_timeout_secs(STEP_TIMEOUT_ENV, DEFAULT_STEP_TIMEOUT_SECS)?;
    let muse = options
        .muse
        .clone()
        .unwrap_or_else(|| PathBuf::from("muse"));
    let plugin_display = absolute_display(&options.plugin_dir);

    // 1. Muse availability and version.
    let StepResult {
        record: version_record,
        stdout_bytes: version_stdout,
        ..
    } = run_step(
        &steps_dir,
        "muse-version",
        muse_command(&muse, &["--version".to_string()]),
        Some(&controlled_path),
        step_timeout,
    )?;
    let muse_available =
        version_record.spawn_error.is_none() && version_record.exit_code == Some(0);
    let _ = version_stdout;

    // 2. ripr version on the controlled PATH (an absent binary is retained as
    // a typed spawn error, not a harness failure).
    let StepResult {
        record: ripr_version_record,
        stdout_bytes: ripr_version_stdout,
        ..
    } = run_step(
        &steps_dir,
        "ripr-version",
        {
            let mut command = Command::new(&ripr_target);
            command.arg("--version");
            command
        },
        Some(&controlled_path),
        step_timeout,
    )?;
    let _ = ripr_version_stdout;

    // 3. Bounded MCP initialize handshake over stdio.
    let probe = probe_ripr_mcp(&ripr_target, &controlled_path)?;
    let probe_record = probe.step_record();
    let ripr_class = classify_ripr_probe(
        probe.spawn_error.as_deref(),
        probe.exit_code,
        probe.terminated.as_deref(),
        &probe.combined_output(),
        probe.got_initialize_reply,
    );

    // 4. The documented native setup sequence: validate, install, inspect.
    // Approval is deliberately NOT run, so the unapproved-capability setup
    // keeps its typed blocker.
    let StepResult {
        record: validate_record,
        ..
    } = run_step(
        &steps_dir,
        "plugins-validate",
        muse_command(
            &muse,
            &[
                "plugins".to_string(),
                "validate".to_string(),
                plugin_display.clone(),
                "--json".to_string(),
            ],
        ),
        Some(&controlled_path),
        step_timeout,
    )?;
    // `plugins install` mutates the host plugin store. Read-only trials keep
    // the host untouched: the remaining blockers (binary, approval state read
    // from inspect) stay fully typed.
    let install_record = if options.read_only {
        println!(
            "read-only trial: skipping `plugins install`; the host plugin store is not mutated"
        );
        None
    } else {
        Some(
            run_step(
                &steps_dir,
                "plugins-install",
                muse_command(
                    &muse,
                    &[
                        "plugins".to_string(),
                        "install".to_string(),
                        plugin_display,
                        "--json".to_string(),
                    ],
                ),
                Some(&controlled_path),
                step_timeout,
            )?
            .record,
        )
    };
    let StepResult {
        record: inspect_record,
        stdout_bytes: inspect_stdout,
        ..
    } = run_step(
        &steps_dir,
        "plugins-inspect",
        muse_command(
            &muse,
            &[
                "plugins".to_string(),
                "inspect".to_string(),
                "ripr".to_string(),
                "--json".to_string(),
            ],
        ),
        Some(&controlled_path),
        step_timeout,
    )?;
    let approval = classify_approval(&inspect_stdout);

    let (setup_outcome, judgment) = derive_setup_outcome(muse_available, ripr_class, approval);
    let setup_failed = setup_outcome != SETUP_OUTCOME_READY;

    // The probe's retained bytes are evidence too.
    fs::write(
        steps_dir.join("ripr-mcp-probe.stdout"),
        probe.stdout_bytes(),
    )
    .map_err(|err| format!("write probe stdout evidence: {err}"))?;
    fs::write(
        steps_dir.join("ripr-mcp-probe.stderr"),
        probe.stderr_bytes(),
    )
    .map_err(|err| format!("write probe stderr evidence: {err}"))?;

    let mut manifest = BundleManifest {
        schema_version: BUNDLE_SCHEMA.to_string(),
        trial_id: bundle_dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| format!("{}-{stamp}", scenario.id)),
        scenario_id: scenario.id.clone(),
        setup_kind: options.setup.as_str().to_string(),
        suite_dir: absolute_display(&options.suite_dir),
        plugin_dir: absolute_display(&options.plugin_dir),
        plugin_package_sha256: plugin_package,
        muse_entry: muse.to_string_lossy().into_owned(),
        created_unix_ms: stamp,
        controlled_path: ControlledPath {
            setup: options.setup.as_str().to_string(),
            ripr_filter_removed_entries: removed,
            exposed_ripr_dir: options
                .expose_ripr
                .as_ref()
                .map(|path| absolute_display(path)),
        },
        install_executed: !options.read_only,
        fixture_digest: fixture_digest.clone(),
        fixture_source_digest,
        files: Vec::new(),
        steps: {
            let mut steps = vec![
                version_record,
                ripr_version_record,
                probe_record,
                validate_record,
            ];
            if let Some(install_record) = install_record {
                steps.push(install_record);
            }
            steps.push(inspect_record);
            steps
        },
        producer_claim: ProducerClaim {
            setup_outcome: setup_outcome.to_string(),
            ripr_binary: ripr_class.recorded(setup_failed).to_string(),
        },
    };
    // Bind the probe's FileRefs now that its evidence bytes are on disk.
    for step in &mut manifest.steps {
        if step.id == "ripr-mcp-probe" {
            step.stdout = hash_bundle_file(&bundle_dir, &step.stdout.path)?;
            step.stderr = hash_bundle_file(&bundle_dir, &step.stderr.path)?;
        }
    }
    manifest.files = bundle_files(&bundle_dir)?;
    let manifest_text = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("serialize the bundle manifest: {err}"))?;
    fs::write(bundle_dir.join(MANIFEST_FILE), format!("{manifest_text}\n"))
        .map_err(|err| format!("write the bundle manifest: {err}"))?;
    println!(
        "produced trial bundle {} (producer claim: setup_outcome {setup_outcome}; {judgment})",
        bundle_dir.display()
    );
    Ok(bundle_dir)
}

fn dir_exposes_ripr(dir: &Path) -> bool {
    dir.join("ripr.exe").is_file()
        || dir.join("ripr").is_file()
        || dir.join("ripr.bat").is_file()
        || dir.join("ripr.cmd").is_file()
}

fn absolute_display(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let text = absolute.to_string_lossy().replace('/', "\\");
    text.trim_end_matches('\\').to_string()
}

/// The plugin package digest binds the exact installed bytes: the manifest,
/// skills, and every other tracked file under the plugin directory, in sorted
/// path order.
fn plugin_manifest_digest(plugin_dir: &Path) -> Result<String, String> {
    let files = collect_tree_files(plugin_dir)?;
    Ok(digest_files(&files))
}

fn hash_bundle_file(bundle_dir: &Path, relative: &str) -> Result<FileRef, String> {
    let mut file = hash_file(&bundle_dir.join(relative))?;
    file.path = relative.to_string();
    Ok(file)
}

// ---------------------------------------------------------------------------
// Bounded MCP probe
// ---------------------------------------------------------------------------

struct ProbeOutcome {
    spawn_error: Option<String>,
    exit_code: Option<i32>,
    terminated: Option<String>,
    stdout_lines: Vec<Vec<u8>>,
    stderr_bytes: Vec<u8>,
    got_initialize_reply: bool,
    start_unix_ms: u64,
    end_unix_ms: u64,
}

impl ProbeOutcome {
    fn step_record(&self) -> StepRecord {
        let stdout_bytes = self.stdout_bytes();
        let stderr_bytes = self.stderr_bytes();
        StepRecord {
            id: "ripr-mcp-probe".to_string(),
            argv: vec!["ripr".to_string(), "mcp".to_string(), "--stdio".to_string()],
            start_unix_ms: self.start_unix_ms,
            end_unix_ms: self.end_unix_ms,
            spawn_error: self.spawn_error.clone(),
            exit_code: self.exit_code,
            terminated: self.terminated.clone(),
            stdout: FileRef {
                path: "steps/ripr-mcp-probe.stdout".to_string(),
                len: stdout_bytes.len() as u64,
                sha256: sha256_hex(&stdout_bytes),
            },
            stderr: FileRef {
                path: "steps/ripr-mcp-probe.stderr".to_string(),
                len: stderr_bytes.len() as u64,
                sha256: sha256_hex(&stderr_bytes),
            },
        }
    }

    fn stdout_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        for line in &self.stdout_lines {
            bytes.extend_from_slice(line);
        }
        bytes
    }

    fn stderr_bytes(&self) -> Vec<u8> {
        self.stderr_bytes.clone()
    }

    fn combined_output(&self) -> Vec<u8> {
        let mut bytes = self.stdout_bytes();
        bytes.extend_from_slice(&self.stderr_bytes);
        bytes
    }
}

/// Is this stdout line an MCP initialize reply for the probe request? Any
/// well-formed reply frame carrying the request id or a result
/// `protocolVersion` proves the server speaks MCP.
fn is_initialize_reply(line: &[u8]) -> bool {
    serde_json::from_slice::<Value>(line)
        .map(|value| {
            value.pointer("/result/protocolVersion").is_some()
                || value.get("id").and_then(Value::as_str) == Some("setup-probe")
        })
        .unwrap_or(false)
}

/// Resolve `ripr` against exactly the given directories, in order. Explicit
/// resolution keeps the trial deterministic: the OS executable search also
/// consults the application directory and the current directory (before PATH
/// on Windows), which would leak host binaries into a controlled trial.
fn resolve_ripr_on_path(dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        for name in ["ripr.exe", "ripr"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Bounded `ripr mcp --stdio` initialize handshake. The probe sends one
/// initialize request and waits up to the probe budget for a reply frame; a
/// binary that exits with CLI usage text (the released pre-MCP shape) or never
/// answers is classified from the retained bytes, never retried into
/// compliance. ripr is a single process, so a timeout kill reaps the whole
/// tree and both pipes close.
fn probe_ripr_mcp(target: &Path, path_env: &std::ffi::OsStr) -> Result<ProbeOutcome, String> {
    let timeout = env_timeout_secs(PROBE_TIMEOUT_ENV, DEFAULT_PROBE_TIMEOUT_SECS)?;
    let start = unix_ms()?;
    let spawn = Command::new(target)
        .args(["mcp", "--stdio"])
        .env("PATH", path_env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match spawn {
        Ok(child) => child,
        Err(error) => {
            return Ok(ProbeOutcome {
                spawn_error: Some(error.to_string()),
                exit_code: None,
                terminated: None,
                stdout_lines: Vec::new(),
                stderr_bytes: Vec::new(),
                got_initialize_reply: false,
                start_unix_ms: start,
                end_unix_ms: unix_ms()?,
            });
        }
    };
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "probe child exposed no stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "probe child exposed no stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "probe child exposed no stderr".to_string())?;
    let (sender, inbox) = std::sync::mpsc::channel::<Vec<u8>>();
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut line = Vec::new();
            match reader.read_until(b'\n', &mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    let stderr_reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut BufReader::new(stderr), &mut bytes);
        bytes
    });
    let request = json!({
        "jsonrpc": "2.0",
        "id": "setup-probe",
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-11-25",
            "capabilities": {},
            "clientInfo": { "name": "ripr-muse-setup-probe", "version": "1" }
        }
    });
    let mut frame =
        serde_json::to_vec(&request).map_err(|err| format!("encode the probe request: {err}"))?;
    frame.push(b'\n');

    let mut got_initialize_reply = false;
    let mut terminated = None;
    match stdin.write_all(&frame) {
        Ok(()) => {
            drop(stdin);
            let deadline = std::time::Instant::now() + timeout;
            let mut lines: Vec<Vec<u8>> = Vec::new();
            loop {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    terminated = Some("timeout".to_string());
                    break;
                }
                if let Ok(status) = child.try_wait()
                    && status.is_some()
                {
                    // The child exited before any deadline; drain whatever the
                    // reader collected below.
                    break;
                }
                match inbox.recv_timeout(Duration::from_millis(50)) {
                    Ok(line) => {
                        if is_initialize_reply(&line) {
                            got_initialize_reply = true;
                            let _ = child.kill();
                            let _ = child.wait();
                            terminated = Some("handshake complete".to_string());
                            lines.push(line);
                            break;
                        }
                        lines.push(line);
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            // Collect any trailing bytes the reader buffered after the loop.
            for line in inbox.try_iter() {
                if !got_initialize_reply && is_initialize_reply(&line) {
                    got_initialize_reply = true;
                }
                lines.push(line);
            }
            let _ = reader.join();
            let stdout_lines = lines;
            let stderr_bytes = stderr_reader
                .join()
                .map_err(|error| format!("probe stderr reader panicked: {error:?}"))?;
            let exit_code = child.wait().ok().and_then(|status| status.code());
            Ok(ProbeOutcome {
                spawn_error: None,
                exit_code,
                terminated,
                stdout_lines,
                stderr_bytes,
                got_initialize_reply,
                start_unix_ms: start,
                end_unix_ms: unix_ms()?,
            })
        }
        Err(error) => {
            drop(stdin);
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            let stderr_bytes = stderr_reader
                .join()
                .map_err(|error| format!("probe stderr reader panicked: {error:?}"))?;
            Ok(ProbeOutcome {
                spawn_error: None,
                exit_code: None,
                terminated: Some(format!("stdin write failed: {error}")),
                stdout_lines: Vec::new(),
                stderr_bytes,
                got_initialize_reply: false,
                start_unix_ms: start,
                end_unix_ms: unix_ms()?,
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Environment facts for the record
// ---------------------------------------------------------------------------

fn git_fact(args: &[&str], suite_dir: &Path) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(suite_dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

fn environment_block(suite_dir: &Path, ripr_binary: &str) -> Value {
    let head = git_fact(&["rev-parse", "HEAD"], suite_dir).unwrap_or_else(|| "unknown".to_string());
    let working_tree = match git_fact(&["status", "--porcelain"], suite_dir) {
        None => "unknown",
        Some(status) if status.is_empty() => "clean",
        Some(_) => "dirty",
    };
    json!({
        "os": std::env::consts::OS,
        "git_head": head,
        "working_tree": working_tree,
        "ripr_binary": ripr_binary,
    })
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

/// Read the manifest and refuse any bundle whose retained bytes fail digest
/// verification. This is the evaluator's first line of defense: nothing the
/// producer claimed is trusted before the bytes check out.
fn load_verified_bundle(bundle_dir: &Path) -> Result<BundleManifest, String> {
    let manifest_path = bundle_dir.join(MANIFEST_FILE);
    let text = fs::read_to_string(&manifest_path).map_err(|err| {
        format!(
            "read {}: {err}; {} is not a trial bundle",
            manifest_path.display(),
            bundle_dir.display()
        )
    })?;
    let manifest: BundleManifest = serde_json::from_str(&text)
        .map_err(|err| format!("parse {}: {err}", manifest_path.display()))?;
    if manifest.schema_version != BUNDLE_SCHEMA {
        return Err(format!(
            "bundle schema_version {:?} is not {BUNDLE_SCHEMA}; refusing to evaluate",
            manifest.schema_version
        ));
    }
    verify_bundle_bytes(&manifest, bundle_dir)?;
    Ok(manifest)
}

fn step_stdout(manifest: &BundleManifest, bundle_dir: &Path, step_id: &str) -> Vec<u8> {
    manifest
        .steps
        .iter()
        .find(|step| step.id == step_id)
        .map(|step| fs::read(bundle_dir.join(&step.stdout.path)).unwrap_or_default())
        .unwrap_or_default()
}

/// Re-derive every observation from the retained bundle bytes. This mirrors
/// the producer's classification exactly, but reads only the bundle: the
/// producer's claim field is advisory and is never consulted here.
fn rederive_facts(manifest: &BundleManifest, bundle_dir: &Path) -> SetupFacts {
    let version_step = manifest.steps.iter().find(|step| step.id == "muse-version");
    let muse_available =
        version_step.is_some_and(|step| step.spawn_error.is_none() && step.exit_code == Some(0));
    let muse_version = if muse_available {
        version_label(&step_stdout(manifest, bundle_dir, "muse-version"))
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    let ripr_version_step = manifest.steps.iter().find(|step| step.id == "ripr-version");
    let ripr_version = if ripr_version_step
        .is_some_and(|step| step.spawn_error.is_none() && step.exit_code == Some(0))
    {
        version_label(&step_stdout(manifest, bundle_dir, "ripr-version"))
            .unwrap_or_else(|| "unknown".to_string())
    } else {
        "unknown".to_string()
    };
    let probe_step = manifest
        .steps
        .iter()
        .find(|step| step.id == "ripr-mcp-probe");
    let probe_output = probe_step
        .map(|step| {
            let mut bytes = fs::read(bundle_dir.join(&step.stdout.path)).unwrap_or_default();
            bytes.extend(fs::read(bundle_dir.join(&step.stderr.path)).unwrap_or_default());
            bytes
        })
        .unwrap_or_default();
    let ripr_class = probe_step.map_or(RiprBinaryClass::Unknown, |step| {
        classify_ripr_probe(
            step.spawn_error.as_deref(),
            step.exit_code,
            step.terminated.as_deref(),
            &probe_output,
            probe_output
                .split(|byte| *byte == b'\n')
                .any(is_initialize_reply),
        )
    });
    let approval = classify_approval(&step_stdout(manifest, bundle_dir, "plugins-inspect"));
    let (setup_outcome, judgment) = derive_setup_outcome(muse_available, ripr_class, approval);
    SetupFacts {
        muse_version,
        ripr_version,
        ripr_class,
        setup_outcome,
        judgment,
    }
}

struct SetupFacts {
    muse_version: String,
    ripr_version: String,
    ripr_class: RiprBinaryClass,
    setup_outcome: &'static str,
    judgment: String,
}

fn load_scenario_for_manifest(manifest: &BundleManifest) -> Result<Scenario, String> {
    load_scenario(Path::new(&manifest.suite_dir), &manifest.scenario_id)
}

/// Evaluate one trial bundle: verify every retained byte, re-derive the typed
/// observations from those bytes, build the `muse-plugin-eval-v1` record, pin
/// its wire shape, and write it into the bundle.
fn evaluate_bundle(bundle_dir: &Path) -> Result<Value, String> {
    let manifest = load_verified_bundle(bundle_dir)?;
    let scenario = load_scenario_for_manifest(&manifest)?;
    let facts = rederive_facts(&manifest, bundle_dir);
    let record = build_record(&manifest, &scenario, &facts, bundle_dir)?;
    structural_validate_record(&record)?;
    cross_check_record(&record, &manifest, bundle_dir)?;
    let text = serde_json::to_string_pretty(&record).map_err(|err| err.to_string())?;
    fs::write(bundle_dir.join(RECORD_FILE), format!("{text}\n"))
        .map_err(|err| format!("write the eval record: {err}"))?;
    Ok(record)
}

/// Build the record. Unmeasured dimensions stay ABSENT (the schema's
/// missing-value rule) rather than zero: no model ran, so `model` is null and
/// `efficiency` is omitted entirely; no tests ran, so `execution` is null and
/// the executed dimension reads `not_applicable`.
fn build_record(
    manifest: &BundleManifest,
    scenario: &Scenario,
    facts: &SetupFacts,
    bundle_dir: &Path,
) -> Result<Value, String> {
    let setup_failed = facts.setup_outcome != SETUP_OUTCOME_READY;
    let recorded_ripr = facts.ripr_class.recorded(setup_failed);
    let combined_claim = if setup_failed {
        "refused"
    } else {
        "not_applicable"
    };
    let judgment = if manifest.install_executed {
        facts.judgment.clone()
    } else {
        format!(
            "{} (read-only trial: `plugins install` was not executed, so the host plugin store was not mutated)",
            facts.judgment
        )
    };
    let mut output_digests: Vec<String> = manifest
        .files
        .iter()
        .map(|file| format!("{}:sha256:{}", file.path, file.sha256))
        .collect();
    output_digests.push(format!(
        "{}:sha256:{}",
        MANIFEST_FILE,
        sha256_hex(&fs::read(bundle_dir.join(MANIFEST_FILE)).unwrap_or_default(),)
    ));
    output_digests.sort();
    // A setup-only trial runs no analysis and no tests: both dimensions read
    // `not_applicable` on every path, ready or failed.
    let static_executed = "not_applicable";
    Ok(json!({
        "schema_version": RECORD_SCHEMA,
        "identity": {
            "scenario_id": scenario.id,
            "fixture_digest": manifest.fixture_digest,
            "ripr_commit": null,
            "ripr_version": facts.ripr_version,
            "plugin_version": plugin_version_of(manifest),
            "muse_version": facts.muse_version,
        },
        "environment": environment_block(Path::new(&manifest.suite_dir), recorded_ripr),
        "model": null,
        "task": {
            "initial_request": scenario.summary,
            "expected_oracle": format!("benchmarks/muse-plugin/{}", scenario.expected),
        },
        "mcp": {
            "tools_discovered": [],
            "setup_outcome": facts.setup_outcome,
        },
        "repair": null,
        "execution": null,
        "verdicts": {
            "static_verdict": static_executed,
            "executed_verdict": static_executed,
            "combined_claim": combined_claim,
            "unsupported_claims": [],
            "unintended_edits": [],
        },
        "provenance": {
            "transcript_path": null,
            "output_digests": output_digests,
            "evaluator_version": EVALUATOR_VERSION,
            "judgment": judgment,
        },
    }))
}

fn plugin_version_of(manifest: &BundleManifest) -> String {
    fs::read(Path::new(&manifest.plugin_dir).join(".muse-plugin/plugin.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| {
            value
                .get("version")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Structural record validation (the emitted muse-plugin-eval-v1 wire shape)
// ---------------------------------------------------------------------------

/// Pin the emitted record against the canonical `muse-plugin-eval-v1` schema
/// shape from #13165 for the subset this harness emits: required keys, enum
/// memberships, and closed key sets (the schema sets `additionalProperties:
/// false` at every level). The schema bytes themselves are not vendored on
/// this branch, so this validator is the executable contract the tests prove
/// both ways.
fn record_object<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<&'a serde_json::Map<String, Value>, String> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{context} is not a JSON object"))
}

fn record_require_only(
    map: &serde_json::Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), String> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!(
                "{context} carries key {key:?} outside the schema's closed key set"
            ));
        }
    }
    Ok(())
}

fn record_require(
    map: &serde_json::Map<String, Value>,
    keys: &[&str],
    context: &str,
) -> Result<(), String> {
    for key in keys {
        if !map.contains_key(*key) {
            return Err(format!("{context} is missing required key {key:?}"));
        }
    }
    Ok(())
}

fn record_nonempty_string(value: Option<&Value>, context: &str) -> Result<(), String> {
    match value.and_then(Value::as_str) {
        Some(text) if !text.is_empty() => Ok(()),
        _ => Err(format!("{context} must be a non-empty string")),
    }
}

fn record_enum_string(
    value: Option<&Value>,
    allowed: &[&str],
    context: &str,
) -> Result<(), String> {
    match value.and_then(Value::as_str) {
        Some(text) if allowed.contains(&text) => Ok(()),
        other => Err(format!(
            "{context} must be one of {allowed:?}, found {other:?}"
        )),
    }
}

fn structural_validate_record(record: &Value) -> Result<(), String> {
    let require_only = record_require_only;
    let require = record_require;
    let nonempty_string = record_nonempty_string;
    let enum_string = record_enum_string;

    let root = record_object(Some(record), "the record")?;
    require_only(
        root,
        &[
            "schema_version",
            "identity",
            "environment",
            "model",
            "task",
            "mcp",
            "repair",
            "execution",
            "verdicts",
            "efficiency",
            "provenance",
            "observations",
        ],
        "the record root",
    )?;
    require(
        root,
        &[
            "schema_version",
            "identity",
            "environment",
            "task",
            "mcp",
            "verdicts",
        ],
        "the record root",
    )?;
    if root.get("schema_version").and_then(Value::as_str) != Some(RECORD_SCHEMA) {
        return Err(format!(
            "schema_version must be {RECORD_SCHEMA:?}, found {:?}",
            root.get("schema_version")
        ));
    }

    let identity = record_object(root.get("identity"), "identity")?;
    require_only(
        identity,
        &[
            "scenario_id",
            "fixture_digest",
            "ripr_commit",
            "ripr_version",
            "plugin_version",
            "muse_version",
        ],
        "identity",
    )?;
    require(
        identity,
        &[
            "scenario_id",
            "fixture_digest",
            "ripr_version",
            "plugin_version",
            "muse_version",
        ],
        "identity",
    )?;
    nonempty_string(identity.get("scenario_id"), "identity.scenario_id")?;
    nonempty_string(identity.get("fixture_digest"), "identity.fixture_digest")?;
    nonempty_string(identity.get("ripr_version"), "identity.ripr_version")?;
    nonempty_string(identity.get("plugin_version"), "identity.plugin_version")?;
    nonempty_string(identity.get("muse_version"), "identity.muse_version")?;
    match identity.get("ripr_commit") {
        None | Some(Value::Null) => {}
        Some(Value::String(_)) => {}
        other => {
            return Err(format!(
                "identity.ripr_commit must be null or a string, found {other:?}"
            ));
        }
    }

    let environment = record_object(root.get("environment"), "environment")?;
    require_only(
        environment,
        &[
            "os",
            "runner_class",
            "git_head",
            "base",
            "working_tree",
            "ripr_binary",
        ],
        "environment",
    )?;
    require(
        environment,
        &["os", "git_head", "working_tree"],
        "environment",
    )?;
    nonempty_string(environment.get("os"), "environment.os")?;
    nonempty_string(environment.get("git_head"), "environment.git_head")?;
    enum_string(
        environment.get("working_tree"),
        &["clean", "dirty", "unknown"],
        "environment.working_tree",
    )?;
    enum_string(
        environment.get("ripr_binary"),
        &["mcp_capable", "predates_mcp", "missing", "unknown"],
        "environment.ripr_binary",
    )?;

    match root.get("model") {
        None | Some(Value::Null) => {}
        Some(model) => {
            let model = record_object(Some(model), "model")?;
            require_only(
                model,
                &["provider", "model", "configuration", "trial_id"],
                "model",
            )?;
        }
    }

    let task = record_object(root.get("task"), "task")?;
    require_only(task, &["initial_request", "expected_oracle"], "task")?;
    require(task, &["initial_request", "expected_oracle"], "task")?;
    nonempty_string(task.get("initial_request"), "task.initial_request")?;
    nonempty_string(task.get("expected_oracle"), "task.expected_oracle")?;

    let mcp = record_object(root.get("mcp"), "mcp")?;
    require_only(
        mcp,
        &[
            "tools_discovered",
            "setup_outcome",
            "calls",
            "refusals",
            "snapshot_ids",
            "fresh_for_working_tree",
        ],
        "mcp",
    )?;
    require(mcp, &["tools_discovered", "setup_outcome"], "mcp")?;
    let tools = mcp
        .get("tools_discovered")
        .and_then(Value::as_array)
        .ok_or_else(|| "mcp.tools_discovered must be an array".to_string())?;
    if tools.len() > 64 {
        return Err("mcp.tools_discovered exceeds the 64-item evidence bound".to_string());
    }
    if tools.iter().any(|tool| tool.as_str().is_none()) {
        return Err("mcp.tools_discovered must contain only strings".to_string());
    }
    enum_string(
        mcp.get("setup_outcome"),
        &[
            "ready",
            "approval_required",
            "server_unavailable",
            "no_scope",
            "base_required",
            "limited",
            "not_run",
        ],
        "mcp.setup_outcome",
    )?;

    for optional in ["repair", "execution"] {
        match root.get(optional) {
            None | Some(Value::Null) => {}
            Some(value) => {
                let value = record_object(Some(value), optional)?;
                require_only(
                    value,
                    if optional == "repair" {
                        &[
                            "canonical_id",
                            "seam_id",
                            "attempt_id",
                            "edit_paths",
                            "before_class",
                            "after_class",
                        ]
                    } else {
                        &["commands"]
                    },
                    optional,
                )?;
            }
        }
    }

    let verdicts = record_object(root.get("verdicts"), "verdicts")?;
    require_only(
        verdicts,
        &[
            "static_verdict",
            "executed_verdict",
            "combined_claim",
            "unsupported_claims",
            "unintended_edits",
        ],
        "verdicts",
    )?;
    require(
        verdicts,
        &["static_verdict", "executed_verdict", "combined_claim"],
        "verdicts",
    )?;
    nonempty_string(verdicts.get("static_verdict"), "verdicts.static_verdict")?;
    nonempty_string(
        verdicts.get("executed_verdict"),
        "verdicts.executed_verdict",
    )?;
    enum_string(
        verdicts.get("combined_claim"),
        &[
            "completed_repair",
            "not_completed",
            "refused",
            "not_applicable",
        ],
        "verdicts.combined_claim",
    )?;
    for key in ["unsupported_claims", "unintended_edits"] {
        match verdicts.get(key) {
            None => {}
            Some(value) => {
                let items = value
                    .as_array()
                    .ok_or_else(|| format!("verdicts.{key} must be an array"))?;
                if items.iter().any(|item| item.as_str().is_none()) {
                    return Err(format!("verdicts.{key} must contain only strings"));
                }
            }
        }
    }

    match root.get("efficiency") {
        None | Some(Value::Null) => {}
        Some(efficiency) => {
            let efficiency = record_object(Some(efficiency), "efficiency")?;
            require_only(
                efficiency,
                &[
                    "wall_time",
                    "time_to_first_useful_finding",
                    "model_calls",
                    "tool_calls",
                    "input_tokens",
                    "output_tokens",
                    "reported_cost",
                    "peak_memory",
                ],
                "efficiency",
            )?;
        }
    }

    match root.get("provenance") {
        None => {}
        Some(provenance) => {
            let provenance = record_object(Some(provenance), "provenance")?;
            require_only(
                provenance,
                &[
                    "transcript_path",
                    "output_digests",
                    "evaluator_version",
                    "judgment",
                ],
                "provenance",
            )?;
            match provenance.get("output_digests") {
                None => {}
                Some(value) => {
                    let items = value
                        .as_array()
                        .ok_or_else(|| "provenance.output_digests must be an array".to_string())?;
                    if items.iter().any(|item| item.as_str().is_none()) {
                        return Err(
                            "provenance.output_digests must contain only strings".to_string()
                        );
                    }
                }
            }
        }
    }

    if let Some(observations) = root.get("observations") {
        let observations = record_object(Some(observations), "observations")?;
        require_only(
            observations,
            &[
                "run_id",
                "trial_id",
                "recorded_unix_ms",
                "subject",
                "scenario",
                "evidence",
                "setup",
                "static",
                "durable",
                "executions",
                "usage",
            ],
            "observations",
        )?;
        require(
            observations,
            &[
                "run_id",
                "trial_id",
                "recorded_unix_ms",
                "subject",
                "scenario",
                "evidence",
            ],
            "observations",
        )?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Cross-check: refuse dishonest credit over verified bytes
// ---------------------------------------------------------------------------

/// Refuse any record whose claims outrun the verified bundle bytes:
///
/// - the scenario identity and fixture digest must match the manifest;
/// - the typed setup outcome and `environment.ripr_binary` must equal the
///   values re-derived from the retained bytes (a schema-valid record claiming
///   `ready`/loaded tools from a failed-setup bundle is refused);
/// - a failed setup forbids `completed_repair`, non-empty
///   `tools_discovered`, and any `mcp_capable` environment claim;
/// - with zero executed-test evidence in the bundle, any executed credit
///   beyond `not_applicable`/`unknown`/`not_run` is refused.
fn cross_check_record(
    record: &Value,
    manifest: &BundleManifest,
    bundle_dir: &Path,
) -> Result<(), String> {
    let facts = rederive_facts(manifest, bundle_dir);
    let setup_failed = facts.setup_outcome != SETUP_OUTCOME_READY;
    let recorded_ripr = facts.ripr_class.recorded(setup_failed);

    let scenario_id = record
        .pointer("/identity/scenario_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if scenario_id != manifest.scenario_id {
        return Err(format!(
            "record claims scenario {scenario_id:?} but the bundle holds {:?}; refusing",
            manifest.scenario_id
        ));
    }
    let fixture_digest = record
        .pointer("/identity/fixture_digest")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if fixture_digest != manifest.fixture_digest {
        return Err(
            "record fixture_digest does not match the materialized fixture bytes; refusing"
                .to_string(),
        );
    }
    let claim_outcome = record
        .pointer("/mcp/setup_outcome")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if claim_outcome != facts.setup_outcome {
        return Err(format!(
            "record claims setup_outcome {claim_outcome:?} but the retained bytes derive {:?} ({}); refusing",
            facts.setup_outcome, facts.judgment
        ));
    }
    let claim_ripr = record
        .pointer("/environment/ripr_binary")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if claim_ripr != recorded_ripr {
        return Err(format!(
            "record claims environment.ripr_binary {claim_ripr:?} but the retained bytes derive {recorded_ripr:?}; refusing"
        ));
    }
    if setup_failed {
        let tools = record
            .pointer("/mcp/tools_discovered")
            .and_then(Value::as_array)
            .map(|tools| tools.len())
            .unwrap_or(0);
        if claim_outcome == SETUP_OUTCOME_READY {
            return Err(
                "record claims setup ready from a bundle whose setup failed; refusing fabricated success"
                    .to_string(),
            );
        }
        if tools > 0 {
            return Err(format!(
                "record claims {tools} discovered tools from a failed setup bundle; exit 0 is not tools-loaded; refusing"
            ));
        }
        let combined = record
            .pointer("/verdicts/combined_claim")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if combined == "completed_repair" {
            return Err(
                "record claims a completed repair from a failed-setup bundle; refusing".to_string(),
            );
        }
    }
    // Executed credit requires executed evidence. This slice retains none:
    // every bundle carries zero executed-test artifacts by construction.
    let executed = record
        .pointer("/verdicts/executed_verdict")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !EXECUTED_ABSENT_VALUES.contains(&executed) {
        return Err(format!(
            "record claims executed_verdict {executed:?} but the bundle retains zero executed-test evidence; refusing executed credit"
        ));
    }
    let static_verdict = record
        .pointer("/verdicts/static_verdict")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !EXECUTED_ABSENT_VALUES.contains(&static_verdict) {
        return Err(format!(
            "record claims static_verdict {static_verdict:?} but the bundle retains no static analysis evidence; refusing unearned credit"
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test scope uses expect for fail-fast assertions, receipted per policy/no-panic-allowlist.toml convention"
)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_root(label: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-muse-setup-{label}-{}-{stamp}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    /// A minimal but faithful suite copy: the real scenario table entry, one
    /// fixture file, and the independent expected verdict.
    fn write_suite(root: &Path) -> PathBuf {
        let suite = root.join("benchmarks/muse-plugin");
        fs::create_dir_all(suite.join("fixtures/rust-boundary-gap/src"))
            .expect("create fixture dirs");
        fs::write(
            suite.join("fixtures/rust-boundary-gap/src/lib.rs"),
            "pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {\n    if amount >= discount_threshold { amount - 10 } else { amount }\n}\n",
        )
        .expect("write fixture");
        fs::write(
            suite.join("scenarios.toml"),
            "[[scenario]]\nid = \"missing-or-unapproved-server\"\nfixture = \"fixtures/rust-boundary-gap\"\nkind = \"setup\"\nexpected = \"expected/missing-or-unapproved-server.json\"\nsummary = \"Plugin installed but the MCP server is unapproved, absent, or served by a pre-MCP binary.\"\n",
        )
        .expect("write scenarios");
        fs::create_dir_all(suite.join("expected")).expect("create expected dir");
        fs::write(
            suite.join("expected/missing-or-unapproved-server.json"),
            "{\n  \"scenario_id\": \"missing-or-unapproved-server\",\n  \"setup_outcome\": \"approval_required | server_unavailable\",\n  \"required_edit\": \"none\",\n  \"executed_oracle\": \"not_applicable\",\n  \"combined_claim\": \"refused\",\n  \"notes\": \"With a 0.10.x binary the diagnosis must name the MCP version floor.\"\n}\n",
        )
        .expect("write expected");
        suite
    }

    fn write_plugin(root: &Path) -> PathBuf {
        let plugin = root.join("plugins/muse/ripr");
        fs::create_dir_all(plugin.join(".muse-plugin")).expect("create plugin dirs");
        fs::write(
            plugin.join(".muse-plugin/plugin.json"),
            "{\n  \"schemaVersion\": 1,\n  \"name\": \"ripr\",\n  \"version\": \"0.1.0\",\n  \"capabilities\": { \"mcpServers\": [ { \"id\": \"core\", \"transport\": \"stdio\", \"command\": [\"ripr\", \"mcp\", \"--stdio\"] } ] }\n}\n",
        )
        .expect("write plugin manifest");
        plugin
    }

    /// A hermetic Muse entry point. The stub answers every documented CLI
    /// surface with canned JSON so the producer runs end to end with no real
    /// Muse installed. `--muse-not-found` is exercised by passing a path that
    /// does not exist instead.
    fn write_stub_muse(root: &Path) -> PathBuf {
        let stub = root.join("stub-muse.cmd");
        let body = r#"@echo off
setlocal
if /i "%1"=="--version" (echo Muse Code 9.9.9-test ^(9.9.9-test^)& exit /b 0)
if /i "%1 %2"=="plugins validate" (echo {"ok": true, "command": "validate"}& exit /b 0)
if /i "%1 %2"=="plugins install" (echo {"ok": true, "command": "install"}& exit /b 0)
if /i "%1 %2"=="plugins inspect" (echo {"plugin": "ripr", "capabilities": [{"id": "plugin:ripr:mcp_server:core", "trust": "review_needed"}]}& exit /b 0)
echo {"error": "unsupported stub invocation"}& exit /b 2
"#;
        fs::write(&stub, body).expect("write stub muse");
        stub
    }

    fn base_options(root: &Path, suite: &Path, plugin: &Path, bundle: PathBuf) -> Options {
        Options {
            scenario: SETUP_SCENARIO.to_string(),
            setup: SetupKind::MissingRipr,
            muse: Some(write_stub_muse(root)),
            suite_dir: suite.to_path_buf(),
            plugin_dir: plugin.to_path_buf(),
            expose_ripr: None,
            bundle_dir: Some(bundle),
            evaluate_only: None,
            read_only: false,
        }
    }

    #[test]
    fn missing_ripr_setup_emits_the_typed_record_and_schema_shape() {
        let root = temp_root("positive");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let bundle = root.join("bundle-positive");
        let options = base_options(&root, &suite, &plugin, bundle.clone());

        let produced = produce_trial(&options).expect("produce the typed setup trial");
        assert_eq!(produced, bundle);
        let record = evaluate_bundle(&produced).expect("evaluate the retained bytes");

        // The positive control: a correctly-typed missing setup trial emits
        // exactly the schema subset the shared consumer accepts.
        structural_validate_record(&record).expect("record validates against the wire shape");
        assert_eq!(
            record.pointer("/mcp/setup_outcome").and_then(Value::as_str),
            Some("server_unavailable")
        );
        assert_eq!(
            record
                .pointer("/environment/ripr_binary")
                .and_then(Value::as_str),
            Some("missing")
        );
        assert_eq!(
            record
                .pointer("/verdicts/combined_claim")
                .and_then(Value::as_str),
            Some("refused")
        );
        assert_eq!(
            record
                .pointer("/verdicts/executed_verdict")
                .and_then(Value::as_str),
            Some("not_applicable")
        );
        assert_eq!(
            record
                .pointer("/identity/scenario_id")
                .and_then(Value::as_str),
            Some("missing-or-unapproved-server")
        );
        assert_eq!(
            record
                .pointer("/identity/muse_version")
                .and_then(Value::as_str),
            Some("Muse Code 9.9.9-test (9.9.9-test)")
        );
        assert!(
            record
                .pointer("/provenance/output_digests")
                .and_then(Value::as_array)
                .is_some_and(|digests| digests.len() >= 6),
            "every retained artifact carries a digest: {record}"
        );
        // The evaluator refuses to claim capability it did not observe.
        assert_ne!(
            record
                .pointer("/environment/ripr_binary")
                .and_then(Value::as_str),
            Some("mcp_capable")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn producer_never_writes_expected_answers_into_the_bundle() {
        let root = temp_root("isolation");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let bundle = root.join("bundle-isolation");
        let options = base_options(&root, &suite, &plugin, bundle.clone());
        produce_trial(&options).expect("produce the trial");
        let files = collect_tree_files(&bundle).expect("list the bundle");
        assert!(
            files.iter().all(|file| !file.path.starts_with("expected/")),
            "the trial workspace must not see the expected answers: {files:?}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Negative control (a): a schema-valid record claiming ready/tools from
    /// a failed-setup bundle must be refused by the cross-check.
    #[test]
    fn cross_check_refutes_success_claimed_from_a_failed_setup_bundle() {
        let root = temp_root("refute-success");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let bundle = root.join("bundle-refute");
        let options = base_options(&root, &suite, &plugin, bundle.clone());
        let produced = produce_trial(&options).expect("produce the trial");
        let record = evaluate_bundle(&produced).expect("evaluate the bundle");
        assert_eq!(
            record.pointer("/mcp/setup_outcome").and_then(Value::as_str),
            Some("server_unavailable")
        );

        // Hand the cross-check a doctored but schema-valid record.
        let mut doctored = record.clone();
        *doctored.pointer_mut("/mcp/setup_outcome").expect("pointer") =
            Value::String("ready".to_string());
        *doctored
            .pointer_mut("/verdicts/combined_claim")
            .expect("pointer") = Value::String("completed_repair".to_string());
        *doctored
            .pointer_mut("/mcp/tools_discovered")
            .expect("pointer") = json!(["ripr_workspace_status"]);
        *doctored
            .pointer_mut("/environment/ripr_binary")
            .expect("pointer") = Value::String("mcp_capable".to_string());
        structural_validate_record(&doctored).expect("the doctored record stays schema-valid");
        let manifest = load_verified_bundle(&produced).expect("verify the bundle");
        let refusal = cross_check_record(&doctored, &manifest, &produced)
            .expect_err("dishonest credit must be refused");
        assert!(
            refusal.contains("setup_outcome") || refusal.contains("tools"),
            "refusal names the false claim: {refusal}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Negative control (b): flipping one byte in a retained artifact fails
    /// digest verification before any verdict is derived.
    #[test]
    fn flipped_evidence_byte_is_refused_by_digest_verification() {
        let root = temp_root("flip-byte");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let bundle = root.join("bundle-flip");
        let options = base_options(&root, &suite, &plugin, bundle.clone());
        let produced = produce_trial(&options).expect("produce the trial");

        let target = produced.join("steps/plugins-inspect.stdout");
        let mut bytes = fs::read(&target).expect("read retained evidence");
        let index = bytes
            .iter()
            .position(|byte| *byte == b'r')
            .expect("find a byte");
        bytes[index] = b'R';
        fs::write(&target, bytes).expect("flip one byte");

        let refusal = evaluate_bundle(&produced).expect_err("tampered bytes must be refused");
        assert!(
            refusal.contains("digest verification"),
            "refusal names digest verification: {refusal}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// Negative control (c): substituting an artifact from a different trial
    /// (same length, different bytes) is refused by digest verification.
    #[test]
    fn substituted_artifact_from_another_trial_is_refused() {
        let root = temp_root("substitute");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let first = root.join("bundle-first");
        let second = root.join("bundle-second");
        produce_trial(&base_options(&root, &suite, &plugin, first.clone()))
            .expect("produce the first trial");
        produce_trial(&base_options(&root, &suite, &plugin, second.clone()))
            .expect("produce the second trial");

        // Same-length, different-content substitution from another trial.
        let donor = fs::read(second.join("steps/muse-version.stdout")).expect("read donor");
        let mut forged = donor.clone();
        let index = forged
            .iter()
            .position(|byte| *byte == b'9')
            .expect("find a digit");
        forged[index] = b'8';
        assert_eq!(forged.len(), donor.len(), "substitution keeps the length");
        fs::write(first.join("steps/muse-version.stdout"), forged).expect("substitute evidence");

        let refusal = evaluate_bundle(&first).expect_err("substituted bytes must be refused");
        assert!(
            refusal.contains("digest verification"),
            "refusal names digest verification: {refusal}"
        );
        // The donor bundle itself still verifies clean.
        evaluate_bundle(&second).expect("the untouched donor trial still evaluates");
        let _ = fs::remove_dir_all(&root);
    }

    /// Negative control (d): a bundle with zero executed tests cannot support
    /// executed credit; any executed verdict beyond the absent-vocabulary set
    /// is refused.
    #[test]
    fn executed_credit_with_zero_executed_evidence_is_refused() {
        let root = temp_root("executed-credit");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let bundle = root.join("bundle-executed");
        let produced = produce_trial(&base_options(&root, &suite, &plugin, bundle))
            .expect("produce the trial");
        let record = evaluate_bundle(&produced).expect("evaluate the bundle");

        let mut doctored = record.clone();
        *doctored
            .pointer_mut("/verdicts/executed_verdict")
            .expect("pointer") = Value::String("passed".to_string());
        structural_validate_record(&doctored).expect("doctored record stays schema-valid");
        let manifest = load_verified_bundle(&produced).expect("verify the bundle");
        let refusal = cross_check_record(&doctored, &manifest, &produced)
            .expect_err("executed credit without evidence must be refused");
        assert!(
            refusal.contains("executed credit"),
            "refusal names the executed-credit rule: {refusal}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_muse_entry_point_is_a_typed_outcome_not_a_panic() {
        let root = temp_root("muse-missing");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let bundle = root.join("bundle-no-muse");
        let mut options = base_options(&root, &suite, &plugin, bundle);
        options.muse = Some(root.join("does-not-exist-muse.cmd"));
        let produced = produce_trial(&options).expect("an absent Muse is a typed observation");
        let record = evaluate_bundle(&produced).expect("evaluate the typed outcome");
        assert_eq!(
            record.pointer("/mcp/setup_outcome").and_then(Value::as_str),
            Some("server_unavailable")
        );
        assert_eq!(
            record
                .pointer("/identity/muse_version")
                .and_then(Value::as_str),
            Some("unknown")
        );
        assert_eq!(
            record
                .pointer("/verdicts/combined_claim")
                .and_then(Value::as_str),
            Some("refused")
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn structural_validation_rejects_closed_set_violations() {
        let root = temp_root("schema-shape");
        let suite = write_suite(&root);
        let plugin = write_plugin(&root);
        let bundle = root.join("bundle-schema");
        let produced = produce_trial(&base_options(&root, &suite, &plugin, bundle))
            .expect("produce the trial");
        let record = evaluate_bundle(&produced).expect("evaluate the bundle");

        let mut extra_key = record.clone();
        extra_key
            .as_object_mut()
            .expect("object")
            .insert("hallucinated".to_string(), Value::Bool(true));
        structural_validate_record(&extra_key)
            .expect_err("an additionalProperties violation must be refused");

        let mut bad_enum = record.clone();
        *bad_enum
            .pointer_mut("/environment/ripr_binary")
            .expect("pointer") = Value::String("definitely_there".to_string());
        structural_validate_record(&bad_enum).expect_err("an unknown enum value must be refused");

        let mut missing_key = record.clone();
        missing_key
            .as_object_mut()
            .expect("object")
            .remove("verdicts");
        structural_validate_record(&missing_key)
            .expect_err("a missing required key must be refused");
        let _ = fs::remove_dir_all(&root);
    }

    // -- Pure classification rules over captured byte shapes ----------------

    #[test]
    fn probe_classification_covers_missing_predates_capable_and_unknown() {
        let missing = classify_ripr_probe(
            Some("The system cannot find the file specified"),
            None,
            None,
            &[],
            false,
        );
        assert_eq!(missing, RiprBinaryClass::Missing);

        // The released 0.10.x shape observed on the recording host.
        let predates = classify_ripr_probe(
            None,
            Some(2),
            None,
            b"ripr: unknown command \"mcp\". Run `ripr --help`.\n",
            false,
        );
        assert_eq!(predates, RiprBinaryClass::PredatesMcp);

        let capable = classify_ripr_probe(None, None, Some("handshake complete"), b"", true);
        assert_eq!(capable, RiprBinaryClass::McpCapable);

        let timed_out = classify_ripr_probe(None, None, Some("timeout"), b"", false);
        assert_eq!(timed_out, RiprBinaryClass::Unknown);
    }

    #[test]
    fn failed_setup_never_records_an_mcp_capable_environment() {
        assert_eq!(
            RiprBinaryClass::McpCapable.recorded(true),
            "unknown",
            "a setup-failure trial must not claim mcp_capable"
        );
        assert_eq!(RiprBinaryClass::McpCapable.recorded(false), "mcp_capable");
        assert_eq!(RiprBinaryClass::Missing.recorded(true), "missing");
        assert_eq!(RiprBinaryClass::PredatesMcp.recorded(true), "predates_mcp");
    }

    #[test]
    fn setup_outcome_derivation_is_fully_deterministic() {
        let (outcome, _) =
            derive_setup_outcome(false, RiprBinaryClass::Missing, ApprovalState::Unknown);
        assert_eq!(outcome, "server_unavailable");
        let (outcome, judgment) =
            derive_setup_outcome(true, RiprBinaryClass::PredatesMcp, ApprovalState::Unknown);
        assert_eq!(outcome, "server_unavailable");
        assert!(
            judgment.contains("mcp") && judgment.contains("version floor"),
            "the pre-MCP diagnosis names the version floor: {judgment}"
        );
        let (outcome, _) = derive_setup_outcome(
            true,
            RiprBinaryClass::McpCapable,
            ApprovalState::ReviewNeeded,
        );
        assert_eq!(outcome, "approval_required");
        let (outcome, _) = derive_setup_outcome(
            true,
            RiprBinaryClass::McpCapable,
            ApprovalState::TrustedEnabled,
        );
        assert_eq!(outcome, "ready");
        let (outcome, _) = derive_setup_outcome(
            true,
            RiprBinaryClass::McpCapable,
            ApprovalState::NotInstalled,
        );
        assert_eq!(outcome, "server_unavailable");
    }

    #[test]
    fn approval_classification_reads_the_capability_identity() {
        let trusted = format!("{{\"id\": \"{CAPABILITY_ID}\", \"trust\": \"trusted_enabled\"}}");
        assert_eq!(
            classify_approval(trusted.as_bytes()),
            ApprovalState::TrustedEnabled
        );
        let review = format!("{{\"id\": \"{CAPABILITY_ID}\", \"trust\": \"review_needed\"}}");
        assert_eq!(
            classify_approval(review.as_bytes()),
            ApprovalState::ReviewNeeded
        );
        let modified = format!("{{\"id\": \"{CAPABILITY_ID}\", \"trust\": \"modified\"}}");
        assert_eq!(
            classify_approval(modified.as_bytes()),
            ApprovalState::Modified
        );
        assert_eq!(
            classify_approval(b"{\"plugin\": \"other\"}"),
            ApprovalState::NotInstalled,
            "no capability identity means the plugin did not install"
        );
    }

    #[test]
    fn version_label_extracts_the_first_line_with_a_digit() {
        assert_eq!(
            version_label(b"Muse Code 1.4.14 (1.4.14-R7309.1)\r\n"),
            Some("Muse Code 1.4.14 (1.4.14-R7309.1)".to_string())
        );
        assert_eq!(
            version_label(b"ripr 0.10.0\n"),
            Some("ripr 0.10.0".to_string())
        );
        assert_eq!(version_label(b"\n"), None);
    }

    #[test]
    fn fixture_tree_digest_is_order_independent_and_byte_exact() {
        let root = temp_root("digest");
        fs::write(root.join("a.txt"), b"alpha").expect("write a");
        fs::create_dir_all(root.join("dir")).expect("create dir");
        fs::write(root.join("dir/b.txt"), b"beta").expect("write b");
        let files = collect_tree_files(&root).expect("collect files");
        let digest = digest_files(&files);
        assert!(digest.starts_with("sha256:"));
        // Re-hashing the same bytes yields the same digest; one flipped byte
        // does not.
        let again = digest_files(&collect_tree_files(&root).expect("collect again"));
        assert_eq!(digest, again);
        fs::write(root.join("dir/b.txt"), b"betb").expect("flip b");
        let flipped = digest_files(&collect_tree_files(&root).expect("collect flipped"));
        assert_ne!(digest, flipped);
        let _ = fs::remove_dir_all(&root);
    }
}
