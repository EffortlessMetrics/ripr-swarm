//! The published `ripr help --json` schema, checked against the bytes the
//! real command writes.
//!
//! `schemas/ripr/help.schema.json` describes the command and workflow
//! discovery document. The document is built from static catalog tables and
//! is byte-stable, so one run reads every command row, flag and workflow; the
//! coverage test below asserts that run reaches each exit-contract kind, the
//! alias relation and every flag scope, so no branch of the schema is
//! "covered" by a document that skipped it.

use serde_json::{Value, json};
use std::collections::BTreeSet;

#[path = "common/published_schema.rs"]
mod published_schema;

const HELP_SCHEMA: &str = "schemas/ripr/help.schema.json";

fn help_document() -> Result<Value, String> {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["help", "--json"])
        .output()
        .map_err(|error| format!("spawn ripr help --json: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ripr help --json: exit {:?}; stderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "help --json did not parse: {error}\nstdout:\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn conforms(document: &Value, schema: &Value, label: &str) -> Result<(), String> {
    published_schema::check(document, schema, schema, &format!("help[{label}]"))
}

/// The distinct values of `field` across the command rows.
fn command_values(document: &Value, pointer: &str) -> BTreeSet<String> {
    document["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| row.pointer(pointer))
        .map(|value| {
            value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_string)
        })
        .collect()
}

#[test]
fn the_checker_covers_every_keyword_the_help_schema_uses() -> Result<(), String> {
    let schema = published_schema::read(HELP_SCHEMA)?;
    let ignored = published_schema::unsupported_keywords(&schema);
    if ignored.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "the conformance checker ignores these keywords of {HELP_SCHEMA}, so it would under-check the schema: {ignored:?}"
        ))
    }
}

