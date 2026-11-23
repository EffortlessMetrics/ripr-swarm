//! The workspace formatting step behind `cargo fmt [--check]`.
//!
//! `cargo fmt` hands rustfmt every target root of the workspace on one command
//! line. Windows caps a command line at 32,767 characters, and this workspace
//! has outgrown that (#12004): package `ripr` alone carries more than four
//! hundred integration-test roots, so `cargo fmt --check` fails with os error
//! 206 before rustfmt starts. This owner lists the same roots from
//! `cargo metadata`, groups them by edition and runs rustfmt over bounded
//! batches. rustfmt formats each root and the modules it declares, so a batch
//! boundary only changes how often a shared module is visited.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::Value;

use crate::run::{self, CommandDiagnostic};

/// The target kinds `cargo fmt` hands to rustfmt: the first kind of a target.
const FORMATTED_TARGET_KINDS: [&str; 7] = [
    "lib",
    "bin",
    "example",
    "test",
    "bench",
    "custom-build",
    "proc-macro",
];

/// Windows `CreateProcess` caps a command line at 32,767 characters. Half of
/// that leaves room for the program name, quoting and a longer checkout path.
pub(crate) const COMMAND_LINE_BUDGET: usize = 16_000;

const RUSTFMT_CONFIG_FILES: [&str; 2] = ["rustfmt.toml", ".rustfmt.toml"];

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
    /// The arguments without the roots, for the echo and a failure message.
    fn reported(&self) -> Vec<String> {
        let mut reported = self.argv[..self.argv.len() - self.root_count].to_vec();
        reported.push(format!("({} target roots)", self.root_count));
        reported
    }
}

/// Runs the workspace formatting step, stopping at the first failed batch.
pub(crate) fn run(step: FormatStep<'_>) -> Result<(), String> {
    let program = rustfmt_program();
    let FormatStep {
        check,
        envs,
        mut diagnostics,
    } = step;
    for invocation in plan(check, envs)? {
        let args = invocation
            .argv
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let reported = invocation.reported();
        let reported = reported.iter().map(String::as_str).collect::<Vec<_>>();
        run::run_summary(&program, &args, envs, diagnostics.as_deref_mut(), &reported)?;
    }
    Ok(())
}

/// The rustfmt runs that cover every target root of the workspace around `.`,
/// read and run under the same child environment.
pub(crate) fn plan(check: bool, envs: &[(&str, &str)]) -> Result<Vec<RustfmtInvocation>, String> {
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
    let metadata: Value = serde_json::from_str(&metadata)
        .map_err(|err| format!("cargo metadata --no-deps printed no JSON document: {err}"))?;
    invocations(&metadata, check)
}

/// [`plan`] over one parsed `cargo metadata --no-deps` document.
fn invocations(metadata: &Value, check: bool) -> Result<Vec<RustfmtInvocation>, String> {
    let workspace_root = metadata["workspace_root"].as_str().unwrap_or_default();
    let mut common = Vec::new();
    if let Some(config) = RUSTFMT_CONFIG_FILES
        .iter()
        .map(|name| Path::new(workspace_root).join(name))
        .find(|path| path.is_file())
    {
        common.extend([
            "--config-path".to_owned(),
            config.to_string_lossy().into_owned(),
        ]);
    }
    let mut plan = Vec::new();
    for (edition, roots) in roots_by_edition(metadata)? {
        let mut head = common.clone();
        head.extend(["--edition".to_owned(), edition]);
        if check {
            head.push("--check".to_owned());
        }
        plan.extend(batch(&head, &roots));
    }
    Ok(plan)
}

/// Target roots of the workspace packages, by edition, each list sorted.
fn roots_by_edition(metadata: &Value) -> Result<BTreeMap<String, Vec<String>>, String> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata --no-deps listed no packages")?;
    let mut roots: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for package in packages {
        let id = package["id"].as_str().unwrap_or_default();
        let edition = package["edition"]
            .as_str()
            .filter(|edition| !edition.is_empty())
            .ok_or_else(|| format!("workspace package {id} reports no edition"))?;
        for target in package["targets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
        {
            let src_path = target["src_path"].as_str().unwrap_or_default();
            if !src_path.ends_with(".rs") {
                continue;
            }
            let formatted = target["kind"]
                .as_array()
                .and_then(|kinds| kinds.first())
                .and_then(Value::as_str)
                .is_some_and(|kind| FORMATTED_TARGET_KINDS.contains(&kind));
            if formatted {
                roots
                    .entry(edition.to_owned())
                    .or_default()
                    .push(src_path.to_owned());
            }
        }
    }
    for list in roots.values_mut() {
        list.sort();
        list.dedup();
    }
    Ok(roots)
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

/// The formatter `cargo fmt` runs: `RUSTFMT` when it is set, else `rustfmt`.
fn rustfmt_program() -> String {
    std::env::var("RUSTFMT").unwrap_or_else(|_| "rustfmt".to_owned())
}

#[cfg(test)]
mod tests;
