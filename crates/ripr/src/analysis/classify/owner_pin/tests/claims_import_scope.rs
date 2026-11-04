//! #12462: a `use claims::assert_ok;` makes the macro resolve only where it
//! is in scope. The verified import used to be read at file granularity, so
//! an import in another function, in a sibling module, or under a `cfg` that
//! compiles it out credited a test that would not resolve the macro. Each
//! row names the test as `asserts` and pins which claims macros reach it.

use crate::analysis::syntax::source_imported_claims_macros;

/// The 1-based first and last line of the `fn asserts` test: its signature
/// line to the first line at or after it that holds a closing brace (the same
/// line when the function is written on one line).
fn asserts_span(source: &str) -> Result<(usize, usize), String> {
    let lines: Vec<&str> = source.lines().collect();
    let first = lines
        .iter()
        .position(|line| line.contains("fn asserts"))
        .ok_or("no `fn asserts` in the source")?;
    let last = (first..lines.len())
        .find(|&at| lines[at].contains('}'))
        .ok_or("no closing brace for `fn asserts`")?;
    Ok((first + 1, last + 1))
}

fn imported(source: &str) -> Result<Vec<&'static str>, String> {
    Ok(source_imported_claims_macros(source, asserts_span(source)?))
}

#[test]
fn an_import_that_reaches_the_test_resolves_the_macro() -> Result<(), String> {
    let at_root = "use claims::assert_ok;\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n}\n";
    assert_eq!(imported(at_root)?, vec!["assert_ok"], "file root");

    let in_enclosing_module = "mod tests {\n    use claims::assert_ok;\n\n    #[test]\n    fn asserts() {\n        assert_ok!(f());\n    }\n}\n";
    assert_eq!(
        imported(in_enclosing_module)?,
        vec!["assert_ok"],
        "module that encloses the test"
    );

    let in_own_body =
        "#[test]\nfn asserts() {\n    use claims::assert_ok;\n    assert_ok!(f());\n}\n";
    assert_eq!(
        imported(in_own_body)?,
        vec!["assert_ok"],
        "the test's own body"
    );

    let reimported_by_the_test_module = "use claims::assert_ok;\n\nmod tests {\n    use super::*;\n\n    #[test]\n    fn asserts() {\n        assert_ok!(f());\n    }\n}\n";
    assert_eq!(
        imported(reimported_by_the_test_module)?,
        vec!["assert_ok"],
        "a root import the test's module re-imports with `use super::*`"
    );

    let reimported_named = "use claims::assert_ok;\n\nmod tests {\n    use super::assert_ok;\n\n    #[test]\n    fn asserts() {\n        assert_ok!(f());\n    }\n}\n";
    assert_eq!(
        imported(reimported_named)?,
        vec!["assert_ok"],
        "re-imported by name"
    );

    let two_levels = "mod outer {\n    use claims::assert_ok;\n\n    mod inner {\n        use super::*;\n\n        #[test]\n        fn asserts() {\n            assert_ok!(f());\n        }\n    }\n}\n";
    assert_eq!(
        imported(two_levels)?,
        vec!["assert_ok"],
        "an ancestor module's import, re-imported by the child"
    );

    let plain_test_gate = "#[cfg(test)]\nuse claims::assert_some;\n\n#[test]\nfn asserts() {\n    assert_some!(f());\n}\n";
    assert_eq!(
        imported(plain_test_gate)?,
        vec!["assert_some"],
        "the plain test gate compiles in wherever the test does"
    );
    Ok(())
}

