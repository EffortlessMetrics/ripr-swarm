//! `cargo xtask capability-docs [--check|--write]`: keep the generated
//! capability-registry block in `docs/status/SUPPORT_TIERS.md` and in
//! `docs/LANGUAGE_ADAPTER_PREVIEW.md` equal to the registry's own rendering
//! (#5531). The preview guide embeds the same block so its per-cell support
//! statements are generated, not hand-written; the guide's remaining prose is
//! checked against the registry by the `language_capability_drift` test.
//!
//! There is one renderer, `domain::language_capability::render_registry_block`
//! in the `ripr` library. This command never re-implements it: it runs
//! `ripr capabilities --markdown` (a binary built from this tree) and compares
//! or splices that exact text. The in-crate drift test compares the same
//! renderer against the doc, but it is a Rust test and a docs-only PR does not
//! run it; this command is the standalone way to catch and fix a hand edit of
//! the block without a manual paste.
//!
//! `--check` (the default) fails when the block differs. `--write` rewrites
//! only the text from the begin marker to the end marker. Both fail closed:
//! missing, duplicated or reversed markers, and an empty or malformed
//! rendering, are errors and nothing is written.
//!
//! Limits: not wired into `precommit` (hosted in rust-gates.yml and the docs-gate), because producing
//! the rendering needs a `ripr` build (see docs/CI.md). `--markdown-file`
//! reads a saved rendering instead (offline, can be stale).
use std::path::{Path, PathBuf};

use crate::{FixKind, PolicyDisclosure, PolicyReportSpec, read_text_lossy};

#[cfg(test)]
mod tests;

/// Every document that embeds the generated block. Each must contain exactly
/// one marker pair; a missing one is a violation, never a skip.
const DOC_PATHS: &[&str] = &[
    "docs/status/SUPPORT_TIERS.md",
    "docs/LANGUAGE_ADAPTER_PREVIEW.md",
];
const BEGIN_PREFIX: &str = "<!-- capability-registry:begin";
const END_MARKER: &str = "<!-- capability-registry:end -->";
const USAGE: &str =
    "usage: cargo xtask capability-docs [--check | --write] [--markdown-file <path>]";

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Check,
    Write,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Options {
    pub(super) mode: Mode,
    pub(super) markdown_file: Option<PathBuf>,
}

pub(crate) fn capability_docs(args: &[String]) -> Result<(), String> {
    let options = parse_args(args)?;
    let (rendered_text, source_note) = match &options.markdown_file {
        Some(path) => (
            read_text_lossy(path)?,
            format!(
                "rendering read from `{}` (offline input, not built from this tree; it can be stale)",
                path.display()
            ),
        ),
        None => (
            rendering_from_binary()?,
            "rendering read from `ripr capabilities --markdown` of a binary built from this tree"
                .to_string(),
        ),
    };
    let rendered = validate_rendering(&rendered_text)?;
    match options.mode {
        Mode::Write => {
            // Read and splice every doc before writing any, so a missing
            // marker in one document leaves all of them untouched.
            let mut updates = Vec::new();
            for path in DOC_PATHS {
                let doc = read_text_lossy(Path::new(path))?;
                let updated =
                    replace_block(&doc, &rendered).map_err(|reason| format!("{path}: {reason}"))?;
                updates.push((*path, doc, updated));
            }
            for (path, doc, updated) in updates {
                if updated == doc {
                    println!("{path}: capability-registry block already current");
                } else {
                    std::fs::write(path, updated)
                        .map_err(|err| format!("failed to write {path}: {err}"))?;
                    println!("{path}: capability-registry block rewritten");
                }
            }
            Ok(())
        }
        Mode::Check => {
            let mut violations = Vec::new();
            for path in DOC_PATHS {
                let doc = read_text_lossy(Path::new(path))?;
                violations.extend(block_violations(path, &doc, &rendered));
            }
            let disclosures = vec![PolicyDisclosure {
                heading: "Scope and limits".to_string(),
                intro: format!("{source_note}."),
                items: vec![
                    format!(
                        "Enforced: the marker block in each of {} equals the registry's own rendering.",
                        DOC_PATHS.join(", ")
                    ),
                    "Not enforced here: tier prose and row bindings in SUPPORT_TIERS.md, and per-cell statements in the preview guide's prose (the in-crate `language_capability_drift` test and check-support-tiers). Not part of precommit; hosted in rust-gates.yml and the docs-gate.".to_string(),
                ],
            }];
            crate::finish_policy_report_with_disclosures(
                PolicyReportSpec {
                    report_file: "capability-docs.md",
                    check: "capability-docs",
                    why_it_matters: "The capability-registry blocks in SUPPORT_TIERS.md and LANGUAGE_ADAPTER_PREVIEW.md are generated projections of the registry. A hand edit or a missed regeneration makes a doc state a support state the binary does not. When run, this reports a stale block as a failure; it runs in rust-gates.yml and, for SUPPORT_TIERS.md, LANGUAGE_ADAPTER_PREVIEW.md or docs/specs changes, the docs-gate.",
                    fix_kind: FixKind::AuthorDecisionRequired,
                    recommended_fixes: &[
                        "Run `cargo xtask capability-docs --write` to regenerate only the marker blocks.",
                        "Change cell states in `domain/language_capability.rs`, never in a generated block.",
                    ],
                    rerun_command: "cargo xtask capability-docs --check",
                    exception_template: None,
                },
                &violations,
                &disclosures,
            )
        }
    }
}

