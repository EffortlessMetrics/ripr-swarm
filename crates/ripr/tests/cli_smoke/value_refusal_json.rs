//! #11979: under an explicit JSON request, a value flag with no value and a
//! malformed number write the same minimal `cli_refusal` document as an
//! unknown flag or value. Before, these left stdout empty, so an agent that
//! asked for JSON had to scrape stderr. Exit code and stderr are unchanged;
//! without a JSON request stdout stays empty.

use super::run_ripr;

/// The exact document for `check --json --base` with nothing after it:
/// the typed code, the flag as token, the stderr sentence and the help route.
#[test]
fn check_base_without_a_value_under_json_writes_the_missing_value_document() {
    let output = run_ripr(&["check", "--json", "--base"]);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "ripr: missing value for --base; pass a value right after it\n"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        r#"{
  "error": {
    "code": "missing_value",
    "message": "missing value for --base; pass a value right after it",
    "next": {
      "command": "ripr check --help --all",
      "purpose": "list_valid_arguments"
    },
    "token": "--base"
  },
  "schema_version": "0.1",
  "status": "refused",
  "tool": "ripr"
}
"#
    );
}

/// Each shared reader types its refusal, and the help route names the
/// two-word command path when that path has its own help.
#[test]
fn missing_and_malformed_values_under_json_name_code_token_and_help_route() -> Result<(), String> {
    let cases: [(&[&str], &str, &str, &str); 8] = [
        (
            &["agent", "status", "--json", "--attempt"],
            "missing_value",
            "--attempt",
            "ripr agent status --help",
        ),
        (
            &["doctor", "--json", "--profile"],
            "missing_value",
            "--profile",
            "ripr doctor --help",
        ),
        (
            &["capabilities", "--json", "--language"],
            "missing_value",
            "--language",
            "ripr capabilities --help",
        ),
        (
            &["context", "--json", "--max-related-tests", "zz"],
            "invalid_value",
            "zz",
            "ripr context --help",
        ),
        (
            &["check", "--json", "--timeout", "zz"],
            "invalid_value",
            "zz",
            "ripr check --help --all",
        ),
        (
            &["check", "--format", "json", "--git-timeout", "-1"],
            "invalid_value",
            "-1",
            "ripr check --help --all",
        ),
        // #8835: the brief cap names the typed value and is typed too.
        (
            &["agent", "brief", "--json", "--max-seams", "11"],
            "invalid_value",
            "11",
            "ripr agent brief --help",
        ),
        (
            &["swarm", "queue", "--json", "--top", "0"],
            "invalid_value",
            "0",
            "ripr swarm queue --help",
        ),
    ];
    for (args, code, token, help) in cases {
        let output = run_ripr(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let document: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|err| format!("{args:?}: stdout must be one JSON document: {err}"))?;
        assert_eq!(document["status"], "refused", "{args:?}");
        assert_eq!(document["error"]["code"], code, "{args:?}");
        assert_eq!(document["error"]["token"], token, "{args:?}");
        assert_eq!(document["error"]["next"]["command"], help, "{args:?}");
        assert_eq!(
            document["error"]["next"]["purpose"], "list_valid_arguments",
            "{args:?}"
        );
        // The document's message is the stderr sentence, not a second wording.
        let message = document["error"]["message"].as_str().unwrap_or_default();
        assert!(!message.is_empty(), "{args:?}");
        assert_eq!(stderr, format!("ripr: {message}\n"), "{args:?}");
    }
    Ok(())
}

/// Without a JSON request the same refusals keep stdout empty and exit 2.
#[test]
fn missing_and_malformed_values_without_json_keep_stdout_empty() {
    for args in [
        &["check", "--base"][..],
        &["agent", "status", "--attempt"],
        &["context", "--max-related-tests", "zz"],
        &["check", "--timeout", "zz"],
        &["check", "--format", "human", "--git-timeout", "-1"],
    ] {
        let output = run_ripr(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        assert!(!output.stderr.is_empty(), "{args:?}");
    }
}

/// The environment-variable spelling of a timeout is not an argv value: it
/// keeps the check refusal envelope, never a second `cli_refusal` document.
#[test]
fn a_malformed_timeout_env_var_keeps_the_one_check_refusal_document() -> Result<(), String> {
    let root = super::workspace_root().display().to_string();
    let output = super::run_ripr_with_env(
        &["check", "--root", &root, "--json"],
        &[("RIPR_CHECK_TIMEOUT", "zz")],
    );
    assert_eq!(output.status.code(), Some(2));
    // `from_slice` refuses trailing content, so a second document fails here.
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("stdout must be one JSON document: {err}"))?;
    assert_eq!(document["error"]["code"], "analysis_failed", "{document}");
    // The check envelope's own schema, not the `cli_refusal` document's.
    assert_eq!(document["schema_version"], "0.2", "{document}");
    assert!(document.get("findings").is_some(), "{document}");
    Ok(())
}

