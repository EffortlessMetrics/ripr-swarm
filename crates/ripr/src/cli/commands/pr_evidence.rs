//! `ripr pr-evidence` argv adapter.

use crate::app::pr_evidence::{self, PrEvidenceOptions, PR_EVIDENCE_HELP};
use crate::cli::help;
use crate::cli::suggest::unknown_argument;

pub(super) fn pr_evidence(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", help::render_command_help(PR_EVIDENCE_HELP, &[]));
        return Ok(());
    }
    let options = parse_options(args)?;
    pr_evidence::execute_pr_evidence(options)
}

pub(super) fn parse_options(args: &[String]) -> Result<PrEvidenceOptions, String> {
    let mut options = PrEvidenceOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                options.root = non_empty_arg(args, i, "--root")?.to_string();
            }
            "--base" => {
                i += 1;
                options.base = non_empty_arg(args, i, "--base")?.to_string();
                options.base_explicit = true;
            }
            "--head" => {
                i += 1;
                options.head = non_empty_arg(args, i, "--head")?.to_string();
            }
            "--check" => options.check = true,
            other => return Err(unknown_argument("pr-evidence", other)),
        }
        i += 1;
    }
    Ok(options)
}

fn non_empty_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let Some(value) = args.get(index) else {
        return Err(format!("missing value for {flag}; pass a value after it"));
    };
    if value.trim().is_empty() {
        return Err(format!(
            "pr-evidence {flag} requires a non-empty value; pass `{flag} <value>`"
        ));
    }
    Ok(value)
}
