//! Errors must suggest commands that exist and name the command the user ran
//! (#15257). Each case fails on the pre-fix text: a suggested replacement that
//! the named command rejects, or a command the user never typed.
use super::{
    DeclaredHunkScratch, init_producer_fixture_repo, probe_command, unique_temp_workspace,
};
use std::path::Path;
use std::process::{Output, Stdio};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn ripr(root: &Path, args: &[&str]) -> Result<Output, std::io::Error> {
    probe_command(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

fn scratch(label: &str) -> Result<(std::path::PathBuf, DeclaredHunkScratch), std::io::Error> {
    let root = unique_temp_workspace(label);
    std::fs::create_dir_all(&root)?;
    Ok((root.clone(), DeclaredHunkScratch(root)))
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn a_stray_option_with_no_command_does_not_name_check() -> TestResult {
    let (root, _cleanup) = scratch("err-stray-option")?;
    for args in [&["--bogus"][..], &["--"][..], &["help", "--bogus"][..]] {
        let output = ripr(&root, args)?;
        let text = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {text}");
        assert!(
            text.contains("unknown option") && text.contains("ripr --help"),
            "{args:?}: {text}"
        );
        assert!(!text.contains("ripr check"), "{args:?} named check: {text}");
    }
    // A command typed after the stray option keeps the placement advice.
    let output = ripr(&root, &["--bogus", "check"])?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("options go after the command"), "{text}");
    Ok(())
}

#[test]
fn cache_export_without_a_cache_names_the_producing_check() -> TestResult {
    let (root, _cleanup) = scratch("err-cache-export")?;
    let out = root.join("exported");
    let output = ripr(
        &root,
        &[
            "cache",
            "export",
            "--root",
            ".",
            "--out",
            &out.display().to_string(),
        ],
    )?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("`ripr check --format repo-exposure-summary-json`"),
        "{text}"
    );
    assert!(!text.contains("repo badge analysis"), "{text}");
    // The suggested command exists: `check` documents that format.
    let help = ripr(&root, &["check", "--help", "--all"])?;
    assert!(
        stdout(&help).contains("repo-exposure-summary-json"),
        "check must accept the suggested format"
    );
    Ok(())
}

#[test]
fn verify_rollback_alone_asks_for_the_authority_pair() -> TestResult {
    let (root, _cleanup) = scratch("err-verify-rollback")?;
    let output = ripr(
        &root,
        &[
            "agent",
            "repair",
            "--phase",
            "verify",
            "--attempt",
            "repair-attempt-0123456789abcdef01234567",
            "--verify-rollback",
        ],
    )?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("--verify-authorized --verify-authority <identity>"),
        "{text}"
    );
    assert!(!text.contains("ripr bug"), "{text}");
    Ok(())
}

#[test]
fn verify_without_authority_names_agent_repair_not_python() -> TestResult {
    let (root, _cleanup) = scratch("err-verify-authority")?;
    let output = ripr(
        &root,
        &[
            "agent",
            "repair",
            "--phase",
            "verify",
            "--attempt",
            "repair-attempt-0123456789abcdef01234567",
        ],
    )?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("agent repair --phase verify refuses to run"),
        "{text}"
    );
    assert!(!text.contains("python repair verification"), "{text}");
    Ok(())
}

