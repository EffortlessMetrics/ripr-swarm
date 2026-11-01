//! Installed-boundary helpers for the #3165 closure journey.
//!
//! Git fixture setup goes through [`super::common::fixture_git`]. Product
//! and cargo-test spawns share one process construction site so PATH
//! substitution cannot hide behind a second program lookup.

use super::common::fixture_git::{fixture_git_ok, fixture_git_output};
use ripr::process_owner::OwnedProcess;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[path = "printed_words.rs"]
mod printed_words;

pub(crate) const LIB_CLOSED: &str = r#"pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {
    if amount >= discount_threshold {
        amount - 10
    } else {
        amount
    }
}
"#;

pub(crate) const LIB_OPEN: &str = r#"pub fn discounted_total(amount: i32, discount_threshold: i32) -> i32 {
    if amount > discount_threshold {
        amount - 10
    } else {
        amount
    }
}
"#;

pub(crate) const BEFORE_TESTS: &str = r#"use boundary_gap_closure::discounted_total;

#[test]
fn below_threshold_has_no_discount() {
    assert_eq!(discounted_total(50, 100), 50);
}

#[test]
fn far_above_threshold_discounts() {
    assert_eq!(discounted_total(10_000, 100), 9_990);
}
"#;

pub(crate) const AFTER_TESTS: &str = r#"use boundary_gap_closure::discounted_total;

#[test]
fn below_threshold_has_no_discount() {
    assert_eq!(discounted_total(50, 100), 50);
}

#[test]
fn far_above_threshold_discounts() {
    assert_eq!(discounted_total(10_000, 100), 9_990);
}

#[test]
fn equality_boundary_discounts() {
    assert_eq!(discounted_total(100, 100), 90);
}
"#;

/// Equality arguments aimed at a sibling far-above input, not the boundary.
pub(crate) const SIBLING_TESTS: &str = r#"use boundary_gap_closure::discounted_total;

#[test]
fn below_threshold_has_no_discount() {
    assert_eq!(discounted_total(50, 100), 50);
}

#[test]
fn far_above_threshold_discounts() {
    assert_eq!(discounted_total(10_000, 100), 9_990);
}

#[test]
fn equality_boundary_discounts() {
    assert_eq!(discounted_total(101, 100), 91);
}
"#;

pub(crate) const EQUALITY_SELECTOR: &str = "equality_boundary_discounts";
pub(crate) const EQUALITY_DISCRIMINATOR: &str = "equality boundary";
const DECOY_MARKER: &str = "decoy-invoked";

pub(crate) struct JourneyRoots {
    pub(crate) fixture: PathBuf,
    pub(crate) install: PathBuf,
    pub(crate) cargo_target: PathBuf,
    pub(crate) decoy: PathBuf,
}

impl Drop for JourneyRoots {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.fixture);
        let _ = std::fs::remove_dir_all(&self.install);
        let _ = std::fs::remove_dir_all(&self.cargo_target);
        let _ = std::fs::remove_dir_all(&self.decoy);
    }
}

impl JourneyRoots {
    /// Explicit cleanup is required before an installed journey is complete.
    /// Drop remains a best-effort fallback for a failed assertion.
    pub(crate) fn cleanup(&self) -> Result<(), String> {
        for path in [
            &self.fixture,
            &self.install,
            &self.cargo_target,
            &self.decoy,
        ] {
            match std::fs::remove_dir_all(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("cleanup {} failed: {error}", path.display())),
            }
        }
        Ok(())
    }
}

pub(crate) struct InstalledCandidate {
    pub(crate) binary: PathBuf,
    pub(crate) digest: String,
    pub(crate) version: String,
    pub(crate) evidence: Option<CommandEvidence>,
}

/// These are admission inputs, not a package-producer provenance claim. The
/// package lane binds them to its installed artifact and uploads the evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstalledInputs {
    pub(crate) binary: PathBuf,
    pub(crate) digest: String,
    pub(crate) version: String,
    pub(crate) evidence_dir: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum OrdinaryMode {
    CopiedCargoBinary,
    Installed(InstalledInputs),
}

pub(crate) const ORDINARY_INPUT_KEYS: [&str; 5] = [
    "RIPR_ORDINARY_MODE",
    "RIPR_ORDINARY_INSTALLED_BINARY",
    "RIPR_ORDINARY_EXPECTED_SHA256",
    "RIPR_ORDINARY_EXPECTED_VERSION",
    "RIPR_ORDINARY_EVIDENCE_DIR",
];

/// An injected reader keeps refusal controls independent of process-wide
/// environment mutation. Any partial opt-in refuses instead of falling back.
pub(crate) fn ordinary_mode(
    read: impl FnMut(&str) -> Option<OsString>,
) -> Result<OrdinaryMode, String> {
    let values = ORDINARY_INPUT_KEYS.map(read);
    if values.iter().all(Option::is_none) {
        return Ok(OrdinaryMode::CopiedCargoBinary);
    }
    let mut inputs = Vec::new();
    for (key, value) in ORDINARY_INPUT_KEYS.iter().zip(values) {
        let value = value
            .ok_or_else(|| format!("strict installed ordinary journey requires {key}"))?
            .into_string()
            .map_err(|invalid| {
                format!(
                    "{key} must be UTF-8 for exact command evidence ({} OS bytes supplied)",
                    invalid.as_encoded_bytes().len()
                )
            })?;
        if value.is_empty() {
            return Err(format!(
                "strict installed ordinary journey requires nonempty {key}"
            ));
        }
        inputs.push(value);
    }
    if inputs[0] != "installed" {
        return Err("RIPR_ORDINARY_MODE must be installed when any ordinary input is set".into());
    }
    let binary = PathBuf::from(&inputs[1]);
    let evidence_dir = PathBuf::from(&inputs[4]);
    if !binary.is_absolute() || !evidence_dir.is_absolute() {
        return Err("installed binary and evidence directory must be absolute paths".into());
    }
    if inputs[2].len() != 64 || !inputs[2].bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("RIPR_ORDINARY_EXPECTED_SHA256 must contain exactly 64 hex digits".into());
    }
    validate_full_version(&inputs[3])?;
    Ok(OrdinaryMode::Installed(InstalledInputs {
        binary,
        digest: format!("sha256:{}", inputs[2].to_ascii_lowercase()),
        version: inputs[3].clone(),
        evidence_dir,
    }))
}

