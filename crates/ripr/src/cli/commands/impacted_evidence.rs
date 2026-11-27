//! `ripr impacted-evidence` argv adapter (#5193): parses flags and help, then
//! calls the application use case in `app::impacted_evidence`.

use crate::app::impacted_evidence::{
    IMPACTED_EVIDENCE_HELP, ImpactedEvidenceOptions, execute_impacted_evidence_at,
    normalize_labels, split_labels,
};
use crate::app::subcommand::{NonEmptyPlaceholder, invocation_repo_root, non_empty_arg};
use crate::cli::suggest::unknown_argument;

pub(crate) fn impacted_evidence(args: &[String]) -> Result<(), String> {
    run_impacted_evidence_at(&invocation_repo_root()?, args)
}

/// Entry point for the compatibility `cargo xtask impacted-evidence` route,
/// rooted at `repo`. Shares this adapter's parser and help with
/// `ripr impacted-evidence`.
pub fn run_impacted_evidence_at(repo: &std::path::Path, args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    execute_impacted_evidence_at(repo, &parse_options(args)?)
}

pub(crate) fn parse_options(args: &[String]) -> Result<ImpactedEvidenceOptions, String> {
    let mut options = ImpactedEvidenceOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--pr-evidence" => {
                i += 1;
                options.pr_evidence = non_empty_arg(
                    "impacted-evidence",
                    args,
                    i,
                    "--pr-evidence",
                    NonEmptyPlaceholder::Value,
                )?
                .to_string();
            }
            "--label" => {
                i += 1;
                options.labels.push(
                    non_empty_arg(
                        "impacted-evidence",
                        args,
                        i,
                        "--label",
                        NonEmptyPlaceholder::Value,
                    )?
                    .to_string(),
                );
            }
            "--labels" => {
                i += 1;
                options.labels.extend(split_labels(non_empty_arg(
                    "impacted-evidence",
                    args,
                    i,
                    "--labels",
                    NonEmptyPlaceholder::Value,
                )?));
            }
            "--check" => options.check = true,
            other => return Err(unknown_argument("impacted-evidence", other)),
        }
        i += 1;
    }
    options.labels = normalize_labels(&options.labels);
    Ok(options)
}

fn print_help() {
    println!(
        "{}",
        crate::cli::help::render_command_help(IMPACTED_EVIDENCE_HELP, &[])
    );
}
