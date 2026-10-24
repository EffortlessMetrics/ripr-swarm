use super::super::{FunctionSourceRole, build_index};
use super::*;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

fn index_for(source: &str) -> Result<RustIndex, Box<dyn Error>> {
    index_for_files(&[("src/lib.rs", source)])
}

fn index_for_files(files: &[(&str, &str)]) -> Result<RustIndex, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!(
        "ripr-test-helpers-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut paths = Vec::new();
    for (relative, source) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, source)?;
        if relative.ends_with(".rs") {
            paths.push(PathBuf::from(*relative));
        }
    }
    let index = build_index(&root, &paths);
    fs::remove_dir_all(&root)?;
    Ok(index?)
}

fn test_named<'index>(index: &'index RustIndex, name: &str) -> Result<&'index TestFact, String> {
    index
        .tests()
        .iter()
        .find(|test| test.name == name)
        .ok_or_else(|| {
            let names: Vec<&str> = index
                .tests()
                .iter()
                .map(|test| test.name.as_str())
                .collect();
            format!("premise: test `{name}` is indexed: {names:?}")
        })
}

fn calls(test: &TestFact) -> Vec<&str> {
    test.calls.iter().map(|call| call.name.as_str()).collect()
}

fn assertion_texts(test: &TestFact) -> Vec<&str> {
    test.assertions
        .iter()
        .map(|assertion| assertion.text.as_str())
        .collect()
}

const GATE: &str = "pub fn gate(x: u32) -> bool {\n    x > 10\n}\n\n";

#[test]
fn test_calling_cfg_test_helper_gains_the_helper_call_and_assertion() -> Result<(), Box<dyn Error>>
{
    let index = index_for(&format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn check(x: u32, want: bool) {{\n        assert_eq!(gate(x), want);\n    }}\n\n    #[test]\n    fn boundary() {{\n        check(10, false);\n        check(11, true);\n    }}\n}}\n"
    ))?;

    let test = test_named(&index, "boundary")?;

    assert!(calls(test).contains(&"check"), "{:?}", calls(test));
    assert!(calls(test).contains(&"gate"), "{:?}", calls(test));
    assert_eq!(assertion_texts(test), vec!["assert_eq!(gate(x), want);"]);
    assert_eq!(test.assertions[0].line, 10, "the helper's own line");
    let file_test = index
        .files()
        .get(Path::new("src/lib.rs"))
        .and_then(|facts| facts.tests.iter().find(|test| test.name == "boundary"))
        .ok_or("premise: per-file test fact exists")?;
    assert_eq!(file_test, test, "per-file and flat facts agree");
    Ok(())
}

#[test]
fn helper_called_twice_is_credited_once() -> Result<(), Box<dyn Error>> {
    let index = index_for(&format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn check(x: u32, want: bool) {{\n        assert_eq!(gate(x), want);\n    }}\n\n    #[test]\n    fn boundary() {{\n        check(10, false);\n        check(11, true);\n    }}\n}}\n"
    ))?;

    let test = test_named(&index, "boundary")?;

    assert_eq!(test.assertions.len(), 1, "{:?}", assertion_texts(test));
    Ok(())
}

