//! Argument parsing, command entry, and report publication for
//! `python-repair-trust check-verification`.
//!
//! Validation, aggregation, and rendering stay in their owning modules. This
//! adapter parses flags, prints the human summary, writes the JSON/Markdown
//! pair, and maps an `inconsistent` verdict to a nonzero exit.

use super::super::python_repair_trust::DEFAULT_MANIFEST;
use super::aggregate::check_verification_artifacts;
use super::render::{render_check_verification_json, render_check_verification_markdown};
use super::schema::RERUN_COMMAND;

const CHECK_REPORT_JSON: &str = "python-repair-verification-check.json";
const CHECK_REPORT_MD: &str = "python-repair-verification-check.md";

fn parse_check_verification_args(args: &[String]) -> Result<(String, String), String> {
    let mut manifest = DEFAULT_MANIFEST.to_string();
    let mut receipts: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--manifest" => {
                index += 1;
                manifest = args.get(index).cloned().ok_or_else(|| {
                    format!(
                        "python-repair-trust check-verification --manifest requires a value\nrerun: {RERUN_COMMAND}"
                    )
                })?;
            }
            "--receipts" => {
                index += 1;
                receipts = Some(args.get(index).cloned().ok_or_else(|| {
                    format!(
                        "python-repair-trust check-verification --receipts requires a value\nrerun: {RERUN_COMMAND}"
                    )
                })?);
            }
            other => {
                return Err(format!(
                    "unknown python-repair-trust check-verification argument: {other}\nusage: cargo xtask python-repair-trust check-verification [--manifest <path>] --receipts <dir-or-file>\nrerun: {RERUN_COMMAND}"
                ));
            }
        }
        index += 1;
    }
    let receipts = receipts.ok_or_else(|| {
        format!(
            "python-repair-trust check-verification requires --receipts <dir-or-file>\nrerun: {RERUN_COMMAND}"
        )
    })?;
    Ok((manifest, receipts))
}

/// The command entry: `python-repair-trust check-verification`, dispatched
/// from the `python-repair-trust` subcommand router.
pub(crate) fn run_check_verification(args: &[String]) -> Result<(), String> {
    let (manifest_path, receipts_input) = parse_check_verification_args(args)?;
    let outcome = check_verification_artifacts(&manifest_path, &receipts_input)?;
    let verdict = outcome.verdict();

    match &outcome.manifest {
        None => println!(
            "python-repair-trust check-verification: manifest={manifest_path} selections=<none> (no accepted selection manifest; verification receipts cannot bind without one)"
        ),
        Some(manifest) => println!(
            "python-repair-trust check-verification: manifest={manifest_path} selections={} sha256={}",
            manifest.selections.len(),
            manifest.sha256
        ),
    }
    println!(
        "python-repair-trust check-verification: receipts={receipts_input} receipts={} violations={} verdict={verdict}",
        outcome.receipts.len(),
        outcome.violations.len()
    );
    for violation in &outcome.violations {
        println!("  violation: {violation}");
    }
    println!(
        "python-repair-trust check-verification verdict: {verdict} — receipt structural validation only; no completed or correct repair is established and execution never implies movement"
    );
    println!("rerun: {RERUN_COMMAND}");

    let report = render_check_verification_json(&outcome)?;
    crate::write_report(CHECK_REPORT_JSON, &format!("{report}\n"))?;
    let markdown = render_check_verification_markdown(&outcome, verdict);
    crate::write_report(CHECK_REPORT_MD, &markdown)?;
    if verdict == "inconsistent" {
        return Err(format!(
            "python-repair-trust check-verification found {} violation(s); see target/ripr/reports/{CHECK_REPORT_JSON}",
            outcome.violations.len()
        ));
    }
    Ok(())
}