/// Run `ripr` with `PATH` pointing at an empty directory, so the VCS binary is
/// missing.
fn ripr_without_vcs(root: &Path, args: &[&str]) -> Result<Output, std::io::Error> {
    let empty = root.join("empty-path");
    std::fs::create_dir_all(&empty)?;
    probe_command(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .env("PATH", &empty)
        .args(args)
        .stdin(Stdio::null())
        .output()
}

fn write_manifest(root: &Path) -> Result<(), std::io::Error> {
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"p\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
}

#[test]
fn missing_vcs_does_not_suggest_diff_to_commands_without_it() -> TestResult {
    let (root, _cleanup) = scratch("err-no-vcs")?;
    write_manifest(&root)?;
    for args in [
        &["pr-evidence", "--root", "."][..],
        &[
            "pr-evidence",
            "--base",
            "HEAD",
            "--head",
            "HEAD",
            "--root",
            ".",
        ][..],
        &[
            "review-comments",
            "--base",
            "HEAD",
            "--head",
            "HEAD",
            "--root",
            ".",
        ][..],
        &[
            "gate",
            "check",
            "--base",
            "HEAD",
            "--root",
            ".",
            "--guidance-out",
            "g.json",
        ][..],
    ] {
        let output = ripr_without_vcs(&root, args)?;
        let text = stderr(&output);
        assert!(text.contains("not found on PATH"), "{args:?}: {text}");
        assert!(
            !text.contains("--diff"),
            "{args:?} suggested --diff: {text}"
        );
    }
    // `check` does take `--diff`, so its advice is kept.
    let output = ripr_without_vcs(&root, &["check", "--root", "."])?;
    assert!(stderr(&output).contains("--diff"), "{}", stderr(&output));
    Ok(())
}

#[test]
fn gate_check_after_a_failed_producer_does_not_blame_the_removed_guidance() -> TestResult {
    let (root, _cleanup) = scratch("err-gate-check")?;
    write_manifest(&root)?;
    let output = ripr_without_vcs(
        &root,
        &[
            "gate",
            "check",
            "--base",
            "HEAD",
            "--root",
            ".",
            "--guidance-out",
            "g.json",
        ],
    )?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("review-comments could not complete"),
        "{text}"
    );
    assert!(
        text.contains("review-comments produced no guidance"),
        "{text}"
    );
    assert!(
        !text.contains("required PR guidance input"),
        "must not report the removed guidance as a user input: {text}"
    );
    assert!(
        !text.contains("ripr: ripr gate"),
        "doubled command prefix: {text}"
    );
    assert!(
        root.join("target/ripr/reports/gate-decision.json").exists(),
        "the config_error report is still written"
    );
    Ok(())
}

#[test]
fn gap_ledger_missing_check_output_is_not_sent_to_pilot() -> TestResult {
    let (root, _cleanup) = scratch("err-gap-ledger")?;
    let output = ripr(
        &root,
        &[
            "reports",
            "gap-ledger",
            "--check-output",
            "nope.json",
            "--root",
            ".",
        ],
    )?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("ripr check --format json"), "{text}");
    assert!(!text.contains("ripr pilot"), "{text}");
    Ok(())
}

const FEEDBACK_REASONS: [&str; 25] = [
    "useful_actionable",
    "useful_limitation",
    "correct_target",
    "correct_discriminator",
    "clear_explanation",
    "wrong_target",
    "wrong_discriminator",
    "false_actionable",
    "duplicate_item",
    "already_covered",
    "incorrect_causal_attribution",
    "stale_result",
    "missing_context",
    "ambiguous_target",
    "unclear_explanation",
    "verify_command_failed",
    "receipt_command_failed",
    "route_too_broad",
    "too_slow",
    "too_noisy",
    "unsupported_workflow",
    "intentional_defensive_code",
    "intentional_low_grip",
    "manual_review_preferred",
    "other",
];

#[test]
fn feedback_bad_reason_lists_the_taxonomy_and_help_lists_it_too() -> TestResult {
    let (root, _cleanup) = scratch("err-feedback")?;
    let output = ripr(
        &root,
        &["feedback", "record", "--snapshot", "s", "--reason", "bad"],
    )?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(text.contains("feedback record --reason"), "{text}");
    for code in FEEDBACK_REASONS {
        assert!(text.contains(code), "error must list {code}: {text}");
    }
    let help = ripr(&root, &["feedback", "record", "--help"])?;
    let help = stdout(&help);
    // Both lists come from the binary: every code the error offers is in help.
    let offered = text
        .split("one of: ")
        .nth(1)
        .ok_or("error must list the accepted codes")?
        .trim();
    assert_eq!(
        offered.split(", ").count(),
        FEEDBACK_REASONS.len(),
        "{text}"
    );
    for code in offered.split(", ") {
        assert!(help.contains(code), "help must list {code}");
    }
    assert!(!help.contains("other, ..."), "help still elides codes");
    Ok(())
}

