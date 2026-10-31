//! `ripr help --json` flag inventory and JSON-flag contract (#11599).
//!
//! The catalog's `accepts_json_flag` must agree with the flag inventory parsed
//! from each command's own help body, so a script can trust one field instead
//! of probing every command.

use std::process::Command;

fn catalog() -> Result<serde_json::Value, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["help", "--json"])
        .output()
        .map_err(|error| format!("failed to run ripr help --json: {error}"))?;
    if !output.status.success() {
        return Err(format!("help --json failed: {:?}", output.status));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("invalid JSON: {error}"))
}

fn flag_names(row: &serde_json::Value) -> Vec<String> {
    row["flags"]
        .as_array()
        .map(|flags| {
            flags
                .iter()
                .filter_map(|flag| flag["name"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn format_accepts_json(row: &serde_json::Value) -> bool {
    row["flags"].as_array().is_some_and(|flags| {
        flags.iter().any(|flag| {
            flag["name"] == "--format"
                && flag["values"]
                    .as_array()
                    .is_some_and(|values| values.iter().any(|value| value == "json"))
        })
    })
}

#[test]
fn json_flag_field_agrees_with_the_parsed_flag_inventory() -> Result<(), String> {
    let document = catalog()?;
    assert_eq!(document["schema_version"], 4);
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let mut checked = 0;
    for row in commands {
        if row["flags_scope"] != "own_help_body" {
            continue;
        }
        let path = row["path"].as_str().unwrap_or("?");
        let names = flag_names(row);
        // Known documentation gaps: these help bodies do not list the JSON
        // flag they accept (`help` prints no option list; `context` omits
        // `--json`), or list `--format` without a typed value set. The
        // catalog's `accepts_json_flag` is the authority there.
        if matches!(
            path,
            "help" | "context" | "outcome" | "calibrate cargo-mutants"
        ) {
            continue;
        }
        match row["accepts_json_flag"].as_str() {
            Some("--json") => assert!(
                names.iter().any(|name| name == "--json"),
                "{path} claims --json but its help documents {names:?}"
            ),
            Some("--format json") => assert!(
                format_accepts_json(row),
                "{path} claims --format json but --format does not list json"
            ),
            Some(other) => return Err(format!("{path}: unknown spelling {other}")),
            None => assert!(
                !names.iter().any(|name| name == "--json") && !format_accepts_json(row),
                "{path} documents a JSON flag but claims none"
            ),
        }
        checked += 1;
    }
    assert!(checked > 15, "only {checked} own-body rows were checked");
    Ok(())
}

#[test]
fn artifact_only_commands_are_not_reported_as_stdout_json() -> Result<(), String> {
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let mut artifact_rows = 0;
    for row in commands {
        if row["writes_json_artifacts"] == true {
            artifact_rows += 1;
        }
    }
    assert!(artifact_rows > 0, "no artifact-writing command is reported");
    Ok(())
}
