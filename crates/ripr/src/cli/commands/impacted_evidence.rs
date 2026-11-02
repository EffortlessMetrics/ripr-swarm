//! `ripr impacted-evidence` argv adapter.

use crate::app::impacted_evidence::{self, ImpactedEvidenceOptions, IMPACTED_EVIDENCE_HELP};
use crate::cli::help;
use crate::cli::suggest::unknown_argument;
use std::path::Path;

pub(super) fn impacted_evidence(args: &[String]) -> Result<(), String> {
    let repo = repo_root()?;
    run_at(&repo, args)
}

/// argv parsing for `ripr impacted-evidence` and compatibility `xtask` routes.
pub fn parse_impacted_evidence_options(
    args: &[String],
) -> Result<ImpactedEvidenceOptions, String> {
    parse_options(args)
}

pub(super) fn parse_options(args: &[String]) -> Result<ImpactedEvidenceOptions, String> {
    let mut options = ImpactedEvidenceOptions::default();
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--pr-evidence" => {
                i += 1;
                options.pr_evidence = non_empty_arg(args, i, "--pr-evidence")?.to_string();
            }
            "--label" => {
                i += 1;
                options
                    .labels
                    .push(non_empty_arg(args, i, "--label")?.to_string());
            }
            "--labels" => {
                i += 1;
                options
                    .labels
                    .extend(split_labels(non_empty_arg(args, i, "--labels")?));
            }
            "--check" => options.check = true,
            other => return Err(unknown_argument("impacted-evidence", other)),
        }
        i += 1;
    }
    options.labels = impacted_evidence::normalize_labels(&options.labels);
    Ok(options)
}

pub(super) fn run_at(repo: &Path, args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("{}", help::render_command_help(IMPACTED_EVIDENCE_HELP, &[]));
        return Ok(());
    }
    let options = parse_options(args)?;
    impacted_evidence::execute_impacted_evidence_at(repo, &options)
}

fn non_empty_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let Some(value) = args.get(index) else {
        return Err(format!("missing value for {flag}; pass a value after it"));
    };
    if value.trim().is_empty() {
        return Err(format!(
            "impacted-evidence {flag} requires a non-empty value; pass `{flag} <value>`"
        ));
    }
    Ok(value)
}

fn split_labels(labels: &str) -> Vec<String> {
    labels
        .split([',', '\n', ';'])
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .collect()
}

fn repo_root() -> Result<std::path::PathBuf, String> {
    std::env::current_dir()
        .map_err(|err| format!("failed to determine working directory: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rejects_blank_label_values() -> Result<(), String> {
        let err = match parse_options(&["--label".to_string(), "   ".to_string()]) {
            Ok(_) => return Err("blank --label should fail".to_string()),
            Err(err) => err,
        };
        assert_eq!(
            err,
            "impacted-evidence --label requires a non-empty value; pass `--label <value>`"
                .to_string()
        );
        Ok(())
    }

    #[test]
    fn parse_accepts_labels_and_check() -> Result<(), String> {
        let parsed = parse_options(&[
            "--label".to_string(),
            "release-risk".to_string(),
            "--labels".to_string(),
            "mutation, docs".to_string(),
            "--check".to_string(),
        ])?;
        assert_eq!(
            parsed.labels,
            vec![
                "docs".to_string(),
                "mutation".to_string(),
                "release-risk".to_string()
            ]
        );
        assert!(parsed.check);
        Ok(())
    }
}
