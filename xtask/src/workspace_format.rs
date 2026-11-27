//! The workspace formatting step behind `cargo fmt [--check]`.
//!
//! `cargo fmt` hands rustfmt every target root of the workspace on one command
//! line. Windows caps a command line at 32,767 characters, and this workspace
//! has outgrown that (#12004): the workspace has 491 roots, about 33,500
//! characters, so `cargo fmt --check` fails with os error 206 before rustfmt
//! starts. This owner makes the decisions the pinned `cargo fmt` (rustfmt
//! 1.95.0, `src/cargo-fmt/src/main.rs`) makes without `--all`. It differs in
//! running rustfmt over bounded batches, and where a bullet below says so.
//! rustfmt formats each root and the modules it
//! declares, so a batch boundary only changes how often a shared module is
//! visited.
//!
//! What `cargo fmt` decides, and what is kept:
//!
//! - Which cargo reads the metadata: the `CARGO` environment variable names
//!   the `cargo` that started this step, as it names cargo-fmt's; otherwise
//!   the first `cargo` on `PATH`. The metadata is asked for offline first and
//!   once more without `--offline` when that fails, as cargo-fmt asks, and a
//!   reply that is not a document fails the question too. cargo-fmt would try
//!   to run an empty `CARGO` and fail; this step reads an empty value as
//!   unset.
//! - Which formatter: `RUSTFMT` when it is set and not empty, else the
//!   `rustfmt` beside the `cargo-fmt` that `cargo fmt` runs. Cargo finds an
//!   external subcommand in the `bin` directory of its home (`CARGO_HOME`)
//!   when `PATH` does not already hold that directory, then along `PATH`,
//!   skipping an empty `PATH` entry as cargo does; it does not look beside
//!   `CARGO`. cargo-fmt then runs the `rustfmt` beside
//!   its own executable, the program it finds itself running as: on Linux a
//!   `cargo-fmt` reached through a symbolic link is read beside the link's
//!   target, and on macOS and Windows beside the link. So the formatter is
//!   read apart from the `cargo` that
//!   answers the metadata: with `CARGO` naming another toolchain's `cargo`,
//!   `cargo fmt` and this step still start one formatter. A `cargo-fmt` that
//!   is the rustup proxy runs the toolchain's own rustfmt, whatever sits
//!   beside it, so the `rustfmt` beside a proxy is run only where it is the
//!   same proxy, as rustup installs it; beside any other, or none, the step
//!   refuses and names `RUSTFMT`. With no `cargo-fmt` there is none to run
//!   and the step says so. Never a `rustfmt` found on `PATH` on its own, and
//!   never one named by an empty `PATH` entry, which cargo skips. cargo-fmt
//!   would try to run an empty `RUSTFMT` and fail; this step reads an empty
//!   value as unset, and reads the cargo home only for the default formatter.
//! - Which packages: `cargo metadata --no-deps`, read from the invocation
//!   directory. Every member when that is the workspace root, otherwise the
//!   member whose manifest is the invocation directory's `Cargo.toml`. In a
//!   workspace of one package that package is formatted from any directory,
//!   as cargo-fmt formats it; in a workspace of more, any other directory
//!   has nothing to format and the step refuses.
//! - Which targets: every target, deduplicated by path, the first target that
//!   names a path deciding its edition, grouped by edition.
//! - Which config: none is passed, so rustfmt discovers the nearest
//!   `rustfmt.toml` or `.rustfmt.toml` for each file, as it does for cargo-fmt.
//! - When it stops: at a formatter that cannot be started, as cargo-fmt stops
//!   there; otherwise never early. Every edition group runs and the first
//!   failure is what a failed step reports, so a `--check` run shows all of
//!   the diffs. A failure is the step's own exit status, where cargo-fmt
//!   returns rustfmt's; and a formatter that stops without an exit code (a
//!   signal, on Unix) is a failure here, where cargo-fmt reports success.
//! - The metadata: read as a document that holds what the plan reads, a
//!   workspace root and packages whose targets each name a path and an
//!   edition. A reply that parses and lacks one of these fails its question,
//!   as it fails cargo-fmt's typed reader, so the question is asked once more
//!   without `--offline`.
//! - What it refuses to start: a run whose Windows command line, the
//!   formatter's path quoted and the terminating NUL included, is longer than
//!   `CreateProcess` takes (32,767 UTF-16 code units). Every run is measured
//!   before the first starts, so a target root too long to pass fails the
//!   step with nothing formatted, where a batch that cannot start after other
//!   batches have written would leave the workspace half formatted. The limit
//!   is applied on every host, so one checkout formats everywhere or nowhere.
//!   The line is the one the runner's `Command` writes for a program that is
//!   an executable; a batch-file formatter goes through `cmd.exe`, whose own
//!   shorter limit is not measured.
//! - What it cannot start: a `CARGO` or `RUSTFMT` that is not UTF-8. The
//!   runner names a program as UTF-8, so the step refuses the path by name;
//!   cargo-fmt runs it.

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
    let path = std::env::var_os("PATH");
    let path = path.as_deref();
    let cargo = serving_cargo(std::env::var_os("CARGO").as_deref(), path);
    let cargo = cargo.as_deref().unwrap_or(Path::new("cargo")).as_os_str();
    let rustfmt = std::env::var_os("RUSTFMT");
    let home_bin = home_bin_for(rustfmt.as_deref())?;
    let FormatStep {
        check,
        envs,
        diagnostics,
    } = step;
    let (program, plan) = prepare(check, envs, cargo, rustfmt.as_deref(), path, &home_bin)?;
    run_plan(&program, &plan, envs, diagnostics)
}

