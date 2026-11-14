//! `ripr plus` argv adapter (#5193): parses flags and help, then calls the
//! application use case in `app::ripr_plus`.

use std::path::PathBuf;

use crate::app::ripr_plus::{PLUS_HELP, RiprPlusOptions, execute_ripr_plus};
use crate::cli::suggest::unknown_argument;

pub(crate) fn ripr_plus(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    execute_ripr_plus(parse_options(args)?)
}

pub(crate) fn parse_options(args: &[String]) -> Result<RiprPlusOptions, String> {
    let mut options = RiprPlusOptions::default();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--check" => options.check = true,
            "--gap-ledger" => {
                index += 1;
                options.gap_ledger =
                    Some(PathBuf::from(non_empty_arg(args, index, "--gap-ledger")?));
            }
            "--repo-exposure-summary" => {
                index += 1;
                options.repo_exposure_summary = Some(PathBuf::from(non_empty_arg(
                    args,
                    index,
                    "--repo-exposure-summary",
                )?));
            }
            other => return Err(unknown_argument("plus", other)),
        }
        index += 1;
    }
    if options.gap_ledger.is_some() && options.repo_exposure_summary.is_some() {
        return Err(
            "plus accepts either --gap-ledger or --repo-exposure-summary, not both".to_string(),
        );
    }
    if options.gap_ledger.is_none() && options.repo_exposure_summary.is_none() {
        return Err(
            "plus requires either --repo-exposure-summary <path> or --gap-ledger <path>; \
             the binary-first `ripr plus` composes a receipt from a pre-computed artifact and \
             does not run an in-process full-repo scan. \
             Run `ripr check --root . --format repo-exposure-summary-json > summary.json` first, \
             then `ripr plus --repo-exposure-summary summary.json`."
                .to_string(),
        );
    }
    Ok(options)
}

fn non_empty_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let Some(value) = args.get(index) else {
        return Err(format!("plus {flag} requires a path; pass `{flag} <path>`"));
    };
    if value.trim().is_empty() {
        return Err(format!(
            "plus {flag} requires a non-empty path; pass `{flag} <path>`"
        ));
    }
    Ok(value)
}

fn print_help() {
    println!("{}", crate::cli::help::render_command_help(PLUS_HELP, &[]));
}
