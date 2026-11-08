//! Base-ref compatibility check for published JSON Schemas (#1720).
//!
//! `check-verification-contracts` proves each schema's pinned version agrees
//! with its producer and the docs table. It cannot notice a schema that got
//! *stricter* for existing consumers while keeping the same version. This
//! module compares every published schema in the working tree with the same
//! file at the merge base and reports breaking edits that did not bump
//! `properties.schema_version.const`.
//!
//! The comparison is deliberately conservative: when it cannot prove an edit
//! is a widening, it calls it breaking. A version bump is the explicit,
//! reviewable way to accept a breaking change. Annotation keywords
//! (`description`, `title`, `examples`, ...) never count.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

const ANNOTATIONS: &[&str] = &[
    "description",
    "title",
    "$comment",
    "examples",
    "default",
    "deprecated",
    "$schema",
    "$id",
    "readOnly",
    "writeOnly",
];

/// Outcome of comparing one schema against its base.
#[derive(Debug, PartialEq)]
pub(crate) enum Compat {
    /// Nothing breaking, or breaking edits accepted by a version bump.
    Compatible { bumped: bool, breaking: usize },
    /// Breaking edits with no version bump.
    Violations(Vec<String>),
}

/// Compare `old` to `new` and decide whether the edit is acceptable.
pub(crate) fn assess(old: &Value, new: &Value) -> Compat {
    let mut breaking = Vec::new();
    compare(old, new, "#", &mut breaking);
    // Only a changed pin counts; deleting the pin is not a bump.
    let bumped = matches!(
        (version_const(old), version_const(new)),
        (Some(o), Some(n)) if o != n
    );
    if breaking.is_empty() || bumped {
        Compat::Compatible {
            bumped,
            breaking: breaking.len(),
        }
    } else {
        Compat::Violations(breaking)
    }
}

fn version_const(schema: &Value) -> Option<&Value> {
    schema.pointer("/properties/schema_version/const")
}

fn type_set(value: &Value) -> BTreeSet<String> {
    match value {
        Value::String(name) => BTreeSet::from([name.clone()]),
        Value::Array(items) => items
            .iter()
            .filter_map(|item| item.as_str().map(str::to_string))
            .collect(),
        _ => BTreeSet::new(),
    }
}

/// Copy of `value` with annotation keywords removed at every depth, so
/// verbatim comparisons ignore wording-only edits.
fn strip_annotations(value: &Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !ANNOTATIONS.contains(&key.as_str()))
                .map(|(key, value)| (key.clone(), strip_annotations(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_annotations).collect()),
        other => other.clone(),
    }
}

/// `true`, absent, or an object schema with no constraining keyword.
fn accepts_everything(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Bool(true)) => true,
        Some(Value::Object(map)) => map.keys().all(|key| ANNOTATIONS.contains(&key.as_str())),
        _ => false,
    }
}