/// The `bin` directory of cargo's home, where cargo looks for an external
/// subcommand before it looks on `PATH`.
fn cargo_home_bin() -> Result<PathBuf, String> {
    home::cargo_home()
        .map(|home| home.join("bin"))
        .map_err(|err| format!("the cargo home cannot be read (CARGO_HOME, HOME): {err}"))
}

/// The directory [`cargo_fmt`] looks in first: the cargo home's `bin`, read
/// only when the default formatter is needed. An explicit `RUSTFMT` decides
/// without it, so a home that cannot be read refuses no override; the empty
/// path stands in for it and is never looked in.
fn home_bin_for(rustfmt: Option<&OsStr>) -> Result<PathBuf, String> {
    if rustfmt.filter(|value| !value.is_empty()).is_some() {
        return Ok(PathBuf::new());
    }
    cargo_home_bin()
}

/// The formatter to run and the runs it is given. The metadata is read
/// first, as cargo-fmt reads it, so a `CARGO` that names nothing fails as a
/// cargo that cannot be started and not as a missing rustfmt.
fn prepare(
    check: bool,
    envs: &[(&str, &str)],
    cargo: &OsStr,
    rustfmt: Option<&OsStr>,
    path: Option<&OsStr>,
    home_bin: &Path,
) -> Result<(String, Vec<RustfmtInvocation>), String> {
    let plan = plan_with(check, envs, cargo)?;
    let program = formatter(rustfmt, path, home_bin)?;
    Ok((
        utf8_program(&program, "the formatter to run")?.to_owned(),
        plan,
    ))
}

/// How the runner names a program: its path, which the runner takes as UTF-8.
/// A path that is not UTF-8 is refused by name.
fn utf8_program<'a>(program: &'a Path, what: &str) -> Result<&'a str, String> {
    program.to_str().ok_or_else(|| {
        format!(
            "{what} is not a UTF-8 path ({}), which this step cannot start",
            program.display()
        )
    })
}

/// Runs every invocation, after [`preflight`] has read that each can start.
fn run_plan(
    program: &str,
    plan: &[RustfmtInvocation],
    envs: &[(&str, &str)],
    diagnostics: Option<&mut Vec<CommandDiagnostic>>,
) -> Result<(), String> {
    preflight(program, plan)?;
    run_all(program, plan, envs, diagnostics)
}

