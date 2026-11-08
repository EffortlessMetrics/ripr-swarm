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
    // `checked` counts own-body rows; `discriminating` counts those whose
    // body documents a JSON flag, the only rows where a wrong declared
    // spelling can be caught. Body-less and shared-body rows are not counted.
    let mut checked = 0;
    let mut discriminating = 0;
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
        if names.iter().any(|name| name == "--json") || format_accepts_json(row) {
            discriminating += 1;
        }
    }
    assert!(checked >= 35, "only {checked} own-body rows were checked");
    assert!(
        discriminating >= 15,
        "only {discriminating} rows had a documented JSON flag to discriminate on"
    );
    Ok(())
}

/// `writes_json_artifacts` is independent of `accepts_json_flag`: a command
/// may print JSON on stdout, write JSON files, both, or neither. Pin one real
/// row of each quadrant so the definition cannot drift to "file-only".
#[test]
fn writes_json_artifacts_is_independent_of_the_stdout_json_flag() -> Result<(), String> {
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let row = |path: &str| {
        commands
            .iter()
            .find(|row| row["path"] == path)
            .ok_or_else(|| format!("{path} missing from help --json"))
    };
    // Both: stdout JSON mode and artifact files.
    for (path, spelling) in [("check", "--json"), ("pilot", "--format json")] {
        let r = row(path)?;
        assert_eq!(r["accepts_json_flag"], spelling, "{path}");
        assert_eq!(r["writes_json_artifacts"], true, "{path}");
    }
    // Every row carrying both fields is reported with both populated.
    let both = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_string() && r["writes_json_artifacts"] == true)
        .count();
    assert!(both >= 6, "expected at least 6 rows with both, got {both}");
    // Artifacts only: writes files, accepts no stdout JSON flag.
    let artifact_only = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_null() && r["writes_json_artifacts"] == true)
        .count();
    assert!(artifact_only > 0, "no artifact-only row is reported");
    // Stdout JSON only: prints JSON, writes no artifact files.
    let stdout_only = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_string() && r["writes_json_artifacts"] == false)
        .count();
    assert!(stdout_only > 0, "no stdout-JSON-only row is reported");
    // Neither: the metadata is not all-true.
    let neither = commands
        .iter()
        .filter(|r| r["accepts_json_flag"].is_null() && r["writes_json_artifacts"] == false)
        .count();
    assert!(neither > 0, "no row reports neither");
    Ok(())
}

/// Body-less rows are labelled so an empty `flags` list is not read as "no
/// flags documented".
#[test]
fn rows_without_a_help_body_are_scoped_no_help_body() -> Result<(), String> {
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let agent = commands
        .iter()
        .find(|row| row["path"] == "agent")
        .ok_or("agent row missing")?;
    assert_eq!(agent["flags_scope"], "no_help_body");
    assert_eq!(agent["flags"], serde_json::json!([]));
    let no_body = commands
        .iter()
        .filter(|row| row["flags_scope"] == "no_help_body")
        .count();
    assert!(no_body >= 10, "expected body-less rows, got {no_body}");
    Ok(())
}
