use super::super::build_index;
use super::*;
use std::error::Error;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ROOT: AtomicU64 = AtomicU64::new(0);

fn index_for(source: &str) -> Result<RustIndex, Box<dyn Error>> {
    let root = std::env::temp_dir().join(format!(
        "ripr-test-helpers-{}-{}",
        std::process::id(),
        NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/lib.rs"), source)?;
    let index = build_index(&root, &[PathBuf::from("src/lib.rs")]);
    fs::remove_dir_all(&root)?;
    Ok(index?)
}

fn test_named<'index>(index: &'index RustIndex, name: &str) -> Result<&'index TestFact, String> {
    index
        .tests
        .iter()
        .find(|test| test.name == name)
        .ok_or_else(|| format!("premise: test `{name}` is indexed: {:?}", index.tests))
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
        .files
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