#[test]
fn an_import_the_test_cannot_see_resolves_nothing() -> Result<(), String> {
    let other_fn = "fn helper() {\n    use claims::assert_ok;\n    assert_ok!(g());\n}\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n}\n";
    assert!(imported(other_fn)?.is_empty(), "import in another function");

    let sibling_module = "mod other {\n    use claims::assert_ok;\n}\n\nmod tests {\n    #[test]\n    fn asserts() {\n        assert_ok!(f());\n    }\n}\n";
    assert!(
        imported(sibling_module)?.is_empty(),
        "import in a sibling module"
    );

    let compiled_out = "#[cfg(any())]\nuse claims::assert_ok;\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n}\n";
    assert!(
        imported(compiled_out)?.is_empty(),
        "import under cfg(any())"
    );

    let feature_gated = "#[cfg(feature = \"extra\")]\nuse claims::assert_ok;\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n}\n";
    assert!(
        imported(feature_gated)?.is_empty(),
        "import under a feature gate"
    );

    let attr_rewritten = "#[cfg_attr(unix, cfg(any()))]\nuse claims::assert_ok;\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n}\n";
    assert!(
        imported(attr_rewritten)?.is_empty(),
        "import under cfg_attr"
    );

    let child_without_reimport = "mod outer {\n    use claims::assert_ok;\n\n    mod inner {\n        #[test]\n        fn asserts() {\n            assert_ok!(f());\n        }\n    }\n}\n";
    assert!(
        imported(child_without_reimport)?.is_empty(),
        "a child module does not inherit its parent's import"
    );

    let root_without_reimport = "use claims::assert_ok;\n\nmod tests {\n    #[test]\n    fn asserts() {\n        assert_ok!(f());\n    }\n}\n";
    assert!(
        imported(root_without_reimport)?.is_empty(),
        "a root import does not reach a test module that lacks `use super::*`"
    );

    let gated_reimport = "use claims::assert_ok;\n\nmod tests {\n    #[cfg(feature = \"extra\")]\n    use super::*;\n\n    #[test]\n    fn asserts() {\n        assert_ok!(f());\n    }\n}\n";
    assert!(
        imported(gated_reimport)?.is_empty(),
        "a re-import under a feature gate may be compiled out"
    );

    let nested_block = "#[test]\nfn asserts() {\n    {\n        use claims::assert_ok;\n    }\n    assert_ok!(f());\n}\n";
    assert!(
        imported(nested_block)?.is_empty(),
        "import in a nested block"
    );
    Ok(())
}

#[test]
fn a_test_is_placed_by_where_it_starts() -> Result<(), String> {
    // A trailing comment on the closing line, or a neighbour after it, makes
    // the span longer than the function; the function is still the test.
    let trailing_comment =
        "use claims::assert_ok;\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n} // checked\n";
    assert_eq!(
        imported(trailing_comment)?,
        vec!["assert_ok"],
        "trailing comment"
    );
    let neighbour =
        "use claims::assert_ok;\n\n#[test]\nfn asserts() { assert_ok!(f()); } fn other() {}\n";
    assert_eq!(
        imported(neighbour)?,
        vec!["assert_ok"],
        "same-line neighbour"
    );

    // A doc example is a synthetic source whose test fn opens on the fence
    // line: padding lines, then the example inside the function.
    let doc_example = "\n\n#[test] fn __doc_example() {\nuse claims::assert_ok;\nlet q = assert_ok!(f());\nassert_eq!(q, 3);\n}\n";
    assert_eq!(
        source_imported_claims_macros(doc_example, (3, 7)),
        vec!["assert_ok"],
        "an import written in the example"
    );
    let doc_without_import =
        "use claims::assert_ok;\n\n#[test] fn __doc_example() {\nlet q = assert_ok!(f());\n}\n";
    assert_eq!(
        source_imported_claims_macros(doc_without_import, (3, 5)),
        vec!["assert_ok"],
        "a root import above is the file's, and the file root is in scope"
    );
    Ok(())
}

#[test]
fn a_span_that_places_no_function_credits_nothing() -> Result<(), String> {
    let at_root = "use claims::assert_ok;\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n}\n";
    assert!(source_imported_claims_macros(at_root, (90, 99)).is_empty());
    // The first line of the span is a `use`, not inside any function.
    assert!(source_imported_claims_macros(at_root, (1, 1)).is_empty());
    Ok(())
}
