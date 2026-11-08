//! Binds the parser's accepted keys, the published JSON schema, and the
//! `ripr.toml` reference in `docs/CONFIGURATION.md` to one another, so none of
//! them can drift (every config key documented, docs match code).

use super::*;
use serde_json::Value;
use std::collections::BTreeSet;

const SCHEMA: &str = include_str!("../../../../schemas/ripr/ripr-config.schema.json");
const CONFIG_DOC: &str = include_str!("../../../../docs/CONFIGURATION.md");

fn schema() -> Result<Value, String> {
    serde_json::from_str(SCHEMA).map_err(|err| format!("schema is not JSON: {err}"))
}

/// Every table in the schema as (TOML header, schema node). Array-of-tables
/// headers are written `[[name]]`.
fn tables<'a>(
    header: &str,
    node: &'a Value,
    out: &mut Vec<(String, &'a Value)>,
) -> Result<(), String> {
    out.push((header.to_string(), node));
    let properties = node
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{header} has no properties"))?;
    for (key, child) in properties {
        let child_header = if header.is_empty() {
            key.clone()
        } else {
            format!("{header}.{key}")
        };
        match child.get("type").and_then(Value::as_str) {
            Some("object") => tables(&child_header, child, out)?,
            Some("array") if child["items"]["type"] == "object" => {
                tables(&format!("{child_header}[]"), &child["items"], out)?;
            }
            _ => {}
        }
    }
    Ok(())
}

/// A TOML document that places `key = 1` inside the table.
fn document_with_key(header: &str, key: &str) -> String {
    match header {
        "" => format!("{key} = 1\n"),
        _ if header.ends_with("[]") => {
            format!("[[{}]]\n{key} = 1\n", header.trim_end_matches("[]"))
        }
        _ if header.contains("[]") => {
            // Nested under an array of tables: open the array row first.
            let (outer, inner) = header.split_once("[].").unwrap_or((header, ""));
            format!("[[{outer}]]\n[{outer}.{inner}]\n{key} = 1\n")
        }
        _ => format!("[{header}]\n{key} = 1\n"),
    }
}

fn parser_keys(header: &str) -> Result<BTreeSet<String>, String> {
    let error = match parse_config_diagnostic(&document_with_key(header, "zz_probe")) {
        Ok(_) => return Err(format!("{header}: probe key was accepted")),
        Err(diagnostic) => diagnostic.message,
    };
    // The structural message embeds the parser reason after the excerpt.
    let reason = error
        .lines()
        .find(|line| line.starts_with("unknown field `zz_probe`"))
        .ok_or_else(|| format!("{header}: no unknown-field reason in {error}"))?;
    Ok(suggest::expected_fields(reason)
        .into_iter()
        .map(str::to_string)
        .collect())
}

#[test]
fn schema_properties_equal_parser_accepted_keys_for_every_table() -> Result<(), String> {
    let schema = schema()?;
    let mut all = Vec::new();
    tables("", &schema, &mut all)?;
    assert!(
        all.len() >= 17,
        "schema walk found only {} tables",
        all.len()
    );
    for (header, node) in all {
        assert_eq!(
            node["additionalProperties"],
            Value::Bool(false),
            "{header} must reject unknown keys in the schema"
        );
        let declared: BTreeSet<String> = node["properties"]
            .as_object()
            .ok_or_else(|| format!("{header} has no properties"))?
            .keys()
            .cloned()
            .collect();
        assert_eq!(
            declared,
            parser_keys(&header)?,
            "schema and parser disagree on keys in table `{header}`"
        );
    }
    Ok(())
}

#[test]
fn known_keys_list_equals_every_key_name_in_the_schema() -> Result<(), String> {
    let schema = schema()?;
    let mut all = Vec::new();
    tables("", &schema, &mut all)?;
    let mut names = BTreeSet::new();
    for (_, node) in all {
        if let Some(properties) = node["properties"].as_object() {
            names.extend(properties.keys().cloned());
        }
    }
    let known: BTreeSet<String> = suggest::KNOWN_KEYS.iter().map(|k| k.to_string()).collect();
    assert_eq!(known, names, "suggest::KNOWN_KEYS drifted from the schema");
    Ok(())
}

#[test]
fn every_schema_leaf_key_is_documented_with_its_full_path() -> Result<(), String> {
    let schema = schema()?;
    let mut all = Vec::new();
    tables("", &schema, &mut all)?;
    let mut checked = 0;
    for (header, node) in all {
        for (key, child) in node["properties"].as_object().into_iter().flatten() {
            if child["type"] == "object" {
                continue;
            }
            let suffix = if child["items"]["type"] == "object" {
                "[]"
            } else {
                ""
            };
            let path = if header.is_empty() {
                format!("{key}{suffix}")
            } else {
                format!("{header}.{key}{suffix}")
            };
            assert!(
                CONFIG_DOC.contains(&format!("| `{path}` |")),
                "docs/CONFIGURATION.md has no reference row for `{path}`"
            );
            checked += 1;
        }
    }
    assert!(checked >= 60, "only {checked} keys checked");
    Ok(())
}

