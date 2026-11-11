//! A fail-closed checker for the JSON Schema keywords the small published
//! schemas use, for integration tests that validate bytes the real `ripr`
//! binary wrote.
//!
//! Wire it per target with
//! `#[path = "common/published_schema.rs"] mod published_schema;`.
//!
//! It is deliberately not a general validator. `unsupported_keywords` reports
//! every keyword a schema uses that `check` does not evaluate, and a target
//! must assert that list is empty, so a schema that grows a keyword cannot be
//! under-checked silently. `cargo xtask check-verification-contracts` runs the
//! repository's full validator over committed fixtures; this module is the
//! producer-byte side.

use serde_json::{Map, Value};
use std::path::Path;

/// Read one published schema by its repository-relative path.
pub fn read(repo_relative: &str) -> Result<Value, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(repo_relative);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

/// Check `value` against `schema`, resolving `$ref` against `root`.
pub fn check(value: &Value, schema: &Value, root: &Value, location: &str) -> Result<(), String> {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        let pointer = reference.strip_prefix('#').ok_or_else(|| {
            format!("{location}: only local references are supported, got {reference}")
        })?;
        let resolved = root
            .pointer(pointer)
            .ok_or_else(|| format!("{location}: unresolved schema reference {reference}"))?;
        // `$ref` does not suppress its siblings; they are checked below.
        check(value, resolved, root, location)?;
    }
    if let Some(expected) = schema.get("const")
        && value != expected
    {
        return Err(format!(
            "{location}: expected const {expected}, got {value}"
        ));
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.iter().any(|candidate| candidate == value)
    {
        return Err(format!(
            "{location}: {value} is not one of the {} allowed values",
            allowed.len()
        ));
    }
    if let Some(expected) = schema.get("type") {
        check_type(value, expected, location)?;
    }
    for branch in schema
        .get("allOf")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        check(value, branch, root, location)?;
    }
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        let matched = branches
            .iter()
            .filter(|branch| check(value, branch, root, location).is_ok())
            .count();
        if matched != 1 {
            return Err(format!(
                "{location}: matched {matched} of {} oneOf branches, expected exactly 1",
                branches.len()
            ));
        }
    }
    if let Some(condition) = schema.get("if") {
        let branch = if check(value, condition, root, location).is_ok() {
            "then"
        } else {
            "else"
        };
        if let Some(consequence) = schema.get(branch) {
            check(value, consequence, root, location)
                .map_err(|error| format!("{error} (in the `{branch}` branch)"))?;
        }
    }
    if let Some(number) = value.as_i64()
        && let Some(minimum) = schema.get("minimum").and_then(Value::as_i64)
        && number < minimum
    {
        return Err(format!("{location}: {number} is below minimum {minimum}"));
    }
    if let Some(object) = value.as_object() {
        check_object(object, schema, root, location)?;
    }
    if let Some(items) = value.as_array() {
        check_array(items, schema, root, location)?;
    }
    if let Some(text) = value.as_str() {
        check_string(text, schema, location)?;
    }
    Ok(())
}

fn check_type(value: &Value, expected: &Value, location: &str) -> Result<(), String> {
    let names: Vec<&str> = match expected {
        Value::String(name) => vec![name.as_str()],
        Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
        _ => {
            return Err(format!(
                "{location}: unsupported `type` declaration {expected}"
            ));
        }
    };
    for name in &names {
        let matched = match *name {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "integer" => value.is_i64() || value.is_u64(),
            "number" => value.is_number(),
            other => {
                return Err(format!("{location}: unsupported `type` value `{other}`"));
            }
        };
        if matched {
            return Ok(());
        }
    }
    Err(format!(
        "{location}: {value} is not of type {}",
        names.join(" or ")
    ))
}