/// Whether every run can be started on Windows, read before the first one
/// starts. A batch that cannot be started after other batches have written
/// would leave the workspace half formatted, so one that cannot be started
/// fails the step with nothing run.
fn preflight(program: &str, plan: &[RustfmtInvocation]) -> Result<(), String> {
    let mut refused = Vec::new();
    for (index, invocation) in plan.iter().enumerate() {
        let units = windows_command_line_units(program, &invocation.argv);
        if units > WINDOWS_COMMAND_LINE_LIMIT {
            refused.push(format!(
                "batch {} of {} takes {units} UTF-16 code units",
                index + 1,
                plan.len()
            ));
        }
    }
    if refused.is_empty() {
        return Ok(());
    }
    Err(format!(
        "the formatter cannot be started on Windows: {} (CreateProcess takes at most \
         {WINDOWS_COMMAND_LINE_LIMIT}, the terminating NUL included; a target root or the \
         formatter's own path is too long \
         to pass to rustfmt); no batch was run",
        refused.join("; ")
    ))
}

/// UTF-16 code units `CreateProcess` takes in one command line at most, the
/// terminating NUL included.
pub(crate) const WINDOWS_COMMAND_LINE_LIMIT: usize = 32_767;

/// The command line the runner's `Command` hands `CreateProcess` for one
/// program and its arguments, without the terminating NUL: the program in
/// quotes; each argument after a space, in quotes when it is empty or holds a
/// space or a tab, with a backslash run before a quote doubled and one more
/// added, and a trailing backslash run doubled before the closing quote. These
/// are the rules of the standard library this workspace builds with
/// (`library/std/src/sys/args/windows.rs`, `append_arg`).
pub(crate) fn windows_command_line(program: &str, arguments: &[String]) -> String {
    fn append(command: &mut String, argument: &str) {
        let quote = argument.is_empty() || argument.contains([' ', '\t']);
        if quote {
            command.push('"');
        }
        let mut backslashes = 0;
        for ch in argument.chars() {
            if ch == '\\' {
                backslashes += 1;
                command.push(ch);
                continue;
            }
            if ch == '"' {
                // Two backslashes for each one before the quote, and one more.
                command.extend(std::iter::repeat_n('\\', backslashes + 1));
            }
            backslashes = 0;
            command.push(ch);
        }
        if quote {
            // The trailing run is doubled before the closing quote.
            command.extend(std::iter::repeat_n('\\', backslashes));
            command.push('"');
        }
    }
    let mut command = String::new();
    command.push('"');
    command.push_str(program);
    command.push('"');
    for argument in arguments {
        command.push(' ');
        append(&mut command, argument);
    }
    command
}

/// UTF-16 code units the command line of one run takes on Windows, the
/// terminating NUL included.
pub(crate) fn windows_command_line_units(program: &str, arguments: &[String]) -> usize {
    windows_command_line(program, arguments)
        .encode_utf16()
        .count()
        + 1
}

/// Runs every invocation before reporting the first failure. Stopping at the
/// first failed batch would hide the rest of a `--check` run's diffs. A
/// formatter that cannot be started is another matter: cargo-fmt stops there,
/// so no later batch is started against the same missing program.
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
            // The runner's own words for a program it could not start.
            if error.starts_with(&format!("failed to run {program}:")) {
                return Err(error);
            }
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
/// invocation directory, read through `cargo` and run under the same child
/// environment.
fn plan_with(
    check: bool,
    envs: &[(&str, &str)],
    cargo: &OsStr,
) -> Result<Vec<RustfmtInvocation>, String> {
    let metadata = read_metadata(cargo, envs)?;
    let current_dir = std::env::current_dir()
        .map_err(|err| format!("the invocation directory cannot be read: {err}"))?;
    invocations(&metadata, check, &current_dir)
}

/// The `cargo` that serves this step: `CARGO` names it when cargo started
/// this process, as it names cargo-fmt's; otherwise it is the first `cargo`
/// on `PATH`, the one a bare `cargo` starts. `None` when neither answers. On
/// Windows that is `cargo.exe`: the system appends `.exe` to a name that has
/// no extension, so an extensionless `cargo` there is never the one started.
fn serving_cargo(cargo: Option<&OsStr>, path: Option<&OsStr>) -> Option<PathBuf> {
    if let Some(cargo) = cargo.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(cargo));
    }
    let name = if cfg!(windows) { "cargo.exe" } else { "cargo" };
    std::env::split_paths(path?)
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// `cargo metadata --no-deps` as cargo-fmt reads it: from the invocation
/// directory, which decides the workspace and, with it, the packages; offline
/// first, and once more without `--offline` when that fails.
fn read_metadata(cargo: &OsStr, envs: &[(&str, &str)]) -> Result<Value, String> {
    let program = utf8_program(Path::new(cargo), "the cargo that serves this step")?;
    read_metadata_through(program, &mut |offline| {
        run::run_output_owned_with_envs(program, &metadata_arguments(offline), envs)
            .and_then(|output| read_document(&output))
    })
}

