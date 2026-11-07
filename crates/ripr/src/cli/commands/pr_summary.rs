//! `ripr pr-summary` argv adapter (#5193): parses flags and help, then calls
//! the application use case in `app::pr_summary`.

use std::path::PathBuf;

use crate::app::pr_summary::{PR_SUMMARY_HELP, SummaryOptions, execute_pr_summary};
use crate::cli::expect_value;
use crate::cli::suggest::unknown_argument;

pub(crate) fn pr_summary(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    execute_pr_summary(parse_options(args)?)
}

pub(crate) fn parse_options(args: &[String]) -> Result<SummaryOptions, String> {
    let mut root = PathBuf::from(".");
    let mut check = false;
    let mut baseline: Option<String> = None;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--check" => check = true,
            "--root" | "--baseline" => {
                index += 1;
                let path = expect_value(args, index, arg)?;
                if path.trim().is_empty() || path.starts_with('-') {
                    return Err(format!(
                        "pr-summary {arg} requires a non-empty path value; pass `{arg} <path>`"
                    ));
                }
                #[cfg(windows)]
                {
                    let parsed = std::path::Path::new(path);
                    if !parsed.is_absolute()
                        && (parsed.has_root()
                            || matches!(
                                parsed.components().next(),
                                Some(std::path::Component::Prefix(_))
                            ))
                    {
                        return Err(format!(
                            "pr-summary {arg} requires a fully qualified or ordinary relative path; \
                             Windows partially qualified paths are not supported; pass a drive-qualified absolute path or a plain relative path"
                        ));
                    }
                }
                if arg == "--root" {
                    root = PathBuf::from(path);
                } else {
                    baseline = Some(path.to_string());
                }
            }
            other => return Err(unknown_argument("pr-summary", other)),
        }
        index += 1;
    }
    Ok(SummaryOptions {
        root,
        check,
        baseline,
    })
}

fn print_help() {
    println!(
        "{}",
        crate::cli::help::render_command_help(PR_SUMMARY_HELP, &[])
    );
}
