//! `ripr reports ci-summary`: print the generated workflow's step summary.

use crate::cli::parse::expect_value;
use crate::cli::suggest::unknown_argument;
use crate::output::ci_summary::{CiSummaryInput, render_ci_summary};
use std::io::Write;
use std::path::PathBuf;

use super::non_empty_string_arg;

const COMMAND: &str = "reports ci-summary";

/// The flags carry the workflow's environment as values, so an unset
/// variable arrives as an empty string and keeps its shell meaning. A flag
/// left out reads the workflow environment instead (#5409): `RIPR_*` for
/// the settings, and `github.base_ref || default_branch` for the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CiSummaryOptions {
    pub(super) root: PathBuf,
    pub(super) base_ref: String,
    pub(super) upload_sarif: String,
    pub(super) gate_baseline: String,
    pub(super) comment_mode: String,
}

pub(super) fn ci_summary(args: &[String]) -> Result<(), String> {
    let settings = super::ci_packet::CiSettings::from_env()?;
    let options = parse_ci_summary_options_with(args, &settings)?;
    let input = CiSummaryInput {
        configured_languages: configured_languages(&options.root),
        root: options.root,
        base_ref: options.base_ref,
        upload_sarif: options.upload_sarif == "true",
        gate_baseline: !options.gate_baseline.is_empty(),
        comment_mode: options.comment_mode,
    };
    let summary = render_ci_summary(&input);
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&summary)
        .and_then(|()| stdout.flush())
        .map_err(|err| format!("{COMMAND} could not write the summary: {err}"))
}

