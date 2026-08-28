use super::*;
use crate::analysis::syntax::{LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn test_names(facts: &FileFacts) -> Vec<&str> {
    facts.tests.iter().map(|test| test.name.as_str()).collect()
}

fn index_test_names(index: &RustIndex) -> Vec<&str> {
    index.tests.iter().map(|test| test.name.as_str()).collect()
}

fn assert_non_test_functions(facts: &FileFacts, names: &[&str]) {
    for name in names {
        assert!(
            facts
                .functions
                .iter()
                .find(|function| function.name == *name)
                .is_some_and(|function| !function.is_test),
            "ordinary function was incorrectly classified as a test: {name}"
        );
    }
}

#[test]
fn exact_classifier_accepts_supported_test_defining_attributes() {
    let supported = [
        "#[test]",
        "#[ tokio :: test (flavor = \"multi_thread\") ]",
        "#[async_std::test]",
        "#[rstest]",
        "#[rstest::rstest]",
        "#[quickcheck]",
        "#[quickcheck_macros::quickcheck]",
        "#[wasm_bindgen_test(unsupported = test)]",
        "#[wasm_bindgen_test::wasm_bindgen_test]",
        "#[test_case(1)]",
        "#[test_case::test_case(1)]",
        "#[ntest::test_case(1)]",
        "#[test_matrix([1, 2])]",
        "#[test_case::test_matrix([1, 2])] // generated cases",
    ];

    for attribute in supported {
        assert!(
            normalized_test_attribute_path(attribute)
                .as_deref()
                .is_some_and(is_test_attribute_path),
            "supported test-defining attribute was not recognized: {attribute}"
        );
    }
}

#[test]
fn exact_classifier_rejects_prefix_lookalikes_and_ambiguous_attributes() {
    let unsupported = [
        "#[tokio::test_helper]",
        "#[async_std::test_config]",
        "#[rstest_helper]",
        "#[quickcheck_config]",
        "#[wasm_bindgen_test_helper]",
        "#[test_case_helper]",
        "#[test_matrix_config]",
        "#[cfg_attr(test, test)]",
        "#[test = \"custom\"]",
        "#[quickcheck(foo]",
        "#[test_case(1))]",
        "#[test] trailing_tokens",
        "#[proptest]",
        "#[doc(hidden)]",
    ];

    for attribute in unsupported {
        assert!(
            normalized_test_attribute_path(attribute)
                .as_deref()
                .is_none_or(|path| !is_test_attribute_path(path)),
            "non-test or ambiguous attribute was promoted: {attribute}"
        );
    }
}

#[test]
fn parser_facts_recognize_explicit_nonstandard_test_styles() -> Result<(), String> {
    let adapter = RaRustSyntaxAdapter;
    let mut facts = adapter.summarize_file(
        Path::new("src/lib.rs"),
        r#"
#[test]
fn standard_test() { assert!(true); }

#[tokio::test]
async fn tokio_test() { assert!(true); }

#[async_std::test]
async fn async_std_test() { assert!(true); }

#[rstest]
fn rstest_test() { assert!(true); }

#[rstest::rstest]
fn qualified_rstest() { assert!(true); }

#[quickcheck]
fn quickcheck_test(value: u8) -> bool { value == value }

#[quickcheck_macros::quickcheck]
fn qualified_quickcheck(value: u8) -> bool { value == value }

#[wasm_bindgen_test(unsupported = test)]
fn wasm_test() { assert!(true); }

#[wasm_bindgen_test::wasm_bindgen_test]
fn qualified_wasm() { assert!(true); }

#[test_case(1)]
fn test_case_test(value: i32) { assert_eq!(value, 1); }

#[test_case::test_case(1)]
fn qualified_test_case(value: i32) { assert_eq!(value, 1); }

#[ntest::test_case(1)]
fn ntest_case(value: i32) { assert_eq!(value, 1); }

#[test_matrix([1, 2])]
fn test_matrix_test(value: i32) { assert!(value > 0); }

#[test_case::test_matrix([1, 2])]
fn qualified_test_matrix(value: i32) { assert!(value > 0); }

#[tokio::test_helper]
fn tokio_lookalike() {}

#[rstest_helper]
fn rstest_lookalike() {}

#[test_case_helper]
fn test_case_lookalike() {}

#[quickcheck_config]
fn quickcheck_lookalike() {}

fn unannotated_helper() {}

#[cfg(test)]
mod tests {
    fn cfg_test_helper() {}
}
"#,
    )?;

    normalize_file_test_styles(&mut facts);

    assert_eq!(
        test_names(&facts),
        [
            "standard_test",
            "tokio_test",
            "async_std_test",
            "rstest_test",
            "qualified_rstest",
            "quickcheck_test",
            "qualified_quickcheck",
            "wasm_test",
            "qualified_wasm",
            "test_case_test",
            "qualified_test_case",
            "ntest_case",
            "test_matrix_test",
            "qualified_test_matrix",
        ]
    );
    assert_non_test_functions(
        &facts,
        &[
            "tokio_lookalike",
            "rstest_lookalike",
            "test_case_lookalike",
            "quickcheck_lookalike",
            "unannotated_helper",
        ],
    );
    assert!(
        facts
            .functions
            .iter()
            .find(|function| function.name == "cfg_test_helper")
            .is_some_and(|function| function.is_test),
        "cfg(test) helper role must remain evidence-only"
    );
    assert!(
        facts
            .tests
            .iter()
            .all(|test| test.name != "cfg_test_helper"),
        "cfg(test) helper must not become an executable TestFact"
    );
    Ok(())
}

#[test]
fn lexical_facts_share_exact_test_style_recognition() -> Result<(), String> {
    let adapter = LexicalRustSyntaxAdapter;
    let mut facts = adapter.summarize_file(
        Path::new("tests/unconventional.rs"),
        r#"
#[test]
fn standard_test() { assert!(true); }

#[ quickcheck ]
fn whitespace_quickcheck(value: u8) -> bool { value == value }

#[quickcheck_macros::quickcheck]
fn qualified_quickcheck(value: u8) -> bool { value == value }

#[test_case (1)]
fn parameterized_case(value: i32) { assert_eq!(value, 1); }

#[wasm_bindgen_test(unsupported = test)]
fn wasm_case() { assert!(true); }

#[rstest_helper]
fn rstest_lookalike() {}

#[tokio::test_helper]
fn tokio_lookalike() {}

#[cfg_attr(test, test)]
fn conditional_test_attribute() {}
"#,
    )?;

    normalize_file_test_styles(&mut facts);

    assert_eq!(
        test_names(&facts),
        [
            "standard_test",
            "whitespace_quickcheck",
            "qualified_quickcheck",
            "parameterized_case",
            "wasm_case",
        ]
    );
    assert!(
        facts.tests.iter().all(|test| test.attrs.is_empty()),
        "lexical fallback must not invent parser-backed attribute facts"
    );
    assert_non_test_functions(
        &facts,
        &[
            "rstest_lookalike",
            "tokio_lookalike",
            "conditional_test_attribute",
        ],
    );
    Ok(())
}

#[test]
fn build_paths_normalize_parser_and_fallback_test_roles() -> Result<(), Box<dyn Error>> {
    struct Cleanup(PathBuf);

    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("ripr-test-styles-{stamp}"));
    let _cleanup = Cleanup(root.clone());
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='test-styles'\nversion='0.1.0'\nedition='2024'\n",
    )?;
    fs::write(
        root.join("src/lib.rs"),
        r#"
#[quickcheck]
fn parser_property(value: u8) -> bool { value == value }

#[tokio::test_helper]
fn parser_lookalike() {}
"#,
    )?;
    fs::write(
        root.join("tests/fallback.rs"),
        r#"
this is intentionally invalid rust

#[ quickcheck ]
fn fallback_property(value: u8) -> bool { value == value }

#[rstest_helper]
fn fallback_lookalike() {}
"#,
    )?;

    let files = vec![
        PathBuf::from("src/lib.rs"),
        PathBuf::from("tests/fallback.rs"),
    ];
    let index = super::super::build_index(&root, &files).map_err(std::io::Error::other)?;
    let expected_tests = ["parser_property", "fallback_property"];
    assert_eq!(index_test_names(&index), expected_tests);

    let loaded_files = files
        .iter()
        .map(|file| fs::read(root.join(file)).map(|bytes| (file.clone(), bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    let cold_cached = super::super::build_index_from_loaded_files_with_cache(&root, &loaded_files)
        .map_err(std::io::Error::other)?;
    let warm_cached = super::super::build_index_from_loaded_files_with_cache(&root, &loaded_files)
        .map_err(std::io::Error::other)?;
    assert_eq!(index_test_names(&cold_cached.index), expected_tests);
    assert_eq!(index_test_names(&warm_cached.index), expected_tests);
    assert!(
        index
            .files
            .get(Path::new("tests/fallback.rs"))
            .is_some_and(|facts| facts.used_lexical_fallback),
        "invalid source must exercise the lexical fallback producer"
    );
    assert!(
        index
            .functions
            .iter()
            .find(|function| function.name == "parser_lookalike")
            .is_some_and(|function| !function.is_test)
    );
    assert!(
        index
            .functions
            .iter()
            .find(|function| function.name == "fallback_lookalike")
            .is_some_and(|function| !function.is_test)
    );
    Ok(())
}