#[test]
fn ts_false_actionable_error_points_at_help_that_documents_the_corpus() -> TestResult {
    let (root, _cleanup) = scratch("err-ts-corpus")?;
    let output = ripr(&root, &["reports", "ts-false-actionable"])?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(2), "{text}");
    assert!(
        text.contains("`ripr reports ts-false-actionable --help` documents the corpus format"),
        "{text}"
    );
    let help = ripr(&root, &["reports", "ts-false-actionable", "--help"])?;
    assert!(
        stdout(&help).contains("typescript_preview_false_actionable_audit_corpus"),
        "the help the error names must document the corpus format"
    );
    Ok(())
}

#[test]
fn agent_repair_unknown_seam_names_agent_repair() -> TestResult {
    let (root, _cleanup) = scratch("err-repair-seam")?;
    init_producer_fixture_repo(&root)?;
    let output = ripr(
        &root,
        &[
            "agent",
            "repair",
            "--seam-id",
            "nope",
            "--phase",
            "before",
            "--root",
            ".",
        ],
    )?;
    let text = stderr(&output);
    assert_eq!(output.status.code(), Some(3), "{text}");
    assert!(
        text.contains("agent repair --seam-id nope was not found"),
        "{text}"
    );
    assert!(!text.contains("agent packet"), "{text}");
    Ok(())
}

#[test]
fn help_text_matches_behavior() -> TestResult {
    let (root, _cleanup) = scratch("err-help")?;
    let packet = stdout(&ripr(&root, &["agent", "packet", "--help"])?);
    assert!(!packet.contains("empty stdout"), "{packet}");
    assert!(packet.contains("refusal document on stdout"), "{packet}");
    let receipt = stdout(&ripr(&root, &["receipt", "write", "--help"])?);
    assert!(!receipt.contains("format-checked only"), "{receipt}");
    assert!(receipt.contains("compared against the HEAD"), "{receipt}");
    let stub = stdout(&ripr(&root, &["agent", "stub", "--help"])?);
    for family in ["call_deletion", "field_construction", "side_effect"] {
        assert!(stub.contains(family), "stub help must list {family}");
    }
    Ok(())
}

