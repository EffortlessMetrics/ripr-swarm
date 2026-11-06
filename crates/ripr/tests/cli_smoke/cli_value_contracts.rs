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

// RIPR-SPEC-0009 / #10538: guidance must not turn rejection into an alias.
#[test]
fn json_rejections_name_the_accepted_route_before_io() -> Result<(), String> {
    let root = super::unique_temp_workspace("json-rejection-guidance");
    std::fs::create_dir_all(&root).map_err(|err| err.to_string())?;
    // If parsing were to continue, config/input validation would replace the
    // expected argv refusal. No analysis or output creation should begin.
    std::fs::write(root.join("ripr.toml"), "invalid = [").map_err(|err| err.to_string())?;
    let root_arg = root.display().to_string();
    let out = root.join("must-not-write");
    let out_arg = out.display().to_string();
    let missing = root.join("missing.json").display().to_string();
    for (command, args) in [
        ("pilot", vec!["pilot", "--root", &root_arg]),
        (
            "outcome",
            vec!["outcome", "--before", &missing, "--after", &missing],
        ),
        (
            "calibrate cargo-mutants",
            vec![
                "calibrate",
                "cargo-mutants",
                "--mutants-json",
                &missing,
                "--repo-exposure-json",
                &missing,
            ],
        ),
    ] {
        let mut args = args;
        args.extend(["--out", &out_arg, "--json"]);
        assert_usage_error(
            run_ripr(&args),
            &format!(
                "unknown {command} argument \"--json\". Use `--format json` for JSON output. Run `ripr {command} --help`."
            ),
        );
        assert!(!out.exists(), "rejected {command} created output");
        assert!(
            !root.join("target").exists(),
            "rejected {command} started work"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("ripr.toml")).map_err(|err| err.to_string())?,
            "invalid = ["
        );
        std::fs::write(&out, "keep existing output").map_err(|err| err.to_string())?;
        assert_usage_error(
            run_ripr(&args),
            &format!(
                "unknown {command} argument \"--json\". Use `--format json` for JSON output. Run `ripr {command} --help`."
            ),
        );
        assert_eq!(
            std::fs::read_to_string(&out).map_err(|err| err.to_string())?,
            "keep existing output"
        );
        std::fs::remove_file(&out).map_err(|err| err.to_string())?;
    }
    super::ignore_remove_dir_all(&root);
    Ok(())
}

#[test]
fn json_guidance_preserves_neighboring_option_and_format_errors() {
    for (command, prefix, label, accepted) in [
        ("pilot", vec!["pilot"], "format", "human, json"),
        (
            "outcome",
            vec!["outcome"],
            "outcome format",
            "md, markdown, text, json",
        ),
        (
            "calibrate cargo-mutants",
            vec!["calibrate", "cargo-mutants"],
            "calibrate format",
            "md, markdown, text, json",
        ),
    ] {
        let mut args = prefix.clone();
        args.push("--wat");
        assert_usage_error(
            run_ripr(&args),
            &format!("unknown {command} argument \"--wat\". Run `ripr {command} --help`."),
        );
        let mut args = prefix.clone();
        args.push("--forma");
        assert_usage_error(
            run_ripr(&args),
            &format!(
                "unknown {command} argument \"--forma\". Did you mean `--format`? Run `ripr {command} --help`."
            ),
        );
        for (value, hint) in [("yaml", ""), ("jsonn", " Did you mean `json`?")] {
            let mut args = prefix.clone();
            args.extend(["--format", value]);
            assert_usage_error(
                run_ripr(&args),
                &format!("unknown {label} {value:?}.{hint} Accepted: {accepted}."),
            );
        }
    }
}
