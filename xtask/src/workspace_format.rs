//! The workspace formatting step behind `cargo fmt [--check]`.
//!
//! `cargo fmt` hands rustfmt every target root of the workspace on one command
//! line. Windows caps a command line at 32,767 characters, and this workspace
//! has outgrown that (#12004): the workspace has 491 roots, about 33,500
//! characters, so `cargo fmt --check` fails with os error 206 before rustfmt
//! starts. This owner makes the same decisions as the pinned `cargo fmt`
//! (rustfmt 1.95.0, `src/cargo-fmt/src/main.rs`) and differs only in running
//! rustfmt over bounded batches. rustfmt formats each root and the modules it
//! declares, so a batch boundary only changes how often a shared module is
//! visited.
//!
//! What `cargo fmt` decides, and what is kept:
//!
//! - Which formatter: `RUSTFMT` when it is set, else the `rustfmt` beside the
//!   `cargo` that serves the step, as cargo-fmt runs its own toolchain
//!   sibling. Never a `rustfmt` found on `PATH` on its own: a caller can have
//!   pinned `cargo` and another formatter first on `PATH`.
//! - Which packages: `cargo metadata --no-deps`, read from the invocation
//!   directory. Every member when that is the workspace root, otherwise the
//!   member whose manifest is the invocation directory's `Cargo.toml`; from
//!   any other directory there is nothing to format and the step refuses.
//! - Which targets: every target, deduplicated by path, the first target that
//!   names a path deciding its edition, grouped by edition.
//! - Which config: none is passed, so rustfmt discovers the nearest
//!   `rustfmt.toml` or `.rustfmt.toml` for each file, as it does for cargo-fmt.
//! - When it stops: never early. cargo-fmt runs every edition group and then
//!   reports the first failure, so a `--check` run shows all of the diffs.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::run::{self, CommandDiagnostic};

/// Windows `CreateProcess` caps a command line at 32,767 characters. Half of
/// that leaves room for the program name, quoting and a longer checkout path.
pub(crate) const COMMAND_LINE_BUDGET: usize = 16_000;

/// What to format: `cargo fmt`, or `cargo fmt --check` when `check` is set.
pub(crate) struct FormatStep<'a> {
    pub(crate) check: bool,
    pub(crate) envs: &'a [(&'a str, &'a str)],
    pub(crate) diagnostics: Option<&'a mut Vec<CommandDiagnostic>>,
}

impl FormatStep<'static> {
    pub(crate) const fn new(check: bool) -> Self {
        Self {
            check,
            envs: &[],
            diagnostics: None,
        }
    }
}

/// One rustfmt run: its arguments after the program name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RustfmtInvocation {
    pub(crate) argv: Vec<String>,
    /// Target roots in this batch.
    pub(crate) root_count: usize,
}

impl RustfmtInvocation {
    /// The arguments without the roots, for the echo, the diagnostic record
    /// and a failure message: a count stands in for the root paths, and says
    /// so, so a recorded `argv` is not read as the arguments rustfmt ran with.
    fn reported(&self) -> Vec<String> {
        let mut reported = self.argv[..self.argv.len() - self.root_count].to_vec();
        reported.push(format!("({} target roots; paths elided)", self.root_count));
        reported
    }
}

/// Runs the workspace formatting step. Every batch runs, as every edition
/// group runs under `cargo fmt`; the first failure is what a failed step
/// reports, with how many batches failed.
pub(crate) fn run(step: FormatStep<'_>) -> Result<(), String> {
    let program = formatter(
        std::env::var_os("RUSTFMT").as_deref(),
        std::env::var_os("PATH").as_deref(),
    )?;
    let FormatStep {
        check,
        envs,
        diagnostics,
    } = step;
    run_all(
        &program.to_string_lossy(),
        &plan(check, envs)?,
        envs,
        diagnostics,
    )
}

/// Runs every invocation before reporting the first failure. Stopping at the
/// first failed batch would hide the rest of a `--check` run's diffs.
fn run_all(
    program: &str,
    plan: &[RustfmtInvocation],
    envs: &[(&str, &str)],
    mut diagnostics: Option<&mut Vec<CommandDiagnostic>>,
) -> Result<(), String> {
    let mut first_failure = None;
    let mut failed_batches = 0;
    for invocation in plan {
        let args = invocation
            .argv
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let reported = invocation.reported();
        let reported = reported.iter().map(String::as_str).collect::<Vec<_>>();
        if let Err(error) =
            run::run_summary(program, &args, envs, diagnostics.as_deref_mut(), &reported)
        {
            first_failure.get_or_insert(error);
            failed_batches += 1;
        }
    }
    let Some(error) = first_failure else {
        return Ok(());
    };
    Err(match failed_batches {
        1 => error,
        _ => format!("{error}; {failed_batches} batches failed"),
    })
}

/// The rustfmt runs that cover the targets `cargo fmt` would format from the
/// invocation directory, read and run under the same child environment.
pub(crate) fn plan(check: bool, envs: &[(&str, &str)]) -> Result<Vec<RustfmtInvocation>, String> {
    let metadata = read_metadata(envs)?;
    let current_dir = std::env::current_dir()
        .map_err(|err| format!("the invocation directory cannot be read: {err}"))?;
    invocations(&metadata, check, &current_dir)
}

