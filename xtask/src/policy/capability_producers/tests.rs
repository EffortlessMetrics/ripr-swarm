use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::{Inventory, capability_violations, language_id_variants, parse_args, read_inventory};

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
            {"id": "javascript/any/diff/static_facts", "language": "javascript", "proof_refs": ["RIPR-SPEC-0001"]}
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
        .push(json!({"id": "go/x", "language": "go", "proof_refs": ["RIPR-SPEC-0001"]}));
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
    if violations.len() == 3 && violations.iter().all(|v| v.contains("proof ref")) {
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

#[test]
fn malformed_empty_or_missing_proof_refs_fail() -> Result<(), String> {
    for (refs, needle) in [
        (Some(json!([null, 5])), "non-string"),
        (Some(json!([])), "empty `proof_refs`"),
        (None, "lacks a `proof_refs` array"),
    ] {
        let mut reg = registry();
        let cell = reg["cells"][0].as_object_mut().ok_or("cell")?;
        match refs {
            Some(value) => cell.insert("proof_refs".to_string(), value),
            None => cell.remove("proof_refs"),
        };
        let violations = capability_violations(&reg, &inventory());
        if !violations.iter().any(|v| v.contains(needle)) {
            return Err(format!("{needle}: {violations:?}"));
        }
    }
    Ok(())
}

#[test]
fn duplicate_language_entry_fails() -> Result<(), String> {
    let mut reg = registry();
    reg["languages"]
        .as_array_mut()
        .ok_or("languages")?
        .push(json!({"language": "rust", "required_feature": "lang-perl"}));
    let violations = capability_violations(&reg, &inventory());
    if violations.iter().any(|v| v.contains("more than once")) {
        Ok(())
    } else {
        Err(format!("{violations:?}"))
    }
}

#[test]
fn parse_args_accepts_known_shapes_and_rejects_the_rest() -> Result<(), String> {
    assert_eq!(parse_args(&[])?, super::Options::default());
    let ok = ["--registry-json", "r.json"].map(String::from);
    assert_eq!(
        parse_args(&ok)?.registry_file,
        Some(std::path::PathBuf::from("r.json"))
    );
    for bad in [
        vec!["--registry-json".to_string()],
        vec!["--bogus".to_string()],
        vec![
            "--registry-json".to_string(),
            "a".to_string(),
            "b".to_string(),
        ],
    ] {
        if let Ok(parsed) = parse_args(&bad) {
            return Err(format!("accepted {bad:?} as {parsed:?}"));
        }
    }
    Ok(())
}

#[test]
fn rendered_registry_matches_real_inventory() -> Result<(), String> {
    // A registry rendered from the real inventory (every enum language, its
    // feature, one cell each, a real spec id) must pass; removing one
    // language's cells must fail. Binds the pure check to real repo facts.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let inv = read_inventory(&root)?;
    let spec = inv.spec_ids.iter().next().ok_or("no specs")?.clone();
    let languages: Vec<Value> = inv
        .enum_languages
        .iter()
        .map(|l| {
            let dir = if l == "javascript" {
                "typescript"
            } else {
                l.as_str()
            };
            json!({"language": l, "required_feature": format!("lang-{dir}")})
        })
        .collect();
    let cells: Vec<Value> = inv
        .enum_languages
        .iter()
        .map(|l| json!({"id": format!("{l}/x"), "language": l, "proof_refs": [spec]}))
        .collect();
    let mut reg = json!({
        "kind": "ripr_language_capabilities", "schema_version": "0.1",
        "semantic_digest": "sha256:x", "languages": languages, "cells": cells
    });
    assert_eq!(capability_violations(&reg, &inv), Vec::<String>::new());
    reg["cells"]
        .as_array_mut()
        .ok_or("cells")?
        .retain(|c| c["language"] != "python");
    if capability_violations(&reg, &inv).is_empty() {
        return Err("missing python cell accepted".to_string());
    }
    Ok(())
}

#[test]
fn enum_parser_rejects_discriminants_and_multiline_attributes() -> Result<(), String> {
    for bad in [
        "pub enum LanguageId {\n    Rust = 1,\n}\n",
        "pub enum LanguageId {\n    Rust, // trailing\n}\n",
        "pub enum LanguageId {\n    #[serde(\n        rename = \"x\"\n    )]\n    Rust,\n}\n",
    ] {
        if let Ok(variants) = language_id_variants(bad) {
            return Err(format!("accepted {bad:?} as {variants:?}"));
        }
    }
    Ok(())
}

fn build_registry(compiled: &[&str]) -> Value {
    // Literal contract pins independent of cfg! or the production feature resolver.
    json!({"languages": [
        {"language": "rust", "required_feature": "lang-rust", "adapter_compiled": compiled.contains(&"rust")},
        {"language": "typescript", "required_feature": "lang-typescript", "adapter_compiled": compiled.contains(&"typescript")},
        {"language": "javascript", "required_feature": "lang-typescript", "adapter_compiled": compiled.contains(&"javascript")},
        {"language": "python", "required_feature": "lang-python", "adapter_compiled": compiled.contains(&"python")},
        {"language": "perl", "required_feature": "lang-perl", "adapter_compiled": compiled.contains(&"perl")}
    ]})
}

#[test]
fn minimal_and_individual_builds_require_exact_producer_presence() -> Result<(), String> {
    use super::features::{BuildSelection, compiled_feature_violations};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let manifest =
        std::fs::read_to_string(root.join(super::RIPR_CARGO_TOML)).map_err(|e| e.to_string())?;
    let inv = read_inventory(&root)?;
    let cases = [
        (None, vec![]),
        (Some("lang-rust"), vec!["rust"]),
        (Some("lang-typescript"), vec!["typescript", "javascript"]),
        (Some("lang-python"), vec!["python"]),
        (Some("lang-perl"), vec!["perl"]),
    ];
    assert_eq!(
        inv.lang_features,
        set(&["lang-rust", "lang-typescript", "lang-python", "lang-perl"]),
        "new language features need an explicit presence/absence control"
    );
    for (feature, compiled) in cases {
        let selection = BuildSelection {
            no_default_features: true,
            features: feature.into_iter().map(String::from).collect(),
        };
        let enabled = selection.enabled_features(&manifest)?;
        assert_eq!(enabled, feature.into_iter().map(String::from).collect());
        let good = build_registry(&compiled);
        assert_eq!(
            compiled_feature_violations(&good, &enabled),
            Vec::<String>::new()
        );
        // Flip EVERY language separately, testing missing enabled producers and
        // phantom disabled producers (including the shared TS/JS feature).
        for index in 0..5 {
            let mut bad = good.clone();
            let present = bad["languages"][index]["adapter_compiled"]
                .as_bool()
                .ok_or("bool")?;
            bad["languages"][index]["adapter_compiled"] = json!(!present);
            only(
                &compiled_feature_violations(&bad, &enabled),
                "adapter_compiled must be",
            )?;
        }
    }
    Ok(())
}

#[test]
fn selected_features_and_manifest_defaults_match_cargo_arguments() -> Result<(), String> {
    use super::features::compiled_feature_violations;
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(super::RIPR_CARGO_TOML),
    )
    .map_err(|e| e.to_string())?;
    let defaults = parse_args(&[])?;
    assert_eq!(
        defaults.build.enabled_features(&manifest)?,
        set(&["lang-rust", "lang-typescript", "lang-python"])
    );
    assert_eq!(
        defaults.build.cargo_args(),
        ["build", "-p", "ripr", "--quiet"]
    );
    assert_eq!(
        compiled_feature_violations(
            &build_registry(&["rust", "typescript", "javascript", "python"]),
            &defaults.build.enabled_features(&manifest)?
        ),
        Vec::<String>::new()
    );
    let options = parse_args(
        &[
            "--registry-json",
            "offline.json",
            "--no-default-features",
            "--features",
            "lang-python,lang-perl",
        ]
        .map(String::from),
    )?;
    assert_eq!(
        options.build.cargo_args(),
        [
            "build",
            "-p",
            "ripr",
            "--quiet",
            "--no-default-features",
            "--features",
            "lang-perl,lang-python"
        ]
    );
    assert_eq!(
        options.build.enabled_features(&manifest)?,
        set(&["lang-python", "lang-perl"])
    );
    let additive = parse_args(&["--features", "lang-perl"].map(String::from))?;
    assert_eq!(
        additive.build.enabled_features(&manifest)?,
        set(&["lang-rust", "lang-typescript", "lang-python", "lang-perl"])
    );
    let unknown = parse_args(&["--features", "lang-go"].map(String::from))?;
    assert!(unknown.build.enabled_features(&manifest).is_err());
    for args in [
        vec!["--features"],
        vec!["--features", ""],
        vec!["--features", "lang-rust,"],
        vec!["--features", "lang-rust,lang-rust"],
        vec!["--no-default-features", "--no-default-features"],
        vec!["--features", "lang-rust", "--features", "lang-perl"],
    ] {
        assert!(parse_args(&args.into_iter().map(String::from).collect::<Vec<_>>()).is_err());
    }
    Ok(())
}

#[test]
fn compiled_producers_fail_closed_on_missing_or_non_boolean_availability() -> Result<(), String> {
    use super::features::compiled_feature_violations;
    for availability in [json!(null), json!("false"), json!(0)] {
        let mut reg = build_registry(&[]);
        reg["languages"][0]["adapter_compiled"] = availability;
        only(
            &compiled_feature_violations(&reg, &set(&[])),
            "adapter_compiled must be false",
        )?;
    }
    let mut reg = build_registry(&[]);
    reg["languages"][0]
        .as_object_mut()
        .ok_or("language")?
        .remove("adapter_compiled");
    only(
        &compiled_feature_violations(&reg, &set(&[])),
        "adapter_compiled must be false",
    )
}