fn validate_full_version(version: &str) -> Result<(), String> {
    let Some((release, source)) = version
        .strip_prefix("ripr ")
        .and_then(|v| v.split_once(" ("))
    else {
        return Err("expected version must be the complete ripr --version line".into());
    };
    let source = source.strip_suffix(')').unwrap_or("");
    let source = source.strip_suffix("-dirty").unwrap_or(source);
    if release.is_empty()
        || release.chars().any(char::is_whitespace)
        || source.len() != 40
        || !source.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("expected ripr version must include its full 40-digit source identity".into());
    }
    Ok(())
}

/// Raw command records are kept outside the disposable fixture. A completed
/// record is written only after all controls, identity readback and cleanup.
pub(crate) struct CommandEvidence {
    root: PathBuf,
    next_command: Cell<usize>,
}

impl CommandEvidence {
    pub(crate) fn create(root: &Path) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("evidence directory must be absolute".into());
        }
        std::fs::create_dir(root)
            .map_err(|error| format!("create fresh evidence {}: {error}", root.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
            next_command: Cell::new(0),
        })
    }

    pub(crate) fn write(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let path = self.root.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("create evidence {}: {error}", path.display()))?;
        file.write_all(bytes)
            .map_err(|error| format!("write evidence {}: {error}", path.display()))
    }

    pub(crate) fn json(&self, name: &str, value: &Value) -> Result<(), String> {
        self.write(name, &json_bytes(value)?)
    }

    fn command_dir(&self) -> Result<PathBuf, String> {
        let ordinal = self.next_command.get();
        self.next_command.set(ordinal + 1);
        let directory = self.root.join(format!("command-{ordinal:03}"));
        std::fs::create_dir(&directory)
            .map_err(|error| format!("create command evidence: {error}"))?;
        Ok(directory)
    }
}

fn json_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

impl InstalledCandidate {
    pub(crate) fn record(&self, name: &str, value: &Value) -> Result<(), String> {
        if let Some(evidence) = &self.evidence {
            evidence.json(name, value)?;
        }
        Ok(())
    }

    pub(crate) fn record_source(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        if let Some(evidence) = &self.evidence {
            evidence.write(name, bytes)?;
        }
        Ok(())
    }

    fn verify_digest(&self) -> Result<(), String> {
        if self.evidence.is_some() && file_digest(&self.binary)? != self.digest {
            return Err("admitted installed executable changed before invocation".into());
        }
        Ok(())
    }

    pub(crate) fn verify_final_identity(&self) -> Result<(), String> {
        self.verify_digest()?;
        let output = run_program_recorded(
            &self.binary,
            None,
            &["--version"],
            &[],
            self.evidence.as_ref(),
            COMMAND_BUDGET,
        )?;
        require_success("final installed ripr --version", &output)?;
        if version_line(&output)? != self.version {
            return Err("installed executable full version changed during the journey".into());
        }
        self.verify_digest()?;
        Ok(())
    }
}

pub(crate) fn ordinary_candidate(roots: &JourneyRoots) -> Result<InstalledCandidate, String> {
    match ordinary_mode(|key| std::env::var_os(key))? {
        OrdinaryMode::CopiedCargoBinary => {
            eprintln!("ordinary journey input: copied_cargo_binary (workspace regression)");
            install_candidate(&roots.install)
        }
        OrdinaryMode::Installed(inputs) => {
            let parent = inputs
                .evidence_dir
                .parent()
                .ok_or_else(|| "evidence directory has no parent".to_string())?
                .canonicalize()
                .map_err(|error| format!("resolve evidence parent: {error}"))?;
            for disposable in [
                &roots.fixture,
                &roots.install,
                &roots.cargo_target,
                &roots.decoy,
            ] {
                if parent.starts_with(
                    disposable
                        .canonicalize()
                        .map_err(|error| error.to_string())?,
                ) {
                    return Err("strict evidence must be outside disposable journey roots".into());
                }
            }
            admit_installed_candidate(inputs)
        }
    }
}

