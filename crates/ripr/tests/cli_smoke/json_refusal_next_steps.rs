//! #15260: under `--json`, a refusal writes one JSON refusal document on
//! stdout with a non-null `next` that matches the human next step. Before,
//! `receipt check`, `cache status` and `agent status --attempt` left stdout
//! empty, and an invalid `ripr.toml` shortened the config error and carried
//! `next: null` while stderr named the fix.

use super::{init_check_refusal_repo, run_ripr, unique_temp_workspace};
use serde_json::Value;
use std::process::Output;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn document(output: &Output, code: i32) -> Result<Value, Box<dyn std::error::Error>> {
    assert_eq!(output.status.code(), Some(code), "{output:?}");
    let stdout = String::from_utf8(output.stdout.clone())?;
    assert!(
        !stdout.is_empty(),
        "stdout must carry the document: {output:?}"
    );
    Ok(serde_json::from_str(&stdout)?)
}

fn broken_config_repo(label: &str) -> Result<std::path::PathBuf, String> {
    let root = init_check_refusal_repo(label, 1)?;
    std::fs::write(root.join("ripr.toml"), "unkkey\n").map_err(|err| err.to_string())?;
    Ok(root)
}

#[test]
fn receipt_check_json_refuses_a_missing_receipt_and_a_bad_ledger() -> TestResult {
    let dir = unique_temp_workspace("receipt-refusal-15260");
    std::fs::create_dir_all(&dir)?;
    let missing = dir.join("absent-receipt.json");
    let missing_arg = missing.display().to_string();
    let value = document(
        &run_ripr(&["receipt", "check", "--json", "--path", &missing_arg]),
        2,
    )?;
    assert_eq!(value["status"], "refused", "{value}");
    assert_eq!(value["error"]["code"], "input_unusable", "{value}");
    assert_eq!(
        value["error"]["next"]["command"], "ripr receipt check --help",
        "{value}"
    );
    assert!(
        value["error"]["message"]
            .as_str()
            .is_some_and(|message| message.contains("not found or unreadable")),
        "{value}"
    );

    let receipt = dir.join("receipt.json");
    std::fs::write(
        &receipt,
        r#"{"schema_version":"0.1","tool":"ripr","kind":"receipt","canonical_gap_id":"gap-1","packet_id":null,"packet_id_available":false,"verify_command":"x","verify_status":"passed","current_head":null,"written_at":"2026-01-01T00:00:00Z","limits_note":"n"}"#,
    )?;
    let receipt_arg = receipt.display().to_string();
    let ledger_arg = dir.join("absent-ledger.json").display().to_string();
    let output = run_ripr(&[
        "receipt",
        "check",
        "--json",
        "--path",
        &receipt_arg,
        "--ledger",
        &ledger_arg,
    ]);
    let value = document(&output, 2)?;
    assert_eq!(value["error"]["code"], "input_unusable", "{value}");
    assert!(
        value["error"]["next"]["command"].is_string(),
        "a bad ledger must carry a next step: {value}"
    );

    // Without a JSON request stdout stays empty exactly as before.
    let plain = run_ripr(&["receipt", "check", "--path", &missing_arg]);
    assert_eq!(plain.status.code(), Some(2));
    assert!(plain.stdout.is_empty(), "{plain:?}");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn cache_status_json_refuses_a_root_that_is_not_a_directory() -> TestResult {
    let dir = unique_temp_workspace("cache-refusal-15260");
    let missing = dir.join("nonexistent");
    let missing_arg = missing.display().to_string();
    let value = document(
        &run_ripr(&["cache", "status", "--json", "--root", &missing_arg]),
        2,
    )?;
    assert_eq!(value["status"], "refused", "{value}");
    assert_eq!(value["error"]["code"], "input_unusable", "{value}");
    assert_eq!(value["error"]["token"], missing_arg.as_str(), "{value}");
    assert_eq!(
        value["error"]["next"]["command"], "ripr cache status --help",
        "{value}"
    );
    let plain = run_ripr(&["cache", "status", "--root", &missing_arg]);
    assert_eq!(plain.status.code(), Some(2));
    assert!(plain.stdout.is_empty(), "{plain:?}");
    Ok(())
}

#[test]
fn agent_status_json_refuses_a_malformed_attempt_id_with_the_listing_command() -> TestResult {
    let root = init_check_refusal_repo("attempt-id-15260", 1)?;
    let root_arg = root.display().to_string();
    let value = document(
        &run_ripr(&[
            "agent",
            "status",
            "--attempt",
            "nope",
            "--json",
            "--root",
            &root_arg,
        ]),
        2,
    )?;
    assert_eq!(value["error"]["code"], "invalid_value", "{value}");
    assert_eq!(value["error"]["token"], "nope", "{value}");
    let next = value["error"]["next"]["command"]
        .as_str()
        .unwrap_or_default();
    assert!(
        next.starts_with("ripr agent status --root ") && !next.contains("--attempt"),
        "{value}"
    );
    assert!(!next.contains("--store"), "{value}");

    // An explicit store is kept, so the listing reads the same inventory.
    let value = document(
        &run_ripr(&[
            "agent",
            "status",
            "--attempt",
            "nope",
            "--json",
            "--root",
            &root_arg,
            "--store",
            "target/ripr/alt-attempts",
        ]),
        2,
    )?;
    let next = value["error"]["next"]["command"]
        .as_str()
        .unwrap_or_default();
    assert!(
        next.starts_with("ripr agent status --root ")
            && next.ends_with(" --store target/ripr/alt-attempts"),
        "{value}"
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

#[test]
fn invalid_config_json_refusals_carry_the_human_next_step() -> TestResult {
    let root = broken_config_repo("config-refusal-15260")?;
    let root_arg = root.display().to_string();
    // The printed command spells the root with `/` on every platform (the
    // human-path rule), so on Windows it differs from the native argument.
    let printed_root = root_arg.replace('\\', "/");
    let validate = format!("ripr config validate --root {printed_root}");

    let check = document(&run_ripr(&["check", "--json", "--root", &root_arg]), 2)?;
    assert_eq!(check["error"]["code"], "config_invalid", "{check}");
    assert_eq!(
        check["error"]["next"]["command"],
        validate.as_str(),
        "{check}"
    );
    assert_eq!(check["error"]["next"]["purpose"], "follow_stated_step");
    let message = check["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("fix the error in ripr.toml and rerun"),
        "{message}"
    );
    assert!(
        !message.contains("unkkey"),
        "source text stays redacted: {message}"
    );

    let brief = document(
        &run_ripr(&[
            "agent", "brief", "--json", "--root", &root_arg, "--base", "HEAD",
        ]),
        2,
    )?;
    assert_eq!(
        brief["run_limitations"][0]["remedy"]["command"],
        validate.as_str(),
        "{brief}"
    );
    assert!(
        brief["run_limitations"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("fix the error in ripr.toml and rerun")),
        "{brief}"
    );

    let rerun = document(
        &run_ripr(&[
            "rerun",
            "--json",
            "--root",
            &root_arg,
            "--changed-test",
            "src/lib.rs",
        ]),
        2,
    )?;
    assert_eq!(
        rerun["run_limitations"][0]["remedy"]["command"],
        validate.as_str(),
        "{rerun}"
    );

    let cont = document(&run_ripr(&["continue", "--json", "--root", &root_arg]), 3)?;
    assert_eq!(cont["error"]["code"], "no_current_attempt", "{cont}");
    assert_eq!(
        cont["error"]["next"]["command"],
        format!("ripr status --root {printed_root}"),
        "{cont}"
    );
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// A root holding a backtick cannot round-trip through the backtick-delimited
/// prose the remedy parser reads, so the config step is named without a
/// runnable command and `next` stays null rather than a truncated command.
#[cfg(unix)]
#[test]
fn invalid_config_under_a_backticked_root_leaves_next_null() -> TestResult {
    let base = unique_temp_workspace("config-backtick-15260");
    let root = base.join("repo``name");
    std::fs::create_dir_all(&root)?;
    let seeded = broken_config_repo("config-backtick-seed-15260")?;
    std::fs::remove_dir_all(&root)?;
    std::fs::rename(&seeded, &root)?;
    let root_arg = root.display().to_string();
    let check = document(&run_ripr(&["check", "--json", "--root", &root_arg]), 2)?;
    assert!(check["error"]["next"].is_null(), "{check}");
    let message = check["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("fix the error in ripr.toml and rerun"),
        "{message}"
    );
    let _ = std::fs::remove_dir_all(&base);
    Ok(())
}

/// #15985: an empty command name used to publish `"token": ""`, which the
/// refusal schema rejects (`minLength: 1`). The document now carries no token
/// and the message says the name is empty.
#[test]
fn an_empty_command_name_json_refusal_carries_no_token() -> TestResult {
    let value = document(&run_ripr(&["", "--json"]), 2)?;
    assert_eq!(value["status"], "refused", "{value}");
    assert_eq!(value["error"]["code"], "unknown_command", "{value}");
    assert!(value["error"].get("token").is_none(), "{value}");
    assert_eq!(
        value["error"]["message"],
        "the command name is empty, which usually means an unset shell variable. Run `ripr --help` to list commands.",
        "{value}"
    );
    assert_eq!(value["error"]["next"]["command"], "ripr --help", "{value}");
    Ok(())
}

/// #15985: a word after `--version` is a stray positional, so a JSON request
/// gets the `unknown_argument` document naming it, and an empty word carries
/// no token. Without the request stdout stays empty and no version prints.
#[test]
fn a_word_after_version_json_refusal_names_the_word() -> TestResult {
    let value = document(&run_ripr(&["--version", "extra", "--json"]), 2)?;
    assert_eq!(value["status"], "refused", "{value}");
    assert_eq!(value["error"]["code"], "unknown_argument", "{value}");
    assert_eq!(value["error"]["token"], "extra", "{value}");
    assert_eq!(value["error"]["next"]["command"], "ripr --help", "{value}");

    let value = document(&run_ripr(&["--json", "--version", ""]), 2)?;
    assert_eq!(value["error"]["code"], "unknown_argument", "{value}");
    assert!(value["error"].get("token").is_none(), "{value}");

    let plain = run_ripr(&["--version", "extra"]);
    assert_eq!(plain.status.code(), Some(2));
    assert!(plain.stdout.is_empty(), "{plain:?}");
    Ok(())
}

/// #16072: the strict help routes refuse extra arguments, and a JSON request
/// gets the stray-argument document naming the first extra. Stderr keeps the
/// usage line. Without a JSON request stdout stays empty, and the accepted
/// `help --json` form still prints the discovery document, not a refusal.
#[test]
fn strict_help_routes_publish_the_stray_argument_document() -> TestResult {
    for (args, route, token) in [
        (
            &["--help", "--all", "extra", "--json"][..],
            "--help --all",
            "extra",
        ),
        (
            &["help", "--all", "extra", "--json"][..],
            "help --all",
            "extra",
        ),
        (&["help", "--json", "extra"][..], "help --json", "extra"),
        (&["help", "--json", "--quiet"][..], "help --json", "--quiet"),
        (&["-v", "help", "--json"][..], "help --json", "-v"),
        (
            &["help", "--json", "--verbose"][..],
            "help --json",
            "--verbose",
        ),
        // Devin on #16090: an extra ahead of the verbosity flag is the first
        // extra, so it is the one named.
        (
            &["help", "--json", "--quiet", "--verbose"][..],
            "help --json",
            "--quiet",
        ),
        (&["help", "-v", "--json", "extra"][..], "help --json", "-v"),
    ] {
        let output = run_ripr(args);
        let usage = format!(
            "ripr: usage: ripr {route} (this route accepts no other arguments); drop the extra arguments, or run `ripr --help` to list commands\n"
        );
        assert_eq!(String::from_utf8_lossy(&output.stderr), usage, "{args:?}");
        let value = document(&output, 2)?;
        assert_eq!(value["status"], "refused", "{args:?}: {value}");
        assert_eq!(
            value["error"]["code"], "unknown_argument",
            "{args:?}: {value}"
        );
        assert_eq!(value["error"]["token"], token, "{args:?}: {value}");
        assert_eq!(
            value["error"]["next"]["command"], "ripr --help",
            "{args:?}: {value}"
        );
    }

    let plain = run_ripr(&["--help", "--all", "extra"]);
    assert_eq!(plain.status.code(), Some(2));
    assert!(plain.stdout.is_empty(), "{plain:?}");

    let accepted = run_ripr(&["help", "--json"]);
    assert_eq!(accepted.status.code(), Some(0));
    let discovery: serde_json::Value =
        serde_json::from_slice(&accepted.stdout).map_err(|error| error.to_string())?;
    assert!(discovery["commands"].is_array(), "{discovery}");
    assert!(discovery.get("error").is_none(), "{discovery}");
    Ok(())
}