/// The enabled languages `ripr doctor --json` reports; none when the
/// configuration does not load, as the doctor projection leaves them.
fn configured_languages(root: &std::path::Path) -> Vec<String> {
    crate::config::load_for_root(root)
        .map(|config| {
            config
                .languages()
                .enabled()
                .iter()
                .map(|language| language.as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
pub(super) fn parse_ci_summary_options(args: &[String]) -> Result<CiSummaryOptions, String> {
    parse_ci_summary_options_with(args, &super::ci_packet::CiSettings::default())
}

/// The options, with every flag left out taken from `workflow`.
pub(super) fn parse_ci_summary_options_with(
    args: &[String],
    workflow: &super::ci_packet::CiSettings,
) -> Result<CiSummaryOptions, String> {
    // #7368: preserve an absent base instead of inventing `main`. When the
    // event payload names neither base_ref nor default_branch, substituting
    // `main` would print `origin/main` even for a repository whose default
    // branch is `trunk`; the summary then cannot tell "missing base" apart
    // from "base is main". An empty base_ref keeps the fail-closed wording in
    // `first_run_status` reachable. A local run with no event at all gets the
    // same honest answer: pass `--base-ref` explicitly.
    let base_ref = [&workflow.base_ref, &workflow.default_branch]
        .into_iter()
        .find(|name| !name.is_empty())
        .cloned()
        .unwrap_or_default();
    let mut options = CiSummaryOptions {
        root: PathBuf::from("."),
        base_ref,
        upload_sarif: workflow.upload_sarif.clone(),
        gate_baseline: workflow.gate_baseline.clone(),
        comment_mode: workflow.comment_mode.clone(),
    };
    let mut i = 0usize;
    while i < args.len() {
        let flag = args[i].as_str();
        i += 1;
        match flag {
            "--root" => options.root = PathBuf::from(non_empty_string_arg(args, i, flag, COMMAND)?),
            "--base-ref" => options.base_ref = non_empty_string_arg(args, i, flag, COMMAND)?,
            "--upload-sarif" => options.upload_sarif = expect_value(args, i, flag)?.to_string(),
            "--gate-baseline" => options.gate_baseline = expect_value(args, i, flag)?.to_string(),
            "--comment-mode" => options.comment_mode = expect_value(args, i, flag)?.to_string(),
            other => return Err(unknown_argument(COMMAND, other)),
        }
        i += 1;
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::super::ci_packet::CiSettings;
    use super::{CiSummaryOptions, configured_languages, parse_ci_summary_options_with};
    use std::path::PathBuf;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    fn parse(items: &[&str], settings: &CiSettings) -> Result<CiSummaryOptions, String> {
        parse_ci_summary_options_with(&args(items), settings)
    }

    fn accepts(items: &[&str], settings: &CiSettings) -> bool {
        parse(items, settings).is_ok()
    }

    fn workflow() -> CiSettings {
        CiSettings {
            base_ref: "release".to_string(),
            default_branch: "trunk".to_string(),
            upload_sarif: "true".to_string(),
            gate_baseline: "baseline.json".to_string(),
            comment_mode: "inline".to_string(),
            ..CiSettings::default()
        }
    }

    #[test]
    fn every_flag_overrides_its_workflow_environment_value() -> Result<(), String> {
        let settings = workflow();
        // Control: with no flags the environment supplies every value.
        let from_env = parse(&[], &settings)?;
        assert_eq!(from_env.root, PathBuf::from("."));
        assert_eq!(from_env.base_ref, "release");
        assert_eq!(from_env.upload_sarif, "true");
        assert_eq!(from_env.gate_baseline, "baseline.json");
        assert_eq!(from_env.comment_mode, "inline");
        let flagged = parse(
            &[
                "--root",
                "/repo",
                "--base-ref",
                "dev",
                "--upload-sarif",
                "false",
                "--gate-baseline",
                "",
                "--comment-mode",
                "",
            ],
            &settings,
        )?;
        assert_eq!(flagged.root, PathBuf::from("/repo"));
        assert_eq!(flagged.base_ref, "dev");
        assert_eq!(flagged.upload_sarif, "false");
        // An unset workflow variable arrives as an empty string and is kept.
        assert_eq!(flagged.gate_baseline, "");
        assert_eq!(flagged.comment_mode, "");
        Ok(())
    }

    #[test]
    fn the_base_falls_back_from_the_pr_base_to_the_default_branch_to_empty() -> Result<(), String> {
        let pr = workflow();
        let push = CiSettings {
            base_ref: String::new(),
            ..workflow()
        };
        let neither = CiSettings {
            base_ref: String::new(),
            default_branch: String::new(),
            ..workflow()
        };
        assert_eq!(parse(&[], &pr)?.base_ref, "release");
        assert_eq!(parse(&[], &push)?.base_ref, "trunk");
        assert_eq!(parse(&[], &neither)?.base_ref, "");
        // An explicit flag wins over every fallback.
        assert_eq!(parse(&["--base-ref", "dev"], &neither)?.base_ref, "dev");
        Ok(())
    }

    #[test]
    fn a_flag_without_a_value_is_rejected_by_name() {
        let settings = workflow();
        for flag in [
            "--root",
            "--base-ref",
            "--upload-sarif",
            "--gate-baseline",
            "--comment-mode",
        ] {
            // Control: the same flag with a value parses.
            assert!(parse(&[flag, "v"], &settings).is_ok(), "{flag}");
            assert_eq!(
                parse(&[flag], &settings),
                Err(format!("missing value for {flag}"))
            );
        }
    }

    #[test]
    fn blank_root_and_base_values_are_rejected_but_blank_settings_are_kept() {
        let settings = workflow();
        assert_eq!(
            parse(&["--root", ""], &settings),
            Err("reports ci-summary --root requires a non-empty value".to_string())
        );
        assert_eq!(
            parse(&["--base-ref", "  "], &settings),
            Err("reports ci-summary --base-ref requires a non-empty value".to_string())
        );
        assert!(accepts(&["--root", "r", "--base-ref", "b"], &settings));
        assert!(accepts(&["--upload-sarif", ""], &settings));
        assert!(accepts(&["--gate-baseline", ""], &settings));
        assert!(accepts(&["--comment-mode", ""], &settings));
    }

    #[test]
    fn an_unknown_argument_stops_parsing_even_after_valid_flags() {
        let settings = workflow();
        assert!(accepts(&["--root", "r"], &settings));
        for bad in ["--bogus", "positional", "--root=r"] {
            let message = parse(&["--root", "r", bad], &settings)
                .err()
                .unwrap_or_default();
            assert!(
                message.starts_with(&format!("unknown reports ci-summary argument {bad:?}")),
                "{message}"
            );
        }
        // A value is consumed by its flag, not parsed as a flag itself.
        assert!(accepts(&["--root", "--bogus"], &settings));
    }

    #[test]
    fn configured_languages_come_from_the_config_and_default_to_none()
    -> Result<(), Box<dyn std::error::Error>> {
        let parent =
            std::env::temp_dir().join(format!("ripr-ci-summary-cli-{}", std::process::id()));
        let loads = parent.join("loads");
        let invalid = parent.join("invalid");
        std::fs::create_dir_all(&loads)?;
        std::fs::create_dir_all(&invalid)?;
        std::fs::write(invalid.join("ripr.toml"), "this is not = [valid toml")?;
        // Constructs: the invalid file exists and a missing config is a root
        // with no file at all.
        assert!(invalid.join("ripr.toml").is_file());
        assert!(!loads.join("ripr.toml").exists());
        let defaulted = configured_languages(&loads);
        let broken = configured_languages(&invalid);
        std::fs::remove_dir_all(&parent)?;
        assert_eq!(defaulted, vec!["rust".to_string()]);
        // A configuration that fails to load reports no languages rather than
        // the default, as the doctor projection does.
        assert!(broken.is_empty(), "{broken:?}");
        Ok(())
    }
}