pub(super) fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut mode: Option<Mode> = None;
    let mut markdown_file: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--check" | "--write" => {
                let wanted = if arg == "--check" {
                    Mode::Check
                } else {
                    Mode::Write
                };
                if mode.is_some() {
                    return Err(format!("{USAGE} (at most one of --check/--write)"));
                }
                mode = Some(wanted);
            }
            "--markdown-file" => {
                let value = iter.next().ok_or_else(|| USAGE.to_string())?;
                if markdown_file.is_some() {
                    return Err(USAGE.to_string());
                }
                markdown_file = Some(PathBuf::from(value));
            }
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(Options {
        mode: mode.unwrap_or(Mode::Check),
        markdown_file,
    })
}

fn rendering_from_binary() -> Result<String, String> {
    let timeout = crate::tool_build_timeout()?;
    let build_args = ["build", "-p", "ripr", "--quiet"].map(String::from);
    crate::run_output_owned_with_timeout(
        "cargo",
        &build_args,
        timeout,
        "capability-docs build of ripr",
    )?;
    let binary = crate::ripr_debug_binary();
    if !binary.exists() {
        return Err(format!(
            "capability-docs build reported success but {} is absent",
            binary.display()
        ));
    }
    let program = binary.display().to_string();
    let run_args = ["capabilities", "--markdown"].map(String::from);
    crate::run_output_owned_with_timeout(
        &program,
        &run_args,
        timeout,
        "capability-docs `ripr capabilities --markdown`",
    )
}

/// The rendering as a marker block without the trailing newline, or why it
/// cannot be trusted (empty, no markers, or no cell rows).
pub(super) fn validate_rendering(text: &str) -> Result<String, String> {
    let block = text.trim_end_matches(['\n', '\r']);
    if block.trim().is_empty() {
        return Err("capability rendering is empty; refusing to continue".to_string());
    }
    if !block.starts_with(BEGIN_PREFIX) || !block.ends_with(END_MARKER) {
        return Err(
            "capability rendering is not a begin/end marker block; the binary may be stale or the renderer changed"
                .to_string(),
        );
    }
    if block.matches(BEGIN_PREFIX).count() != 1 || block.matches(END_MARKER).count() != 1 {
        return Err("capability rendering must contain exactly one marker pair".to_string());
    }
    if !block.lines().any(|line| line.starts_with("| `")) {
        return Err("capability rendering has no cell rows (empty registry)".to_string());
    }
    Ok(block.to_string())
}

/// Byte range of the marker block in `doc`, markers included.
pub(super) fn locate_block(doc: &str) -> Result<(usize, usize), String> {
    let begins = doc.matches(BEGIN_PREFIX).count();
    let ends = doc.matches(END_MARKER).count();
    if begins != 1 || ends != 1 {
        return Err(format!(
            "expected exactly one capability-registry begin/end marker pair, found {begins}/{ends}"
        ));
    }
    match (doc.find(BEGIN_PREFIX), doc.find(END_MARKER)) {
        (Some(start), Some(end_start)) if start < end_start => {
            Ok((start, end_start + END_MARKER.len()))
        }
        _ => Err("capability-registry end marker precedes the begin marker".to_string()),
    }
}

pub(super) fn block_violations(path: &str, doc: &str, rendered: &str) -> Vec<String> {
    match locate_block(doc) {
        Err(reason) => vec![format!("{path}: {reason}")],
        Ok((start, end)) if doc.get(start..end) == Some(rendered) => Vec::new(),
        Ok(_) => vec![format!(
            "the generated capability-registry block in {path} differs from `ripr capabilities --markdown`; run `cargo xtask capability-docs --write`"
        )],
    }
}

/// `doc` with only the marker block replaced by `rendered`.
pub(super) fn replace_block(doc: &str, rendered: &str) -> Result<String, String> {
    let (start, end) = locate_block(doc)?;
    let head = doc
        .get(..start)
        .ok_or_else(|| "block start is not a char boundary".to_string())?;
    let tail = doc
        .get(end..)
        .ok_or_else(|| "block end is not a char boundary".to_string())?;
    Ok(format!("{head}{rendered}{tail}"))
}