/// #14859: an empty value refused by a command's own parser is typed too,
/// with the flag as token, and the wording stays the command's own.
#[test]
fn empty_values_under_json_name_the_flag_on_command_parsers() -> Result<(), String> {
    let cases: [(&[&str], &str, &str); 8] = [
        (
            &["receipt", "write", "--json", "--gap", ""],
            "--gap",
            "ripr receipt write --help",
        ),
        (
            &["receipt", "write", "--json", "--verify-command", ""],
            "--verify-command",
            "ripr receipt write --help",
        ),
        (
            &["agent", "status", "--json", "--attempt", ""],
            "--attempt",
            "ripr agent status --help",
        ),
        (
            &["agent", "repair", "--json", "--seam-id", " "],
            "--seam-id",
            "ripr agent repair --help",
        ),
        (
            &["agent", "stub", "--json", "--at", ""],
            "--at",
            "ripr agent stub --help",
        ),
        (
            &["status", "--json", "--attempt", ""],
            "--attempt",
            "ripr status --help",
        ),
        (
            &["repair", "--json", "--base", ""],
            "--base",
            "ripr repair --help",
        ),
        (
            &["swarm", "queue", "--json", "--root", ""],
            "--root",
            "ripr swarm queue --help",
        ),
    ];
    for (args, token, help) in cases {
        let output = run_ripr(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        let document: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|err| format!("{args:?}: stdout must be one JSON document: {err}"))?;
        assert_eq!(document["error"]["code"], "missing_value", "{args:?}");
        assert_eq!(document["error"]["token"], token, "{args:?}");
        assert_eq!(document["error"]["next"]["command"], help, "{args:?}");
        let message = document["error"]["message"].as_str().unwrap_or_default();
        assert!(message.contains("requires"), "{args:?}: {message}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr),
            format!("ripr: {message}\n"),
            "{args:?}"
        );
        // Without the JSON request the same refusal keeps stdout empty.
        let plain: Vec<&str> = args
            .iter()
            .copied()
            .filter(|arg| *arg != "--json")
            .collect();
        let plain = run_ripr(&plain);
        assert_eq!(plain.status.code(), Some(2), "{args:?}");
        assert!(plain.stdout.is_empty(), "{args:?}");
    }
    Ok(())
}

/// #14859: a global `-v`/`--verbose` between command words is not a command
/// word, so the help route still names the two-word path.
#[test]
fn verbose_between_command_words_keeps_the_help_route() -> Result<(), String> {
    for verbose in ["-v", "--verbose"] {
        let output = run_ripr(&["agent", verbose, "status", "--json", "--attempt"]);
        assert_eq!(output.status.code(), Some(2), "{verbose}");
        let document: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|err| format!("{verbose}: stdout must be one JSON document: {err}"))?;
        assert_eq!(document["error"]["code"], "missing_value", "{verbose}");
        assert_eq!(
            document["error"]["next"]["command"], "ripr agent status --help",
            "{verbose}"
        );
    }
    Ok(())
}

/// #14859: an empty entry in `agent brief --files` is a malformed list, so it
/// is `invalid_value` with the whole value as the token.
#[test]
fn an_empty_files_entry_under_json_is_an_invalid_value() -> Result<(), String> {
    let output = run_ripr(&["agent", "brief", "--json", "--files", "a.rs,,b.rs"]);
    assert_eq!(output.status.code(), Some(2));
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("stdout must be one JSON document: {err}"))?;
    assert_eq!(document["error"]["code"], "invalid_value", "{document}");
    assert_eq!(document["error"]["token"], "a.rs,,b.rs", "{document}");
    assert_eq!(
        document["error"]["next"]["command"], "ripr agent brief --help",
        "{document}"
    );
    Ok(())
}
