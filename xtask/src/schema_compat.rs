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
    // Keys of `properties`-style maps are data names, not keywords: a
    // property called `description` keeps its constraints.
    const NAME_MAPS: &[&str] = &["properties", "patternProperties", "$defs", "definitions"];
    match value {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(key, _)| !ANNOTATIONS.contains(&key.as_str()))
                .map(|(key, value)| {
                    let value = match value {
                        Value::Object(names) if NAME_MAPS.contains(&key.as_str()) => Value::Object(
                            names
                                .iter()
                                .map(|(name, schema)| (name.clone(), strip_annotations(schema)))
                                .collect(),
                        ),
                        other => strip_annotations(other),
                    };
                    (key.clone(), value)
                })
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

/// A properties-only branch is satisfied by every formerly valid instance if
/// each named scalar guard was already required whenever that property existed.
/// Missing properties still satisfy the branch. Exclude references and other
/// context-dependent keywords whose meaning can change when moved.
fn repeats_scalar_properties(old: &serde_json::Map<String, Value>, branch: &Value) -> bool {
    let Some(branch) = branch.as_object().filter(|branch| branch.len() == 1) else {
        return false;
    };
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return false;
    };
    let Some(previous) = old.get("properties").and_then(Value::as_object) else {
        return false;
    };
    !properties.is_empty()
        && properties.iter().all(|(name, guard)| {
            let Some(guard) = guard.as_object().filter(|g| !g.is_empty()) else {
                return false;
            };
            let Some(prior) = previous.get(name).and_then(Value::as_object) else {
                return false;
            };
            guard.iter().all(|(keyword, value)| {
                matches!(keyword.as_str(), "type" | "minLength")
                    && prior.get(keyword) == Some(value)
            })
        })
}

fn valid_required_names(value: &Value) -> bool {
    let Some(names) = value.as_array() else {
        return false;
    };
    let mut seen = BTreeSet::new();
    !names.is_empty()
        && names
            .iter()
            .all(|name| name.as_str().is_some_and(|name| seen.insert(name)))
}

fn named_definition(reference: &str) -> Option<&str> {
    let name = reference.strip_prefix("#/$defs/")?;
    (!name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
    .then_some(name)
}

/// A reference back to the changed property can reverse the implication,
/// for instance underneath a sibling's `not`. Keep this proof within a
/// single schema resource whose references can only address named definitions.
fn resolved_schema_refs(key: &str, node: &Value, resolved: &BTreeSet<&str>) -> bool {
    match key {
        "$ref" => node
            .as_str()
            .and_then(named_definition)
            .is_some_and(|name| resolved.contains(name)),
        "$schema" | "$id" | "$anchor" | "$dynamicAnchor" | "$dynamicRef" | "$recursiveAnchor"
        | "$recursiveRef" => false,
        _ => match node {
            Value::Object(fields) => fields
                .iter()
                .all(|(key, value)| resolved_schema_refs(key, value, resolved)),
            Value::Array(items) => items
                .iter()
                .all(|value| resolved_schema_refs("", value, resolved)),
            _ => true,
        },
    }
}

/// Admit a definition only after its dependencies resolve. A pass without
/// progress leaves a cycle, dangling reference, or unsupported schema resource.
fn closed_definition_graph(root: &serde_json::Map<String, Value>) -> bool {
    let Some(defs) = root.get("$defs").and_then(Value::as_object) else {
        return false;
    };
    let mut resolved = BTreeSet::new();
    while resolved.len() < defs.len() {
        let prior = resolved.len();
        for (name, schema) in defs {
            if !resolved.contains(name.as_str())
                && (schema.is_boolean() || schema.is_object())
                && resolved_schema_refs("", schema, &resolved)
            {
                resolved.insert(name.as_str());
            }
        }
        if resolved.len() == prior {
            return false;
        }
    }
    root.iter().all(|(key, node)| {
        matches!(key.as_str(), "$schema" | "$id" | "$defs")
            || resolved_schema_refs(key, node, &resolved)
    })
}

/// A top-level property can add a receipt version by retaining its original
/// local reference as one arm of an inclusive union. Check the legacy
/// definition itself, and resolve every other arm before accepting the union.
fn retains_local_reference(
    prior: &Value,
    candidate: &Value,
    old_root: &serde_json::Map<String, Value>,
    new_root: &serde_json::Map<String, Value>,
) -> bool {
    fn definition<'a>(root: &'a serde_json::Map<String, Value>, arm: &Value) -> Option<&'a Value> {
        let arm = arm.as_object().filter(|arm| arm.len() == 1)?;
        let name = named_definition(arm.get("$ref")?.as_str()?)?;
        root.get("$defs")?
            .get(name)
            .filter(|schema| schema.is_object() || schema.is_boolean())
    }

    let Some(old_defs) = old_root.get("$defs").and_then(Value::as_object) else {
        return false;
    };
    let Some(arms) = candidate
        .as_object()
        .filter(|node| node.len() == 1)
        .and_then(|node| node.get("anyOf"))
        .and_then(Value::as_array)
    else {
        return false;
    };
    // Preserving the first target alone misses a changed transitive target
    // whose use under `not` reverses an otherwise widening edit.
    old_root.get("$schema") == new_root.get("$schema")
        && old_root.get("$id") == new_root.get("$id")
        && definition(old_root, prior).is_some()
        && old_defs.iter().all(|(key, schema)| {
            new_root.get("$defs").and_then(|defs| defs.get(key)) == Some(schema)
        })
        && closed_definition_graph(old_root)
        && closed_definition_graph(new_root)
        && arms.contains(prior)
        && arms.iter().all(|arm| definition(new_root, arm).is_some())
}

