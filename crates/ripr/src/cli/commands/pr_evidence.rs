//! `ripr pr-evidence` argv adapter (#5193): parses flags and help, then calls
//! the application use case in `app::pr_evidence`.

use crate::app::pr_evidence::{PR_EVIDENCE_HELP, PrEvidenceOptions, execute_pr_evidence};
use crate::app::subcommand::{NonEmptyPlaceholder, non_empty_arg};
use crate::cli::reconcile_root;
use crate::cli::suggest::unknown_argument;

pub(crate) fn pr_evidence(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    execute_pr_evidence(parse_options(args)?)
}

pub(crate) fn parse_options(args: &[String]) -> Result<PrEvidenceOptions, String> {
    let mut options = PrEvidenceOptions::default();
    let mut root_explicit = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value =
                    non_empty_arg("pr-evidence", args, i, "--root", NonEmptyPlaceholder::Value)?;
                // #9008/#10542: an identical repeat is absorbed for scripted
                // double-root invocations; a conflicting value is a usage
                // error naming both values.
                let reconciled = reconcile_root(
                    std::path::Path::new(&options.root),
                    root_explicit,
                    value,
                    "pr-evidence",
                )?;
                options.root = reconciled.display().to_string();
                root_explicit = true;
            }
            "--base" => {
                i += 1;
                options.base =
                    non_empty_arg("pr-evidence", args, i, "--base", NonEmptyPlaceholder::Value)?
                        .to_string();
                options.base_explicit = true;
            }
            "--head" => {
                i += 1;
                options.head =
                    non_empty_arg("pr-evidence", args, i, "--head", NonEmptyPlaceholder::Value)?
                        .to_string();
            }
            "--check" => options.check = true,
            other => return Err(unknown_argument("pr-evidence", other)),
        }
        i += 1;
    }
    Ok(options)
}

fn print_help() {
    println!(
        "{}",
        crate::cli::help::render_command_help(PR_EVIDENCE_HELP, &[])
    );
}