fn check_object(
    object: &Map<String, Value>,
    schema: &Value,
    root: &Value,
    location: &str,
) -> Result<(), String> {
    let empty = Map::new();
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let field = required
            .as_str()
            .ok_or_else(|| format!("{location}: schema required entries must be strings"))?;
        if !object.contains_key(field) {
            return Err(format!("{location}: missing required field `{field}`"));
        }
    }
    match schema.get("additionalProperties") {
        None => {}
        Some(Value::Bool(false)) => {
            for field in object.keys() {
                if !properties.contains_key(field) {
                    return Err(format!(
                        "{location}: emitted `{field}`, which the published schema does not declare"
                    ));
                }
            }
        }
        Some(other) => {
            return Err(format!(
                "{location}: unsupported `additionalProperties` value {other}"
            ));
        }
    }
    for (field, declared) in properties {
        if let Some(emitted) = object.get(field) {
            check(emitted, declared, root, &format!("{location}.{field}"))?;
        }
    }
    Ok(())
}

fn check_array(
    items: &[Value],
    schema: &Value,
    root: &Value,
    location: &str,
) -> Result<(), String> {
    if let Some(item_schema) = schema.get("items") {
        for (index, item) in items.iter().enumerate() {
            check(item, item_schema, root, &format!("{location}[{index}]"))?;
        }
    }
    let count = items.len() as u64;
    if let Some(minimum) = schema.get("minItems").and_then(Value::as_u64)
        && count < minimum
    {
        return Err(format!(
            "{location}: {count} items is below minItems {minimum}"
        ));
    }
    if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
        for (index, item) in items.iter().enumerate() {
            if items.iter().take(index).any(|earlier| earlier == item) {
                return Err(format!("{location}[{index}]: duplicate item {item}"));
            }
        }
    }
    Ok(())
}

fn check_string(text: &str, schema: &Value, location: &str) -> Result<(), String> {
    if let Some(minimum) = schema.get("minLength").and_then(Value::as_u64)
        && (text.chars().count() as u64) < minimum
    {
        return Err(format!("{location}: shorter than minLength {minimum}"));
    }
    if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
        match pattern {
            SHA256_DIGEST_PATTERN => {
                let hex = text.strip_prefix("sha256:").unwrap_or("");
                let lowercase_hex = hex
                    .chars()
                    .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c));
                if hex.len() != 64 || !lowercase_hex {
                    return Err(format!("{location}: {text:?} does not match {pattern}"));
                }
            }
            other => {
                return Err(format!(
                    "{location}: unsupported `pattern` {other:?}; this checker only evaluates {SHA256_DIGEST_PATTERN:?}"
                ));
            }
        }
    }
    Ok(())
}

/// The one regular expression this checker evaluates. Any other `pattern` is
/// an error at check time, never an assumed match.
pub const SHA256_DIGEST_PATTERN: &str = "^sha256:[0-9a-f]{64}$";

const SUPPORTED_KEYWORDS: &[&str] = &[
    "$defs",
    "$id",
    "$ref",
    "$schema",
    "additionalProperties",
    "allOf",
    "const",
    "description",
    "else",
    "enum",
    "if",
    "items",
    "minItems",
    "minLength",
    "minimum",
    "oneOf",
    "pattern",
    "properties",
    "required",
    "then",
    "title",
    "type",
    "uniqueItems",
];

/// Every keyword `schema` uses that `check` does not evaluate, in document
/// order. Only schema positions are walked: a `properties` or `$defs` map
/// holds names beside schemas, so its keys are never read as keywords.
pub fn unsupported_keywords(schema: &Value) -> Vec<String> {
    let mut found = Vec::new();
    collect_unsupported(schema, &mut found);
    found
}

fn collect_unsupported(schema: &Value, found: &mut Vec<String>) {
    let Some(object) = schema.as_object() else {
        return;
    };
    for (keyword, nested) in object {
        match keyword.as_str() {
            "properties" | "$defs" => {
                for member in nested.as_object().into_iter().flat_map(Map::values) {
                    collect_unsupported(member, found);
                }
            }
            "items" | "if" | "then" | "else" => collect_unsupported(nested, found),
            "allOf" | "oneOf" => {
                for member in nested.as_array().into_iter().flatten() {
                    collect_unsupported(member, found);
                }
            }
            other if SUPPORTED_KEYWORDS.contains(&other) => {}
            other => found.push(other.to_string()),
        }
    }
}
