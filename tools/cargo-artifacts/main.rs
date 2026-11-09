#![forbid(unsafe_code)]
//! Bounded lifecycle for Cargo artifacts owned by one RIPR checkout.
//!
//! The tool never guesses whether an unrelated workspace is live. It reports or
//! removes only direct entries of the current checkout's exact Cargo target,
//! preserves `target/ripr`, and requires `--apply` for deletion.

use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};

mod metadata;
mod render;
mod storage;

use metadata::*;
use render::*;
use storage::*;

#[cfg(test)]
mod tests;

const DEFAULT_MAX_SIZE_GB: u64 = 12;
const BYTES_PER_GB: u64 = 1_000_000_000;
const PRESERVED_ENTRY: &str = "ripr";
const REPORT_JSON: &str = "cargo-artifacts.json";
const REPORT_MARKDOWN: &str = "cargo-artifacts.md";
const ARTIFACT_OVERRIDE_ENV: [&str; 3] = [
    "CARGO_TARGET_DIR",
    "CARGO_BUILD_TARGET_DIR",
    "CARGO_BUILD_BUILD_DIR",
];

#[derive(Debug, Clone, Eq, PartialEq)]
struct ArtifactError(String);

impl ArtifactError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ArtifactError {}

type Result<T> = std::result::Result<T, ArtifactError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operation {
    Report,
    CleanCurrent,
}

impl Operation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Report => "report",
            Self::CleanCurrent => "clean-current",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Options {
    operation: Operation,
    apply: bool,
    max_size_bytes: u64,
    no_receipt: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CargoPaths {
    workspace_root: PathBuf,
    target_directory: PathBuf,
    build_directory: PathBuf,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Measurement {
    bytes: u64,
    files: u64,
    directories: u64,
}

impl Measurement {
    fn add(&mut self, other: Self) {
        self.bytes = self.bytes.saturating_add(other.bytes);
        self.files = self.files.saturating_add(other.files);
        self.directories = self.directories.saturating_add(other.directories);
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PlannedEntry {
    name: String,
    path: PathBuf,
    measurement: Measurement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CleanupPlan {
    target_directory: PathBuf,
    selected: Vec<PlannedEntry>,
    selected_measurement: Measurement,
    retained_measurement: Measurement,
    max_size_bytes: u64,
    over_budget_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Failure {
    path: String,
    error: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ApplyState {
    status: String,
    deleted: Vec<String>,
    failure: Option<Failure>,
}

impl ApplyState {
    fn planned() -> Self {
        Self {
            status: "planned".to_string(),
            deleted: Vec::new(),
            failure: None,
        }
    }
}

#[cfg(not(test))]
fn main() {
    process::exit(run_cli(env::args().skip(1)));
}

fn run_cli(args: impl IntoIterator<Item = String>) -> i32 {
    match parse_options(args).and_then(execute) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("cargo-artifacts: {error}");
            2
        }
    }
}

fn usage() -> String {
    [
        "Usage:",
        "  cargo-artifacts report [--max-size-gb <n>] [--no-receipt]",
        "  cargo-artifacts clean-current [--max-size-gb <n>] [--apply] [--no-receipt]",
        "",
        "Deletion is a dry run unless --apply is present. Destructive runs always",
        "write receipts under target/ripr/reports and preserve target/ripr.",
    ]
    .join("\n")
}

fn parse_options(args: impl IntoIterator<Item = String>) -> Result<Options> {
    let mut args = args.into_iter();
    let operation = match args.next().as_deref() {
        Some("report") => Operation::Report,
        Some("clean-current") => Operation::CleanCurrent,
        Some("help" | "--help" | "-h") | None => return Err(ArtifactError::new(usage())),
        Some(other) => {
            return Err(ArtifactError::new(format!(
                "unknown command `{other}`\n\n{}",
                usage()
            )))
        }
    };
    let mut apply = false;
    let mut no_receipt = false;
    let mut max_size_gb = DEFAULT_MAX_SIZE_GB;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--apply" => apply = true,
            "--no-receipt" => no_receipt = true,
            "--max-size-gb" => {
                let value = args
                    .next()
                    .ok_or_else(|| ArtifactError::new("--max-size-gb requires a value"))?;
                max_size_gb = parse_non_negative(&value, "--max-size-gb")?;
            }
            "--help" | "-h" => return Err(ArtifactError::new(usage())),
            value if value.starts_with("--max-size-gb=") => {
                max_size_gb = parse_non_negative(
                    value.trim_start_matches("--max-size-gb="),
                    "--max-size-gb",
                )?;
            }
            other => {
                return Err(ArtifactError::new(format!(
                    "unknown option `{other}`\n\n{}",
                    usage()
                )))
            }
        }
    }
    if operation == Operation::Report && apply {
        return Err(ArtifactError::new("--apply is valid only with clean-current"));
    }
    if apply && no_receipt {
        return Err(ArtifactError::new(
            "destructive cleanup refuses --no-receipt; retain the deletion record",
        ));
    }
    let max_size_bytes = max_size_gb
        .checked_mul(BYTES_PER_GB)
        .ok_or_else(|| ArtifactError::new("--max-size-gb is too large"))?;
    Ok(Options {
        operation,
        apply,
        max_size_bytes,
        no_receipt,
    })
}

fn parse_non_negative(value: &str, flag: &str) -> Result<u64> {
    value
        .parse::<u64>()
        .map_err(|error| ArtifactError::new(format!("invalid {flag} value `{value}`: {error}")))
}

fn execute(options: Options) -> Result<()> {
    reject_artifact_overrides()?;
    let repository = repository_root(&env::current_dir().map_err(|error| {
        ArtifactError::new(format!("locate current directory: {error}"))
    })?)?;
    let cargo_paths = cargo_metadata(&repository)?;
    let target_directory = validate_cargo_paths(&repository, &cargo_paths)?;
    let plan = build_plan(&target_directory, options.max_size_bytes)?;
    let mut state = ApplyState::planned();

    if !options.no_receipt {
        write_receipts(&target_directory, &options, &plan, &state)?;
    }
    print!("{}", render_markdown(&options, &plan, &state));

    if options.operation != Operation::CleanCurrent || !options.apply {
        return Ok(());
    }

    state.status = "applying".to_string();
    write_receipts(&target_directory, &options, &plan, &state)?;
    for entry in &plan.selected {
        if let Err(error) = remove_planned_entry(&target_directory, entry) {
            state.status = "partial_failure".to_string();
            state.failure = Some(Failure {
                path: entry.path.display().to_string(),
                error: error.to_string(),
            });
            write_receipts(&target_directory, &options, &plan, &state)?;
            return Err(ArtifactError::new(format!(
                "cleanup stopped at {}: {}; prior deletions and the exact remainder are recorded in target/ripr/reports/{REPORT_JSON}",
                entry.path.display(),
                error
            )));
        }
        state.deleted.push(entry.name.clone());
        write_receipts(&target_directory, &options, &plan, &state)?;
    }
    state.status = "complete".to_string();
    write_receipts(&target_directory, &options, &plan, &state)?;
    print!("{}", render_markdown(&options, &plan, &state));
    Ok(())
}

fn reject_artifact_overrides() -> Result<()> {
    let mut active = Vec::new();
    for name in ARTIFACT_OVERRIDE_ENV {
        if env::var_os(name).is_some_and(|value| !value.is_empty()) {
            active.push(name);
        }
    }
    if active.is_empty() {
        Ok(())
    } else {
        Err(ArtifactError::new(format!(
            "unset Cargo artifact overrides before using checkout-owned cleanup: {}",
            active.join(", ")
        )))
    }
}