#[test]
fn helpers_that_cannot_be_resolved_to_one_cfg_test_function_grant_nothing()
-> Result<(), Box<dyn Error>> {
    for (shape, helpers, body) in [
        (
            "production callee",
            "",
            "        assert!(!gate(10));\n        let _ = gate(11);\n",
        ),
        (
            "two same-file definitions",
            "    mod a {\n        pub fn check(x: u32, want: bool) { assert_eq!(super::super::gate(x), want); }\n    }\n    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        check(10, false);\n",
        ),
        (
            "method call",
            "    struct Case;\n    impl Case {\n        fn check(&self, x: u32, want: bool) { assert_eq!(gate(x), want); }\n    }\n",
            "        Case.check(10, false);\n",
        ),
        (
            "path call",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        other::check(10, false);\n",
        ),
        (
            "nested fn shadow",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        fn check(_: u32, _: bool) {}\n        check(10, false);\n",
        ),
        (
            "let shadow",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        let check = |_: u32, _: bool| {};\n        check(10, false);\n",
        ),
        (
            "call only in a string beside a method call",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        other::make().check(10, \"check(10, false) failed\");\n",
        ),
        (
            "cfg-gated helper",
            "    #[cfg(any())]\n    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        check(10, false);\n",
        ),
        (
            "cfg attribute with spaces",
            "    #[ cfg(any()) ]\n    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        check(10, false);\n",
        ),
        (
            "inner cfg attribute",
            "    fn check(x: u32, want: bool) {\n        #![cfg(any())]\n        assert_eq!(gate(x), want);\n    }\n",
            "        check(10, false);\n",
        ),
        (
            "for binding",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n    fn noop(_: u32, _: bool) {}\n",
            "        for check in [noop as fn(u32, bool)] {\n            check(10, false);\n        }\n",
        ),
        (
            "closure parameter",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n    fn noop(_: u32, _: bool) {}\n",
            "        [noop as fn(u32, bool)].iter().for_each(|check| check(10, false));\n",
        ),
        (
            "if let binding",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n    fn noop(_: u32, _: bool) {}\n",
            "        if let Some(check) = Some(noop as fn(u32, bool)) {\n            check(10, false);\n        }\n",
        ),
        (
            "let without initializer",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n    fn noop(_: u32, _: bool) {}\n",
            "        let check: fn(u32, bool);\n        check = noop;\n        check(10, false);\n",
        ),
        (
            "const item",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n    fn noop(_: u32, _: bool) {}\n",
            "        #[allow(non_upper_case_globals)]\n        const check: fn(u32, bool) = noop;\n        check(10, false);\n",
        ),
        (
            "tuple struct in the test body",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        struct check(u32, bool);\n        check(10, false);\n",
        ),
        (
            "cfg-gated statement in the test",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        #[cfg(any())]\n        check(10, false);\n",
        ),
        (
            "let closure in the helper body",
            "    fn check(x: u32, want: bool) {\n        let gate = |_: u32| true;\n        assert_eq!(gate(x), want);\n    }\n",
            "        check(10, false);\n",
        ),
        (
            "async helper",
            "    async fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        let _ = check(10, false);\n",
        ),
        (
            "helper returning an uncalled closure",
            "    fn check(x: u32, want: bool) -> impl Fn() {\n        move || assert_eq!(gate(x), want)\n    }\n",
            "        let _ = check(10, false);\n",
        ),
        (
            "async block in the helper body",
            "    fn check(x: u32, want: bool) {\n        let _ = async move { assert_eq!(gate(x), want) };\n    }\n",
            "        check(10, false);\n",
        ),
        (
            "call only inside an uncalled closure in the test",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        let _later = || check(10, false);\n",
        ),
        (
            "call only inside an unpolled async block in the test",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        let _later = async { check(10, false) };\n",
        ),
        (
            "call only inside an uncalled nested fn in the test",
            "    fn check(x: u32, want: bool) {\n        assert_eq!(gate(x), want);\n    }\n",
            "        fn _never() {\n            check(10, false);\n        }\n",
        ),
        (
            "use item in the helper body",
            "    fn check(x: u32, want: bool) {\n        use crate::other::gate;\n        assert_eq!(gate(x), want);\n    }\n",
            "        check(10, false);\n",
        ),
        (
            "nested fn in the helper body",
            "    fn check(x: u32, want: bool) {\n        fn gate(_: u32) -> bool {\n            true\n        }\n        assert_eq!(gate(x), want);\n    }\n",
            "        check(10, false);\n",
        ),
    ] {
        let index = index_for(&format!(
            "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{helpers}\n    #[test]\n    fn boundary() {{\n{body}    }}\n}}\n"
        ))?;

        let test = test_named(&index, "boundary")?;

        assert!(
            test.assertions
                .iter()
                .all(|assertion| !assertion.text.contains("want")),
            "{shape}: no helper assertion is credited: {:?}",
            assertion_texts(test)
        );
    }
    Ok(())
}

#[test]
fn helper_in_production_scope_is_not_an_assertion_helper() -> Result<(), Box<dyn Error>> {
    // A non-cfg(test) function the test calls is production code under
    // test, not a helper: its body must not become test evidence.
    let index = index_for(&format!(
        "{GATE}pub fn check(x: u32, want: bool) {{\n    assert_eq!(gate(x), want);\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    #[test]\n    fn boundary() {{\n        check(10, false);\n    }}\n}}\n"
    ))?;

    let test = test_named(&index, "boundary")?;

    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn integration_target_top_level_helper_stays_production_and_is_credited()
-> Result<(), Box<dyn Error>> {
    // #7125: a tests/*.rs helper is item-role Production (no cfg(test)
    // module), but the file is an integration target, so this producer
    // credits it without minting CfgTestModule.
    let index = index_for_files(&[
        ("src/lib.rs", GATE),
        (
            "tests/gate.rs",
            concat!(
                "fn check(input: u32, want: bool) {\n",
                "    assert_eq!(gate(input), want);\n",
                "}\n\n",
                "#[test]\n",
                "fn boundary() {\n",
                "    check(10, false);\n",
                "    check(11, true);\n",
                "}\n",
            ),
        ),
    ])?;

    let helper = index
        .functions()
        .iter()
        .find(|function| function.name == "check" && function.file.ends_with("gate.rs"))
        .ok_or("premise: the integration helper is indexed")?;
    assert_eq!(
        helper.source_role,
        FunctionSourceRole::Production,
        "the producer must not reclassify the helper: {:?}",
        helper.source_role
    );

    let test = test_named(&index, "boundary")?;
    assert!(calls(test).contains(&"check"), "{:?}", calls(test));
    assert!(calls(test).contains(&"gate"), "{:?}", calls(test));
    assert_eq!(
        assertion_texts(test),
        vec!["assert_eq!(gate(input), want);"]
    );
    Ok(())
}

#[test]
fn integration_target_assert_macro_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // #7125 is the assert_eq! check-helper cell. Copying assert!(owner(..))
    // from a tests/*.rs Production helper rewrites RIPR-SPEC-0114's last
    // established edge onto the public API the helper calls.
    let index = index_for_files(&[
        ("src/lib.rs", GATE),
        (
            "tests/gate.rs",
            concat!(
                "fn check(input: u32) {\n",
                "    assert!(gate(input));\n",
                "}\n\n",
                "#[test]\n",
                "fn boundary() {\n",
                "    check(11);\n",
                "}\n",
            ),
        ),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn integration_target_contains_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // RIPR-SPEC-0155: a tests/*.rs harness helper's .contains() is not
    // exact-oracle evidence. Copying its owner call still inflates
    // related_tests.
    let index = index_for_files(&[
        ("src/lib.rs", GATE),
        (
            "tests/gate.rs",
            concat!(
                "fn driver() {\n",
                "    let output = gate(11).to_string();\n",
                "    if !output.contains(\"true\") {\n",
                "        panic!(\"{output}\");\n",
                "    }\n",
                "}\n\n",
                "#[test]\n",
                "fn boundary() {\n",
                "    driver();\n",
                "}\n",
            ),
        ),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn production_file_top_level_helper_beside_a_top_level_test_is_not_credited()
-> Result<(), Box<dyn Error>> {
    // Same-module Production helpers in src/ are production code under
    // test, not integration-target evidence. Dropping the CfgTestModule
    // gate without the tests/** fence would falsely credit this.
    let index = index_for(&format!(
        "{GATE}fn check(x: u32, want: bool) {{\n    assert_eq!(gate(x), want);\n}}\n\n#[test]\nfn boundary() {{\n    check(10, false);\n}}\n"
    ))?;

    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn src_tests_module_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // `src/tests/gate.rs` is a production module directory (#6979). The
    // shared is_test_file check matches any `/tests/` component; this
    // producer must still refuse, or the helper's assert_eq! would
    // become test evidence and the owner could read exposed.
    let index = index_for_files(&[
        ("src/lib.rs", GATE),
        (
            "src/tests/gate.rs",
            concat!(
                "fn check(input: u32, want: bool) {\n",
                "    assert_eq!(gate(input), want);\n",
                "}\n\n",
                "#[test]\n",
                "fn boundary() {\n",
                "    check(10, false);\n",
                "}\n",
            ),
        ),
    ])?;
    let helper = index
        .functions()
        .iter()
        .find(|function| function.name == "check" && function.file.ends_with("gate.rs"))
        .ok_or("premise: the src/tests helper is indexed")?;
    assert_eq!(
        helper.source_role,
        FunctionSourceRole::Production,
        "premise: item role stays Production: {:?}",
        helper.source_role
    );
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn workspace_crate_integration_helper_is_credited() -> Result<(), Box<dyn Error>> {
    // `crates/*/tests/*.rs` is still a crate-root integration target.
    let index = index_for_files(&[
        ("crates/demo/src/lib.rs", GATE),
        (
            "crates/demo/tests/gate.rs",
            concat!(
                "fn check(input: u32, want: bool) {\n",
                "    assert_eq!(gate(input), want);\n",
                "}\n\n",
                "#[test]\n",
                "fn boundary() {\n",
                "    check(10, false);\n",
                "}\n",
            ),
        ),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(calls(test).contains(&"gate"), "{:?}", calls(test));
    assert_eq!(
        assertion_texts(test),
        vec!["assert_eq!(gate(input), want);"]
    );
    Ok(())
}

#[test]
fn crate_root_integration_layout_rejects_src_tests_and_keeps_tests_roots() {
    fn layout(path: &str) -> bool {
        super::is_crate_root_integration_test_file(
            Path::new(path),
            None,
            &mut super::ManifestInventory::default(),
        )
    }
    assert!(layout("tests/gate.rs"));
    assert!(layout("crates/demo/tests/gate.rs"));
    assert!(layout("tests\\gate.rs"));
    assert!(layout("tests/foo/main.rs"));
    assert!(!layout("src/tests/gate.rs"));
    assert!(!layout("src\\tests\\gate.rs"));
    assert!(!layout("crates/demo/src/tests/gate.rs"));
    assert!(!layout("src/lib.rs"));
    assert!(!layout("benches/gate.rs"));
    assert!(!layout("tests/support/gate.rs"));
    assert!(!layout("examples/tests/gate.rs"));
    assert!(!layout("benches/tests/gate.rs"));
    assert!(!layout("tests/foo/mod.rs"));
    // Nested-member autotest needs the owning manifest; path-only fallback
    // must not treat the first `tests` as that package root.
    assert!(!layout("tests/harness/tests/gate.rs"));
    assert!(!layout("tests/support/tests/gate.rs"));
}

const INTEGRATION_CHECK: &str = concat!(
    "fn check(input: u32, want: bool) {\n",
    "    assert_eq!(gate(input), want);\n",
    "}\n\n",
    "#[test]\n",
    "fn boundary() {\n",
    "    check(10, false);\n",
    "}\n",
);

#[test]
fn nested_package_integration_helper_is_credited() -> Result<(), Box<dyn Error>> {
    // A workspace member nested under `tests/` still owns `tests/gate.rs`
    // relative to its manifest (`tests/harness/tests/gate.rs`). The first
    // repository `tests` component is the ancestor directory, not the
    // autotest root.
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\nmembers = ['tests/harness']\n",
        ),
        ("src/lib.rs", GATE),
        (
            "tests/harness/Cargo.toml",
            "[package]\nname = 'harness'\nversion = '0.1.0'\nedition = '2021'\n",
        ),
        ("tests/harness/src/lib.rs", ""),
        ("tests/harness/tests/gate.rs", INTEGRATION_CHECK),
    ])?;
    let helper = index
        .functions()
        .iter()
        .find(|function| function.name == "check" && function.file.ends_with("gate.rs"))
        .ok_or("premise: the nested-package helper is indexed")?;
    assert_eq!(
        helper.source_role,
        FunctionSourceRole::Production,
        "the producer must not reclassify the helper: {:?}",
        helper.source_role
    );
    let test = test_named(&index, "boundary")?;
    assert!(calls(test).contains(&"gate"), "{:?}", calls(test));
    assert_eq!(
        assertion_texts(test),
        vec!["assert_eq!(gate(input), want);"]
    );
    Ok(())
}

#[test]
fn excluded_nested_package_integration_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // Same autotest remaining-path as the nested-member pin, but the
    // workspace excludes tests/harness. Cargo metadata does not list
    // that package; nearest-manifest path shape alone would over-credit.
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\nexclude = ['tests/harness']\n",
        ),
        ("src/lib.rs", GATE),
        (
            "tests/harness/Cargo.toml",
            "[package]\nname = 'harness'\nversion = '0.1.0'\nedition = '2021'\n",
        ),
        ("tests/harness/src/lib.rs", ""),
        ("tests/harness/tests/gate.rs", INTEGRATION_CHECK),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn test_false_integration_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // Declared [[test]] with test = false is a Cargo target, but cargo
    // test does not run it. ManifestInventory refuses HarnessEnabled from
    // metadata's test flag.
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n[[test]]\nname = 'gate'\ntest = false\n",
        ),
        ("src/lib.rs", GATE),
        ("tests/gate.rs", INTEGRATION_CHECK),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn test_false_parent_path_spelling_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // Producer-local TOML component matching missed `tests/../tests/gate.rs`
    // because it does not collapse `..`. Inventory keys are cargo's
    // lexically resolved src_path, so the skip holds.
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n[[test]]\nname = 'gate'\npath = 'tests/../tests/gate.rs'\ntest = false\n",
        ),
        ("src/lib.rs", GATE),
        ("tests/gate.rs", INTEGRATION_CHECK),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn required_features_unmet_integration_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // Declared [[test]] with required-features unmet under cargo defaults
    // is a Cargo target, but cargo test does not run it.
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n[features]\ndefault = []\nspecial = []\n[[test]]\nname = 'gate'\nrequired-features = ['special']\n",
        ),
        ("src/lib.rs", GATE),
        ("tests/gate.rs", INTEGRATION_CHECK),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn required_features_default_enabled_integration_helper_is_credited() -> Result<(), Box<dyn Error>>
{
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n[features]\ndefault = ['special']\nspecial = []\n[[test]]\nname = 'gate'\nrequired-features = ['special']\n",
        ),
        ("src/lib.rs", GATE),
        ("tests/gate.rs", INTEGRATION_CHECK),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(
        !test.assertions.is_empty(),
        "default-enabled required-features stay live: {:?}",
        assertion_texts(test)
    );
    assert!(calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn harness_false_integration_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // Declared [[test]] with harness = false is a Cargo target, but
    // libtest never collects its #[test] items.
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n[[test]]\nname = 'gate'\nharness = false\n",
        ),
        ("src/lib.rs", GATE),
        (
            "tests/gate.rs",
            concat!(
                "fn check(input: u32, want: bool) {\n",
                "    assert_eq!(gate(input), want);\n",
                "}\n\n",
                "#[test]\n",
                "fn boundary() {\n",
                "    check(10, false);\n",
                "}\n\n",
                "fn main() {}\n",
            ),
        ),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn nested_support_tests_helper_is_not_credited() -> Result<(), Box<dyn Error>> {
    // Same path shape as tests/harness/tests/gate.rs, but tests/support is
    // not a package. Nearest manifest is the root, remaining path is not
    // an autotest root, so last-`tests` path-only credit would over-credit.
    let index = index_for_files(&[
        (
            "Cargo.toml",
            "[package]\nname = 'root'\nversion = '0.1.0'\nedition = '2021'\n[workspace]\n",
        ),
        ("src/lib.rs", GATE),
        ("tests/support/tests/gate.rs", INTEGRATION_CHECK),
    ])?;
    let test = test_named(&index, "boundary")?;
    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    assert!(!calls(test).contains(&"gate"), "{:?}", calls(test));
    Ok(())
}

#[test]
fn nested_or_example_tests_helpers_are_not_credited() -> Result<(), Box<dyn Error>> {
    // Cargo does not treat nested tests/support/*.rs or examples/tests/**
    // as default autotest roots. Path-only credit must not promote them.
    for path in [
        "tests/support/gate.rs",
        "examples/tests/gate.rs",
        "benches/tests/gate.rs",
    ] {
        let index = index_for_files(&[
            ("src/lib.rs", GATE),
            (
                path,
                concat!(
                    "fn check(input: u32, want: bool) {\n",
                    "    assert_eq!(gate(input), want);\n",
                    "}\n\n",
                    "#[test]\n",
                    "fn boundary() {\n",
                    "    check(10, false);\n",
                    "}\n",
                ),
            ),
        ])?;
        let test = test_named(&index, "boundary")?;
        assert!(
            test.assertions.is_empty(),
            "{path}: {:?}",
            assertion_texts(test)
        );
        assert!(!calls(test).contains(&"gate"), "{path}: {:?}", calls(test));
    }
    Ok(())
}

#[test]
fn bench_and_example_helpers_are_not_credited() -> Result<(), Box<dyn Error>> {
    for path in ["benches/gate.rs", "examples/gate.rs"] {
        let index = index_for_files(&[
            ("src/lib.rs", GATE),
            (
                path,
                concat!(
                    "fn check(input: u32, want: bool) {\n",
                    "    assert_eq!(gate(input), want);\n",
                    "}\n\n",
                    "#[test]\n",
                    "fn boundary() {\n",
                    "    check(10, false);\n",
                    "}\n",
                ),
            ),
        ])?;
        let test = test_named(&index, "boundary")?;
        assert!(
            test.assertions.is_empty(),
            "{path}: {:?}",
            assertion_texts(test)
        );
        assert!(!calls(test).contains(&"gate"), "{path}: {:?}", calls(test));
    }
    Ok(())
}

#[test]
fn integration_target_helpers_keep_the_cfg_test_refusal_gates() -> Result<(), Box<dyn Error>> {
    for (shape, helper, body) in [
        (
            "let shadow",
            "fn check(x: u32, want: bool) {\n    assert_eq!(gate(x), want);\n}\n",
            "    let check = |_: u32, _: bool| {};\n    check(10, false);\n",
        ),
        (
            "call only inside an uncalled closure",
            "fn check(x: u32, want: bool) {\n    assert_eq!(gate(x), want);\n}\n",
            "    let _later = || check(10, false);\n",
        ),
        (
            "two same-file definitions",
            "mod a {\n    pub fn check(x: u32, want: bool) { assert_eq!(super::gate(x), want); }\n}\nfn check(x: u32, want: bool) {\n    assert_eq!(gate(x), want);\n}\n",
            "    check(10, false);\n",
        ),
    ] {
        let index = index_for_files(&[(
            "tests/gate.rs",
            &format!("{helper}\n#[test]\nfn boundary() {{\n{body}}}\n"),
        )])?;
        let test = test_named(&index, "boundary")?;
        assert!(
            test.assertions
                .iter()
                .all(|assertion| !assertion.text.contains("want")),
            "{shape}: {:?}",
            assertion_texts(test)
        );
    }
    Ok(())
}

#[test]
fn commented_out_helper_assertion_is_not_credited() -> Result<(), Box<dyn Error>> {
    let index = index_for(&format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn check(x: u32) {{\n        let _ = gate(x);\n        // assert_eq!(gate(x), false);\n    }}\n\n    #[test]\n    fn boundary() {{\n        check(10);\n    }}\n}}\n"
    ))?;

    let test = test_named(&index, "boundary")?;

    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    Ok(())
}

#[test]
fn credited_helper_calls_stay_out_of_the_test_body_calls() -> Result<(), Box<dyn Error>> {
    // The helper's `gate(x)` names the helper's parameter; the test's own
    // `let x = 10` must not bind it.
    let index = index_for(&format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn check(x: u32, want: bool) {{\n        assert_eq!(gate(x), want);\n    }}\n\n    #[test]\n    fn boundary() {{\n        let x = 10;\n        check(x + 5, true);\n    }}\n}}\n"
    ))?;

    let test = test_named(&index, "boundary")?;

    assert!(calls(test).contains(&"gate"), "premise: {:?}", calls(test));
    let body_calls = test
        .body_calls()
        .map(|call| call.name.as_str())
        .collect::<Vec<_>>();
    assert!(body_calls.contains(&"check"), "{body_calls:?}");
    assert!(!body_calls.contains(&"gate"), "{body_calls:?}");
    Ok(())
}

#[test]
fn helper_outside_the_test_module_is_not_credited() -> Result<(), Box<dyn Error>> {
    // Reviews of #4715: a test in `mod smoke` calling `check` resolves to
    // its own import, not to the unique `check` in sibling `mod strict`; a
    // `use` in the test body, a parent-module helper (`use super::*`) and a
    // helper nested in another fn's body are not credited either.
    for (shape, source) in [
        (
            "sibling module with an import",
            format!(
                "{GATE}pub mod testutil;\n\n#[cfg(test)]\nmod strict {{\n    fn check(x: u32, want: bool) {{\n        assert_eq!(super::gate(x), want);\n    }}\n}}\n\n#[cfg(test)]\nmod smoke {{\n    use crate::testutil::check;\n\n    #[test]\n    fn boundary() {{\n        check(10, false);\n    }}\n}}\n"
            ),
        ),
        (
            "parent module",
            format!(
                "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn check(x: u32, want: bool) {{\n        assert_eq!(gate(x), want);\n    }}\n\n    mod inner {{\n        use super::*;\n\n        #[test]\n        fn boundary() {{\n            check(10, false);\n        }}\n    }}\n}}\n"
            ),
        ),
        (
            "helper on the test's line",
            format!(
                "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn check(x: u32, want: bool) {{ assert_eq!(gate(x), want) }} #[test] fn boundary() {{ check(10, false); }}\n}}\n"
            ),
        ),
        (
            "use item in the test body",
            format!(
                "{GATE}pub mod testutil;\n\n#[cfg(test)]\nmod tests {{\n    fn check(x: u32, want: bool) {{\n        assert_eq!(super::gate(x), want);\n    }}\n\n    #[test]\n    fn boundary() {{\n        use crate::testutil::check;\n        check(10, false);\n    }}\n}}\n"
            ),
        ),
        (
            "helper nested in another fn's body",
            format!(
                "{GATE}pub mod testutil;\n\n#[cfg(test)]\nmod tests {{\n    use crate::testutil::check;\n\n    fn strict_suite() {{\n        fn check(x: u32, want: bool) {{\n            assert_eq!(super::gate(x), want);\n        }}\n        check(0, false);\n    }}\n\n    #[test]\n    fn boundary() {{\n        check(10, false);\n    }}\n}}\n"
            ),
        ),
    ] {
        let index = index_for(&source)?;

        let test = test_named(&index, "boundary")?;

        assert!(
            test.calls.iter().all(|call| call.name != "gate"),
            "{shape}: premise and result: no helper call is credited: {:?}",
            test.calls
        );
        assert!(
            test.assertions.is_empty(),
            "{shape}: no helper assertion is credited: {:?}",
            assertion_texts(test)
        );
    }
    Ok(())
}

#[test]
fn the_parser_stores_the_scopes_crediting_would_parse_and_the_cache_keeps_them()
-> Result<(), Box<dyn Error>> {
    // Warm crediting reads the producer's stored scopes instead of parsing
    // the test file again (#5363), so they must answer every question
    // crediting asks exactly as a fresh parse does, and survive the
    // file-fact cache's wire form.
    let source = format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    fn check(x: u32, want: bool) {{\n        let check_twice = |v| gate(v);\n        assert_eq!(gate(x), want);\n        assert!(check_twice(x) || true);\n    }}\n\n    #[test]\n    fn boundary() {{\n        let local = 1;\n        check(10, false);\n        helper_elsewhere();\n    }}\n\n    #[cfg(feature = \"x\")]\n    #[test]\n    fn gated() {{ check(1, false); }}\n}}\n"
    );
    let facts =
        crate::analysis::syntax::ra::summarize_file_with_parser(Path::new("src/lib.rs"), &source)?;
    let stored = facts
        .item_scopes
        .clone()
        .ok_or("premise: parser-backed facts carry scopes")?;
    let fresh = module_item_scopes(&source).ok_or("premise: the source parses cleanly")?;

    assert_eq!(stored.item_fns, fresh.item_fns);
    assert_eq!(stored.fns_with_local_use, fresh.fns_with_local_use);
    assert_eq!(stored.fns_with_deferred_code, fresh.fns_with_deferred_code);
    assert_eq!(stored.fns_with_cfg, fresh.fns_with_cfg);
    let function_names: BTreeSet<&str> = facts
        .functions
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    for (key, called) in &fresh.direct_calls {
        let kept: BTreeSet<&String> = called
            .iter()
            .filter(|name| function_names.contains(name.as_str()))
            .collect();
        let stored_called = stored
            .direct_calls
            .get(key)
            .ok_or("direct calls kept per fn")?;
        assert!(
            kept.iter().all(|name| stored_called.contains(*name)),
            "{key:?}"
        );
    }
    for (key, bound) in &fresh.bound_names {
        let stored_bound = stored
            .bound_names
            .get(key)
            .ok_or("bound names kept per fn")?;
        assert!(stored_bound.is_subset(bound), "{key:?}");
        for function in facts.functions.iter() {
            let asked = std::iter::once(function.name.as_str())
                .chain(function.calls.iter().map(|call| call.name.as_str()));
            for name in asked {
                assert_eq!(
                    stored_bound.contains(name),
                    bound.contains(name),
                    "{key:?} binds {name}"
                );
            }
        }
    }
    // A local binding that names no function or call is dropped.
    assert!(
        stored
            .bound_names
            .values()
            .all(|bound| !bound.contains("local")),
        "{stored:?}"
    );

    let wire = serde_json::to_string(&facts)?;
    let decoded: crate::analysis::facts::FileFacts = serde_json::from_str(&wire)?;
    assert_eq!(decoded.item_scopes, facts.item_scopes);
    Ok(())
}

const CALLBACK_HELPER: &str = "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n";

fn callback_source(helper: &str, body: &str) -> String {
    format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{helper}\n    #[test]\n    fn boundary() {{\n{body}    }}\n}}\n"
    )
}