/// The document in a `cargo metadata --no-deps` reply, once it is one this
/// step can plan from.
pub(crate) fn read_document(output: &str) -> Result<Value, String> {
    metadata_document(output).and_then(|document| checked_metadata(&document).map(|()| document))
}

/// The document in a `cargo metadata --no-deps` reply.
fn metadata_document(output: &str) -> Result<Value, String> {
    serde_json::from_str(output)
        .map_err(|err| format!("cargo metadata --no-deps printed no JSON document: {err}"))
}

/// The failure of a target that reports no edition, of its own or of its
/// package.
fn missing_edition(src_path: &str) -> String {
    format!("cargo metadata --no-deps reported no edition for target {src_path}")
}

/// Whether the document holds what the plan reads: a workspace root, and
/// packages whose targets each name a path and an edition of their own or of
/// their package. Read inside each question, as cargo-fmt's typed reader
/// reads the reply.
pub(crate) fn checked_metadata(document: &Value) -> Result<(), String> {
    let reported = |what: &str| format!("cargo metadata --no-deps reported no {what}");
    document["workspace_root"]
        .as_str()
        .filter(|root| !root.is_empty())
        .ok_or_else(|| reported("workspace root"))?;
    for package in document["packages"]
        .as_array()
        .ok_or_else(|| reported("package list"))?
    {
        package["manifest_path"]
            .as_str()
            .filter(|path| !path.is_empty())
            .ok_or_else(|| reported("package manifest path"))?;
        let package_edition = package["edition"].as_str().unwrap_or_default();
        for target in package["targets"]
            .as_array()
            .ok_or_else(|| reported("target list"))?
        {
            let src_path = target["src_path"]
                .as_str()
                .filter(|path| !path.is_empty())
                .ok_or_else(|| reported("target path"))?;
            let edition = target["edition"]
                .as_str()
                .filter(|edition| !edition.is_empty())
                .or_else(|| (!package_edition.is_empty()).then_some(package_edition));
            if edition.is_none() {
                return Err(missing_edition(src_path));
            }
        }
    }
    Ok(())
}

/// [`read_metadata`] over one way of asking `cargo` for its metadata. `attempt`
/// says whether to ask offline; it is asked without `--offline` only when the
/// offline question fails, as cargo-fmt asks, and a question whose reply is
/// not a document fails like one the cargo could not answer.
fn read_metadata_through(
    cargo: &str,
    attempt: &mut impl FnMut(bool) -> Result<Value, String>,
) -> Result<Value, String> {
    match attempt(true) {
        Ok(metadata) => Ok(metadata),
        Err(offline_failure) => attempt(false).map_err(|failure| {
            format!(
                "{cargo} metadata --no-deps failed offline ({offline_failure}) and again \
                 without --offline ({failure})"
            )
        }),
    }
}

fn metadata_arguments(offline: bool) -> Vec<String> {
    let mut arguments = vec!["metadata".to_owned(), "--no-deps".to_owned()];
    if offline {
        arguments.push("--offline".to_owned());
    }
    arguments.extend(["--format-version".to_owned(), "1".to_owned()]);
    arguments
}

