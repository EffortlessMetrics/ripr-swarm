//! Thin CLI adapter for `ripr first-pr` (#4854 extraction).

use crate::cli::help;
use crate::cli::parse::reconcile_root;
use crate::cli::suggest::unknown_argument;
use crate::output::first_pr::{FIRST_PR_HELP, FirstPrOptions, first_pr_with_options};

pub(crate) fn first_pr(args: &[String]) -> Result<(), String> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }
    first_pr_with_options(parse_options(args)?)
}

pub(crate) fn parse_options(args: &[String]) -> Result<FirstPrOptions, String> {
    let mut options = FirstPrOptions {
        preflight: true,
        ..FirstPrOptions::default()
    };
    let mut root_explicit = false;
    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                i += 1;
                let value = non_empty_arg(args, i, "--root")?.to_string();
                let reconciled = reconcile_root(
                    std::path::Path::new(&options.root),
                    root_explicit,
                    &value,
                    "first-pr",
                )?;
                options.root = reconciled.display().to_string();
                root_explicit = true;
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
            "--check-output" => {
                i += 1;
                options.check_output = Some(non_empty_arg(args, i, "--check-output")?.to_string());
            }
            "--gap-ledger" => {
                i += 1;
                options.gap_ledger = non_empty_arg(args, i, "--gap-ledger")?.to_string();
                options.gap_ledger_explicit = true;
            }
            "--first-action" => {
                i += 1;
                options.first_action = non_empty_arg(args, i, "--first-action")?.to_string();
            }
            "--review-comments" => {
                i += 1;
                options.review_comments = non_empty_arg(args, i, "--review-comments")?.to_string();
            }
            "--agent-packet" => {
                i += 1;
                options.agent_packet = non_empty_arg(args, i, "--agent-packet")?.to_string();
            }
            "--gate-decision" => {
                i += 1;
                options.gate_decision = non_empty_arg(args, i, "--gate-decision")?.to_string();
            }
            "--receipts-dir" => {
                i += 1;
                options.receipts_dir = non_empty_arg(args, i, "--receipts-dir")?.to_string();
            }
            "--out-dir" => {
                i += 1;
                options.out_dir = non_empty_arg(args, i, "--out-dir")?.to_string();
            }
            "--check" => options.check = true,
            "--run-chain" => options.run_chain = true,
            other => return Err(unknown_argument("first-pr", other)),
        }
        i += 1;
    }
    if options.run_chain && options.check {
        return Err("first-pr --run-chain cannot be combined with --check: --check only validates an existing packet and never runs analysis; run first-pr with only one of the two".to_string());
    }
    if options.run_chain && options.check_output.is_some() {
        return Err("first-pr --run-chain cannot be combined with --check-output: a supplied check output already provides the gap ledger input, so there is no chain to run; run first-pr with only one of the two".to_string());
    }
    Ok(options)
}

fn non_empty_arg<'a>(args: &'a [String], index: usize, flag: &str) -> Result<&'a str, String> {
    let Some(value) = args.get(index) else {
        return Err(format!(
            "missing value for {flag}; pass a value right after it"
        ));
    };
    if value.trim().is_empty() {
        return Err(format!(
            "first-pr {flag} requires a non-empty value; pass a value right after {flag}"
        ));
    }
    Ok(value)
}

fn print_help() {
    println!("{}", help::render_command_help(FIRST_PR_HELP, &[]));
}

#[cfg(test)]
mod tests {
    use super::super::tests::args;
    use super::{first_pr, parse_options};
    use crate::output::first_pr::FirstPrOptions;

    #[test]
    fn first_pr_dispatch_wiring_help_exits_cleanly() {
        assert_eq!(first_pr(&args(&["--help"])), Ok(()));
    }

    #[test]
    fn parse_accepts_artifact_paths_and_check() -> Result<(), String> {
        let parsed = parse_options(&[
            "--root".to_string(),
            "repo".to_string(),
            "--base".to_string(),
            "origin/main".to_string(),
            "--head".to_string(),
            "HEAD".to_string(),
            "--check-output".to_string(),
            "check.json".to_string(),
            "--gap-ledger".to_string(),
            "gap.json".to_string(),
            "--out-dir".to_string(),
            "out".to_string(),
            "--check".to_string(),
        ])?;
        assert_eq!(parsed.root, "repo");
        assert_eq!(parsed.base, "origin/main");
        assert!(parsed.base_explicit);
        assert!(!FirstPrOptions::default().base_explicit);
        assert_eq!(parsed.head, "HEAD");
        assert_eq!(parsed.check_output.as_deref(), Some("check.json"));
        assert_eq!(parsed.gap_ledger, "gap.json");
        assert_eq!(parsed.out_dir, "out");
        assert!(parsed.check);
        assert!(parsed.preflight);
        assert!(!FirstPrOptions::default().preflight);
        assert_eq!(
            parse_options(&["--gap-ledger".to_string(), "".to_string()]),
            Err("first-pr --gap-ledger requires a non-empty value; pass a value right after --gap-ledger".to_string())
        );
        match parse_options(&["--gap-ledgr".to_string()]) {
            Err(unknown)
                if unknown.contains("Did you mean `--gap-ledger`?")
                    && unknown.contains("Run `ripr first-pr --help`.") => {}
            other => {
                return Err(format!(
                    "first-pr typo must suggest --gap-ledger from its help body, got {other:?}"
                ));
            }
        }
        Ok(())
    }
}
