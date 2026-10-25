use serde_json::{Value, json};

use super::cell_join_violations;
use crate::{Capability, parse_capabilities_manifest_text};

fn manifest(fields: &str) -> Result<Vec<Capability>, String> {
    let text = format!("[[capability]]\nid = \"product_status\"\n{fields}\n");
    let (capabilities, errors) = parse_capabilities_manifest_text("test.toml", &text);
    if !errors.is_empty() {
        return Err(format!("{errors:?}"));
    }
    Ok(capabilities)
}

fn registry(ids: &[&str]) -> Value {
    json!({
        "kind": "ripr_language_capabilities", "schema_version": "0.1",
        "digest_scope": "whole_table", "filtered_by": null,
        "cells": ids.iter().map(|id| json!({"id": id, "state": "not_proven", "adapter_compiled": false})).collect::<Vec<_>>()
    })
}

#[test]
fn explicit_inline_and_multiline_cells_parse_and_join() -> Result<(), String> {
    for fields in [
        "cells = [\"python/pytest/diff/repair_routing\", \"python/unittest/diff/repair_routing\"]",
        "cells = [\n  \"python/pytest/diff/repair_routing\",\n  \"python/unittest/diff/repair_routing\"\n]",
    ] {
        let capabilities = manifest(fields)?;
        assert!(
            cell_join_violations(
                &capabilities,
                &registry(&[
                    "python/pytest/diff/repair_routing",
                    "python/unittest/diff/repair_routing"
                ])
            )
            .is_empty()
        );
    }
    Ok(())
}

#[test]
fn optional_per_entry_does_not_disable_global_completeness() -> Result<(), String> {
    let reg = registry(&["rust/any/diff/static_facts"]);
    for fields in ["", "cells = []"] {
        let violations = cell_join_violations(&manifest(fields)?, &reg);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert!(violations[0].contains("unclaimed"), "{violations:?}");
    }
    let mut capabilities = manifest("cells = [\"rust/any/diff/static_facts\"]")?;
    capabilities.extend(manifest("")?);
    assert!(cell_join_violations(&capabilities, &reg).is_empty());
    Ok(())
}

#[test]
fn near_identity_cannot_claim_a_registered_cell() -> Result<(), String> {
    // Vary each axis independently; no framework fallback, mode widening,
    // language-family aliasing, substring, or whitespace normalization.
    for wrong in [
        "javascript/any/diff/static_facts",
        "typescript/jest/diff/static_facts",
        "typescript/any/repo/static_facts",
        "typescript/any/diff/repair_routing",
        "typescript/any/diff/static_facts_extra",
        " typescript/any/diff/static_facts",
        "",
    ] {
        let fields = format!("cells = [\"{wrong}\"]");
        let violations = cell_join_violations(
            &manifest(&fields)?,
            &registry(&["typescript/any/diff/static_facts"]),
        );
        assert_eq!(violations.len(), 2, "{wrong:?}: {violations:?}");
        assert!(
            violations
                .iter()
                .any(|v| v.contains("claims missing registry cell"))
        );
        assert!(violations.iter().any(|v| v.contains("unclaimed")));
    }
    Ok(())
}

#[test]
fn a_new_registry_cell_needs_an_explicit_claim() -> Result<(), String> {
    let capabilities = manifest("cells = [\"rust/any/diff/static_facts\"]")?;
    let violations = cell_join_violations(
        &capabilities,
        &registry(&["rust/any/diff/static_facts", "rust/any/repo/static_facts"]),
    );
    assert_eq!(
        violations,
        vec![
            "registry cell `rust/any/repo/static_facts` is unclaimed by metrics/capabilities.toml"
        ]
    );
    Ok(())
}

#[test]
fn product_status_and_semantic_support_remain_separate() -> Result<(), String> {
    let capabilities = manifest("status = \"stable\"\ncells = [\"perl/any/diff/static_facts\"]")?;
    // Membership establishes no status equivalence or compiled availability.
    assert!(
        cell_join_violations(&capabilities, &registry(&["perl/any/diff/static_facts"])).is_empty()
    );
    Ok(())
}

#[test]
fn duplicate_claim_in_one_entry_is_rejected_but_shared_membership_is_valid() -> Result<(), String> {
    let single = manifest("cells = [\"rust/any/diff/static_facts\"]")?;
    let reg = registry(&["rust/any/diff/static_facts"]);
    let mut shared = single;
    shared[0].id = Some("another_product_capability".to_string());
    shared.extend(manifest("cells = [\"rust/any/diff/static_facts\"]")?);
    assert!(cell_join_violations(&shared, &reg).is_empty());
    let duplicate =
        manifest("cells = [\"rust/any/diff/static_facts\", \"rust/any/diff/static_facts\"]")?;
    let violations = cell_join_violations(&duplicate, &reg);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("repeats cell"));
    Ok(())
}

#[test]
fn malformed_cell_fields_fail_parsing() {
    for fields in [
        "cells = \"rust/any/diff/static_facts\"",
        "cells = [42]",
        "cells = [",
        "cells = []\ncells = []",
    ] {
        assert!(manifest(fields).is_err(), "{fields}");
    }
}

#[test]
fn incomplete_or_malformed_registry_cannot_pass_the_join() -> Result<(), String> {
    let capabilities = manifest("cells = [\"rust/any/diff/static_facts\"]")?;
    let good = registry(&["rust/any/diff/static_facts"]);
    for (key, bad) in [
        ("kind", json!("other")),
        ("schema_version", json!("0.2")),
        ("filtered_by", json!({"language": "rust"})),
        ("digest_scope", json!("filtered")),
        ("cells", json!([])),
        ("cells", Value::Null),
        ("cells", json!([{}])),
        ("cells", json!([{"id": ""}])),
        (
            "cells",
            json!([{"id": "rust/any/diff/static_facts"}, {"id": "rust/any/diff/static_facts"}]),
        ),
    ] {
        let mut broken = good.clone();
        broken[key] = bad;
        assert!(
            !cell_join_violations(&capabilities, &broken).is_empty(),
            "{broken}"
        );
    }
    let mut missing_filter = good;
    missing_filter
        .as_object_mut()
        .ok_or("object")?
        .remove("filtered_by");
    assert!(!cell_join_violations(&capabilities, &missing_filter).is_empty());
    Ok(())
}
