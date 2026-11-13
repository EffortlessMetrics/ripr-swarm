use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn refusal<T>(result: Result<T, String>, expected: &str) -> Result<(), String> {
    let reason = result.err().ok_or("expected a diagnostic refusal")?;
    assert!(reason.contains(expected), "{reason}");
    Ok(())
}

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new(source: &str) -> Result<Self, String> {
        let temporary = std::env::temp_dir();
        fs::create_dir_all(&temporary).map_err(|e| e.to_string())?;
        let root = temporary.join(format!(
            "ripr-native-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let fixture = Self(fs::canonicalize(root).map_err(|e| e.to_string())?);
        let root = &fixture.0;
        fs::create_dir_all(root.join("crates/ripr-contract/src")).map_err(|e| e.to_string())?;
        fs::write(
            root.join("crates/ripr-contract/src/runtime_tests.rs"),
            source,
        )
        .map_err(|e| e.to_string())?;
        fs::write(root.join("outside.data"), b"included bytes").map_err(|e| e.to_string())?;
        Ok(fixture)
    }
    fn inputs(&self) -> Result<BTreeSet<String>, String> {
        let mut paths = BTreeSet::from(["crates/ripr-contract/src/runtime_tests.rs".to_string()]);
        extend_source_inputs(&self.0, &mut paths, &["ripr-contract".to_string()])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
#[test]
fn production_test_named_file_and_qualified_include_remain_in_the_closure() -> Result<(), String> {
    for expression in [
        "core::include_bytes!",
        "std::include_str!",
        "include_bytes!",
    ] {
        let fixture = Fixture::new(&format!(
            "const INPUT: &[u8] = {expression}(\"../../../outside.data\");"
        ))?;
        assert_eq!(
            fixture.inputs()?,
            BTreeSet::from(["outside.data".to_string()])
        );
    }
    Ok(())
}
#[test]
fn dynamic_or_macro_generated_inputs_are_refused_but_test_ancestry_is_explicit()
-> Result<(), String> {
    for (source, expected) in [
        (
            "const INPUT: &str = std::include_str!(concat!(env!(\"OUT_DIR\"), \"/body\"));",
            "dynamic/nonliteral include",
        ),
        (
            "macro_rules! generated { () => { core::include_bytes!(\"../../../outside.data\") }; }",
            "macro-generated includes",
        ),
    ] {
        refusal(Fixture::new(source)?.inputs(), expected)?;
    }
    let test = Fixture::new(
        "#[cfg(test)] mod control { const INPUT: &str = std::include_str!(concat!(env!(\"OUT_DIR\"), \"/body\")); }",
    )?;
    assert!(test.inputs()?.is_empty());
    Ok(())
}
#[cfg(unix)]
#[test]
fn source_include_refuses_a_symlink_even_when_it_targets_a_local_file() -> Result<(), String> {
    let fixture = Fixture::new("const INPUT: &[u8] = include_bytes!(\"../../../alias.data\");")?;
    std::os::unix::fs::symlink(fixture.0.join("outside.data"), fixture.0.join("alias.data"))
        .map_err(|e| e.to_string())?;
    refusal(fixture.inputs(), "included source traverses a symlink")?;
    let alias = "crates/ripr-contract/src/alias.rs";
    std::os::unix::fs::symlink(
        fixture.0.join("crates/ripr-contract/src/runtime_tests.rs"),
        fixture.0.join(alias),
    )
    .map_err(|e| e.to_string())?;
    refusal(
        extend_source_inputs(
            &fixture.0,
            &mut BTreeSet::from([alias.into()]),
            &["ripr-contract".into()],
        ),
        "source input traverses a symlink",
    )?;
    Ok(())
}
#[test]
fn external_test_ancestry_is_followed_and_a_production_edge_defeats_it() -> Result<(), String> {
    let fixture = Fixture::new(
        "const INPUT: &str = include_str!(concat!(env!(\"OUT_DIR\"), \"/generated\"));",
    )?;
    let owner = "crates/ripr-contract/src/lib.rs";
    fs::write(
        fixture.0.join(owner),
        "#[cfg(test)] #[path=\"runtime_tests.rs\"] mod control;",
    )
    .map_err(|e| e.to_string())?;
    let paths = BTreeSet::from([
        owner.to_string(),
        "crates/ripr-contract/src/runtime_tests.rs".to_string(),
    ]);
    assert!(
        extend_source_inputs(
            &fixture.0,
            &mut paths.clone(),
            &["ripr-contract".to_string()]
        )?
        .is_empty()
    );
    fs::write(fixture.0.join(owner), "#[cfg(test)] #[path=\"runtime_tests.rs\"] mod control; #[path=\"runtime_tests.rs\"] mod production;").map_err(|e| e.to_string())?;
    refusal(
        extend_source_inputs(
            &fixture.0,
            &mut paths.clone(),
            &["ripr-contract".to_string()],
        ),
        "dynamic/nonliteral include",
    )?;
    fs::write(
        fixture.0.join(owner),
        "#[cfg(test)] #[path=\"runtime_tests.rs\"] mod control; include!(\"runtime_tests.rs\");",
    )
    .map_err(|e| e.to_string())?;
    refusal(
        extend_source_inputs(
            &fixture.0,
            &mut paths.clone(),
            &["ripr-contract".to_string()],
        ),
        "dynamic/nonliteral include",
    )?;
    Ok(())
}
#[test]
fn package_and_runner_build_scripts_close_literal_inputs() -> Result<(), String> {
    let fixture = Fixture::new("pub fn ordinary() {}")?;
    fs::create_dir_all(fixture.0.join("xtask")).map_err(|e| e.to_string())?;
    fs::write(
        fixture.0.join("crates/ripr-contract/build.rs"),
        "const INPUT: &str = include_str!(\"../../outside.data\"); fn main() {}",
    )
    .map_err(|e| e.to_string())?;
    fs::write(
        fixture.0.join("xtask/build.rs"),
        "const INPUT: &str = include_str!(\"../outside.data\"); fn main() {}",
    )
    .map_err(|e| e.to_string())?;
    assert_eq!(
        fixture.inputs()?,
        BTreeSet::from([
            "crates/ripr-contract/build.rs".to_string(),
            "xtask/build.rs".to_string(),
            "outside.data".to_string()
        ])
    );
    Ok(())
}
#[test]
fn unsupported_inline_module_paths_refuse_instead_of_borrowing_a_physical_base()
-> Result<(), String> {
    for source in [
        "mod nested { #[path = \"../../../outside.data\"] mod production; }",
        "#[path = \"elsewhere\"] mod nested { mod production; }",
    ] {
        refusal(Fixture::new(source)?.inputs(), "inline module path")?;
    }
    Ok(())
}

#[test]
fn commented_and_raw_path_attributes_preserve_literal_spaces() -> Result<(), String> {
    for attribute in [
        "#[path /* before equals */ = \"../../../outside with spaces.rs\"]",
        "#[ /* before name */ path = /* before literal */ \"../../../outside with spaces.rs\" /* after literal */]",
        "#[r#path = \"../../../outside with spaces.rs\"]",
        "#[path\n=\n\"../../../outside with spaces.rs\"]",
    ] {
        let fixture = Fixture::new(&format!("{attribute} mod outside;"))?;
        fs::write(
            fixture.0.join("outside with spaces.rs"),
            "pub fn included() {}",
        )
        .map_err(|e| e.to_string())?;
        assert_eq!(
            fixture.inputs()?,
            BTreeSet::from(["outside with spaces.rs".to_string()]),
            "{attribute}"
        );
    }
    Ok(())
}

#[test]
fn include_trivia_and_raw_names_do_not_hide_inputs() -> Result<(), String> {
    for expression in [
        "core /* path */ :: include_bytes! /* call */ (/* argument */ \"../../../outside.data\", /* tail */)",
        "r#include_bytes!(\"../../../outside.data\")",
        "std::r#include_bytes!(\"../../../outside.data\")",
    ] {
        let fixture = Fixture::new(&format!("const INPUT: &[u8] = {expression};"))?;
        assert_eq!(
            fixture.inputs()?,
            BTreeSet::from(["outside.data".to_string()])
        );
    }
    Ok(())
}

#[test]
fn commented_conditional_module_paths_are_explicitly_refused() -> Result<(), String> {
    for attribute in [
        "#[cfg_attr(feature = \"choose\", path /* comment */ = \"../../../outside.rs\")]",
        "#[r#cfg_attr(feature = \"choose\", r#path = \"../../../outside.rs\")]",
    ] {
        let fixture = Fixture::new(&format!("{attribute} mod outside;"))?;
        let error = fixture
            .inputs()
            .err()
            .ok_or("conditional path was silently omitted")?;
        assert!(error.starts_with("conditional module path is outside captured input profile:"));
    }
    Ok(())
}

#[test]
fn commented_production_edge_defeats_test_only_exclusion() -> Result<(), String> {
    let fixture = Fixture::new(
        "const INPUT: &str = include_str!(concat!(env!(\"OUT_DIR\"), \"/generated\"));",
    )?;
    let owner = "crates/ripr-contract/src/lib.rs";
    let test_edge =
        "#[cfg(/* explanation */ test)] #[path /* test */ = \"runtime_tests.rs\"] mod control;";
    let paths = BTreeSet::from([
        owner.to_string(),
        "crates/ripr-contract/src/runtime_tests.rs".to_string(),
    ]);
    fs::write(fixture.0.join(owner), test_edge).map_err(|e| e.to_string())?;
    assert!(
        extend_source_inputs(
            &fixture.0,
            &mut paths.clone(),
            &["ripr-contract".to_string()]
        )?
        .is_empty()
    );
    fs::write(
        fixture.0.join(owner),
        format!("{test_edge} #[path /* production */ = \"runtime_tests.rs\"] mod production;"),
    )
    .map_err(|e| e.to_string())?;
    let error = extend_source_inputs(
        &fixture.0,
        &mut paths.clone(),
        &["ripr-contract".to_string()],
    )
    .err()
    .ok_or("a live production edge was excluded as test-only")?;
    assert!(error.starts_with("dynamic/nonliteral include is outside captured input profile:"));
    Ok(())
}

#[test]
fn ordinary_harness_attributes_are_trivia_insensitive() -> Result<(), String> {
    for source in [
        "#[kani /* namespace */ :: proof] #[kani::unwind(/* bound */ 4)] fn proof() {}",
        "#[r#kani::r#proof] #[r#kani::r#unwind(4)] fn proof() {}",
        "#[doc = \"keep spaces in literal text\"] #[doc(hidden)] #[allow(dead_code)] #[kani::proof] #[kani::unwind(4)] fn proof() {}",
    ] {
        assert_eq!(
            validate_harness_configuration(source, "proof", Some(4)),
            Ok(())
        );
        refusal(
            validate_harness_configuration(source, "proof", Some(5)),
            "attributes disagree with the declared proof/bounds profile",
        )?;
    }
    Ok(())
}

#[test]
fn doc_prefix_lookalikes_cannot_authorize_transforming_attributes() {
    for attribute in [
        "#[documentation]",
        "#[doc_transform(body)]",
        "#[document = \"replacement\"]",
    ] {
        let source = format!("{attribute} #[kani::proof] fn proof() {{}}");
        assert_eq!(
            validate_harness_configuration(&source, "proof", None),
            Err(
                "registered ordinary harness has an unsupported transforming or verifier attribute"
                    .to_string()
            )
        );
    }
}

#[test]
fn malformed_module_ancestry_is_not_admitted() -> Result<(), String> {
    for source in ["pub mod broken {", "#![cfg(test)]\nfn unfinished( {"] {
        let fixture = Fixture::new(source)?;
        refusal(fixture.inputs(), "cannot determine Rust module ancestry:")?;
    }
    Ok(())
}

#[test]
fn initial_source_inventory_cannot_bypass_the_file_budget() -> Result<(), String> {
    let fixture = Fixture::new("pub fn ordinary() {}")?;
    let mut paths: BTreeSet<String> = (0..=MAX_FILES).map(|i| format!("{i}.data")).collect();
    let before = paths.clone();
    assert_eq!(
        extend_source_inputs(&fixture.0, &mut paths, &["ripr-contract".to_string()]),
        Err("source closure exceeds 20,000 files".to_string())
    );
    assert_eq!(paths, before);
    fs::create_dir(fixture.0.join("xtask")).map_err(|e| e.to_string())?;
    fs::write(fixture.0.join("xtask/build.rs"), "fn main() {}").map_err(|e| e.to_string())?;
    let mut at_limit = (0..MAX_FILES).map(|i| format!("{i}.data")).collect();
    refusal(
        extend_source_inputs(&fixture.0, &mut at_limit, &["ripr-contract".into()]),
        "source closure exceeds 20,000 files",
    )?;
    refusal(
        extend_source_inputs(
            &fixture.0,
            &mut BTreeSet::from(["../outside.rs".into()]),
            &["ripr-contract".into()],
        ),
        "artifact path must remain relative",
    )?;
    refusal(
        extend_source_inputs(&fixture.0, &mut BTreeSet::new(), &["other".into()]),
        "unsupported source-input package",
    )?;
    Ok(())
}

#[test]
fn raw_macro_generated_include_is_not_silently_omitted() -> Result<(), String> {
    let fixture = Fixture::new(
        "macro_rules! generated { () => { r#include_bytes!(\"../../../outside.data\") }; }",
    )?;
    let error = fixture
        .inputs()
        .err()
        .ok_or("raw macro-generated include was omitted")?;
    assert!(
        error.starts_with("macro-generated includes require an explicit supported closure rule:")
    );
    Ok(())
}

#[test]
fn production_only_owner_cannot_hide_a_commented_module_edge() -> Result<(), String> {
    let fixture = Fixture::new(
        "const INPUT: &str = include_str!(concat!(env!(\"OUT_DIR\"), \"/generated\"));",
    )?;
    let test_owner = "crates/ripr-contract/src/lib.rs";
    let production_owner = "crates/ripr-contract/src/producer.rs";
    fs::write(
        fixture.0.join(test_owner),
        "#[cfg(test)] #[path=\"runtime_tests.rs\"] mod control;",
    )
    .map_err(|e| e.to_string())?;
    fs::write(fixture.0.join(production_owner), "pub fn ordinary() {}")
        .map_err(|e| e.to_string())?;
    let paths = BTreeSet::from([
        test_owner.to_string(),
        production_owner.to_string(),
        "crates/ripr-contract/src/runtime_tests.rs".to_string(),
    ]);
    assert!(
        extend_source_inputs(
            &fixture.0,
            &mut paths.clone(),
            &["ripr-contract".to_string()]
        )?
        .is_empty()
    );
    fs::write(
        fixture.0.join(production_owner),
        "#[path /* production */ = \"runtime_tests.rs\"] mod/* edge */live;",
    )
    .map_err(|e| e.to_string())?;
    let error = extend_source_inputs(
        &fixture.0,
        &mut paths.clone(),
        &["ripr-contract".to_string()],
    )
    .err()
    .ok_or("production module was omitted through spelling-dependent discovery")?;
    assert!(error.starts_with("dynamic/nonliteral include is outside captured input profile:"));
    Ok(())
}

#[test]
fn nested_macro_include_cannot_be_silently_omitted() -> Result<(), String> {
    for source in [
        "const INPUT: &str = concat!(include_str!(\"../../../outside.data\"));",
        "wrapper!(std::r#include_bytes! /* call */ (\"../../../outside.data\"));",
    ] {
        refusal(Fixture::new(source)?.inputs(), "nested macro includes")?;
    }
    assert!(
        Fixture::new("const TEXT: &str = stringify!(\"include_str!(outside)\");")?
            .inputs()?
            .is_empty()
    );
    Ok(())
}

#[test]
fn documentation_attribute_include_is_a_captured_input() -> Result<(), String> {
    let fixture =
        Fixture::new("#[doc = include_str!(\"../../../outside.data\")] pub fn documented() {}")?;
    assert_eq!(fixture.inputs()?, BTreeSet::from(["outside.data".into()]));
    Ok(())
}

#[test]
fn literal_rust_inputs_recursively_extend_the_captured_manifest() -> Result<(), String> {
    for source in [
        "include!(\"../../../included.rs\");",
        "#[path=\"../../../included.rs\"] mod input;",
        "#[r#path=\"../../../included.rs\"] mod input;",
    ] {
        let fixture = Fixture::new(source)?;
        fs::write(
            fixture.0.join("included.rs"),
            "const INPUT: &str = include_str!(\"outside.data\");",
        )
        .map_err(|e| e.to_string())?;
        let mut paths = BTreeSet::from(["crates/ripr-contract/src/runtime_tests.rs".into()]);
        let explicit = extend_source_inputs(&fixture.0, &mut paths, &["ripr-contract".into()])?;
        assert_eq!(
            explicit,
            BTreeSet::from(["included.rs".into(), "outside.data".into()])
        );
        assert_eq!(paths.len(), 3);
        assert!(explicit.is_subset(&paths));
    }
    Ok(())
}

#[test]
fn missing_foreign_and_non_utf8_rust_inputs_are_refused() -> Result<(), String> {
    let missing = Fixture::new("include_str!(\"../../../missing.data\");")?;
    refusal(missing.inputs(), "included source is unavailable")?;
    let foreign = Fixture::new("pub fn ordinary() {}")?;
    let outside = foreign.0.join("outside.data");
    let source = format!(
        "include_bytes!({:?});",
        outside.to_str().ok_or("fixture path is not UTF-8")?
    );
    refusal(
        Fixture::new(&source)?.inputs(),
        "included source escapes repository",
    )?;
    let invalid = Fixture::new("include!(\"../../../included.rs\");")?;
    fs::write(invalid.0.join("included.rs"), [0xff]).map_err(|e| e.to_string())?;
    refusal(invalid.inputs(), "utf-8")?;
    let binary = Fixture::new("include_bytes!(\"../../../outside.data\");")?;
    fs::write(binary.0.join("outside.data"), [0xff]).map_err(|e| e.to_string())?;
    assert_eq!(binary.inputs()?, BTreeSet::from(["outside.data".into()]));
    Ok(())
}

#[test]
fn ordinary_harness_refuses_ambiguous_proofs_and_bounds() -> Result<(), String> {
    validate_harness_configuration("#[kani::proof] fn proof() {}", "proof", None)?;
    for (source, expected) in [
        ("fn proof() {}", "attributes disagree"),
        (
            "#[kani::proof] #[kani::proof] fn proof() {}",
            "attributes disagree",
        ),
        (
            "#[kani::proof] #[kani::unwind(0)] fn proof() {}",
            "unwind must be positive",
        ),
        (
            "#[kani::proof] #[kani::unwind(32)] #[kani::unwind(32)] fn proof() {}",
            "duplicate registered harness unwind",
        ),
        (
            "#[kani::proof] fn other() {}",
            "function is absent or ambiguous",
        ),
        (
            "#[kani::proof] fn proof() {} mod child { #[kani::proof] fn proof() {} }",
            "function is absent or ambiguous",
        ),
        (
            "#[kani::proof] #[should_panic] fn proof() {}",
            "unsupported transforming or verifier attribute",
        ),
        (
            "#[kani::proof] #[kani::stub(target, replacement)] fn proof() {}",
            "unsupported transforming or verifier attribute",
        ),
        ("#[kani::proof] fn proof( {", "source cannot be parsed"),
    ] {
        refusal(
            validate_harness_configuration(source, "proof", Some(32)),
            expected,
        )?;
    }
    refusal(
        validate_harness_configuration(
            "#[kani::proof] #[kani::unwind(32)] fn proof() {}",
            "proof",
            None,
        ),
        "attributes disagree with the declared proof/bounds profile",
    )?;
    Ok(())
}