#[test]
fn first_pr_spellings_own_their_usage_errors_and_help() -> TestResult {
    let (root, _cleanup) = scratch("err-first-pr-usage")?;
    let before = super::snapshot_tree(&root)?;
    for (command, other) in [("first-pr", "start-here"), ("start-here", "first-pr")] {
        let output = ripr(&root, &[command, "--gap-ledgr"])?;
        let text = stderr(&output);
        let label = format!("unknown {command} argument");
        let help_command = format!("ripr {command} --help");
        assert_eq!(output.status.code(), Some(2), "{text}");
        assert!(output.stdout.is_empty(), "{}", stdout(&output));
        assert!(text.contains(&label), "{text}");
        assert!(text.contains("Did you mean `--gap-ledger`?"), "{text}");
        assert!(text.contains(&format!("Run `{help_command}`.")), "{text}");
        assert!(!text.contains(&format!("ripr {other} --help")), "{text}");

        // A JSON request still produces the same typed refusal for the first
        // rejected token. --json itself remains unsupported by this parser.
        let output = ripr(&root, &[command, "--gap-ledgr", "--json"])?;
        let refusal =
            super::assert_cli_refusal(&output, "unknown_argument", "--gap-ledgr", &label)?;
        let message = refusal["error"]["message"]
            .as_str()
            .ok_or("missing message")?;
        assert!(message.contains(&label), "{message}");
        assert_eq!(stderr(&output).trim_end(), format!("ripr: {message}"));
        assert_eq!(refusal["error"]["next"]["command"], help_command);
        assert_eq!(refusal["error"]["next"]["purpose"], "list_valid_arguments");

        let help = ripr(&root, &[command, "--help"])?;
        assert!(help.status.success(), "{}", stderr(&help));
        assert!(stdout(&help).contains("--gap-ledger"), "{}", stdout(&help));

        let output = ripr(&root, &[command, "--root", ".", "--root", "other"])?;
        let text = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "{text}");
        assert!(
            text.contains(&format!("{command} takes --root once")),
            "{text}"
        );
        assert!(
            !text.contains(&format!("{other} takes --root once")),
            "{text}"
        );
        assert!(output.stdout.is_empty(), "{}", stdout(&output));
        for (args, diagnostic) in [
            (
                &[command, "--gap-ledger", ""][..],
                "--gap-ledger requires a non-empty value",
            ),
            (
                &[command, "--run-chain", "--check"][..],
                "--run-chain cannot be combined with --check:",
            ),
            (
                &[command, "--run-chain", "--check-output", "saved check.json"][..],
                "--run-chain cannot be combined with --check-output:",
            ),
        ] {
            let output = ripr(&root, args)?;
            let text = stderr(&output);
            assert_eq!(output.status.code(), Some(2), "{text}");
            assert!(text.contains(&format!("{command} {diagnostic}")), "{text}");
            assert!(!text.contains(&format!("{other} {diagnostic}")), "{text}");
            assert!(output.stdout.is_empty(), "{}", stdout(&output));
        }
        assert_eq!(super::snapshot_tree(&root)?, before);
    }
    Ok(())
}

#[test]
fn first_pr_spellings_recover_a_missing_check_packet() -> TestResult {
    let (root, _cleanup) = scratch("err-first-pr-check")?;
    init_producer_fixture_repo(&root)?;
    for (command, other) in [("first-pr", "start-here"), ("start-here", "first-pr")] {
        let output_dir = format!("{command} reports");
        let output = ripr(
            &root,
            &[
                command,
                "--base",
                "HEAD",
                "--output-dir",
                &output_dir,
                "--check",
            ],
        )?;
        let text = stderr(&output);
        assert_eq!(output.status.code(), Some(2), "{text}");
        assert!(
            text.contains(&format!("{command} --check validates an existing")),
            "{text}"
        );
        assert!(
            !text.contains(&format!("{other} --check validates")),
            "{text}"
        );
        assert!(text.contains("start-here.json"), "{text}");
        assert!(!root.join(&output_dir).exists());
        assert!(!root.join("target").exists());

        let suggested = super::suggested_first_pr_write(&text)?;
        assert_eq!(
            suggested.first().map(String::as_str),
            Some(command),
            "{text}"
        );
        let args = suggested.iter().map(String::as_str).collect::<Vec<_>>();
        let write = ripr(&root, &args)?;
        assert!(write.status.success(), "{}", stderr(&write));
        for name in ["start-here.json", "start-here.md"] {
            assert!(root.join(&output_dir).join(name).is_file(), "{text}");
        }
    }
    Ok(())
}

