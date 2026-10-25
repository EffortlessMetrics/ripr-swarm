//! Per-file extraction summaries must stay linear in the size of the file
//! (#10017): a 5 MB minified test file has tens of thousands of identifier
//! writes, and a generated one has thousands of file-level bindings. The
//! earlier per-name rescans cost minutes in debug.

use super::tests_extract::{
    assigned_identifier_names, identifier_writes, rewritten_identifier_names,
    strict_identifier_writes,
};

/// The definition the summaries must keep: names in first-write order, the
/// rewritten ones being those with at least two strict writes.
fn reference_rewritten(text: &str) -> Vec<String> {
    let writes = strict_identifier_writes(text);
    let mut names: Vec<String> = Vec::new();
    for (name, _) in &writes {
        if !names.contains(name) && writes.iter().filter(|(other, _)| other == name).count() > 1 {
            names.push(name.clone());
        }
    }
    names
}

fn reference_assigned(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (name, _) in identifier_writes(text) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

#[test]
fn summaries_match_the_reference_definition_in_first_write_order() {
    let sources = [
        "",
        "let a = 1; a = 2; let b = 3; c = 4; c += 1; b = 5; d++;",
        "let z = 1; let y = 2; y = 3; z = 4; z = 5; for (let i = 0; i < 3; i++) {}",
        "const only = 1; let w; w = 1; ({ w } = other); [w] = [2];",
        "x = 1; x = 2; x = 3; y = 1; x = 4; y = 2;",
    ];
    for source in sources {
        assert_eq!(
            rewritten_identifier_names(source),
            reference_rewritten(source),
            "rewritten names diverged for {source:?}"
        );
        assert_eq!(
            assigned_identifier_names(source),
            reference_assigned(source),
            "assigned names diverged for {source:?}"
        );
    }
    // Control: the named fixtures are not all trivially empty, so the
    // equality above compares real order and membership.
    assert_eq!(
        rewritten_identifier_names("x = 1; x = 2; y = 1; y = 2;"),
        vec!["x".to_string(), "y".to_string()]
    );
}

#[test]
fn many_distinct_writes_summarize_without_a_quadratic_rescan() {
    // 60k distinct single-write names plus two rewritten ones at the end.
    // The old per-name rescan took minutes in debug here; this finishes in
    // well under a second, so a regression shows up as a test timeout.
    let mut text = String::new();
    for index in 0..60_000 {
        text.push_str(&format!("v{index} = {index};"));
    }
    text.push_str("tail = 1; tail = 2; last = 1; last = 2;");
    assert_eq!(
        rewritten_identifier_names(&text),
        vec!["tail".to_string(), "last".to_string()]
    );
    assert_eq!(assigned_identifier_names(&text).len(), 60_002);
}

#[test]
fn file_level_bindings_resolve_in_order_and_without_a_per_name_rescan() {
    use super::tests_extract::extract_tests;
    use std::path::Path;

    // Control: a small file resolves to exactly the declared names, sorted,
    // with the write kind decided by the last hook write.
    let small = "let b = new Cart(); let a = new Cart(); let c;\nbeforeEach(() => { c = new Cart(); });\ntest('t', () => { expect(a.total()).toBe(1); });\n";
    let tests = extract_tests(Path::new("small.test.ts"), small);
    assert_eq!(tests.len(), 1);
    let bound: Vec<(&str, Option<&str>)> = tests[0]
        .scope_bindings
        .iter()
        .map(|binding| (binding.name.as_str(), binding.constructed_by.as_deref()))
        .collect();
    assert_eq!(
        bound,
        vec![
            ("a", Some("Cart")),
            ("b", Some("Cart")),
            ("c", Some("Cart"))
        ]
    );

    // 1,500 file-level bindings seen by 1,500 tests. Resolution walked every
    // binding for every name for every test (cubic); it now groups once per
    // test, so a regression shows up as a test timeout.
    let count = 1_500;
    let mut text = String::new();
    for index in 0..count {
        text.push_str(&format!("let v{index} = {index};\n"));
    }
    for index in 0..count {
        text.push_str(&format!(
            "test('t{index}', () => {{ expect(v{index}).toBe({index}); }});\n"
        ));
    }
    let tests = extract_tests(Path::new("big.test.ts"), &text);
    assert_eq!(tests.len(), count);
    assert_eq!(tests[0].scope_bindings.len(), count);
    assert_eq!(tests[count - 1].scope_bindings.len(), count);
}