#[test]
fn documented_reference_rows_are_all_in_the_schema() -> Result<(), String> {
    let schema = schema()?;
    let mut all = Vec::new();
    tables("", &schema, &mut all)?;
    let mut paths = BTreeSet::new();
    for (header, node) in all {
        if !header.is_empty() {
            paths.insert(header.clone());
        }
        for key in node["properties"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, _)| k)
        {
            paths.insert(if header.is_empty() {
                key.clone()
            } else {
                format!("{header}.{key}")
            });
        }
    }
    let section = CONFIG_DOC
        .split("## `ripr.toml` reference")
        .nth(1)
        .and_then(|rest| rest.split("\n## ").next())
        .ok_or("reference section missing")?;
    for line in section.lines().filter(|line| line.starts_with("| `")) {
        let path = line
            .trim_start_matches("| `")
            .split('`')
            .next()
            .unwrap_or_default();
        assert!(
            paths.contains(path),
            "docs row `{path}` is not a key in the schema"
        );
    }
    Ok(())
}

#[test]
fn schema_enums_match_the_values_the_parser_accepts() -> Result<(), String> {
    let schema = schema()?;
    let props = &schema["properties"];
    let enums: [(&str, &str, &Value); 5] = [
        (
            "analysis",
            "mode",
            &props["analysis"]["properties"]["mode"]["enum"],
        ),
        (
            "oracles",
            "snapshot_strength",
            &props["oracles"]["properties"]["snapshot_strength"]["enum"],
        ),
        (
            "severity.findings",
            "exposed",
            &props["severity"]["properties"]["findings"]["properties"]["exposed"]["enum"],
        ),
        (
            "severity.seams",
            "opaque",
            &props["severity"]["properties"]["seams"]["properties"]["opaque"]["enum"],
        ),
        (
            "lsp",
            "diagnostic_profile",
            &props["lsp"]["properties"]["diagnostic_profile"]["enum"],
        ),
    ];
    for (table, key, values) in enums {
        let values = values
            .as_array()
            .ok_or(format!("{table}.{key} has no enum"))?;
        for value in values {
            let text = value.as_str().ok_or("enum value is not a string")?;
            parse_config(&format!("[{table}]\n{key} = \"{text}\"\n"))
                .map_err(|err| format!("{table}.{key} = {text} rejected: {err}"))?;
        }
        assert!(
            parse_config(&format!("[{table}]\n{key} = \"zz_not_a_value\"\n")).is_err(),
            "{table}.{key} accepted a value outside the schema enum"
        );
    }
    Ok(())
}

#[test]
fn typo_in_a_key_suggests_the_nearest_key_in_the_same_table() {
    let error = parse_config("[analysis]\nmodee = \"draft\"\n")
        .err()
        .unwrap_or_default();
    assert!(error.contains("did you mean `mode`?"), "{error}");
    let error = parse_config("[severity.findings]\nweakly_expose = \"info\"\n")
        .err()
        .unwrap_or_default();
    assert!(error.contains("did you mean `weakly_exposed`?"), "{error}");
}

#[test]
fn unrelated_unknown_key_gets_no_suggestion() {
    let error = parse_config("[analysis]\nzzzzzzzz = 1\n")
        .err()
        .unwrap_or_default();
    assert!(error.contains("unknown field `zzzzzzzz`"), "{error}");
    assert!(!error.contains("did you mean"), "{error}");
}

#[test]
fn suggestion_survives_the_source_free_summary_but_a_forged_one_does_not() {
    let error = parse_config("[analysis]\nmodee = \"draft\"\n")
        .err()
        .unwrap_or_default();
    assert!(config_error_summary(&error).contains("did you mean `mode`?"));
    let forged = "invalid ripr.toml: bad\ndid you mean `secret_value`?";
    assert!(!config_error_summary(forged).contains("secret_value"));
}

#[test]
fn wrong_type_names_the_key_and_the_expected_type() {
    let error = parse_config("[analysis]\ninclude_unchanged_tests = \"yes\"\n")
        .err()
        .unwrap_or_default();
    assert!(error.contains("include_unchanged_tests"), "{error}");
    assert!(error.contains("invalid type"), "{error}");
}

#[test]
fn shipped_example_config_parses() -> Result<(), String> {
    let example = include_str!("../../../../ripr.toml.example");
    parse_config(example).map(|_| ())
}