#[test]
fn first_pr_spellings_protect_an_explicit_malformed_gap_ledger() -> TestResult {
    let (scratch, _cleanup) = scratch("err-first-pr-malformed-ledger")?;
    let root = scratch.join("fixture");
    std::fs::create_dir(&root)?;
    init_producer_fixture_repo(&root)?;
    let ledger_name = "malformed user ledger.json";
    let ledger = root.join(ledger_name);
    let original = b"{not valid JSON\n";
    std::fs::write(&ledger, original)?;
    let before = super::snapshot_tree(&root)?;
    let advice = "Choose a new output path for the regenerated ledger";

    for command in ["first-pr", "start-here"] {
        let reports = scratch.join(format!("{command} reports"));
        let reports_arg = reports.to_str().ok_or("fixture path must be UTF-8")?;
        let args = [
            command,
            "--base",
            "HEAD",
            "--gap-ledger",
            ledger_name,
            "--output-dir",
            reports_arg,
        ];
        let write = ripr(&root, &args)?;
        assert!(write.status.success(), "{}", stderr(&write));
        let text = stdout(&write);
        assert!(text.contains(advice), "{text}");
        assert!(!text.contains("Next command:"), "{text}");

        let packet: serde_json::Value =
            serde_json::from_slice(&std::fs::read(reports.join("start-here.json"))?)?;
        assert_eq!(packet["status"], "blocked");
        assert_eq!(packet["selected"]["state"], "malformed_artifact");
        assert_eq!(packet["selected"]["output_state"], "malformed_artifact");
        assert_eq!(packet["inputs"]["gap_ledger"], ledger_name);
        assert!(packet["selected"]["next_command"].is_null(), "{packet}");
        assert!(packet["commands"].get("next").is_none(), "{packet}");
        assert!(
            packet["commands"].get("regenerate_gap_ledger").is_none(),
            "{packet}"
        );
        let artifact = packet["artifacts"]
            .as_array()
            .and_then(|artifacts| artifacts.iter().find(|entry| entry["id"] == "gap_ledger"))
            .ok_or("missing gap ledger artifact")?;
        assert!(artifact["regeneration_command"].is_null(), "{artifact}");
        let markdown = std::fs::read_to_string(reports.join("start-here.md"))?;
        assert!(markdown.contains(advice), "{markdown}");
        assert!(!markdown.contains("ripr reports gap-ledger"), "{markdown}");
        assert_eq!(super::snapshot_tree(&reports)?.len(), 2);
        assert_eq!(super::snapshot_tree(&root)?, before);
        assert_eq!(std::fs::read(&ledger)?.as_slice(), original);

        let reports_before_check = super::snapshot_tree(&reports)?;
        let mut check_args = args.to_vec();
        check_args.push("--check");
        let check = ripr(&root, &check_args)?;
        let diagnostic = stderr(&check);
        assert_eq!(check.status.code(), Some(2), "{diagnostic}");
        assert!(
            diagnostic.contains(&format!("{command} --check:")),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("blocked state `malformed_artifact`"),
            "{diagnostic}"
        );
        assert!(stdout(&check).contains(advice), "{}", stdout(&check));
        assert_eq!(super::snapshot_tree(&reports)?, reports_before_check);
        assert_eq!(super::snapshot_tree(&root)?, before);
        assert_eq!(std::fs::read(&ledger)?.as_slice(), original);
    }
    Ok(())
}

