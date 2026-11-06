//! RIPR-SPEC-0010: validate both brief profiles with the shared schema owner.

use super::validate_value_against_schema;
use serde_json::{Value, json};

fn documents() -> Result<(Value, Value, Value), String> {
    let parse = |text| serde_json::from_str(text).map_err(|error| error.to_string());
    Ok((
        parse(include_str!("../../../schemas/ripr/agent-brief.schema.json"))?,
        parse(include_str!(
            "../../../fixtures/boundary_gap/expected/editor-agent-loop/agent-brief.json"
        ))?,
        parse(include_str!(
            "../../../tests/fixtures/verification/ripr/agent-brief.line-ranges.valid.json"
        ))?,
    ))
}

fn violations(value: &Value, schema: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    validate_value_against_schema(
        value,
        schema,
        schema,
        "agent brief".to_string(),
        &mut errors,
    );
    errors
}

#[test]
fn agent_brief_schema_admits_full_and_ranged_profiles() -> Result<(), String> {
    let (schema, full, ranged) = documents()?;
    assert!(violations(&full, &schema).is_empty());
    assert!(violations(&ranged, &schema).is_empty());
    assert_eq!(full["top_seams"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        ranged["working_set"]["changed_line_ranges"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let mut empty = ranged;
    empty["working_set"]["changed_line_ranges"] = json!([]);
    assert!(violations(&empty, &schema).is_empty());
    Ok(())
}

#[test]
fn agent_brief_schema_rejects_mixed_missing_and_unknown_profiles() -> Result<(), String> {
    let (schema, full, ranged) = documents()?;
    let mut mixed = ranged.clone();
    mixed["working_set"]["changed_lines"] = json!([]);
    assert!(!violations(&mixed, &schema).is_empty());

    let mut marker_only = full.clone();
    marker_only["rendering_profile"] = json!("agent-brief-line-ranges-v1");
    assert!(!violations(&marker_only, &schema).is_empty());

    let mut unmarked = ranged.clone();
    unmarked
        .as_object_mut()
        .ok_or("brief object")?
        .remove("rendering_profile");
    assert!(!violations(&unmarked, &schema).is_empty());

    let mut unknown = ranged.clone();
    unknown["rendering_profile"] = json!("future-profile");
    assert!(!violations(&unknown, &schema).is_empty());

    for endpoint in [json!(-1), json!(1.5), json!("1")] {
        let mut invalid = ranged.clone();
        invalid["working_set"]["changed_line_ranges"][0]["ranges"][0][0] = endpoint;
        assert!(!violations(&invalid, &schema).is_empty());
    }

    for pair in [json!([]), json!([1]), json!([1, 2, 3])] {
        let mut invalid = ranged.clone();
        invalid["working_set"]["changed_line_ranges"][0]["ranges"][0] = pair;
        assert!(!violations(&invalid, &schema).is_empty());
    }
    let mut empty_group = ranged.clone();
    empty_group["working_set"]["changed_line_ranges"][0]["ranges"] = json!([]);
    assert!(!violations(&empty_group, &schema).is_empty());

    let mut missing = full;
    missing["working_set"]
        .as_object_mut()
        .ok_or("working set object")?
        .remove("changed_lines");
    assert!(!violations(&missing, &schema).is_empty());

    // Removal control: the alternatives, not an unrelated fixture error,
    // reject a working set that carries neither line representation.
    let mut without_alternatives = schema;
    without_alternatives
        .as_object_mut()
        .ok_or("schema object")?
        .remove("oneOf");
    assert!(violations(&missing, &without_alternatives).is_empty());
    Ok(())
}