/// `cargo metadata --no-deps` as cargo-fmt reads it: from the invocation
/// directory, which decides the workspace and, with it, the packages.
fn read_metadata(envs: &[(&str, &str)]) -> Result<Value, String> {
    let metadata = run::run_output_owned_with_envs(
        "cargo",
        &[
            "metadata".to_owned(),
            "--no-deps".to_owned(),
            "--format-version".to_owned(),
            "1".to_owned(),
        ],
        envs,
    )?;
    serde_json::from_str(&metadata)
        .map_err(|err| format!("cargo metadata --no-deps printed no JSON document: {err}"))
}

/// The formatter `cargo fmt` runs: `RUSTFMT` when it is set, else the
/// `rustfmt` beside the first `cargo` on `PATH`, which is the `cargo` this
/// step runs. `Err` names the remedy when neither is there.
pub(crate) fn formatter(rustfmt: Option<&OsStr>, path: Option<&OsStr>) -> Result<PathBuf, String> {
    if let Some(rustfmt) = rustfmt.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(rustfmt));
    }
    beside_cargo("rustfmt", path).ok_or_else(|| {
        "no rustfmt sits beside the cargo on PATH; install the rustfmt component for the \
         pinned toolchain, or set RUSTFMT to the formatter to run"
            .to_string()
    })
}

/// `name` beside the first directory on `PATH` that holds a `cargo`, or `None`.
fn beside_cargo(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    let suffix = if cfg!(windows) { ".exe" } else { "" };
    std::env::split_paths(path?).find_map(|directory| {
        let sibling = directory.join(format!("{name}{suffix}"));
        (directory.join(format!("cargo{suffix}")).is_file() && sibling.is_file()).then_some(sibling)
    })
}

/// [`plan`] over one parsed `cargo metadata --no-deps` document read from
/// `current_dir`, with cargo-fmt's package scoping for `cargo fmt` without
/// `--all`.
fn invocations(
    metadata: &Value,
    check: bool,
    current_dir: &Path,
) -> Result<Vec<RustfmtInvocation>, String> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata --no-deps listed no packages")?;
    let current_dir = canonical_or_keep(current_dir);
    let workspace_root = Path::new(metadata["workspace_root"].as_str().unwrap_or_default());
    let at_workspace_root = canonical_or_keep(workspace_root) == current_dir;
    let member_manifest = current_dir.join("Cargo.toml");
    let selected = packages.iter().filter(|package| {
        at_workspace_root
            || packages.len() == 1
            || canonical_or_keep(Path::new(
                package["manifest_path"].as_str().unwrap_or_default(),
            )) == member_manifest
    });
    // cargo-fmt keeps one target per path, sorted by it: the first target that
    // names a path decides its edition.
    let mut by_path: BTreeMap<PathBuf, &str> = BTreeMap::new();
    for package in selected {
        let package_edition = package["edition"].as_str().unwrap_or_default();
        for target in package["targets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let src_path = target["src_path"].as_str().unwrap_or_default();
            let edition = target["edition"]
                .as_str()
                .filter(|edition| !edition.is_empty())
                .or_else(|| (!package_edition.is_empty()).then_some(package_edition))
                .ok_or_else(|| format!("target {src_path} reports no edition"))?;
            by_path
                .entry(canonical_or_keep(Path::new(src_path)))
                .or_insert(edition);
        }
    }
    if by_path.is_empty() {
        return Err(format!(
            "cargo fmt finds no target from {}: run it from the workspace root or from a \
             member directory",
            current_dir.display()
        ));
    }
    let mut by_edition: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for (path, edition) in by_path {
        by_edition
            .entry(edition)
            .or_default()
            .push(path.to_string_lossy().into_owned());
    }
    let mut plan = Vec::new();
    for (edition, roots) in by_edition {
        let mut head = vec!["--edition".to_owned(), edition.to_owned()];
        if check {
            head.push("--check".to_owned());
        }
        plan.extend(batch(&head, &roots));
    }
    Ok(plan)
}

/// The path with its resolved spelling when it exists, as it is otherwise.
fn canonical_or_keep(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Splits `roots` after `head` so no command line exceeds
/// [`COMMAND_LINE_BUDGET`]. One root longer than the budget still runs, alone.
fn batch(head: &[String], roots: &[String]) -> Vec<RustfmtInvocation> {
    let head_length = command_line_length(head);
    let mut batches = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut length = head_length;
    for root in roots {
        let cost = argument_length(root);
        if !pending.is_empty() && length + cost > COMMAND_LINE_BUDGET {
            length = head_length;
            batches.push(invocation(head, std::mem::take(&mut pending)));
        }
        pending.push(root.clone());
        length += cost;
    }
    if !pending.is_empty() {
        batches.push(invocation(head, pending));
    }
    batches
}

fn invocation(head: &[String], roots: Vec<String>) -> RustfmtInvocation {
    let root_count = roots.len();
    let mut argv = head.to_vec();
    argv.extend(roots);
    RustfmtInvocation { argv, root_count }
}

/// UTF-16 code units a command line spends on one argument: a separating
/// space, its text, and the quotes around an argument that holds a space.
fn argument_length(argument: &str) -> usize {
    argument.encode_utf16().count() + 3
}

fn command_line_length(arguments: &[String]) -> usize {
    arguments
        .iter()
        .map(|argument| argument_length(argument))
        .sum()
}

#[cfg(test)]
mod tests;
