//! #12462: a `use claims::assert_ok;` makes the macro resolve only where it
//! is in scope. The verified import used to be read at file granularity, so
//! an import in another function, in a sibling module, or under a `cfg` that
//! compiles it out credited a test that would not resolve the macro. Each
//! row names the test as `asserts` and pins which claims macros reach it.

use crate::analysis::syntax::source_imported_claims_macros;

/// The 1-based first and last line of the `fn asserts` test: its signature
/// line to the first closing-brace line at or after it (or the same line when
/// the function is written on one line).
fn asserts_span(source: &str) -> Result<(usize, usize), String> {
    let lines: Vec<&str> = source.lines().collect();
    let first = lines
        .iter()
        .position(|line| line.contains("fn asserts"))
        .ok_or("no `fn asserts` in the source")?;
    let last = (first..lines.len())
        .find(|&at| lines[at].trim_end().ends_with('}') && (at > first || lines[at].contains("{ ")))
        .ok_or("no closing brace for `fn asserts`")?;
    Ok((first + 1, last + 1))
}

fn imported(source: &str) -> Result<Vec<&'static str>, String> {
    Ok(source_imported_claims_macros(
        source,
        Some(asserts_span(source)?),
    ))
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

    let nested_block = "#[test]\nfn asserts() {\n    {\n        use claims::assert_ok;\n    }\n    assert_ok!(f());\n}\n";
    assert!(
        imported(nested_block)?.is_empty(),
        "import in a nested block"
    );
    Ok(())
}

#[test]
fn an_unplaced_test_keeps_only_file_root_imports() -> Result<(), String> {
    let at_root = "use claims::assert_ok;\n\n#[test]\nfn asserts() {\n    assert_ok!(f());\n}\n";
    let in_module = "mod tests {\n    use claims::assert_ok;\n\n    #[test]\n    fn asserts() {\n        assert_ok!(f());\n    }\n}\n";
    // No span: a doc example, which has no lines in its own source.
    assert_eq!(
        source_imported_claims_macros(at_root, None),
        vec!["assert_ok"]
    );
    assert!(source_imported_claims_macros(in_module, None).is_empty());
    // A span that is not in the source places nothing either.
    assert!(source_imported_claims_macros(in_module, Some((90, 99))).is_empty());
    assert_eq!(
        source_imported_claims_macros(at_root, Some((90, 99))),
        vec!["assert_ok"]
    );
    Ok(())
}