pub(crate) fn admit_installed_candidate(
    inputs: InstalledInputs,
) -> Result<InstalledCandidate, String> {
    let evidence = CommandEvidence::create(&inputs.evidence_dir)?;
    evidence.json("started.json", &serde_json::json!({
        "state": "not_completed",
        "input_kind": "requested_installed_executable",
        "requested_binary": inputs.binary,
        "expected_sha256": inputs.digest,
        "expected_full_version": inputs.version,
        "package_provenance": "supplied by invoking package lane; not inferred from executable identity",
    }))?;
    let admission = (|| {
        if !inputs.binary.is_absolute() {
            return Err("installed executable must be absolute".to_string());
        }
        validate_full_version(&inputs.version)?;
        let binary = inputs
            .binary
            .canonicalize()
            .map_err(|error| format!("resolve installed executable: {error}"))?;
        if Path::new(env!("CARGO_BIN_EXE_ripr"))
            .canonicalize()
            .ok()
            .as_ref()
            == Some(&binary)
        {
            return Err(
                "strict installed mode refuses the workspace CARGO_BIN_EXE_ripr path".into(),
            );
        }
        let metadata = std::fs::metadata(&binary)
            .map_err(|error| format!("stat installed executable: {error}"))?;
        if !metadata.is_file() {
            return Err("installed executable is not a regular file".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err("installed input is not executable".into());
            }
        }
        let digest = file_digest(&binary)?;
        if digest != inputs.digest {
            return Err(format!(
                "installed executable SHA-256 mismatch: expected {}, got {digest}",
                inputs.digest
            ));
        }
        let version_output = run_program_recorded(
            &binary,
            None,
            &["--version"],
            &[],
            Some(&evidence),
            COMMAND_BUDGET,
        )?;
        require_success("admitted installed ripr --version", &version_output)?;
        let version = version_line(&version_output)?;
        if version != inputs.version {
            return Err(format!(
                "installed full version mismatch: expected {:?}, got {version:?}",
                inputs.version
            ));
        }
        if file_digest(&binary)? != digest {
            return Err("installed executable changed during version admission".into());
        }
        evidence.json(
            "admitted.json",
            &serde_json::json!({
                "resolved_binary": binary, "sha256": digest, "full_version": version,
            }),
        )?;
        Ok((binary, digest, version))
    })();
    match admission {
        Ok((binary, digest, version)) => {
            eprintln!(
                "ordinary journey input: admitted_installed_executable; raw evidence {}",
                inputs.evidence_dir.display()
            );
            Ok(InstalledCandidate {
                binary,
                digest,
                version,
                evidence: Some(evidence),
            })
        }
        Err(error) => {
            evidence.json(
                "admission-refused.json",
                &serde_json::json!({"error": error}),
            )?;
            Err(error)
        }
    }
}

fn version_line(output: &Output) -> Result<String, String> {
    let text = std::str::from_utf8(&output.stdout)
        .map_err(|error| format!("installed --version was not UTF-8: {error}"))?;
    let line = text
        .strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .unwrap_or(text);
    if line.is_empty() || line.contains(['\r', '\n']) {
        return Err("installed --version did not print exactly one nonempty line".into());
    }
    Ok(line.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WitnessFacts {
    pub(crate) expression: String,
    pub(crate) file: String,
    pub(crate) missing_equality: bool,
    pub(crate) related_equality_test: bool,
    pub(crate) public_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FocusedExecution {
    pub(crate) selector: String,
    pub(crate) selected: u64,
    pub(crate) executed: u64,
    pub(crate) passed: u64,
    pub(crate) failed: u64,
    pub(crate) ignored: u64,
    pub(crate) exit_code: Option<i32>,
    pub(crate) command: Vec<String>,
}

impl FocusedExecution {
    pub(crate) fn as_json(&self) -> Value {
        serde_json::json!({
            "selector": self.selector, "selected": self.selected,
            "executed": self.executed, "passed": self.passed,
            "failed": self.failed, "ignored": self.ignored,
            "native_exit_code": self.exit_code, "command": self.command,
        })
    }
}

pub(crate) fn unique_dir(label: &str) -> Result<PathBuf, String> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("clock before Unix epoch: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-{label}-{}-{stamp}", std::process::id()));
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("create {} failed: {error}", root.display()))?;
    Ok(root)
}

pub(crate) fn journey_roots(label: &str) -> Result<JourneyRoots, String> {
    Ok(JourneyRoots {
        fixture: unique_dir(&format!("{label} fixture"))?,
        install: unique_dir(&format!("{label}-install"))?,
        cargo_target: unique_dir(&format!("{label}-cargo-target"))?,
        decoy: unique_dir(&format!("{label}-decoy"))?,
    })
}

pub(crate) fn write_crate(root: &Path, lib: &str, tests: &str) -> Result<(), String> {
    std::fs::create_dir_all(root.join("src"))
        .map_err(|error| format!("create src in {}: {error}", root.display()))?;
    std::fs::create_dir_all(root.join("tests"))
        .map_err(|error| format!("create tests in {}: {error}", root.display()))?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"boundary-gap-closure\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|error| format!("write Cargo.toml failed: {error}"))?;
    // Repair --phase before refuses unless Cargo's build directory is ignored
    // at the repository root before the test edit (edit-cage precondition).
    std::fs::write(root.join(".gitignore"), "/target/\n")
        .map_err(|error| format!("write .gitignore failed: {error}"))?;
    std::fs::write(root.join("src/lib.rs"), lib)
        .map_err(|error| format!("write src/lib.rs failed: {error}"))?;
    std::fs::write(root.join("tests/pricing.rs"), tests)
        .map_err(|error| format!("write tests/pricing.rs failed: {error}"))
}

pub(crate) fn init_git(root: &Path) -> Result<(), String> {
    fixture_git_ok(root, &["-c", "init.defaultBranch=main", "init", "-q"])?;
    fixture_git_ok(root, &["config", "user.name", "ripr fixture"])?;
    fixture_git_ok(root, &["config", "user.email", "fixture@ripr.invalid"])?;
    fixture_git_ok(root, &["config", "core.autocrlf", "false"])?;
    Ok(())
}

