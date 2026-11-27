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
