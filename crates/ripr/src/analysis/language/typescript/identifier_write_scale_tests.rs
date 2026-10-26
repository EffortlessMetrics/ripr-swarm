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
    // binding for every name for every test (cubic, minutes in debug); it now
    // groups once per test. The bound is generous against a loaded host and
    // far below the old cost, and every test must resolve its own binding to
    // its own constructor, not just produce a list of the right length.
    // Two classes alternate, so a binding resolved to a neighbour's
    // constructor fails the per-test check below.
    let class_of = |index: usize| {
        if index.is_multiple_of(2) {
            "Cart"
        } else {
            "Basket"
        }
    };
    let count = 1_500;
    let mut text = String::new();
    for index in 0..count {
        text.push_str(&format!("let v{index} = new {}();\n", class_of(index)));
    }
    for index in 0..count {
        text.push_str(&format!(
            "test('t{index}', () => {{ expect(v{index}.total()).toBe({index}); }});\n"
        ));
    }
    let started = std::time::Instant::now();
    let tests = extract_tests(Path::new("big.test.ts"), &text);
    let elapsed = started.elapsed();
    assert_eq!(tests.len(), count);
    for (index, test) in tests.iter().enumerate() {
        assert_eq!(test.scope_bindings.len(), count, "test {index}");
        let own = format!("v{index}");
        let binding = test
            .scope_bindings
            .iter()
            .find(|binding| binding.name == own)
            .map(|binding| binding.constructed_by.clone());
        assert_eq!(
            binding,
            Some(Some(class_of(index).to_string())),
            "test {index}"
        );
    }
    assert!(
        elapsed < std::time::Duration::from_secs(30),
        "extraction took {elapsed:?}; per-name rescans are back"
    );
}

#[test]
fn an_unrecorded_write_withholds_the_binding_for_every_test() {
    use super::tests_extract::extract_tests;
    use std::path::Path;

    // `b` is rewritten inside a helper the scope walk does not record, so
    // its constructor is withheld in every test (`reset` is a plain function
    // binding with no constructor); `a` keeps its own. The
    // verdict is computed once per name and shared across tests, so every
    // test must see the same answer.
    let source = "let a = new Cart(); let b = new Cart();\nfunction reset() { b = null; }\ntest('one', () => { expect(a.total()).toBe(1); });\ntest('two', () => { expect(b.total()).toBe(2); });\n";
    let tests = extract_tests(Path::new("rebound.test.ts"), source);
    assert_eq!(tests.len(), 2);
    for test in &tests {
        let bound: Vec<(&str, Option<&str>)> = test
            .scope_bindings
            .iter()
            .map(|binding| (binding.name.as_str(), binding.constructed_by.as_deref()))
            .collect();
        assert_eq!(
            bound,
            vec![("a", Some("Cart")), ("b", None), ("reset", None)],
            "{}",
            test.name
        );
    }
}

#[test]
fn tests_of_one_scope_share_their_file_wide_lists() {
    use super::tests_extract::extract_tests;
    use std::path::Path;

    // Every test used to carry its own copy of each file-wide list, so a file
    // with n tests and n rewritten names held n^2 entries (#10017). Tests with
    // no parameters of their own see the same scope and now hold one list.
    let count = 600;
    let mut text = String::from("import { Cart } from '../src/cart';\n");
    for index in 0..count {
        text.push_str(&format!(
            "let v{index} = new Cart(); v{index} = new Cart();\ntest('t{index}', () => {{ expect(v{index}.total()).toBe({index}); }});\n"
        ));
    }
    // A test with a parameter shadows an enclosing name, so it cannot share.
    text.push_str("test.each([[1]])('row', (v0) => { expect(v0).toBe(1); });\n");
    let tests = extract_tests(Path::new("shared.test.ts"), &text);
    assert_eq!(tests.len(), count + 1);
    let (plain, row) = tests.split_at(count);
    let distinct = |identity: &dyn Fn(&super::types::TypeScriptTest) -> usize| {
        plain
            .iter()
            .map(identity)
            .collect::<std::collections::HashSet<_>>()
            .len()
    };
    assert_eq!(distinct(&|t| t.scope_bindings.identity() as usize), 1);
    assert_eq!(
        distinct(&|t| t.written_locals_in_file.identity() as usize),
        1
    );
    assert_eq!(distinct(&|t| t.imports_in_file.identity() as usize), 1);
    // The shared list is still complete and correct for every test.
    assert_eq!(plain[0].scope_bindings.len(), count);
    assert_eq!(plain[count - 1].written_locals_in_file.len(), count);
    // Control: the parameterized test is not served the shared list.
    assert_ne!(
        row[0].scope_bindings.identity(),
        plain[0].scope_bindings.identity()
    );
    let shadowed = row[0]
        .scope_bindings
        .iter()
        .find(|binding| binding.name == "v0");
    assert!(
        shadowed.is_some_and(|binding| binding.constructed_by.is_none()),
        "a row parameter must shadow the enclosing binding"
    );
}
