//! `ripr pr-summary` argv adapter.

use crate::app::pr_summary::{self, SummaryOptions, PR_SUMMARY_HELP};
use crate::cli::help;
use crate::cli::parse::expect_value;
use crate::cli::suggest::unknown_argument;
use std::path::PathBuf;

pub(super) fn pr_summary(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", help::render_command_help(PR_SUMMARY_HELP, &[]));
        return Ok(());
    }
    let options = parse_options(args)?;
    pr_summary::execute_pr_summary(options)
}

pub(super) fn parse_options(args: &[String]) -> Result<SummaryOptions, String> {
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
                    let parsed = Path::new(path);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_check_only() {
        assert_eq!(
            parse_options(&["--check".to_string()]),
            Ok(SummaryOptions {
                root: PathBuf::from("."),
                check: true,
                baseline: None
            })
        );
    }

    #[test]
    fn parse_accepts_baseline() {
        assert_eq!(
            parse_options(&["--baseline".to_string(), "before.json".to_string()]),
            Ok(SummaryOptions {
                root: PathBuf::from("."),
                check: false,
                baseline: Some("before.json".to_string())
            })
        );
    }
}