/// The formatter `cargo fmt` runs: `RUSTFMT` when it is set and not empty,
/// else the `rustfmt` beside the `cargo-fmt` that cargo would run
/// ([`cargo_fmt`]), as cargo-fmt runs the `rustfmt` beside its own
/// executable. `Err` names the remedy when there is none.
pub(crate) fn formatter(
    rustfmt: Option<&OsStr>,
    path: Option<&OsStr>,
    home_bin: &Path,
) -> Result<PathBuf, String> {
    if let Some(rustfmt) = rustfmt.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(rustfmt));
    }
    let Some(cargo_fmt) = cargo_fmt(path, home_bin) else {
        return Err(format!(
            "no cargo-fmt is in {} or on PATH, so `cargo fmt` has no formatter to run; install \
             the rustfmt component for the toolchain, or set RUSTFMT to the formatter to run",
            home_bin.display()
        ));
    };
    let sibling = cargo_fmt.with_file_name(format!("rustfmt{}", exe_suffix()));
    // A rustup proxy named `cargo-fmt` runs the toolchain's own rustfmt, not
    // the `rustfmt` beside it. That one is the formatter `cargo fmt` runs
    // only where it is the same proxy, which rustup installs beside
    // `cargo-fmt` and which runs that toolchain's rustfmt under its own name.
    if let Ok(image) = std::fs::read(home_bin.join(format!("rustup{}", exe_suffix()))) {
        let is_proxy = |program: &Path| std::fs::read(program).is_ok_and(|bytes| bytes == image);
        if is_proxy(&cargo_fmt) && !is_proxy(&sibling) {
            let beside = if sibling.exists() {
                format!(
                    "the rustfmt beside it ({}) is not the same proxy",
                    sibling.display()
                )
            } else {
                "there is no rustfmt beside it".to_owned()
            };
            return Err(format!(
                "the cargo-fmt that `cargo fmt` runs ({}) is the rustup proxy and {beside}: \
                 `cargo fmt` runs the toolchain's own rustfmt there, which this step does not \
                 read, so it runs no formatter; set RUSTFMT to the formatter to run",
                cargo_fmt.display()
            ));
        }
    }
    if is_executable(&sibling) {
        Ok(sibling)
    } else {
        Err(format!(
            "no rustfmt sits beside the cargo-fmt that `cargo fmt` runs ({}); install the \
             rustfmt component for that toolchain, or set RUSTFMT to the formatter to run",
            cargo_fmt.display()
        ))
    }
}

/// The `cargo-fmt` that `cargo fmt` runs, found as cargo finds an external
/// subcommand (`src/bin/cargo/main.rs`, `find_external_subcommand`): in the `bin`
/// directory of its home when `PATH` does not already hold that directory,
/// then along `PATH`; the first executable one. An empty `PATH` entry is
/// skipped, as cargo skips it: read against the working directory it would
/// name a program there that `cargo fmt` never runs. `None` when there is
/// none.
fn cargo_fmt(path: Option<&OsStr>, home_bin: &Path) -> Option<PathBuf> {
    let mut directories: Vec<PathBuf> = path
        .map(|path| std::env::split_paths(path).collect())
        .unwrap_or_default();
    directories.retain(|directory| !directory.as_os_str().is_empty());
    if !directories.iter().any(|directory| directory == home_bin) {
        directories.insert(0, home_bin.to_path_buf());
    }
    let name = format!("cargo-fmt{}", exe_suffix());
    directories
        .into_iter()
        .map(|directory| directory.join(&name))
        .find(|candidate| is_executable(candidate))
        // cargo-fmt runs the `rustfmt` beside the program it finds itself
        // running as, which is not always the path it was started through.
        .map(running_program)
}

/// The path cargo-fmt finds itself running as. On Linux the running program's
/// path has every symbolic link resolved, so a `cargo-fmt` reached through a
/// link is read beside the link's target; elsewhere it is the path as it was
/// found, and a link is read beside the link.
#[cfg(target_os = "linux")]
fn running_program(program: PathBuf) -> PathBuf {
    std::fs::canonicalize(&program).unwrap_or(program)
}

#[cfg(not(target_os = "linux"))]
fn running_program(program: PathBuf) -> PathBuf {
    program
}

/// The suffix cargo gives an external subcommand's file on this host.
fn exe_suffix() -> &'static str {
    if cfg!(windows) { ".exe" } else { "" }
}

/// Whether cargo may run `path` as an external subcommand: a file, which on
/// Unix also carries a permission bit to run.
fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
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
                .ok_or_else(|| missing_edition(src_path))?;
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