fn number(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

fn compare(old: &Value, new: &Value, path: &str, out: &mut Vec<String>) {
    // Boolean schemas: `true` accepts everything, `false` nothing.
    match (old, new) {
        (Value::Bool(true), _) | (_, Value::Bool(true)) if old != new => {
            if *new == Value::Bool(true) {
                return;
            }
            out.push(format!("{path}: schema narrowed from `true`"));
            return;
        }
        (Value::Object(_), Value::Object(_)) => {}
        _ if old == new => return,
        _ => {
            out.push(format!("{path}: schema shape changed"));
            return;
        }
    }
    let (Some(old), Some(new)) = (old.as_object(), new.as_object()) else {
        return;
    };

    // type: narrowing the accepted set, or adding a type constraint.
    match (old.get("type"), new.get("type")) {
        (Some(o), Some(n)) => {
            let removed: Vec<_> = type_set(o).difference(&type_set(n)).cloned().collect();
            let widened_int = type_set(o).contains("integer")
                && type_set(n).contains("number")
                && removed == ["integer"];
            if !removed.is_empty() && !widened_int {
                out.push(format!("{path}: type no longer accepts {removed:?}"));
            }
        }
        (None, Some(_)) => out.push(format!("{path}: type constraint added")),
        _ => {}
    }

    // enum / const.
    match (old.get("enum"), new.get("enum")) {
        (Some(Value::Array(o)), Some(Value::Array(n))) => {
            for value in o.iter().filter(|value| !n.contains(value)) {
                out.push(format!("{path}: enum value {value} removed"));
            }
        }
        // `const: c` widening to an `enum` that still contains `c` is fine.
        (None, Some(Value::Array(n))) if old.get("const").is_some_and(|c| n.contains(c)) => {}
        (None, Some(_)) => out.push(format!("{path}: enum constraint added")),
        _ => {}
    }
    match (old.get("const"), new.get("const")) {
        // The top-level version pin itself is the sanctioned change signal.
        (Some(o), Some(n)) if o != n && path != "#/properties/schema_version" => {
            out.push(format!("{path}: const changed from {o} to {n}"));
        }
        (None, Some(_)) => out.push(format!("{path}: const constraint added")),
        _ => {}
    }

    // required: any newly required name breaks existing producers' readers.
    let required = |map: &serde_json::Map<String, Value>| -> BTreeSet<String> {
        map.get("required")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    for name in required(new).difference(&required(old)) {
        out.push(format!("{path}: `{name}` is newly required"));
    }

    // properties: removal breaks readers; common ones recurse.
    if let Some(old_props) = old.get("properties").and_then(Value::as_object) {
        let new_props = new.get("properties").and_then(Value::as_object);
        for (name, old_schema) in old_props {
            match new_props.and_then(|props| props.get(name)) {
                Some(new_schema) => compare(
                    old_schema,
                    new_schema,
                    &format!("{path}/properties/{name}"),
                    out,
                ),
                None => out.push(format!("{path}: property `{name}` removed")),
            }
        }
    }
    // additionalProperties: closing it, or constraining extras that used to
    // be accepted, narrows the schema.
    let old_extra = old.get("additionalProperties");
    let new_extra = new.get("additionalProperties");
    match (old_extra, new_extra) {
        (_, Some(Value::Bool(false))) if old_extra != Some(&Value::Bool(false)) => {
            out.push(format!("{path}: additionalProperties closed"));
        }
        (Some(o), Some(n)) if o.is_object() && n.is_object() => {
            compare(o, n, &format!("{path}/additionalProperties"), out);
        }
        _ if accepts_everything(old_extra) && !accepts_everything(new_extra) => {
            out.push(format!("{path}: additionalProperties constrained"));
        }
        _ => {}
    }
    // A property added to an object whose extras were accepted constrains
    // values existing documents may already carry under that name.
    if old_extra != Some(&Value::Bool(false))
        && let Some(new_props) = new.get("properties").and_then(Value::as_object)
    {
        let old_props = old.get("properties").and_then(Value::as_object);
        for (name, new_schema) in new_props {
            if old_props.is_some_and(|props| props.contains_key(name)) {
                continue;
            }
            match old_extra {
                Some(extra @ Value::Object(_)) if !accepts_everything(Some(extra)) => compare(
                    extra,
                    new_schema,
                    &format!("{path}/properties/{name}"),
                    out,
                ),
                _ if accepts_everything(Some(new_schema)) => {}
                _ => out.push(format!(
                    "{path}: property `{name}` added with constraints to an object that accepted extra properties"
                )),
            }
        }
    }

    // items.
    match (old.get("items"), new.get("items")) {
        (Some(o), Some(n)) => compare(o, n, &format!("{path}/items"), out),
        (None, Some(_)) => out.push(format!("{path}: items constraint added")),
        _ => {}
    }

    // Numeric/size bounds: tightening breaks, loosening does not.
    for key in [
        "minimum",
        "exclusiveMinimum",
        "minLength",
        "minItems",
        "minProperties",
    ] {
        match (number(old.get(key)), number(new.get(key))) {
            (Some(o), Some(n)) if n > o => out.push(format!("{path}: {key} raised {o} -> {n}")),
            (None, Some(n)) => out.push(format!("{path}: {key} {n} added")),
            _ => {}
        }
    }
    for key in [
        "maximum",
        "exclusiveMaximum",
        "maxLength",
        "maxItems",
        "maxProperties",
    ] {
        match (number(old.get(key)), number(new.get(key))) {
            (Some(o), Some(n)) if n < o => out.push(format!("{path}: {key} lowered {o} -> {n}")),
            (None, Some(n)) => out.push(format!("{path}: {key} {n} added")),
            _ => {}
        }
    }
    // pattern: removal widens; a new or different pattern may narrow.
    match (old.get("pattern"), new.get("pattern")) {
        (Some(o), Some(n)) if o != n => out.push(format!("{path}: pattern changed")),
        (None, Some(_)) => out.push(format!("{path}: pattern added")),
        _ => {}
    }
    if old.get("uniqueItems") != Some(&Value::Bool(true))
        && new.get("uniqueItems") == Some(&Value::Bool(true))
    {
        out.push(format!("{path}: uniqueItems added"));
    }

    // Combinators: fewer alternatives or more conjuncts narrow the schema.
    for key in ["anyOf", "oneOf"] {
        if let (Some(Value::Array(o)), Some(Value::Array(n))) = (old.get(key), new.get(key)) {
            if n.len() < o.len() {
                out.push(format!(
                    "{path}: {key} lost {} alternative(s)",
                    o.len() - n.len()
                ));
            }
            for (index, (o, n)) in o.iter().zip(n).enumerate() {
                compare(o, n, &format!("{path}/{key}/{index}"), out);
            }
        } else if old.get(key).is_none() && new.get(key).is_some() {
            out.push(format!("{path}: {key} constraint added"));
        }
    }
    if let (Some(Value::Array(o)), Some(Value::Array(n))) = (old.get("allOf"), new.get("allOf")) {
        if n.len() > o.len() {
            out.push(format!(
                "{path}: allOf gained {} conjunct(s)",
                n.len() - o.len()
            ));
        }
        for (index, (o, n)) in o.iter().zip(n).enumerate() {
            compare(o, n, &format!("{path}/allOf/{index}"), out);
        }
    } else if old.get("allOf").is_none() && new.get("allOf").is_some() {
        out.push(format!("{path}: allOf constraint added"));
    }

    // Named definitions: removal breaks `$ref`s from consumer copies.
    for key in ["$defs", "definitions"] {
        if let Some(old_defs) = old.get(key).and_then(Value::as_object) {
            let new_defs = new.get(key).and_then(Value::as_object);
            for (name, old_schema) in old_defs {
                match new_defs.and_then(|defs| defs.get(name)) {
                    Some(new_schema) => {
                        compare(old_schema, new_schema, &format!("{path}/{key}/{name}"), out)
                    }
                    None => out.push(format!("{path}: definition `{name}` removed")),
                }
            }
        }
    }

    // Everything else is compared verbatim; unknown keywords fail closed.
    const HANDLED: &[&str] = &[
        "type",
        "enum",
        "const",
        "required",
        "properties",
        "additionalProperties",
        "items",
        "minimum",
        "exclusiveMinimum",
        "minLength",
        "minItems",
        "minProperties",
        "maximum",
        "exclusiveMaximum",
        "maxLength",
        "maxItems",
        "maxProperties",
        "uniqueItems",
        "pattern",
        "anyOf",
        "oneOf",
        "allOf",
        "$defs",
        "definitions",
    ];
    for (key, old_value) in old {
        if HANDLED.contains(&key.as_str()) || ANNOTATIONS.contains(&key.as_str()) {
            continue;
        }
        if new.get(key).map(strip_annotations) != Some(strip_annotations(old_value)) {
            out.push(format!("{path}: `{key}` changed or removed"));
        }
    }
    for (key, new_value) in new {
        if HANDLED.contains(&key.as_str())
            || ANNOTATIONS.contains(&key.as_str())
            || old.contains_key(key)
        {
            continue;
        }
        out.push(format!("{path}: `{key}` constraint added ({new_value})"));
    }
}

/// Resolve the comparison base: the merge base with `origin/main`, or `None`
/// when no such ref exists (shallow or detached checkouts).
fn base_rev(root: &Path) -> Option<String> {
    for candidate in ["origin/main", "main"] {
        let args = [
            "merge-base".to_string(),
            "HEAD".to_string(),
            candidate.to_string(),
        ];
        if let Ok(output) = crate::run::capture_output_in_dir("git", &args, root, "git merge-base")
            && output.status.success()
        {
            let rev = output.stdout.trim().to_string();
            if !rev.is_empty() {
                return Some(rev);
            }
        }
    }
    None
}

/// Check every published schema against the merge base.
///
/// Returns `(violations, summary)`. A missing base is reported in the summary
/// as not established rather than as a pass.
pub(crate) fn check(root: &Path, published: &[String]) -> (Vec<String>, String) {
    let Some(rev) = base_rev(root) else {
        // Locally a shallow checkout is reported, not failed; in CI a gate
        // that silently stops comparing is a false pass.
        let note =
            "schema compatibility not established (no merge base with origin/main)".to_string();
        if std::env::var_os("CI").is_some() {
            return (
                vec![format!("{note}; fetch origin/main with history in CI")],
                note,
            );
        }
        return (Vec::new(), note);
    };
    let git = |args: &[&str], what: &str| {
        let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        crate::run::capture_output_in_dir("git", &args, root, what)
            .ok()
            .filter(|output| output.status.success())
            .map(|output| output.stdout)
    };
    let Some(listing) = git(
        &["ls-tree", "-r", "--name-only", &rev, "--", "schemas"],
        "git ls-tree",
    ) else {
        return (
            vec![format!(
                "schema compatibility: could not list schemas at {rev}"
            )],
            "schema compatibility not established (base listing failed)".to_string(),
        );
    };
    let base_schemas: BTreeSet<&str> = listing
        .lines()
        .filter(|path| path.ends_with(".json"))
        .collect();
    let mut violations = Vec::new();
    for removed in base_schemas
        .iter()
        .filter(|path| !published.iter().any(|current| current == *path))
    {
        violations.push(format!(
            "{removed}: published schema removed or renamed; consumers that vendor it break"
        ));
    }
    let (mut compared, mut added, mut bumped) = (0usize, 0usize, 0usize);
    for rel in published {
        if !base_schemas.contains(rel.as_str()) {
            added += 1; // new schema: nothing to be compatible with
            continue;
        }
        let Some(base_text) = git(&["show", &format!("{rev}:{rel}")], "git show") else {
            violations.push(format!("{rel}: could not read base copy at {rev}"));
            continue;
        };
        let (Ok(old), Ok(new)) = (
            serde_json::from_str::<Value>(&base_text),
            std::fs::read_to_string(root.join(rel))
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())),
        ) else {
            violations.push(format!("{rel}: base or current copy is not valid JSON"));
            continue;
        };
        compared += 1;
        match assess(&old, &new) {
            Compat::Compatible {
                bumped: true,
                breaking,
            } if breaking > 0 => bumped += 1,
            Compat::Compatible { .. } => {}
            Compat::Violations(found) => {
                for line in found {
                    violations.push(format!(
                        "{rel}: breaking schema change without a schema_version bump: {line}"
                    ));
                }
            }
        }
    }
    let head = git(&["rev-parse", "HEAD"], "git rev-parse").unwrap_or_default();
    let scope = if head.trim() == rev {
        " (merge base is HEAD: only uncommitted edits are compared)"
    } else {
        ""
    };
    (
        violations,
        format!(
            "schema compatibility: compared {compared} published schema(s) with merge base {}{scope}, {added} new, {bumped} breaking change(s) accepted by version bump",
            &rev[..rev.len().min(12)]
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "type": "object",
            "required": ["schema_version", "a"],
            "properties": {
                "schema_version": {"const": "0.1"},
                "a": {"type": "string", "enum": ["x", "y"]},
                "b": {"type": "array", "items": {"type": "integer"}, "maxItems": 5}
            },
            "additionalProperties": true
        })
    }

    fn edited(f: impl FnOnce(&mut Value)) -> Value {
        let mut value = base();
        f(&mut value);
        value
    }

    fn violations(new: Value) -> Vec<String> {
        match assess(&base(), &new) {
            Compat::Violations(found) => found,
            _ => Vec::new(),
        }
    }

    #[test]
    fn identical_and_annotation_only_edits_are_compatible() {
        assert!(matches!(
            assess(&base(), &base()),
            Compat::Compatible { breaking: 0, .. }
        ));
        let doc = edited(|v| v["properties"]["a"]["description"] = json!("new words"));
        assert!(matches!(
            assess(&base(), &doc),
            Compat::Compatible { breaking: 0, .. }
        ));
    }

    #[test]
    fn widening_edits_are_compatible() {
        let wider = edited(|v| {
            v["properties"]["a"]["enum"] = json!(["x", "y", "z"]);
            v["properties"]["b"]["maxItems"] = json!(9);
            v["properties"]["c"] = json!({"description": "unconstrained"});
            v["properties"]["a"]["type"] = json!(["string", "null"]);
        });
        assert!(matches!(
            assess(&base(), &wider),
            Compat::Compatible { breaking: 0, .. }
        ));
    }

    #[test]
    fn each_breaking_edit_is_caught_without_a_bump() {
        let cases: Vec<(&str, Value, &str)> = vec![
            (
                "enum removed",
                edited(|v| v["properties"]["a"]["enum"] = json!(["x"])),
                "enum value",
            ),
            (
                "property removed",
                edited(|v| {
                    if let Some(props) = v["properties"].as_object_mut() {
                        props.remove("b");
                    }
                }),
                "removed",
            ),
            (
                "newly required",
                edited(|v| v["required"] = json!(["schema_version", "a", "b"])),
                "newly required",
            ),
            (
                "type narrowed",
                edited(|v| v["properties"]["a"]["type"] = json!("integer")),
                "type no longer",
            ),
            (
                "bound lowered",
                edited(|v| v["properties"]["b"]["maxItems"] = json!(2)),
                "maxItems lowered",
            ),
            (
                "bound added",
                edited(|v| v["properties"]["b"]["minItems"] = json!(1)),
                "minItems",
            ),
            (
                "closed",
                edited(|v| v["additionalProperties"] = json!(false)),
                "additionalProperties closed",
            ),
            (
                "items narrowed",
                edited(|v| v["properties"]["b"]["items"]["type"] = json!("string")),
                "type no longer",
            ),
            (
                "pattern added",
                edited(|v| v["properties"]["a"]["pattern"] = json!("^x$")),
                "pattern",
            ),
        ];
        for (name, new, needle) in cases {
            let found = violations(new);
            assert!(
                found.iter().any(|line| line.contains(needle)),
                "{name}: expected `{needle}` in {found:?}"
            );
        }
    }

    #[test]
    fn review_findings_are_covered() {
        // Extras constrained, a typed property added to an open object.
        let found = violations(edited(|v| {
            v["additionalProperties"] = json!({"type": "string"})
        }));
        assert!(
            found
                .iter()
                .any(|l| l.contains("additionalProperties constrained")),
            "{found:?}"
        );
        let found = violations(edited(|v| v["properties"]["c"] = json!({"type": "string"})));
        assert!(
            found.iter().any(|l| l.contains("property `c` added")),
            "{found:?}"
        );
        // Deleting the version pin is not a bump.
        let found = violations(edited(|v| {
            v["properties"]["schema_version"] = json!({});
            v["properties"]["a"]["enum"] = json!(["x"]);
        }));
        assert!(found.iter().any(|l| l.contains("enum value")), "{found:?}");
        // Closed objects may gain typed properties; const may widen to enum;
        // a pattern may be dropped; wording inside verbatim keywords is ignored.
        let old = json!({"properties": {"k": {"const": "a", "pattern": "^a$", "not": {"description": "x", "type": "null"}}}, "additionalProperties": false});
        let new = json!({"properties": {"k": {"enum": ["a", "b"], "not": {"description": "y", "type": "null"}}, "n": {"type": "string"}}, "additionalProperties": false});
        assert!(
            matches!(assess(&old, &new), Compat::Compatible { breaking: 0, .. }),
            "{:?}",
            assess(&old, &new)
        );
    }

    #[test]
    fn a_version_bump_accepts_breaking_edits() {
        let new = edited(|v| {
            v["properties"]["schema_version"]["const"] = json!("0.2");
            v["properties"]["a"]["enum"] = json!(["x"]);
        });
        assert_eq!(
            assess(&base(), &new),
            Compat::Compatible {
                bumped: true,
                breaking: 1
            }
        );
    }

    #[test]
    fn nested_definition_removal_and_combinator_loss_are_caught() {
        let old = json!({"$defs": {"d": {"type": "string"}}, "anyOf": [{"type": "string"}, {"type": "null"}]});
        let new = json!({"$defs": {}, "anyOf": [{"type": "string"}]});
        let found = match assess(&old, &new) {
            Compat::Violations(found) => found,
            Compat::Compatible { .. } => Vec::new(),
        };
        assert!(!found.is_empty(), "expected violations");
        assert!(
            found.iter().any(|l| l.contains("definition `d` removed")),
            "{found:?}"
        );
        assert!(
            found.iter().any(|l| l.contains("lost 1 alternative")),
            "{found:?}"
        );
    }

    #[test]
    fn published_schemas_are_compatible_with_themselves() -> Result<(), String> {
        // Positive control on real bytes: every committed schema parses and a
        // self-comparison has no breaking edits.
        let root = crate::repo_root()?;
        let mut seen = 0;
        for entry in std::fs::read_dir(root.join("schemas/ripr")).map_err(|e| e.to_string())? {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            let value: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            assert!(matches!(
                assess(&value, &value),
                Compat::Compatible { breaking: 0, .. }
            ));
            seen += 1;
        }
        assert!(seen >= 10, "expected the published schema set, saw {seen}");
        Ok(())
    }
}
