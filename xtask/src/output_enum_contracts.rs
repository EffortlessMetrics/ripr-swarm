//! Completeness half of `cargo xtask check-output-contracts`.
//!
//! The row loop in `check_output_contracts` proves each registered value is
//! still mentioned by its producer and the docs. It cannot notice a value the
//! producer emits that nobody registered, and its docs check is a substring
//! search, so `transitive_reach_unresolved` counted as documented because
//! `rust_transitive_reach_unresolved` appears in a different list.
//!
//! This module closes both gaps for the finding enums agents branch on: the
//! wire set is derived from the owning enum declaration, and the registry and
//! the `## Enums` list in `docs/OUTPUT_SCHEMA.md` must each equal it exactly.

use std::collections::{BTreeMap, BTreeSet};

/// One governed output enum: its registry kind, the heading of its list under
/// `## Enums` in `docs/OUTPUT_SCHEMA.md`, and the file that owns the Rust enum.
pub(crate) struct GovernedEnum {
    pub(crate) kind: &'static str,
    pub(crate) doc_list: &'static str,
    pub(crate) enum_name: &'static str,
    pub(crate) source_path: &'static str,
}

pub(crate) const GOVERNED_ENUMS: &[GovernedEnum] = &[
    governed(
        "exposure_class",
        "classification",
        "ExposureClass",
        "crates/ripr/src/domain/classification.rs",
    ),
    governed(
        "probe_family",
        "family",
        "ProbeFamily",
        "crates/ripr/src/domain/probe.rs",
    ),
    governed(
        "delta",
        "delta",
        "DeltaKind",
        "crates/ripr/src/domain/probe.rs",
    ),
    governed(
        "static_limit_kind",
        "static_limit_kind",
        "StaticLimitKind",
        "crates/ripr/src/domain/language.rs",
    ),
    governed(
        "flow_sink",
        "flow_sink",
        "FlowSinkKind",
        "crates/ripr/src/domain/probe.rs",
    ),
    governed(
        "stage_state",
        "state",
        "StageState",
        "crates/ripr/src/domain/evidence.rs",
    ),
    governed(
        "confidence",
        "confidence",
        "Confidence",
        "crates/ripr/src/domain/evidence.rs",
    ),
    governed(
        "oracle_strength",
        "oracle_strength",
        "OracleStrength",
        "crates/ripr/src/domain/evidence.rs",
    ),
    governed(
        "oracle_kind",
        "oracle_kind",
        "OracleKind",
        "crates/ripr/src/domain/evidence.rs",
    ),
    governed(
        "value_context",
        "value_context",
        "ValueContext",
        "crates/ripr/src/domain/probe.rs",
    ),
    governed(
        "stop_reason",
        "stop_reason",
        "StopReason",
        "crates/ripr/src/domain/probe.rs",
    ),
];

const fn governed(
    kind: &'static str,
    doc_list: &'static str,
    enum_name: &'static str,
    source_path: &'static str,
) -> GovernedEnum {
    GovernedEnum {
        kind,
        doc_list,
        enum_name,
        source_path,
    }
}

/// Compares every governed enum's wire set with the registry rows and the
/// documented list, reporting each missing or extra value in either direction.
pub(crate) fn check_enum_completeness(
    registry: &BTreeMap<String, BTreeSet<String>>,
    output_schema_doc: &str,
    read_source: &dyn Fn(&str) -> Result<String, String>,
    violations: &mut Vec<String>,
) -> Result<(), String> {
    let doc_lists = documented_enum_lists(output_schema_doc);
    for governed in GOVERNED_ENUMS {
        let source = read_source(governed.source_path)?;
        let wire = match enum_wire_values(&source, governed.enum_name) {
            Ok(wire) => wire,
            Err(err) => {
                violations.push(format!("{}: {err}", governed.source_path));
                continue;
            }
        };
        let empty = BTreeSet::new();
        let registered = registry.get(governed.kind).unwrap_or(&empty);
        compare(
            &wire,
            registered,
            &format!("{} `{}`", governed.source_path, governed.enum_name),
            &format!("policy/output_contracts.txt `{}` rows", governed.kind),
            violations,
        );
        match doc_lists.get(governed.doc_list) {
            Some(documented) => compare(
                &wire,
                documented,
                &format!("{} `{}`", governed.source_path, governed.enum_name),
                &format!(
                    "docs/OUTPUT_SCHEMA.md `## Enums` `{}` list",
                    governed.doc_list
                ),
                violations,
            ),
            None => violations.push(format!(
                "docs/OUTPUT_SCHEMA.md `## Enums` has no `{}` values list",
                governed.doc_list
            )),
        }
    }
    Ok(())
}

fn compare(
    wire: &BTreeSet<String>,
    other: &BTreeSet<String>,
    wire_label: &str,
    other_label: &str,
    violations: &mut Vec<String>,
) {
    for value in wire.difference(other) {
        violations.push(format!(
            "{wire_label} emits `{value}` but {other_label} does not list it"
        ));
    }
    for value in other.difference(wire) {
        violations.push(format!(
            "{other_label} lists `{value}` but {wire_label} does not emit it"
        ));
    }
}