pub(crate) fn commit_all(root: &Path, message: &str) -> Result<String, String> {
    fixture_git_ok(root, &["add", "."])?;
    fixture_git_ok(
        root,
        &[
            "-c",
            "user.name=ripr fixture",
            "-c",
            "user.email=fixture@ripr.invalid",
            "commit",
            "-qm",
            message,
        ],
    )?;
    fixture_git_output(root, &["rev-parse", "HEAD"]).map(|text| text.trim().to_string())
}

/// Two-commit fixture: production `>` then `>=`, tests still miss equality.
pub(crate) fn two_commit_before_repo(root: &Path, cargo_target: &Path) -> Result<String, String> {
    write_crate(root, LIB_OPEN, BEFORE_TESTS)?;
    init_git(root)?;
    generate_lockfile(root, cargo_target)?;
    let production_base = commit_all(root, "open predicate")?;
    std::fs::write(root.join("src/lib.rs"), LIB_CLOSED)
        .map_err(|error| format!("write closed production failed: {error}"))?;
    commit_all(root, "closed predicate, weak tests")?;
    Ok(production_base)
}

pub(crate) fn generate_lockfile(fixture: &Path, cargo_target: &Path) -> Result<(), String> {
    std::fs::create_dir_all(cargo_target)
        .map_err(|error| format!("create cargo target dir failed: {error}"))?;
    let manifest = path_arg(&fixture.join("Cargo.toml"));
    let output = run_program(
        "cargo",
        Some(fixture),
        &["generate-lockfile", "--manifest-path", &manifest],
        &[
            (OsStr::new("CARGO_TARGET_DIR"), cargo_target.as_os_str()),
            (OsStr::new("CARGO_TERM_COLOR"), OsStr::new("never")),
        ],
    )?;
    require_success("cargo generate-lockfile", &output)
}

pub(crate) fn install_candidate(install_root: &Path) -> Result<InstalledCandidate, String> {
    let bin_dir = install_root.join("bin");
    std::fs::create_dir_all(&bin_dir)
        .map_err(|error| format!("create {} failed: {error}", bin_dir.display()))?;
    let source = Path::new(env!("CARGO_BIN_EXE_ripr"));
    let installed = bin_dir.join(if cfg!(windows) { "ripr.exe" } else { "ripr" });
    std::fs::copy(source, &installed).map_err(|error| {
        format!(
            "copy {} to {} failed: {error}",
            source.display(),
            installed.display()
        )
    })?;
    let digest = file_digest(&installed)?;
    let version_output = run_program(&installed, None, &["--version"], &[])?;
    if !version_output.status.success() {
        return Err(command_failure("installed ripr --version", &version_output));
    }
    let version = String::from_utf8(version_output.stdout)
        .map_err(|error| format!("installed --version was not UTF-8: {error}"))?
        .trim()
        .to_string();
    if version.is_empty() {
        return Err("installed ripr --version printed an empty version".to_string());
    }
    Ok(InstalledCandidate {
        binary: installed,
        digest,
        version,
        evidence: None,
    })
}

pub(crate) fn plant_path_decoy(decoy_dir: &Path) -> Result<PathBuf, String> {
    let marker = decoy_dir.join(DECOY_MARKER);
    let decoy = if cfg!(windows) {
        decoy_dir.join("ripr.cmd")
    } else {
        decoy_dir.join("ripr")
    };
    let body = if cfg!(windows) {
        format!("@echo invoked>\"{}\"\r\nexit /b 99\r\n", marker.display())
    } else {
        format!(
            "#!/bin/sh\nprintf invoked > '{}'\nexit 99\n",
            marker.display()
        )
    };
    std::fs::write(&decoy, body)
        .map_err(|error| format!("write decoy {} failed: {error}", decoy.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&decoy)
            .map_err(|error| format!("stat decoy failed: {error}"))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&decoy, permissions)
            .map_err(|error| format!("chmod decoy failed: {error}"))?;
    }
    Ok(marker)
}

pub(crate) fn decoy_was_invoked(marker: &Path) -> bool {
    marker.exists()
}

pub(crate) fn isolated_path(decoy_dir: &Path) -> Result<std::ffi::OsString, String> {
    let host = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(
        std::iter::once(decoy_dir.to_path_buf()).chain(std::env::split_paths(&host)),
    )
    .map_err(|error| format!("join PATH: {error}"))
}

pub(crate) fn run_installed(
    candidate: &InstalledCandidate,
    fixture: &Path,
    decoy_dir: &Path,
    args: &[&str],
) -> Result<Output, String> {
    candidate.verify_digest()?;
    let path = isolated_path(decoy_dir)?;
    let output = run_program_recorded(
        &candidate.binary,
        Some(fixture),
        args,
        &[(OsStr::new("PATH"), path.as_os_str())],
        candidate.evidence.as_ref(),
        COMMAND_BUDGET,
    )?;
    candidate.verify_digest()?;
    Ok(output)
}

pub(crate) fn require_success(label: &str, output: &Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    Err(command_failure(label, output))
}

pub(crate) fn stdout_text(label: &str, output: &Output) -> Result<String, String> {
    String::from_utf8(output.stdout.clone())
        .map_err(|error| format!("{label} emitted non-UTF-8 stdout: {error}"))
}

pub(crate) fn parse_json(label: &str, text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("{label} JSON failed to parse: {error}"))
}

