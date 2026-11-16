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