#[test]
fn first_pr_spellings_withhold_uninspected_explicit_ledger_regeneration() -> TestResult {
    let (scratch, _cleanup) = scratch("err-first-pr-uninspected-ledger")?;
    let root = scratch.join("fixture");
    std::fs::create_dir(&root)?;
    init_producer_fixture_repo(&root)?;
    let ledger = root.join("supplied ledger.json");
    let original = b"{malformed JSON\n";
    std::fs::write(&ledger, original)?;
    let before = super::snapshot_tree(&root)?;
    for command in ["first-pr", "start-here"] {
        let reports = scratch.join(format!("{command} reports"));
        let args = [
            command,
            "--base",
            "origin/unavailable-review-base",
            "--gap-ledger",
            "supplied ledger.json",
            "--output-dir",
            reports.to_str().ok_or("fixture path must be UTF-8")?,
        ];
        let write = ripr(&root, &args)?;
        assert_eq!(write.status.code(), Some(0), "{}", stderr(&write));
        let packet: serde_json::Value =
            serde_json::from_slice(&std::fs::read(reports.join("start-here.json"))?)?;
        assert_eq!(packet["selected"]["state"], "blocked_artifact");
        let message = packet["selected"]["message"]
            .as_str()
            .ok_or("missing message")?;
        assert!(
            message.contains("does not resolve to a commit"),
            "{message}"
        );
        assert!(message.contains("ledger was not inspected"), "{message}");
        assert!(message.contains("choose a new output path"), "{message}");
        assert!(packet["commands"].get("regenerate_gap_ledger").is_none());
        let artifacts = packet["artifacts"].as_array().ok_or("missing artifacts")?;
        let gap = artifacts
            .iter()
            .find(|entry| entry["id"] == "gap_ledger")
            .ok_or("missing ledger artifact")?;
        assert_eq!(gap["regeneration_command"], serde_json::Value::Null);
        let markdown = std::fs::read_to_string(reports.join("start-here.md"))?;
        assert!(
            markdown.contains(&format!("ripr {command} --root")),
            "{markdown}"
        );
        assert!(!markdown.contains("ripr reports gap-ledger"), "{markdown}");
        let outputs = super::snapshot_tree(&reports)?;
        assert_eq!(outputs.len(), 2);
        let check = ripr(&root, &[&args[..], &["--check"]].concat())?;
        assert_eq!(check.status.code(), Some(2), "{}", stderr(&check));
        let diagnostic = stderr(&check);
        assert!(
            diagnostic.contains(&format!("{command} --check:"))
                && diagnostic.contains("blocked state `missing_artifacts`"),
            "{diagnostic}"
        );
        assert_eq!(packet["selected"]["output_state"], "missing_artifacts");
        assert!(!stdout(&check).contains("ripr reports gap-ledger"));
        assert!(!diagnostic.contains("ripr reports gap-ledger"));
        assert_eq!(super::snapshot_tree(&reports)?, outputs);
        assert_eq!(super::snapshot_tree(&root)?, before);
        assert_eq!(std::fs::read(&ledger)?, original);
    }

    // A valid, parsed blocked ledger still gets its normal refresh command.
    let valid = br#"{"status":"blocked","warnings":["valid blocked control"],"records":[]}"#;
    std::fs::write(&ledger, valid)?;
    let output = ripr(
        &root,
        &[
            "start-here",
            "--base",
            "HEAD",
            "--gap-ledger",
            "supplied ledger.json",
            "--output-dir",
            "../valid-control",
        ],
    )?;
    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    let packet: serde_json::Value = serde_json::from_slice(&std::fs::read(
        scratch.join("valid-control/start-here.json"),
    )?)?;
    assert_eq!(packet["selected"]["state"], "blocked_artifact");
    assert!(
        packet["selected"]["message"]
            .as_str()
            .is_some_and(|text| text.contains("valid blocked control"))
    );
    assert!(
        packet["commands"]["regenerate_gap_ledger"].is_string(),
        "{packet}"
    );
    assert_eq!(std::fs::read(&ledger)?, valid);
    Ok(())
}

#[test]
fn first_pr_spellings_name_themselves_in_saved_missing_report_markdown() -> TestResult {
    let (root, _cleanup) = scratch("err-first-pr-missing-report-markdown")?;
    init_producer_fixture_repo(&root)?;
    for (command, other) in [("first-pr", "start-here"), ("start-here", "first-pr")] {
        let reports = format!("{command} reports");
        let output = ripr(
            &root,
            &[command, "--base", "HEAD", "--output-dir", &reports],
        )?;
        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let markdown = std::fs::read_to_string(root.join(reports).join("start-here.md"))?;
        assert!(
            markdown.contains(&format!("Later steps (rerun `ripr {command}`")),
            "{markdown}"
        );
        assert!(
            !markdown.contains(&format!("Later steps (rerun `ripr {other}`")),
            "{markdown}"
        );
        assert!(
            markdown.contains("Gap decision ledger; PR repair cards"),
            "{markdown}"
        );
        assert!(stdout(&output).contains(&format!("Later steps (rerun `ripr {command}`")));
    }
    Ok(())
}