pub(crate) fn run_repo_exposure_text(
    candidate: &InstalledCandidate,
    fixture: &Path,
    decoy_dir: &Path,
) -> Result<String, String> {
    let output = run_installed(
        candidate,
        fixture,
        decoy_dir,
        &[
            "check",
            "--root",
            &path_arg(fixture),
            "--mode",
            "ready",
            "--format",
            "repo-exposure-json",
        ],
    )?;
    require_success("ripr check --format repo-exposure-json", &output)?;
    stdout_text("repo exposure", &output)
}

pub(crate) fn run_repo_exposure(
    candidate: &InstalledCandidate,
    fixture: &Path,
    decoy_dir: &Path,
) -> Result<Value, String> {
    parse_json(
        "repo exposure",
        &run_repo_exposure_text(candidate, fixture, decoy_dir)?,
    )
}

pub(crate) fn run_diff_check(
    candidate: &InstalledCandidate,
    fixture: &Path,
    decoy_dir: &Path,
    base: &str,
) -> Result<Value, String> {
    let root = path_arg(fixture);
    let output = run_installed(
        candidate,
        fixture,
        decoy_dir,
        &[
            "check",
            "--root",
            &root,
            "--base",
            base,
            "--worktree",
            "--mode",
            "ready",
            "--format",
            "json",
        ],
    )?;
    require_success("ripr check --base --worktree --format json", &output)?;
    parse_json("diff check", &stdout_text("diff check", &output)?)
}

pub(crate) fn run_outcome(
    candidate: &InstalledCandidate,
    fixture: &Path,
    decoy_dir: &Path,
    before: &Path,
    after: &Path,
) -> Result<Value, String> {
    let output = run_installed(
        candidate,
        fixture,
        decoy_dir,
        &[
            "outcome",
            "--before",
            &path_arg(before),
            "--after",
            &path_arg(after),
            "--format",
            "json",
        ],
    )?;
    require_success("ripr outcome --format json", &output)?;
    parse_json("outcome", &stdout_text("outcome", &output)?)
}

pub(crate) fn boundary_seam(value: &Value) -> Result<&Value, String> {
    value
        .get("seams")
        .and_then(Value::as_array)
        .and_then(|seams| {
            seams.iter().find(|seam| {
                seam.get("kind").and_then(Value::as_str) == Some("predicate_boundary")
                    && seam
                        .get("expression")
                        .and_then(Value::as_str)
                        .is_some_and(|expression| {
                            expression.contains("amount >= discount_threshold")
                        })
            })
        })
        .ok_or_else(|| "repo exposure did not contain the boundary predicate seam".to_string())
}

pub(crate) fn boundary_finding(value: &Value) -> Result<&Value, String> {
    let findings = value
        .get("findings")
        .and_then(Value::as_array)
        .ok_or_else(|| "diff check is missing findings".to_string())?;
    let matches_boundary = |finding: &&Value| {
        finding
            .pointer("/probe/expression")
            .and_then(Value::as_str)
            .is_some_and(|expression| expression.contains("amount >= discount_threshold"))
    };
    findings
        .iter()
        .find(|finding| {
            matches_boundary(finding)
                && finding.pointer("/probe/family").and_then(Value::as_str) == Some("predicate")
        })
        .or_else(|| findings.iter().find(matches_boundary))
        .ok_or_else(|| "diff check did not contain the boundary predicate finding".to_string())
}

pub(crate) fn missing_equality(container: &Value) -> Result<bool, String> {
    let facts = container
        .get("missing_discriminators")
        .or_else(|| container.pointer("/activation/missing_discriminators"))
        .and_then(Value::as_array)
        .ok_or_else(|| "missing_discriminators array is absent".to_string())?;
    Ok(facts.iter().any(fact_names_equality))
}

fn fact_names_equality(fact: &Value) -> bool {
    let text = match fact {
        Value::String(text) => text.as_str(),
        Value::Object(_) => fact.get("value").and_then(Value::as_str).unwrap_or(""),
        _ => return false,
    };
    // Repo exposure names `discount_threshold (equality boundary)`; diff check
    // names `amount == discount_threshold`. Those are comparable projections
    // of the same unobserved equality case (#3160 bounded port, not a rewrite).
    text.contains(EQUALITY_DISCRIMINATOR) || text.contains("amount == discount_threshold")
}

pub(crate) fn related_equality_test(container: &Value) -> bool {
    container
        .get("related_tests")
        .and_then(Value::as_array)
        .is_some_and(|tests| {
            tests
                .iter()
                .any(|test| test.get("name").and_then(Value::as_str) == Some(EQUALITY_SELECTOR))
        })
}

