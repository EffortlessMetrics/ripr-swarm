//! `ripr annotations` argv adapter.

use crate::app::annotations::{self, AnnotationOptions, ANNOTATIONS_HELP};
use crate::cli::help;
use crate::cli::suggest::unknown_argument;

pub(super) fn annotations(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", help::render_command_help(ANNOTATIONS_HELP, &[]));
        return Ok(());
    }
    let options = parse_options(args)?;
    annotations::execute_annotations(options)
}

pub(super) fn parse_options(args: &[String]) -> Result<AnnotationOptions, String> {
    let mut options = AnnotationOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--comments" => {
                i += 1;
                options.comments = non_empty_arg(args, i, "--comments")?.to_string();
            }
            "--out" => {
                i += 1;
                options.out = non_empty_arg(args, i, "--out")?.to_string();
            }
            "--check" => options.check = true,
            other => return Err(unknown_argument("annotations", other)),
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
            "annotations {flag} requires a non-empty value; pass `{flag} <path>`"
        ));
    }
    Ok(value)
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