#[test]
fn closure_callback_helper_credits_its_assertion_to_the_calling_test() -> Result<(), Box<dyn Error>>
{
    let index = index_for(&callback_source(
        CALLBACK_HELPER,
        "        run_test(true, || gate(11));\n",
    ))?;

    let test = test_named(&index, "boundary")?;

    assert_eq!(assertion_texts(test), vec!["assert_eq!(got, want);"]);
    assert!(calls(test).contains(&"gate"), "{:?}", calls(test));
    assert!(
        !calls(test).contains(&"f"),
        "the callback parameter names no function: {:?}",
        calls(test)
    );
    Ok(())
}

#[test]
fn closure_callback_helper_credits_a_block_closure_and_a_later_position()
-> Result<(), Box<dyn Error>> {
    let index = index_for(&callback_source(
        "    fn run_test(f: impl FnOnce() -> bool, want: bool) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
        "        run_test(|| {\n            gate(11)\n        }, true);\n",
    ))?;

    let test = test_named(&index, "boundary")?;

    assert_eq!(assertion_texts(test), vec!["assert_eq!(got, want);"]);
    Ok(())
}

#[test]
fn closure_callback_helpers_that_may_not_observe_the_closure_grant_nothing()
-> Result<(), Box<dyn Error>> {
    for (shape, helper, body) in [
        (
            "callback invoked only under if false",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        if false {\n            let _ = f();\n        }\n        assert_eq!(want, true);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback invoked only under if false on one line",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        if false { let _ = f(); }\n        assert_eq!(want, true);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback invoked only in a loop",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        for _ in 0..0 {\n            let _ = f();\n        }\n        assert_eq!(want, true);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback invoked after a possible early return",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        if want {\n            return;\n        }\n        let _ = f();\n        assert_eq!(want, true);\n    }\n",
            "        run_test(false, || gate(11));\n",
        ),
        (
            "same-line assertion before the invocation",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        assert_eq!(want, true); let _ = f();\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "mutable callback parameter, even if never reassigned",
            "    fn run_test(want: bool, mut f: impl FnMut() -> bool) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "parenthesized assignment to the callback parameter",
            "    fn run_test(want: bool, mut f: fn() -> bool) {\n        (f) = always_true;\n        let _ = f();\n        assert_eq!(want, true);\n    }\n    fn always_true() -> bool {\n        true\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "destructuring assignment to the callback parameter",
            "    fn run_test(want: bool, mut f: fn() -> bool) {\n        (f,) = (always_true as fn() -> bool,);\n        let _ = f();\n        assert_eq!(want, true);\n    }\n    fn always_true() -> bool {\n        true\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "mem replace of the callback parameter",
            "    fn run_test(want: bool, mut f: fn() -> bool) {\n        std::mem::replace(&mut (f), always_true);\n        let _ = f();\n        assert_eq!(want, true);\n    }\n    fn always_true() -> bool {\n        true\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback parameter reassigned before the invocation",
            "    fn run_test(want: bool, mut f: fn() -> bool) {\n        f = always_true;\n        let _ = f();\n        assert_eq!(want, true);\n    }\n    fn always_true() -> bool {\n        true\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "opaque macro before the invocation may exit",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        early_exit!();\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "helper call under if false in the test",
            CALLBACK_HELPER,
            "        if false {\n            run_test(true, || gate(11));\n        }\n",
        ),
        (
            "helper call after an opaque macro in the test",
            CALLBACK_HELPER,
            "        early_exit!();\n        run_test(true, || gate(11));\n",
        ),
        (
            "async closure literal",
            "    fn run_test<F, T>(want: bool, f: F) where F: FnOnce() -> T {\n        let _ = f();\n        assert_eq!(want, true);\n    }\n",
            "        run_test(true, async || gate(11));\n",
        ),
        (
            "repeated identical assertion spelling around the invocation on one line",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        assert_eq!(want, true); let _ = f(); assert_eq!(want, true);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback invoked only inside an assertion macro argument",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        assert_eq!(f(), want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "exit in an earlier argument of the helper call",
            CALLBACK_HELPER,
            "        run_test({ return; true }, || gate(11));\n",
        ),
        (
            "exit in a later argument, callback first",
            "    fn run_first(f: impl FnOnce() -> bool, want: bool) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_first(|| gate(11), { return; true });\n",
        ),
        (
            "exit in an argument of a let-initializer helper call",
            CALLBACK_HELPER,
            "        let _ = run_test({ return; true }, || gate(11));\n",
        ),
        (
            "exit in an argument of a tail helper call",
            CALLBACK_HELPER,
            "        run_test({ return; true }, || gate(11))\n",
        ),
        (
            "return written in the callback literal itself",
            CALLBACK_HELPER,
            "        run_test(true, || { return gate(11); });\n",
        ),
        (
            "question mark written in the callback literal itself",
            CALLBACK_HELPER,
            "        run_test(true, || { gate(11)?; true });\n",
        ),
        (
            "exit hidden in a format argument of the helper call",
            CALLBACK_HELPER,
            "        run_test(format!(\"{}\", { return; 1 }).is_empty(), || gate(11));\n",
        ),
        (
            "exit hidden in a preceding transparent macro statement",
            CALLBACK_HELPER,
            "        let _ = format!(\"{}\", { return; \"x\" });\n        run_test(true, || gate(11));\n",
        ),
        (
            "question mark hidden in a preceding transparent macro",
            CALLBACK_HELPER,
            "        let _ = format!(\"{}\", missing()?);\n        run_test(true, || gate(11));\n",
        ),
        (
            "exit hidden in a transparent macro before the helper's invocation",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let _ = format!(\"{}\", { return; 1 });\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "closure whose only call is under an async block",
            CALLBACK_HELPER,
            "        run_test(true, || async { gate(11) });\n",
        ),
        (
            "closure whose only call is under a nested closure",
            CALLBACK_HELPER,
            "        run_test(true, || || gate(11));\n",
        ),
        (
            "boxed closure",
            "    fn run_test(want: bool, f: Box<dyn Fn() -> bool>) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, Box::new(|| gate(11)));\n",
        ),
        (
            "reference to a closure",
            "    fn run_test(want: bool, f: &dyn Fn() -> bool) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, &|| gate(11));\n",
        ),
        (
            "closure at a position the helper does not invoke, plain value at the invoked one",
            "    fn run_test(f: impl FnOnce() -> bool, g: impl FnOnce() -> bool) {\n        let got = g();\n        let _ = f;\n        assert_eq!(got, true);\n    }\n",
            "        let g = || true;\n        run_test(|| gate(11), g);\n",
        ),
        (
            "assertion runs before the callback",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        assert_eq!(want, want);\n        let _ = f();\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback invoked only inside a helper closure",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let run = move || f();\n        let got = run();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "closure passed at a parameter the helper does not invoke",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool, g: impl FnOnce() -> bool) {\n        let got = g();\n        let _ = f;\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11), || true);\n",
        ),
        (
            "callback parameter shadowed by a let",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let f = || true;\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback parameter shadowed by a match binding",
            "    fn run_test(want: bool, f: Option<fn() -> bool>) {\n        let got = match Some(|| true) {\n            Some(f) => f(),\n            None => false,\n        };\n        assert_eq!(got, want);\n        let _ = f;\n    }\n",
            "        run_test(true, Some(|| gate(11)));\n",
        ),
        (
            "argument is a plain value, not a closure literal",
            CALLBACK_HELPER,
            "        let g = || gate(11);\n        run_test(true, g);\n",
        ),
        (
            "closure that makes no call",
            CALLBACK_HELPER,
            "        run_test(true, || true);\n",
        ),
        (
            "helper call sits inside a test closure that may never run",
            CALLBACK_HELPER,
            "        let later = || run_test(true, || gate(11));\n        let _ = later;\n",
        ),
        (
            "test shadows the helper name",
            CALLBACK_HELPER,
            "        let run_test = |_: bool, f: fn() -> bool| { let _ = f; };\n        run_test(true, || gate(11));\n",
        ),
        (
            "async helper",
            "    async fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        let _ = run_test(true, || gate(11));\n",
        ),
        (
            "cfg-gated helper",
            "    #[cfg(any())]\n    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
        (
            "callback method call, not a free call",
            "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let got = Some(1).map(|_| f()).unwrap_or(false);\n        assert_eq!(got, want);\n    }\n",
            "        run_test(true, || gate(11));\n",
        ),
    ] {
        let index = index_for(&callback_source(helper, body))?;
        let test = test_named(&index, "boundary")?;
        assert!(
            test.assertions.is_empty(),
            "{shape}: {:?}",
            assertion_texts(test)
        );
    }
    Ok(())
}

// A helper that invokes the callback but asserts nothing grants no assertion
// either, but that holds with or without the credit (nothing to copy), so it
// is not a discriminating case for the new guards and is not tabled above.

#[test]
fn closure_callback_credit_covers_generic_bounds_and_refuses_two_invoked_callbacks()
-> Result<(), Box<dyn Error>> {
    let generic = index_for(&callback_source(
        "    fn run_test<F: Fn() -> bool>(want: bool, f: F) {\n        let got = f();\n        assert_eq!(got, want);\n    }\n",
        "        run_test(true, || gate(11));\n",
    ))?;
    assert_eq!(
        assertion_texts(test_named(&generic, "boundary")?),
        vec!["assert_eq!(got, want);"]
    );

    // A helper that invokes two callback parameters earns no credit, even
    // when only one reaches a call-making closure literal.
    for (shape, helper, body) in [
        (
            "both closures reached",
            "    fn run_both(a: impl FnOnce() -> bool, b: impl FnOnce() -> bool) {\n        let x = a();\n        let y = b();\n        assert_eq!(x, y);\n    }\n",
            "        run_both(|| gate(11), || gate(12));\n",
        ),
        (
            "one closure reached, plain value at the other",
            "    fn run_both(a: impl FnOnce() -> bool, b: impl FnOnce() -> bool) {\n        let x = a();\n        let y = b();\n        assert_eq!(x, y);\n    }\n",
            "        let b = || true;\n        run_both(|| gate(11), b);\n",
        ),
        (
            "assertion between the invocations observes only the first",
            "    fn run_both(a: impl FnOnce() -> bool, b: impl FnOnce() -> bool) {\n        let x = a();\n        assert_eq!(x, true);\n        let y = b();\n        let _ = y;\n    }\n",
            "        run_both(|| setup(), || gate(11));\n",
        ),
    ] {
        let index = index_for(&callback_source(helper, body))?;
        let test = test_named(&index, "boundary")?;
        assert!(
            test.assertions.is_empty(),
            "{shape}: {:?}",
            assertion_texts(test)
        );
    }
    Ok(())
}

#[test]
fn closure_callback_credit_orders_same_line_statements_by_source_position()
-> Result<(), Box<dyn Error>> {
    let after = index_for(&callback_source(
        "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let got = f(); assert_eq!(got, want);\n    }\n",
        "        run_test(true, || gate(11));\n",
    ))?;
    assert_eq!(
        assertion_texts(test_named(&after, "boundary")?),
        vec!["assert_eq!(got, want);"]
    );

    let whole_fn_on_one_line = index_for(&callback_source(
        "    fn run_test(want: bool, f: impl FnOnce() -> bool) { let got = f(); assert_eq!(got, want); }\n",
        "        run_test(true, || gate(11));\n",
    ))?;
    assert_eq!(
        assertion_texts(test_named(&whole_fn_on_one_line, "boundary")?),
        vec!["assert_eq!(got, want);"]
    );

    let commented = index_for(&callback_source(
        "    fn run_test(want: bool, f: impl FnOnce() -> bool) { /* assert_eq!(got, want); */ let got = f(); assert_eq!(got, want); }\n",
        "        run_test(true, || gate(11));\n",
    ))?;
    assert_eq!(
        assertion_texts(test_named(&commented, "boundary")?),
        vec!["assert_eq!(got, want);"],
        "a comment that repeats the spelling must not hide the real assertion"
    );

    let before = index_for(&callback_source(
        "    fn run_test(want: bool, f: impl FnOnce() -> bool) { assert_eq!(want, true); let _ = f(); }\n",
        "        run_test(true, || gate(11));\n",
    ))?;
    assert!(test_named(&before, "boundary")?.assertions.is_empty());
    Ok(())
}

#[test]
fn closure_callback_credit_ignores_an_unrelated_nested_fn_exit_in_the_test()
-> Result<(), Box<dyn Error>> {
    let index = index_for(&callback_source(
        CALLBACK_HELPER,
        "        fn other() {\n            return;\n        }\n        run_test(true, || gate(11));\n",
    ))?;

    assert_eq!(
        assertion_texts(test_named(&index, "boundary")?),
        vec!["assert_eq!(got, want);"]
    );
    Ok(())
}

#[test]
fn closure_callback_credit_keeps_benign_transparent_macro_arguments() -> Result<(), Box<dyn Error>>
{
    let index = index_for(&callback_source(
        CALLBACK_HELPER,
        "        let label = 1;\n        let _ = format!(\"{}\", label);\n        run_test(format!(\"{}\", label).len() > 0, || gate(11));\n",
    ))?;

    assert_eq!(
        assertion_texts(test_named(&index, "boundary")?),
        vec!["assert_eq!(got, want);"]
    );
    Ok(())
}

#[test]
fn closure_callback_credit_keeps_a_return_after_an_executed_helper_call()
-> Result<(), Box<dyn Error>> {
    let index = index_for(&callback_source(
        CALLBACK_HELPER,
        "        run_test(true, || gate(11));\n        return;\n",
    ))?;

    assert_eq!(
        assertion_texts(test_named(&index, "boundary")?),
        vec!["assert_eq!(got, want);"]
    );
    Ok(())
}

#[test]
fn closure_callback_credit_accepts_an_assertion_unrelated_to_the_closure_result()
-> Result<(), Box<dyn Error>> {
    // Documented accepted limit: the helper really runs the closure and then
    // asserts, so the assertion is credited even though it does not read what
    // the closure returned. The existing argument-helper path has the same
    // reach; value-level grip is judged downstream, not here.
    let index = index_for(&callback_source(
        "    fn run_test(want: bool, f: impl FnOnce() -> bool) {\n        let _ = f();\n        assert_eq!(want, true);\n    }\n",
        "        run_test(true, || gate(11));\n",
    ))?;

    assert_eq!(
        assertion_texts(test_named(&index, "boundary")?),
        vec!["assert_eq!(want, true);"]
    );
    Ok(())
}

#[test]
fn closure_callback_credit_does_not_reach_helper_callbacks_in_another_module()
-> Result<(), Box<dyn Error>> {
    let index = index_for(&format!(
        "{GATE}#[cfg(test)]\nmod tests {{\n    use super::*;\n\n    mod support {{\n        pub fn run_test(want: bool, f: impl FnOnce() -> bool) {{\n            let got = f();\n            assert_eq!(got, want);\n        }}\n    }}\n\n    #[test]\n    fn boundary() {{\n        support::run_test(true, || gate(11));\n    }}\n}}\n"
    ))?;

    let test = test_named(&index, "boundary")?;

    assert!(test.assertions.is_empty(), "{:?}", assertion_texts(test));
    Ok(())
}

mod assertion_macros;
