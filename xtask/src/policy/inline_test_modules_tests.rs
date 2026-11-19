use super::*;

fn count(source: &str) -> Result<usize, String> {
    inline_test_module_count(source)
}

#[test]
fn counts_each_test_gated_inline_module_form() {
    assert_eq!(
        count("fn a() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n"),
        Ok(1)
    );
    assert_eq!(
        count("#[cfg(all(test, not(windows)))]\nmod tests {}\n"),
        Ok(1)
    );
    assert_eq!(
        count("#[cfg(any(test, feature = \"test-fixtures\"))]\npub(crate) mod tests {}\n"),
        Ok(1)
    );
    assert_eq!(
        count("#[cfg( /* gated */ test )]\nmod seam_hover_tests {}\n"),
        Ok(1)
    );
    // A test module nested in a production module still ships in the file.
    assert_eq!(
        count("mod inner {\n    #[cfg(test)]\n    mod tests {}\n}\n"),
        Ok(1)
    );
    // Two siblings count twice.
    assert_eq!(
        count("#[cfg(test)]\nmod a {}\n#[cfg(test)]\nmod b {}\n"),
        Ok(2)
    );
}

#[test]
fn out_of_line_modules_and_non_test_cfgs_are_not_counted() {
    assert_eq!(count("#[cfg(test)]\nmod tests;\n"), Ok(0));
    assert_eq!(
        count("#[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod tests;\n"),
        Ok(0)
    );
    assert_eq!(count("#[cfg(feature = \"test\")]\nmod gated {}\n"), Ok(0));
    assert_eq!(count("#[cfg(unix)]\nmod unix_only {}\n"), Ok(0));
    assert_eq!(count("#[cfg(kani)]\nmod proofs {}\n"), Ok(0));
    // Spelling inside a string or comment is data, not a module.
    assert_eq!(
        count("const S: &str = \"#[cfg(test)]\\nmod tests {}\";\n// #[cfg(test)] mod tests {}\n"),
        Ok(0)
    );
    // A non-module item under cfg(test) is outside this ratchet.
    assert_eq!(count("#[cfg(test)]\nfn helper() {}\n"), Ok(0));
}

#[test]
fn a_module_nested_in_a_counted_module_is_not_counted_again() {
    assert_eq!(
        count(
            "#[cfg(test)]\nmod tests {\n    #[cfg(unix)]\n    mod a {}\n    #[cfg(test)]\n    mod b {}\n}\n"
        ),
        Ok(1)
    );
}

#[test]
fn an_unparsed_file_is_refused_not_counted_as_zero() {
    let refused = inline_test_module_count("#[cfg(test)]\nmod tests {\n");
    assert!(
        matches!(&refused, Err(err) if err.contains("does not parse")),
        "{refused:?}"
    );
}

#[test]
fn negative_experiment_a_new_module_fails_and_a_stale_baseline_fails() {
    let baseline = BTreeMap::from([("crates/ripr/src/a.rs".to_string(), 2usize)]);
    let same = BTreeMap::from([("crates/ripr/src/a.rs".to_string(), 2usize)]);
    assert!(ratchet_violations(&same, &baseline).is_empty());

    let grown = BTreeMap::from([("crates/ripr/src/a.rs".to_string(), 3usize)]);
    let failures = ratchet_violations(&grown, &baseline);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("baseline allows 2"));

    let new_file = BTreeMap::from([
        ("crates/ripr/src/a.rs".to_string(), 2usize),
        ("crates/ripr-contract/src/b.rs".to_string(), 1usize),
    ]);
    let failures = ratchet_violations(&new_file, &baseline);
    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].contains("crates/ripr-contract/src/b.rs") && failures[0].contains("allows 0")
    );

    let moved_out = BTreeMap::new();
    let failures = ratchet_violations(&moved_out, &baseline);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].contains("only 0 remain"));
}

#[test]
fn scanned_roots_are_the_two_published_crates() {
    assert!(is_scanned_path("crates/ripr/src/lsp/hover.rs"));
    assert!(is_scanned_path(
        "crates/ripr-contract/src/domain/language.rs"
    ));
    assert!(!is_scanned_path("xtask/src/main.rs"));
    assert!(!is_scanned_path("crates/ripr/tests/cli_help_hierarchy.rs"));
}
