use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{Inventory, capability_violations, language_id_variants, read_inventory};

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn inventory() -> Inventory {
    Inventory {
        enum_languages: set(&["rust", "typescript", "javascript"]),
        lang_features: set(&["lang-rust", "lang-typescript"]),
        adapter_dirs: set(&["rust", "typescript"]),
        spec_ids: set(&["RIPR-SPEC-0001"]),
    }
}

fn registry() -> Value {
    json!({
        "kind": "ripr_language_capabilities",
        "schema_version": "0.1",
        "semantic_digest": "sha256:abc",
        "languages": [
            {"language": "rust", "required_feature": "lang-rust"},
            {"language": "typescript", "required_feature": "lang-typescript"},
            {"language": "javascript", "required_feature": "lang-typescript"}
        ],
        "cells": [
            {"id": "rust/any/diff/static_facts", "language": "rust", "proof_refs": ["RIPR-SPEC-0001"]},
            {"id": "typescript/any/diff/static_facts", "language": "typescript", "proof_refs": ["RIPR-SPEC-0001"]},
            {"id": "javascript/any/diff/static_facts", "language": "javascript", "proof_refs": []}
        ]
    })
}

fn only(violations: &[String], needle: &str) -> Result<(), String> {
    if violations.len() == 1 && violations[0].contains(needle) {
        Ok(())
    } else {
        Err(format!(
            "expected exactly one `{needle}` violation, got {violations:?}"
        ))
    }
}

#[test]
fn consistent_registry_and_inventory_pass() {
    assert_eq!(
        capability_violations(&registry(), &inventory()),
        Vec::<String>::new()
    );
}

#[test]
fn language_declared_but_not_exported_fails() -> Result<(), String> {
    let mut inv = inventory();
    inv.enum_languages.insert("go".to_string());
    only(
        &capability_violations(&registry(), &inv),
        "`LanguageId::go` is declared",
    )
}

#[test]
fn registered_language_without_cell_fails() -> Result<(), String> {
    let mut reg = registry();
    reg["cells"]
        .as_array_mut()
        .ok_or("cells")?
        .retain(|cell| cell["language"] != "javascript");
    only(
        &capability_violations(&reg, &inventory()),
        "`javascript` has no capability cell",
    )
}

#[test]
fn cell_for_unregistered_language_fails() -> Result<(), String> {
    let mut reg = registry();
    reg["cells"]
        .as_array_mut()
        .ok_or("cells")?
        .push(json!({"id": "go/x", "language": "go", "proof_refs": []}));
    only(
        &capability_violations(&reg, &inventory()),
        "not a registered language",
    )
}

#[test]
fn adapter_directory_without_registered_feature_fails() -> Result<(), String> {
    let mut inv = inventory();
    inv.adapter_dirs.insert("go".to_string());
    only(
        &capability_violations(&registry(), &inv),
        "adapter directory",
    )
}

#[test]
fn registered_feature_without_adapter_directory_fails() -> Result<(), String> {
    let mut inv = inventory();
    inv.adapter_dirs.remove("typescript");
    only(
        &capability_violations(&registry(), &inv),
        "no adapter directory",
    )
}

#[test]
fn cargo_language_feature_without_registered_language_fails() -> Result<(), String> {
    let mut inv = inventory();
    inv.lang_features.insert("lang-go".to_string());
    only(
        &capability_violations(&registry(), &inv),
        "Cargo feature `lang-go`",
    )
}

#[test]
fn unresolved_proof_ref_fails() -> Result<(), String> {
    let mut inv = inventory();
    inv.spec_ids.clear();
    let violations = capability_violations(&registry(), &inv);
    if violations.len() == 2 && violations.iter().all(|v| v.contains("proof ref")) {
        Ok(())
    } else {
        Err(format!("{violations:?}"))
    }
}

#[test]
fn wrong_kind_or_empty_registry_fails_closed() {
    let violations = capability_violations(&json!({}), &inventory());
    assert!(violations.iter().any(|v| v.contains("`kind`")));
    assert!(violations.iter().any(|v| v.contains("`schema_version`")));
    assert!(violations.iter().any(|v| v.contains("semantic_digest")));
    assert!(
        violations
            .iter()
            .any(|v| v.contains("missing the `languages`"))
    );
}

#[test]
fn empty_language_list_is_not_an_established_inventory() {
    let mut reg = registry();
    reg["languages"] = json!([]);
    reg["cells"] = json!([]);
    let violations = capability_violations(&reg, &inventory());
    assert!(
        violations
            .iter()
            .any(|v| v.contains("exports no languages"))
    );
}

#[test]
fn enum_parser_reads_variants_and_rejects_unrecognized_shapes() -> Result<(), String> {
    let source =
        "pub enum LanguageId {\n    Rust,\n    // note\n    #[doc = \"x\"]\n    TypeScript,\n}\n";
    assert_eq!(language_id_variants(source)?, set(&["rust", "typescript"]));
    for bad in [
        "pub enum LanguageId {\n    Rust(u8),\n}\n",
        "enum Other {\n    A,\n}\n",
    ] {
        if let Ok(variants) = language_id_variants(bad) {
            return Err(format!("accepted unrecognized shape: {variants:?}"));
        }
    }
    Ok(())
}

#[test]
fn real_repository_inventory_is_nonempty_and_covers_known_adapters() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let inv = read_inventory(&root)?;
    for language in ["rust", "typescript", "javascript", "python", "perl"] {
        assert!(inv.enum_languages.contains(language), "{language}");
    }
    for dir in ["rust", "typescript", "python", "perl"] {
        assert!(inv.adapter_dirs.contains(dir), "{dir}");
        assert!(inv.lang_features.contains(&format!("lang-{dir}")), "{dir}");
    }
    assert!(inv.spec_ids.contains("RIPR-SPEC-0026"));
    Ok(())
}
