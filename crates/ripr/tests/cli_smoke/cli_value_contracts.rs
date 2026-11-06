use super::run_ripr;
use std::process::Output;

fn assert_usage_error(output: Output, diagnostic: &str) {
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("ripr: {diagnostic}\n")
    );
}

#[test]
fn diff_format_errors_preserve_exit_and_suggestions() {
    for (value, diagnostic) in [
        (
            "xml",
            "unknown diff format \"xml\". Accepted: human, text, md, markdown, json.",
        ),
        (
            "jsonn",
            "unknown diff format \"jsonn\". Did you mean `json`? Accepted: human, text, md, markdown, json.",
        ),
    ] {
        assert_usage_error(run_ripr(&["diff", "--format", value]), diagnostic);
    }
    assert_usage_error(
        run_ripr(&["diff", "--format"]),
        "missing value for --format; pass a value right after it",
    );
}

#[test]
fn diff_format_aliases_reach_revision_validation() {
    // A later, deterministic argv refusal proves each value reached the
    // parser without requiring a repository or doing analysis. `--help`
    // would bypass diff's parser and could falsely accept every spelling.
    for value in ["human", "text", "md", "markdown", "json"] {
        assert_usage_error(
            run_ripr(&["diff", "--format", value, "--head", ""]),
            "diff --head requires a non-empty revision",
        );
    }
}

#[test]
fn pilot_format_help_agrees_with_accepted_values() {
    let output = run_ripr(&["pilot", "--help"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let help = String::from_utf8_lossy(&output.stdout);
    let usage = help.lines().find(|line| line.starts_with("Usage:"));
    assert!(
        usage.is_some_and(|line| line.contains("[--format FORMAT]")),
        "{help}"
    );
    assert!(
        help.lines().any(|line| {
            line.starts_with("  --format FORMAT") && line.contains("human (default) or json")
        }),
        "{help}"
    );
    for value in ["human", "json"] {
        assert_usage_error(
            run_ripr(&["pilot", "--format", value, "--max-seams", "0"]),
            "--max-seams requires a positive integer; pass a whole number of 1 or more, got \"0\"",
        );
    }
    assert_usage_error(
        run_ripr(&["pilot", "--format", "jsonn"]),
        "unknown format \"jsonn\". Did you mean `json`? Accepted: human, json.",
    );
}

#[test]
fn doctor_profile_help_agrees_with_accepted_values() {
    let output = run_ripr(&["doctor", "--help"]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        help.lines().any(|line| {
            line.starts_with("  --profile NAME") && line.contains("analysis (default")
        }),
        "{help}"
    );
    assert!(
        help.contains("or source-build (prerequisites for"),
        "{help}"
    );
    for value in ["analysis", "source-build"] {
        assert_usage_error(
            run_ripr(&["doctor", "--profile", value, "--root"]),
            "missing value for --root; pass a value right after it",
        );
    }
    assert_usage_error(
        run_ripr(&["doctor", "--profile", "foo"]),
        "unknown doctor profile \"foo\". Accepted: analysis, source-build.",
    );
    assert_usage_error(
        run_ripr(&["doctor", "--profile"]),
        "missing value for --profile; pass a value right after it",
    );
}
