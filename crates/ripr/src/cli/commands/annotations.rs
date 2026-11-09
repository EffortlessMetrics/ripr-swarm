//! `ripr annotations` argv adapter (#5193): parses flags and help, then calls
//! the application use case in `app::annotations`.

use crate::app::annotations::{ANNOTATIONS_HELP, AnnotationOptions, execute_annotations};
use crate::app::subcommand::{NonEmptyPlaceholder, non_empty_arg};
use crate::cli::suggest::unknown_argument;

pub(crate) fn annotations(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    execute_annotations(parse_options(args)?)
}

pub(crate) fn parse_options(args: &[String]) -> Result<AnnotationOptions, String> {
    let mut options = AnnotationOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--comments" => {
                i += 1;
                options.comments = non_empty_arg(
                    "annotations",
                    args,
                    i,
                    "--comments",
                    NonEmptyPlaceholder::Path,
                )?
                .to_string();
            }
            "--out" => {
                i += 1;
                options.out =
                    non_empty_arg("annotations", args, i, "--out", NonEmptyPlaceholder::Path)?
                        .to_string();
            }
            "--check" => options.check = true,
            other => return Err(unknown_argument("annotations", other)),
        }
        i += 1;
    }
    Ok(options)
}

fn print_help() {
    println!(
        "{}",
        crate::cli::help::render_command_help(ANNOTATIONS_HELP, &[])
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_supports_paths_and_check() -> Result<(), String> {
        let parsed = parse_options(&[
            "--comments".to_string(),
            "comments.json".to_string(),
            "--out".to_string(),
            "annotations.txt".to_string(),
            "--check".to_string(),
        ])?;
        assert_eq!(parsed.comments, "comments.json");
        assert_eq!(parsed.out, "annotations.txt");
        assert!(parsed.check);
        match parse_options(&["--comments".to_string(), "".to_string()]) {
            Err(msg) if msg.contains("non-empty") => Ok(()),
            other => Err(format!("expected non-empty error, got {other:?}")),
        }
    }

    #[test]
    fn parse_rejects_unknown_arg() -> Result<(), String> {
        match parse_options(&["--bogus".to_string()]) {
            Err(msg) if msg.contains("--bogus") => Ok(()),
            other => Err(format!("expected unknown-arg error, got {other:?}")),
        }
    }
}