/// The real document conforms, and it reaches every exit-contract kind,
/// both live relation kinds, every flag scope, a flag with a closed value
/// set, a flag with an environment twin, and every workflow next-route kind.
#[test]
fn help_json_conforms_and_reaches_every_branch() -> Result<(), String> {
    let schema = published_schema::read(HELP_SCHEMA)?;
    let document = help_document()?;
    conforms(&document, &schema, "real")?;
    let flags = document["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| row["flags"].as_array().cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    let next_kinds = document["workflows"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|row| {
            row["result_families"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .filter_map(|family| family["next"]["kind"].as_str().map(str::to_string))
        .collect::<BTreeSet<_>>();
    let observed = json!({
        "exit": command_values(&document, "/exit/kind"),
        "relation": command_values(&document, "/relation/kind"),
        "flags_scope": command_values(&document, "/flags_scope"),
        "closed_values": flags.iter().any(|flag| flag["values"].as_array().is_some_and(|values| !values.is_empty())),
        "env_twin": flags.iter().any(|flag| flag["env"].is_string()),
        "next": next_kinds,
    });
    let expected = json!({
        "exit": ["advisory_findings", "completed_or_failed", "decision_or_refusal",
                 "doctor_findings", "gate_evaluate", "receipt_check"],
        "relation": ["alias_of", "canonical"],
        "flags_scope": ["leaf_section", "no_help_body", "own_help_body", "shared_help_body"],
        "closed_values": true,
        "env_twin": true,
        "next": ["command", "limitation", "stop"],
    });
    if observed == expected {
        Ok(())
    } else {
        Err(format!("observed {observed}, expected {expected}"))
    }
}

/// Index of the first command row whose `pointer` equals `value`.
fn row_where(document: &Value, pointer: &str, value: &Value) -> Result<usize, String> {
    document["commands"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .position(|row| row.pointer(pointer) == Some(value))
        })
        .ok_or(format!("no command row with {pointer} = {value}"))
}

/// The negative experiment: the schema rejects the real document once one
/// contract term is broken.
#[test]
fn published_help_schema_rejects_documents_it_forbids() -> Result<(), String> {
    type Mutation = fn(&mut Value, &Rows);
    let schema = published_schema::read(HELP_SCHEMA)?;
    let document = help_document()?;
    conforms(&document, &schema, "real")?;
    let rows = Rows {
        public: row_where(&document, "/class", &json!("public"))?,
        alias: row_where(&document, "/relation/kind", &json!("alias_of"))?,
        gate: row_where(&document, "/exit/kind", &json!("gate_evaluate"))?,
        refusal: row_where(&document, "/exit/kind", &json!("decision_or_refusal"))?,
        no_body: row_where(&document, "/flags_scope", &json!("no_help_body"))?,
        with_flags: document["commands"]
            .as_array()
            .and_then(|commands| {
                commands.iter().position(|row| {
                    row["flags"]
                        .as_array()
                        .is_some_and(|flags| flags.iter().any(|flag| flag["takes_value"] == false))
                })
            })
            .ok_or("no command with a boolean flag")?,
    };
    let cases: [(&str, (&str, &str), Mutation); 16] = [
        (
            "an unknown top-level field",
            (
                "help[an unknown top-level field]",
                "emitted `ok`, which the published schema does not declare",
            ),
            |doc, _| doc["ok"] = json!(true),
        ),
        (
            "the version as a string",
            (".schema_version", "expected const 5"),
            |doc, _| doc["schema_version"] = json!("5"),
        ),
        (
            "another version",
            (".schema_version", "expected const 5"),
            |doc, _| doc["schema_version"] = json!(6),
        ),
        (
            "a short catalog digest",
            (".catalog_digest", "does not match ^[0-9a-f]{64}$"),
            |doc, _| doc["catalog_digest"] = json!("43addde4"),
        ),
        (
            "an uppercase catalog digest",
            (".catalog_digest", "does not match ^[0-9a-f]{64}$"),
            |doc, _| {
                let upper = doc["catalog_digest"].as_str().unwrap_or("").to_uppercase();
                doc["catalog_digest"] = json!(upper);
            },
        ),
        (
            "no commands",
            (".commands", "below minItems 1"),
            |doc, _| doc["commands"] = json!([]),
        ),
        (
            "an unknown command class",
            (".class", "is not one of"),
            |doc, rows| doc["commands"][rows.public]["class"] = json!("beta"),
        ),
        (
            "a public canonical command shown as advanced",
            (".discovery", "expected const \"ordinary_public\""),
            |doc, rows| doc["commands"][rows.public]["discovery"] = json!("advanced"),
        ),
        (
            "an alias shown as ordinary public",
            (".discovery", "expected const \"compatibility_alias\""),
            |doc, rows| doc["commands"][rows.alias]["discovery"] = json!("ordinary_public"),
        ),
        (
            "an alias with no target",
            (".relation", "oneOf"),
            |doc, rows| doc["commands"][rows.alias]["relation"] = json!({"kind": "alias_of"}),
        ),
        (
            "a gate that blocks with exit 2",
            (".exit", "oneOf"),
            |doc, rows| doc["commands"][rows.gate]["exit"]["blocked"] = json!(2),
        ),
        (
            "an unknown refusal stdout",
            (".exit", "oneOf"),
            |doc, rows| {
                doc["commands"][rows.refusal]["exit"]["stdout_on_refusal"] = json!("stderr")
            },
        ),
        (
            "flags on a command with no help body",
            (".flags", "above maxItems 0"),
            |doc, rows| doc["commands"][rows.no_body]["flags"] = json!([{"name": "--root", "takes_value": true, "values": [], "required": false, "repeatable": false, "env": null}]),
        ),
        (
            "a boolean flag with a value set",
            (".values", "above maxItems 0"),
            |doc, rows| {
                if let Some(flag) = doc["commands"][rows.with_flags]["flags"]
                    .as_array_mut()
                    .and_then(|flags| flags.iter_mut().find(|flag| flag["takes_value"] == false))
                {
                    flag["values"] = json!(["yes"]);
                }
            },
        ),
        (
            "a flag that is not a long flag",
            (".name", "does not match ^--[a-z]"),
            |doc, rows| doc["commands"][rows.with_flags]["flags"][0]["name"] = json!("-q"),
        ),
        (
            "a workflow with no steps",
            (".steps", "below minItems 1"),
            |doc, _| doc["workflows"][0]["steps"] = json!([]),
        ),
    ];
    // Each mutation must be refused at the field it changed and for the
    // constraint it breaks, so an unrelated rejection cannot pass it.
    for (label, (location, reason), mutate) in cases {
        let mut mutated = document.clone();
        mutate(&mut mutated, &rows);
        if mutated == document {
            return Err(format!("{label}: the mutation changed nothing"));
        }
        match conforms(&mutated, &schema, label) {
            Ok(()) => {
                return Err(format!(
                    "the published help schema accepted a document the contract forbids: {label}"
                ));
            }
            Err(error) => {
                let at_field = error
                    .split(": ")
                    .next()
                    .is_some_and(|path| path.ends_with(location));
                if !at_field || !error.contains(reason) {
                    return Err(format!(
                        "{label}: rejected for another reason; expected `{reason}` at `{location}`, got: {error}"
                    ));
                }
                println!("rejected as expected ({label}): {error}");
            }
        }
    }
    Ok(())
}

struct Rows {
    public: usize,
    alias: usize,
    gate: usize,
    refusal: usize,
    no_body: usize,
    with_flags: usize,
}