/// The wire values of a fieldless enum: each variant name in snake_case, the
/// form both `#[serde(rename_all = "snake_case")]` and the hand-written
/// `as_str` tables produce. When the enum's `as_str` spells its values out as
/// literals, those literals must agree, so a typo there cannot hide.
pub(crate) fn enum_wire_values(source: &str, enum_name: &str) -> Result<BTreeSet<String>, String> {
    let body = braced_body_after(source, &format!("pub enum {enum_name} "))
        .ok_or_else(|| format!("cannot find `pub enum {enum_name}`"))?;
    let mut wire = BTreeSet::new();
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") || line.starts_with("#[") {
            continue;
        }
        let name = line.trim_end_matches(',');
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(format!(
                "`{enum_name}` has a variant line this check cannot read: `{line}`"
            ));
        }
        wire.insert(snake_case(name));
    }
    if wire.is_empty() {
        return Err(format!("`{enum_name}` has no variants"));
    }

    if let Some(as_str) = braced_body_after(source, &format!("impl {enum_name} "))
        .and_then(|impl_body| braced_body_after(impl_body, "fn as_str("))
    {
        let literals = snake_literals(as_str);
        if !literals.is_empty() && literals != wire {
            return Err(format!(
                "`{enum_name}::as_str` literals {literals:?} differ from its variant names {wire:?}"
            ));
        }
    }
    Ok(wire)
}

/// Every `` `values` `` list under `## Enums`, keyed by the list's field name.
pub(crate) fn documented_enum_lists(doc: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut lists = BTreeMap::new();
    let Some(start) = doc.find("\n## Enums\n") else {
        return lists;
    };
    let section = &doc[start + 1..];
    let section = section[3..]
        .find("\n## ")
        .map_or(section, |end| &section[..end + 3]);
    let mut current: Option<String> = None;
    for line in section.lines() {
        let heading = line.strip_prefix("Reserved ").unwrap_or(line);
        if let Some(rest) = heading.strip_prefix('`')
            && let Some((name, tail)) = rest.split_once('`')
            && tail.trim_start().starts_with("values")
        {
            current = Some(name.to_string());
            lists.entry(name.to_string()).or_insert_with(BTreeSet::new);
            continue;
        }
        if let (Some(name), Some(item)) = (&current, line.strip_prefix("- `"))
            && let Some((value, _)) = item.split_once('`')
            && let Some(set) = lists.get_mut(name)
        {
            set.insert(value.to_string());
        }
    }
    lists
}

/// Text between the `{` following `marker` and its matching `}`.
fn braced_body_after<'a>(text: &'a str, marker: &str) -> Option<&'a str> {
    let after = &text[text.find(marker)? + marker.len()..];
    let open = after.find('{')?;
    let mut depth = 0usize;
    for (index, ch) in after[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&after[open + 1..open + index]);
                }
            }
            _ => {}
        }
    }
    None
}

fn snake_literals(text: &str) -> BTreeSet<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .filter(|literal| {
            !literal.is_empty()
                && literal
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        .map(str::to_string)
        .collect()
}

fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if index > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r#"
#[derive(Clone)]
#[serde(rename_all = "snake_case")]
pub enum Sink {
    /// Doc comment with a comma, and braces { }.
    ReturnValue,
    StateWrite,
}

impl Sink {
    pub fn as_str(&self) -> &'static str {
        match self {
            Sink::ReturnValue => "return_value",
            Sink::StateWrite => {
                "state_write"
            }
        }
    }
}
"#;

    fn set(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn wire_values_come_from_variant_names() -> Result<(), String> {
        assert_eq!(
            enum_wire_values(SOURCE, "Sink")?,
            set(&["return_value", "state_write"])
        );
        Ok(())
    }

    #[test]
    fn as_str_literal_that_disagrees_with_its_variant_is_reported() {
        let drifted = SOURCE.replace("\"state_write\"", "\"state_writes\"");
        let err = enum_wire_values(&drifted, "Sink").err().unwrap_or_default();
        assert!(err.contains("state_writes"), "{err}");
    }

    #[test]
    fn documented_lists_split_by_heading_and_stop_at_next_section() {
        let doc = "## Enums\n\n`delta` values:\n\n- `value`\n- `effect`\n\nReserved `flow_sink` values:\n\n- `return_value` -- prose with `rust_value` inside\n\n## Badge Output\n\n- `outside`\n";
        let lists = documented_enum_lists(&format!("intro\n{doc}"));
        assert_eq!(lists.get("delta"), Some(&set(&["value", "effect"])));
        assert_eq!(lists.get("flow_sink"), Some(&set(&["return_value"])));
        assert_eq!(lists.len(), 2);
    }

    #[test]
    fn a_value_documented_only_inside_a_longer_name_is_missing() -> Result<(), String> {
        // The row loop's substring search accepted this shape (#4539).
        let doc = "\n## Enums\n\n`flow_sink` values:\n\n- `rust_state_write`\n- `return_value`\n";
        let mut violations = Vec::new();
        compare(
            &enum_wire_values(SOURCE, "Sink")?,
            documented_enum_lists(doc)
                .get("flow_sink")
                .ok_or("flow_sink list")?,
            "source",
            "docs",
            &mut violations,
        );
        assert_eq!(
            violations,
            vec![
                "source emits `state_write` but docs does not list it".to_string(),
                "docs lists `rust_state_write` but source does not emit it".to_string(),
            ]
        );
        Ok(())
    }

    #[test]
    fn current_tree_output_enums_are_registered_and_documented() -> Result<(), String> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let read = |path: &str| {
            std::fs::read_to_string(root.join(path)).map_err(|err| format!("read {path}: {err}"))
        };
        let mut registry: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for line in read("policy/output_contracts.txt")?.lines() {
            if line.starts_with('#') {
                continue;
            }
            let mut fields = line.splitn(3, '|');
            if let (Some(kind), Some(value)) = (fields.next(), fields.next()) {
                registry
                    .entry(kind.to_string())
                    .or_default()
                    .insert(value.to_string());
            }
        }
        let mut violations = Vec::new();
        check_enum_completeness(
            &registry,
            &read("docs/OUTPUT_SCHEMA.md")?,
            &read,
            &mut violations,
        )?;
        assert!(violations.is_empty(), "{violations:#?}");
        Ok(())
    }
}