/// Once another arm covers every old instance, this sibling only has to be a
/// well-formed predicate. Keep references, combinators and unknown keywords
/// outside this proof; their evaluation may depend on the surrounding schema.
fn supported_additive_arm(schema: &Value, remaining_depth: usize) -> bool {
    let Some(depth) = remaining_depth.checked_sub(1) else {
        return false;
    };
    schema.as_object().is_some_and(|object| {
        object.iter().all(|(key, value)| match key.as_str() {
            "const" => true,
            "description" => value.is_string(),
            "type" => value.as_str() == Some("string"),
            "additionalProperties" => value.is_boolean(),
            "required" => valid_required_names(value),
            "minItems" | "maxItems" => value.as_u64().is_some(),
            "items" => supported_additive_arm(value, depth),
            "properties" => value.as_object().is_some_and(|properties| {
                properties
                    .values()
                    .all(|child| supported_additive_arm(child, depth))
            }),
            _ => false,
        })
    })
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
                Some(new_schema)
                    if path == "#" && retains_local_reference(old_schema, new_schema, old, new) => {
                }
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
            // `oneOf` needs exactly one match: a new branch that overlaps an
            // existing one rejects values that used to match once.
            if key == "oneOf" && n.len() > o.len() {
                out.push(format!(
                    "{path}: oneOf gained {} alternative(s), which may overlap existing ones",
                    n.len() - o.len()
                ));
            }
            for (index, (o, n)) in o.iter().zip(n).enumerate() {
                compare(o, n, &format!("{path}/{key}/{index}"), out);
            }
        } else if old.get(key).is_none()
            && let Some(alternatives) = new.get(key)
        {
            // Widening an existing oneOf arm could overlap another arm. This
            // implication is only a proof for the document root.
            let covered = path == "#"
                && key == "anyOf"
                && alternatives.as_array().is_some_and(|branches| {
                    let mut retained = false;
                    for branch in branches {
                        if repeats_scalar_properties(old, branch) {
                            retained = true;
                        } else if !supported_additive_arm(branch, 32) {
                            return false;
                        }
                    }
                    retained
                });
            if !covered {
                out.push(format!("{path}: {key} constraint added"));
            }
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
    fn top_level_receipt_union_preserves_the_whole_legacy_reference() -> Result<(), String> {
        let legacy = json!({"$ref": "#/$defs/prior"});
        let next = json!({"$ref": "#/$defs/next"});
        let old = json!({"properties":{"receipt":legacy}, "$defs":{"prior":{"const":"old"}}});
        let mut valid = old.clone();
        valid["$defs"]["next"] = json!({"const":"new"});
        valid["properties"]["receipt"] = json!({"anyOf":[next, legacy]});
        if !matches!(assess(&old, &valid), Compat::Compatible { breaking: 0, .. }) {
            return Err("union containing the original reference was refused".into());
        }
        for invalid in [
            json!({"anyOf":[next]}),
            json!({"oneOf":[next, legacy]}),
            json!({"anyOf":[legacy, {"$ref":"#/$defs/absent"}]}),
            json!({"anyOf":[legacy, {"$ref":"remote.json"}]}),
            json!({"anyOf":[next, legacy], "not":legacy}),
            json!({"anyOf":[next, {"$ref":"#/$defs/prior","const":"new"}]}),
        ] {
            let mut doc = valid.clone();
            doc["properties"]["receipt"] = invalid;
            if !matches!(assess(&old, &doc), Compat::Violations(_)) {
                return Err(format!(
                    "accepted a narrowed or unresolved receipt union: {doc}"
                ));
            }
        }
        for reference in [
            json!({"$ref":"#/properties/receipt"}),
            json!({"$ref":"#"}),
            json!({"$dynamicRef":"#receipt"}),
            json!({"$id":"other", "$ref":"#/$defs/prior"}),
        ] {
            let mut referenced_old = old.clone();
            let mut referenced_new = valid.clone();
            for root in [&mut referenced_old, &mut referenced_new] {
                root["properties"]["sibling"] = json!({"not":reference});
            }
            if !matches!(
                assess(&referenced_old, &referenced_new),
                Compat::Violations(_)
            ) {
                return Err(format!(
                    "unproved union with an indirect reference was accepted: {reference}"
                ));
            }
        }
        valid["$defs"]["prior"] = json!({"const":"different"});
        if !matches!(assess(&old, &valid), Compat::Violations(_)) {
            return Err("replacement of the historic receipt target was accepted".into());
        }
        Ok(())
    }

    #[test]
    fn retained_receipt_checks_transitive_targets_and_refuses_unresolved_graphs()
    -> Result<(), String> {
        let old = json!({
            "properties": {"receipt":{"$ref":"#/$defs/prior"}},
            "$defs": {"prior":{"not":{"$ref":"#/$defs/shared"}}, "shared":{"enum":["ban"]}}
        });
        let mut widened = old.clone();
        widened["properties"]["receipt"] =
            json!({"anyOf":[{"$ref":"#/$defs/next"},{"$ref":"#/$defs/prior"}]});
        widened["$defs"]["next"] = json!({"$ref":"#/$defs/leaf"});
        widened["$defs"]["leaf"] = json!({"const":"future"});
        if !matches!(
            assess(&old, &widened),
            Compat::Compatible { breaking: 0, .. }
        ) {
            return Err("refused an acyclic union preserving the entire original graph".into());
        }
        for (key, invalid) in [
            ("shared", json!({"enum":["ban","old"]})),
            ("leaf", json!({"$ref":"#/$defs/missing"})),
            ("leaf", json!({"$ref":"#/$defs/next"})),
            ("leaf", json!({"$ref":"#/$defs/leaf"})),
        ] {
            let mut doc = widened.clone();
            doc["$defs"][key] = invalid;
            if !matches!(assess(&old, &doc), Compat::Violations(_)) {
                return Err(format!(
                    "accepted changed semantics or unresolvable graph: {doc}"
                ));
            }
        }
        Ok(())
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
    fn added_any_of_preserves_a_repeated_scalar_guard_only() -> Result<(), String> {
        let old = json!({"type":"object","properties":{"root":{"type":"string","minLength":1},
            "other":{"type":"integer"}}});
        let guard = json!({"properties":{"root":{"type":"string","minLength":1}}});
        let refusal = json!({"required":["status","error"]});
        let mut widened = old.clone();
        widened["properties"]["root"] = json!({"type":"string"});
        widened["anyOf"] = json!([guard, refusal]);
        let accepted = |candidate: &Value| {
            matches!(
                assess(&old, candidate),
                Compat::Compatible { breaking: 0, .. }
            )
        };
        if !accepted(&widened) {
            return Err("old property guard rejected".into());
        }
        let closed = json!({
            "description":"closed refusal",
            "additionalProperties":false,
            "required":["status","details"],
            "properties":{
                "status":{"const":"refused"},
                "details":{"minItems":1,"maxItems":1,"items":{"properties":{"code":{"type":"string"}}}}
            }
        });
        let mut bounded = widened.clone();
        bounded["anyOf"] = json!([guard, closed]);
        if !accepted(&bounded) {
            return Err("retained guard with a well-formed closed sibling rejected".into());
        }
        for bad in [
            json!({"properties":{"root":{"minLength":2}}}),
            json!({"properties":{"root":{"type":"integer"}}}),
            json!({"properties":{"new":{"type":"string"}}}),
            json!({"properties":{"root":{"$ref":"#/$defs/root"}}}),
            json!({"properties":{"root":{"type":"string"}},"required":["root"]}),
        ] {
            let mut narrowed = widened.clone();
            narrowed["anyOf"] = json!([bad, refusal]);
            if accepted(&narrowed) {
                return Err(format!("unproven guard accepted: {bad}"));
            }
        }
        for malformed in [
            json!({"required":"status"}),
            json!({"required":["status","status"]}),
            json!({"required":[true]}),
            json!({"format":"unknown"}),
            json!({"properties":{"status":{"$ref":"#/$defs/missing"}}}),
            json!({"properties":{"details":{"items":{"type":"unknown"}}}}),
            json!({"properties":{"details":{"items":[]}}}),
            json!({"properties":[]}),
            json!({"additionalProperties":"false"}),
            json!({"maxItems":-1}),
            json!({"minItems":1.5}),
            json!({"description":false}),
        ] {
            let mut candidate = widened.clone();
            candidate["anyOf"] = json!([guard, malformed]);
            if accepted(&candidate) {
                return Err(format!(
                    "invalid or unsupported alternative accepted: {malformed}"
                ));
            }
        }
        let mut exclusive = old.clone();
        exclusive["oneOf"] = json!([guard, refusal]);
        let mut sibling = widened.clone();
        sibling["properties"]["other"]["minimum"] = json!(1);
        if accepted(&exclusive) || accepted(&sibling) {
            return Err("oneOf overlap or sibling narrowing escaped the checker".into());
        }
        Ok(())
    }

    #[test]
    fn published_check_root_widening_does_not_admit_overlapping_cache_receipts()
    -> Result<(), String> {
        let mut before: Value =
            serde_json::from_str(include_str!("../../schemas/ripr/check.schema.json"))
                .map_err(|e| e.to_string())?;
        let guard = json!({"properties":{"root":{"type":"string","minLength":1}}});
        // Exercise both generations of the published file as #15842 lands.
        let obj = before.as_object_mut().ok_or("expected schema object")?;
        let alternatives = obj
            .remove("anyOf")
            .unwrap_or_else(|| json!([guard,{"required":["status","error"]}]));
        before["properties"]["root"] = json!({"type":"string","minLength":1});
        let mut after = before.clone();
        after["properties"]["root"] = json!({"type":"string"});
        after["anyOf"] = alternatives;
        if !matches!(
            assess(&before, &after),
            Compat::Compatible { breaking: 0, .. }
        ) {
            return Err("real check root widening rejected".into());
        }

        let cache = &before["properties"]["file_fact_cache"];
        let mut bad = before.clone();
        let arm = bad
            .pointer_mut("/properties/file_fact_cache/oneOf/1")
            .and_then(Value::as_object_mut)
            .ok_or("missing cache object alternative")?;
        arm.remove("type");
        arm.insert(
            "anyOf".into(),
            json!([{"properties":{"hits":{"type":"integer"}}},{"required":["status"]}]),
        );
        let mut old_errors = Vec::new();
        let mut new_errors = Vec::new();
        crate::verification_contracts::validate_value_against_schema(
            &Value::Null,
            cache,
            &before,
            "cache".into(),
            &mut old_errors,
        );
        crate::verification_contracts::validate_value_against_schema(
            &Value::Null,
            &bad["properties"]["file_fact_cache"],
            &bad,
            "cache".into(),
            &mut new_errors,
        );
        if !old_errors.is_empty()
            || new_errors.is_empty()
            || !matches!(assess(&before, &bad), Compat::Violations(_))
        {
            return Err("null cache receipt became invalid without a compatibility refusal".into());
        }
        Ok(())
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
        // A property literally named `description` inside a verbatim keyword
        // is data, not an annotation.
        let old = json!({"not": {"properties": {"description": {"type": "string"}}}});
        let new = json!({"not": {"properties": {"description": {"type": "integer"}}}});
        assert!(matches!(assess(&old, &new), Compat::Violations(_)));
        // Adding an overlapping oneOf branch breaks exactly-one matching;
        // an enum that excludes an unchanged const is a new constraint.
        let old = json!({"oneOf": [{"type": "string"}, {"type": "null"}]});
        let new = json!({"oneOf": [{"type": "string"}, {"type": "null"}, {"type": "string"}]});
        assert!(matches!(assess(&old, &new), Compat::Violations(_)));
        let old = json!({"const": "ripr"});
        let new = json!({"const": "ripr", "enum": ["other"]});
        assert!(matches!(assess(&old, &new), Compat::Violations(_)));
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
