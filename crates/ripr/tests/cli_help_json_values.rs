//! `ripr help --json` closed value sets agree with the parser (#14294;
//! RIPR-SPEC-0190).
//!
//! A flag's `values` tells a script or agent which spellings are accepted
//! without probing. The catalog builds that list beside the parser, not from
//! it, so this test asks the real binary: a value outside the set must be
//! refused with exactly the catalog's list.

use std::collections::BTreeSet;
use std::process::{Command, Stdio};

fn run_ripr(args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run ripr {args:?}: {error}"))
}

fn catalog() -> Result<serde_json::Value, String> {
    let output = run_ripr(&["help", "--json"])?;
    if !output.status.success() {
        return Err(format!("help --json failed: {:?}", output.status));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("invalid JSON: {error}"))
}

fn flag<'a>(
    document: &'a serde_json::Value,
    path: &str,
    name: &str,
) -> Result<&'a serde_json::Value, String> {
    document["commands"]
        .as_array()
        .ok_or("commands missing")?
        .iter()
        .find(|row| row["path"] == path)
        .ok_or_else(|| format!("no catalog row for {path}"))?["flags"]
        .as_array()
        .ok_or_else(|| format!("{path}: flags missing"))?
        .iter()
        .find(|flag| flag["name"] == name)
        .ok_or_else(|| format!("{path}: no flag {name}"))
}

/// The list after `Accepted:` in a refusal, up to the closing period.
fn accepted_list(stderr: &str) -> Option<BTreeSet<String>> {
    let rest = stderr.split_once("Accepted: ")?.1;
    let list = rest.split_once('.').map_or(rest, |(list, _)| list);
    Some(
        list.split(", ")
            .map(|value| value.trim().to_string())
            .collect(),
    )
}

/// `check --format` answers a wrong value with its common formats and a
/// pointer to `--help --all`, not the full list of 29, so it has no
/// `Accepted:` list to compare. Its catalog values come from
/// `OutputFormat::accepted_cli_names` and are pinned by a unit test.
const NO_ACCEPTED_LIST: &[(&str, &str)] = &[("check", "--format")];

#[test]
fn a_closed_value_set_is_exactly_what_the_parser_accepts() -> Result<(), String> {
    let document = catalog()?;
    let commands = document["commands"].as_array().ok_or("commands missing")?;
    let mut compared = Vec::new();
    for row in commands {
        if row["flags_scope"] != "own_help_body" {
            continue;
        }
        let path = row["path"].as_str().ok_or("path missing")?;
        for flag in row["flags"].as_array().ok_or("flags missing")? {
            let name = flag["name"].as_str().ok_or("flag name missing")?;
            let values: BTreeSet<String> = flag["values"]
                .as_array()
                .ok_or("values missing")?
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect();
            if values.is_empty() || NO_ACCEPTED_LIST.contains(&(path, name)) {
                continue;
            }
            if flag["takes_value"] != true {
                return Err(format!(
                    "{path} {name}: lists values but takes_value is not true"
                ));
            }
            let mut args: Vec<&str> = path.split(' ').collect();
            args.extend([name, "zz-not-a-value"]);
            let output = run_ripr(&args)?;
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.code() != Some(2) || !stderr.contains("\"zz-not-a-value\"") {
                return Err(format!(
                    "{path} {name}: a value outside the set was not refused by name: {:?} {stderr}",
                    output.status
                ));
            }
            let accepted = accepted_list(&stderr)
                .ok_or_else(|| format!("{path} {name}: refusal has no Accepted list: {stderr}"))?;
            if accepted != values {
                return Err(format!(
                    "{path} {name}: catalog values {values:?} differ from the parser's {accepted:?}"
                ));
            }
            compared.push(format!("{path} {name}"));
        }
    }
    // The comparison must keep its subjects: these are the closed sets the
    // catalog carries today.
    for expected in [
        "agent repair --phase",
        "check --mode",
        "init --agents",
        "init --ci",
        "pilot --format",
    ] {
        if !compared.iter().any(|subject| subject == expected) {
            return Err(format!(
                "{expected} was not compared; compared: {compared:?}"
            ));
        }
    }
    Ok(())
}

#[test]
fn agent_repair_phase_and_init_presets_carry_their_values() -> Result<(), String> {
    let document = catalog()?;
    let phase = flag(&document, "agent repair", "--phase")?;
    assert_eq!(phase["takes_value"], true, "{phase}");
    assert_eq!(
        phase["values"],
        serde_json::json!(["before", "after", "verify"])
    );
    let ci = flag(&document, "init", "--ci")?;
    assert_eq!(ci["values"], serde_json::json!(["github", "gitlab"]));
    let agents = flag(&document, "init", "--agents")?;
    assert_eq!(
        agents["values"],
        serde_json::json!(["claude", "codex", "cursor", "all"])
    );
    Ok(())
}
