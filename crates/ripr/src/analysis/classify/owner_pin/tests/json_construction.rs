use super::*;

#[test]
fn literal_json_admission_preserves_binding_and_operand_refusals() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/owner_return_pin_json_real_literal/input");
    for (prelude, body, admitted) in [
        (
            "use serde_json::json;",
            "let p = json!({\"uri\":\"untitled:save\",\"text\":null}); assert_eq!(weight(4),12);",
            true,
        ),
        (
            "use serde_json::{Value, json};",
            "assert_eq!(weight(4), json!(12));",
            true,
        ),
        (
            "use serde_json::json;",
            "let p = json!(-7); assert_eq!(weight(4),12);",
            true,
        ),
        (
            "use serde_json::json;",
            "let p=json!(2147483647); assert_eq!(weight(4),12);",
            true,
        ),
        (
            "use serde_json::json;",
            "let p=json!(-2147483648); assert_eq!(weight(4),12);",
            true,
        ),
        (
            "use serde_json::json;",
            "let p=json!(2147483648); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "use serde_json::json;",
            r#"let p=json!("\u0041"); assert_eq!(weight(4),12);"#,
            false,
        ),
        (
            "use serde_json::json;",
            r#"let p=json!("\/"); assert_eq!(weight(4),12);"#,
            false,
        ),
        ("", "let p = json!(null); assert_eq!(weight(4),12);", false),
        (
            "use foreign::json;",
            "let p = json!(null); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "use serde_json::json; macro_rules! json { ($($t:tt)*) => { return }; }",
            "let p = json!(null); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "mod serde_json {} use serde_json::json;",
            "let p = json!(null); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "use other as serde_json; use serde_json::json;",
            "let p = json!(null); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "mod other { use serde_json::json; }",
            "let p = json!(null); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "#[cfg(any())] use serde_json::json;",
            "let p = json!(null); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "use serde_json::json;",
            "assert_eq!(weight(4),json!(other()));",
            false,
        ),
        (
            "use serde_json::json;",
            "let p = json!({\"x\": nested!()}); assert_eq!(weight(4),12);",
            false,
        ),
        (
            "use serde_json::json;",
            "assert_eq!(weight(4),json!(({return; 12})));",
            false,
        ),
        (
            "use serde_json::json;",
            "let _unused = || { let p=json!(null); assert_eq!(weight(4),12); };",
            false,
        ),
        (
            "use serde_json::json;",
            "let p=json!(1 . 0); assert_eq!(weight(4),12);",
            false,
        ),
    ] {
        let tests = format!("{prelude}\nuse demo::weight;\n#[test]\nfn checks() {{ {body} }}\n");
        let refusal = weight_refusal_under(&tests, &[], Some(&fixture));
        assert_eq!(refusal.is_none(), admitted, "{tests}: {refusal:?}");
    }
    Ok(())
}

#[test]
fn literal_json_namespace_in_another_source_file_stays_uncredited() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/owner_return_pin_json_real_literal/input");
    let tests = "use serde_json::json; use demo::weight; #[test] fn checks() { let p=json!(null); assert_eq!(weight(4),12); }";
    for parent in [
        "extern crate foreign as serde_json;",
        "use foreign as serde_json;",
        "mod serde_json {}",
    ] {
        assert!(
            weight_refusal_under(tests, &[("src/namespace.rs", parent)], Some(&fixture)).is_some(),
            "{parent}"
        );
    }
}

#[test]
fn literal_json_configuration_and_lock_changes_revoke_cached_identity() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/owner_return_pin_json_real_literal/input");
    let manifest =
        std::fs::read_to_string(fixture.join("Cargo.toml")).map_err(|e| e.to_string())?;
    let lock = std::fs::read_to_string(fixture.join("Cargo.lock")).map_err(|e| e.to_string())?;
    let root = crate::analysis::facts::drop_in::temp_workspace(
        "json-identity",
        &[
            ("Cargo.toml", &manifest),
            ("Cargo.lock", &lock),
            ("src/lib.rs", "pub fn weight(x:u32)->u32{x*3}"),
            ("tests/save.rs", "#[test] fn checks() {}"),
        ],
    )?;
    let result = (|| -> Result<(), String> {
        let authority = crate::analysis::facts::drop_in::DropInManifests::new(&root);
        let file = Path::new("tests/save.rs");
        assert!(
            authority.verified_literal_json(file),
            "canonical dependency resolves"
        );
        for config_path in [".cargo/config.toml", "tests/.cargo/config.toml"] {
            let path = root.join(config_path);
            std::fs::create_dir_all(path.parent().ok_or("config parent")?)
                .map_err(|e| e.to_string())?;
            std::fs::write(&path, "[source.crates-io]\nreplace-with='fake'\n")
                .map_err(|e| e.to_string())?;
            assert!(!authority.verified_literal_json(file), "{config_path}");
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
            assert!(authority.verified_literal_json(file));
        }
        let changed_lock = lock.replace(
            "c841b55ecdae098c80dcae9cf767f6f8a0c2cdb3416bbef72181df4d0fe73f14",
            "0000000000000000000000000000000000000000000000000000000000000000",
        );
        assert_ne!(changed_lock, lock);
        std::fs::write(root.join("Cargo.lock"), changed_lock).map_err(|e| e.to_string())?;
        assert!(!authority.verified_literal_json(file));
        std::fs::write(root.join("Cargo.lock"), &lock).map_err(|e| e.to_string())?;
        assert!(authority.verified_literal_json(file));
        for dependency in [
            "serde_json={version='=1.0.151',optional=true}",
            "serde_json={version='=1.0.151',package='foreign'}",
            "serde_json={path='fake'}",
        ] {
            let changed = manifest.replace("serde_json=\"=1.0.151\"", dependency);
            assert_ne!(changed, manifest);
            std::fs::write(root.join("Cargo.toml"), changed).map_err(|e| e.to_string())?;
            assert!(!authority.verified_literal_json(file), "{dependency}");
        }
        std::fs::write(root.join("Cargo.toml"), &manifest).map_err(|e| e.to_string())?;
        std::fs::remove_file(root.join("Cargo.lock")).map_err(|e| e.to_string())?;
        assert!(
            !authority.verified_literal_json(file),
            "unlocked dependency"
        );
        Ok(())
    })();
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    result
}

#[test]
fn literal_json_binding_in_a_withheld_file_remains_a_refusal() -> Result<(), String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/owner_return_pin_json_real_literal/input");
    let tests = "use serde_json::json; use demo::weight; #[test] fn checks() { let p=json!(null); assert_eq!(weight(4),12); }";
    let mut index = index(&[(LIB, WEIGHT_LIB), (TESTS, tests)]);
    index.drop_in_manifests = crate::analysis::facts::drop_in::DropInManifests::new(&fixture);
    let test = index
        .tests()
        .iter()
        .find(|t| t.file == Path::new(TESTS))
        .ok_or("one indexed test")?;
    assert_eq!(test.assertions.len(), 1);
    for source in [
        "extern crate foreign as serde_json;",
        "macro_rules! json { ($($t:tt)*) => { return }; }",
    ] {
        let mut withheld = WithheldMacroBindings::default();
        withheld.absorb(
            Path::new("hidden.rs"),
            source,
            index.macro_scope_crates(),
            &index.drop_in_manifests,
        );
        assert!(
            OwnerPinSyntax::default()
                .with_withheld(withheld)
                .refusal(test, &test.assertions[0], &index)
                .is_some(),
            "{source}"
        );
    }
    Ok(())
}