pub(crate) fn repo_witness(exposure: &Value) -> Result<WitnessFacts, String> {
    let seam = boundary_seam(exposure)?;
    Ok(WitnessFacts {
        expression: seam
            .get("expression")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        file: seam
            .get("file")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        missing_equality: missing_equality(seam)?,
        related_equality_test: related_equality_test(seam),
        public_class: seam
            .get("grip_class")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

pub(crate) fn diff_witness(check: &Value) -> Result<WitnessFacts, String> {
    let finding = boundary_finding(check)?;
    Ok(WitnessFacts {
        expression: finding
            .pointer("/probe/expression")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        file: finding
            .pointer("/probe/file")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        missing_equality: missing_equality(finding)?,
        related_equality_test: related_equality_test(finding),
        public_class: finding
            .get("classification")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}

/// Diff `exposed` and repo `strongly_gripped` may differ as labels; witness facts may not.
pub(crate) fn assert_shared_witness(
    diff: &WitnessFacts,
    repo: &WitnessFacts,
) -> Result<(), String> {
    if !diff.expression.contains("amount >= discount_threshold")
        || diff.expression != repo.expression
    {
        return Err(format!(
            "diff/repo expressions disagree: {diff:?} vs {repo:?}"
        ));
    }
    if path_tail(&diff.file) != path_tail(&repo.file) {
        return Err(format!(
            "diff/repo files disagree: {} vs {}",
            diff.file, repo.file
        ));
    }
    if diff.missing_equality != repo.missing_equality {
        return Err(format!(
            "diff/repo equality discriminator disagree: {diff:?} vs {repo:?}"
        ));
    }
    if diff.related_equality_test != repo.related_equality_test {
        return Err(format!(
            "diff/repo related equality test disagree: {diff:?} vs {repo:?}"
        ));
    }
    if !classes_are_comparable(&diff.public_class, &repo.public_class) {
        return Err(format!(
            "diff/repo public classes are not comparable projections: {} vs {}",
            diff.public_class, repo.public_class
        ));
    }
    Ok(())
}

pub(crate) fn classes_are_comparable(diff: &str, repo: &str) -> bool {
    matches!(
        (diff, repo),
        ("weakly_exposed", "weakly_gripped")
            | ("exposed", "strongly_gripped")
            | ("weakly_exposed", "weakly_exposed")
            | ("exposed", "exposed")
    )
}

pub(crate) fn run_focused_cargo_test(
    fixture: &Path,
    cargo_target: &Path,
    selector: &str,
) -> Result<FocusedExecution, String> {
    run_focused_cargo_test_recorded(fixture, cargo_target, selector, None)
}

pub(crate) fn run_focused_cargo_test_recorded(
    fixture: &Path,
    cargo_target: &Path,
    selector: &str,
    evidence: Option<&CommandEvidence>,
) -> Result<FocusedExecution, String> {
    std::fs::create_dir_all(cargo_target)
        .map_err(|error| format!("create cargo target dir failed: {error}"))?;
    let manifest = fixture.join("Cargo.toml");
    let manifest_arg = path_arg(&manifest);
    let command = vec![
        "cargo".to_string(),
        "test".to_string(),
        "--manifest-path".to_string(),
        manifest_arg.clone(),
        "--test".to_string(),
        "pricing".to_string(),
        selector.to_string(),
        "--".to_string(),
        "--exact".to_string(),
        "--nocapture".to_string(),
    ];
    let args: Vec<&str> = command.iter().skip(1).map(String::as_str).collect();
    let output = run_program_recorded(
        "cargo",
        Some(fixture),
        &args,
        &[
            (OsStr::new("CARGO_TARGET_DIR"), cargo_target.as_os_str()),
            (OsStr::new("CARGO_TERM_COLOR"), OsStr::new("never")),
        ],
        evidence,
        COMMAND_BUDGET,
    )?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    let parsed = parse_libtest(&combined, selector);
    Ok(FocusedExecution {
        selector: selector.to_string(),
        selected: parsed.selected,
        executed: parsed.executed,
        passed: parsed.passed,
        failed: parsed.failed,
        ignored: parsed.ignored,
        exit_code: output.status.code(),
        command,
    })
}

struct LibtestCounts {
    selected: u64,
    executed: u64,
    passed: u64,
    failed: u64,
    ignored: u64,
}

fn parse_libtest(text: &str, selector: &str) -> LibtestCounts {
    let mut selected = 0;
    for line in text.lines() {
        if let Some(rest) = line.trim().strip_prefix("running ")
            && let Some(count) = rest.split_whitespace().next()
            && let Ok(value) = count.parse::<u64>()
        {
            selected = value;
        }
    }
    let named_ok = text.lines().any(|line| {
        line.contains(selector) && (line.contains(" ... ok") || line.contains(" ... FAILED"))
    });
    let mut passed = 0;
    let mut failed = 0;
    let mut ignored = 0;
    if let Some(line) = text.lines().find(|line| line.contains("test result:")) {
        passed = count_after(line, " passed");
        failed = count_after(line, " failed");
        ignored = count_after(line, " ignored");
    }
    let executed = if named_ok { selected.max(1) } else { selected };
    LibtestCounts {
        selected,
        executed,
        passed,
        failed,
        ignored,
    }
}

fn count_after(line: &str, unit: &str) -> u64 {
    line.split(unit)
        .next()
        .and_then(|head| head.split_whitespace().last())
        .and_then(|number| number.parse().ok())
        .unwrap_or(0)
}

pub(crate) fn split_printed_ripr_command(command: &str) -> Result<Vec<String>, String> {
    let tokens = printed_words::printed_words(command)?;
    if tokens.is_empty() {
        return Err("printed command was empty".to_string());
    }
    if tokens.first().map(String::as_str) != Some("ripr") {
        return Err(format!(
            "printed command must start with installed `ripr`, got `{command}`"
        ));
    }
    if tokens.iter().any(|token| {
        token.contains("cargo ")
            || token.contains("target/debug")
            || token.contains("CARGO_BIN_EXE")
    }) {
        return Err(format!(
            "printed command leaked a workspace binary path: `{command}`"
        ));
    }
    Ok(tokens)
}

pub(crate) fn run_workspace_ripr(fixture: &Path, args: &[&str]) -> Result<Output, String> {
    run_program(env!("CARGO_BIN_EXE_ripr"), Some(fixture), args, &[])
}

pub(crate) fn run_printed_ripr(
    candidate: &InstalledCandidate,
    fixture: &Path,
    decoy_dir: &Path,
    printed: &str,
) -> Result<Output, String> {
    candidate.verify_digest()?;
    let tokens = split_printed_ripr_command(printed)?;
    let args: Vec<&str> = tokens.iter().skip(1).map(String::as_str).collect();
    let path = isolated_path(decoy_dir)?;
    let output = run_program_recorded(
        &candidate.binary,
        Some(fixture),
        &args,
        &[(OsStr::new("PATH"), path.as_os_str())],
        candidate.evidence.as_ref(),
        COMMAND_BUDGET,
    )?;
    candidate.verify_digest()?;
    Ok(output)
}

fn run_program(
    program: impl AsRef<OsStr>,
    current_dir: Option<&Path>,
    args: &[&str],
    extra_env: &[(&OsStr, &OsStr)],
) -> Result<Output, String> {
    run_program_recorded(program, current_dir, args, extra_env, None, COMMAND_BUDGET)
}

const COMMAND_BUDGET: Duration = Duration::from_mins(2);
const CAPTURE_LIMIT: u64 = 4 * 1024 * 1024;

/// One construction site serves product, native test and containment controls.
/// The latter can deliberately use default ownership to test the isolated
/// entry point without adding a second executable-resolution path.
pub(crate) fn prepared_command(
    program: impl AsRef<OsStr>,
    current_dir: Option<&Path>,
    args: &[&str],
    extra_env: &[(&OsStr, &OsStr)],
) -> Command {
    let mut command = Command::new(program.as_ref());
    if let Some(dir) = current_dir {
        command.current_dir(dir);
    }
    command.args(args);
    if let Ok(path) = std::env::var("PATH")
        && !extra_env.iter().any(|(key, _)| *key == OsStr::new("PATH"))
    {
        command.env("PATH", path);
    }
    if let Some(profile) = std::env::var_os("LLVM_PROFILE_FILE") {
        command.env("LLVM_PROFILE_FILE", profile);
    }
    for (key, value) in extra_env {
        command.env(key, value);
    }
    command.stdin(Stdio::null());
    command
}

/// File capture avoids a pipe reader that can outlive a terminated primary. The
/// existing owner settles the isolated group/job before any terminal record.
pub(crate) fn run_program_recorded(
    program: impl AsRef<OsStr>,
    current_dir: Option<&Path>,
    args: &[&str],
    extra_env: &[(&OsStr, &OsStr)],
    evidence: Option<&CommandEvidence>,
    budget: Duration,
) -> Result<Output, String> {
    let capture = match evidence {
        Some(evidence) => evidence.command_dir()?,
        None => unique_dir("boundary-command")?,
    };
    let stdout_path = capture.join("stdout");
    let stderr_path = capture.join("stderr");
    let mut command = prepared_command(program.as_ref(), current_dir, args, extra_env);
    let explicit_env: Vec<_> = command
        .get_envs()
        .map(|(key, value)| {
            (
                key.to_string_lossy().into_owned(),
                value.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect();
    let cwd = match current_dir {
        Some(path) => path.to_path_buf(),
        None => std::env::current_dir().map_err(|error| error.to_string())?,
    };
    std::fs::write(
        capture.join("command.json"),
        json_bytes(&serde_json::json!({
            "program": program.as_ref().to_string_lossy(), "args": args, "cwd": cwd,
            "explicit_env": explicit_env, "budget_ms": budget.as_millis(),
            "capture_limit_bytes": CAPTURE_LIMIT, "containment": "OwnedProcess::spawn_isolated",
        }))?,
    )
    .map_err(|error| format!("write command identity: {error}"))?;
    let output_file = |path: &Path| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("create capture {}: {error}", path.display()))
    };
    command
        .stdout(output_file(&stdout_path)?)
        .stderr(output_file(&stderr_path)?);
    let started = Instant::now();
    let mut owned = match OwnedProcess::spawn_isolated(command) {
        Ok(owned) => owned,
        Err(error) => {
            std::fs::write(
                capture.join("result.json"),
                json_bytes(&serde_json::json!({
                    "spawn_error": error.to_string(), "native_exit_code": null,
                    "cleanup": "no_child_created_or_setup_owner_returned_error",
                }))?,
            )
            .map_err(|write_error| write_error.to_string())?;
            return Err(format!(
                "spawn {:?} failed: {error}; raw evidence {}",
                program.as_ref(),
                capture.display()
            ));
        }
    };
    let primary_pid = owned.id();
    let mut stopped_by = None;
    let waited = (|| loop {
        let captured = std::fs::metadata(&stdout_path)
            .and_then(|out| {
                std::fs::metadata(&stderr_path).map(|err| out.len().saturating_add(err.len()))
            })
            .map_err(|error| format!("stat command capture: {error}"))?;
        if captured > CAPTURE_LIMIT {
            stopped_by = Some("output_limit");
            break Err("command exceeded its raw output limit".to_string());
        }
        if let Some(status) = owned
            .try_wait()
            .map_err(|error| format!("observe command exit: {error}"))?
        {
            break Ok(status);
        }
        if started.elapsed() >= budget {
            stopped_by = Some("deadline");
            break Err("command exceeded its harness deadline".to_string());
        }
        std::thread::sleep(Duration::from_millis(10));
    })();
    let cleanup = owned.terminate_tree();
    // Re-admit both settled streams together. A child can write and exit
    // between the running-size observation and try_wait; per-stream limits
    // alone would admit two individually small streams over the shared cap.
    let mut capture_overflow = false;
    let captured = (|| {
        let total = std::fs::metadata(&stdout_path)
            .and_then(|out| {
                std::fs::metadata(&stderr_path).map(|err| out.len().saturating_add(err.len()))
            })
            .map_err(|error| format!("stat settled command capture: {error}"))?;
        if total > CAPTURE_LIMIT {
            capture_overflow = true;
            return Err("settled command capture exceeded its combined byte limit".to_string());
        }
        let read_capture = |path: &Path| -> Result<Vec<u8>, String> {
            let mut bytes = Vec::new();
            std::fs::File::open(path)
                .map_err(|error| error.to_string())?
                .take(CAPTURE_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            Ok(bytes)
        };
        let stdout = read_capture(&stdout_path)?;
        let stderr = read_capture(&stderr_path)?;
        if stdout.len().saturating_add(stderr.len()) as u64 > CAPTURE_LIMIT {
            capture_overflow = true;
            return Err("read command capture exceeded its combined byte limit".to_string());
        }
        Ok((stdout, stderr))
    })();
    if capture_overflow && stopped_by.is_none() {
        stopped_by = Some("output_limit");
    }
    let result = serde_json::json!({
        "primary_pid": primary_pid,
        "native_exit_code": waited.as_ref().ok().and_then(|status| status.code()),
        "native_success": waited.as_ref().ok().map(|status| status.success()),
        "harness_stop": stopped_by,
        "wait_error": waited.as_ref().err(),
        "cleanup_error": cleanup.as_ref().err(),
        "capture_error": captured.as_ref().err(),
        "elapsed_ms": started.elapsed().as_millis(),
        "stdout_sha256": file_digest(&stdout_path)?,
        "stderr_sha256": file_digest(&stderr_path)?,
    });
    std::fs::write(capture.join("result.json"), json_bytes(&result)?)
        .map_err(|error| format!("write command terminal record: {error}"))?;
    cleanup.map_err(|error| format!("{error}; raw evidence {}", capture.display()))?;
    let status = waited.map_err(|error| format!("{error}; raw evidence {}", capture.display()))?;
    let (stdout, stderr) =
        captured.map_err(|error| format!("{error}; raw evidence {}", capture.display()))?;
    let output = Output {
        status,
        stdout,
        stderr,
    };
    if evidence.is_none() {
        std::fs::remove_dir_all(&capture)
            .map_err(|error| format!("cleanup capture {}: {error}", capture.display()))?;
    }
    Ok(output)
}

pub(crate) fn file_digest(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("read {} for SHA-256: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

fn command_failure(label: &str, output: &Output) -> String {
    format!(
        "{label} failed with {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!(
        "sha256:{}",
        digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub(crate) fn path_arg(path: &Path) -> String {
    path.display().to_string()
}

fn path_tail(path: &str) -> &str {
    path.rsplit(['/', '\\'])
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_printed_command_round_trips_quoted_root() -> Result<(), String> {
        // The root is quoted in this host's `shell_arg` form (#7729).
        let printed = if cfg!(windows) {
            r#"ripr agent repair --root "/tmp/gap fixture" --attempt repair-attempt-aa --phase after"#
        } else {
            "ripr agent repair --root '/tmp/gap fixture' --attempt repair-attempt-aa --phase after"
        };
        let tokens = split_printed_ripr_command(printed)?;
        if tokens
            != [
                "ripr",
                "agent",
                "repair",
                "--root",
                "/tmp/gap fixture",
                "--attempt",
                "repair-attempt-aa",
                "--phase",
                "after",
            ]
        {
            return Err(format!("unexpected tokens: {tokens:?}"));
        }
        Ok(())
    }

    #[test]
    fn missing_equality_reads_object_and_string_facts() -> Result<(), String> {
        let objects = serde_json::json!({
            "missing_discriminators": [
                {"value": "discount_threshold (equality boundary)", "reason": "absent"}
            ]
        });
        let strings = serde_json::json!({
            "missing_discriminators": ["discount_threshold (equality boundary)"]
        });
        let diff_projection = serde_json::json!({
            "missing_discriminators": [{"value": "amount == discount_threshold"}]
        });
        let unrelated = serde_json::json!({
            "missing_discriminators": [{"value": "unrelated token"}]
        });
        if !missing_equality(&objects)?
            || !missing_equality(&strings)?
            || !missing_equality(&diff_projection)?
            || missing_equality(&unrelated)?
        {
            return Err("object/string/diff equality facts were not discriminated".to_string());
        }
        Ok(())
    }

    #[test]
    fn boundary_finding_prefers_predicate_family() -> Result<(), String> {
        let report = serde_json::json!({
            "findings": [
                {
                    "classification": "weakly_exposed",
                    "probe": {
                        "family": "call_deletion",
                        "expression": "amount >= discount_threshold",
                        "file": "src/lib.rs"
                    },
                    "missing_discriminators": []
                },
                {
                    "classification": "weakly_exposed",
                    "probe": {
                        "family": "predicate",
                        "expression": "amount >= discount_threshold",
                        "file": "src/lib.rs"
                    },
                    "missing_discriminators": [
                        {"value": "discount_threshold (equality boundary)"}
                    ]
                }
            ]
        });
        let finding = boundary_finding(&report)?;
        if finding.pointer("/probe/family").and_then(Value::as_str) != Some("predicate") {
            return Err(format!("did not prefer the predicate finding: {finding}"));
        }
        if !missing_equality(finding)? {
            return Err("predicate finding lost the equality discriminator".to_string());
        }
        Ok(())
    }
}
