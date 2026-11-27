//! `TreeScopeIndex` answers what `name_scopes_in_tree` answers for every
//! function of a file, while reading the file once (#15185).

use super::{TreeScopeIndex, name_scopes_in_tree};
use crate::analysis::syntax::nesting::parse_clean_source_file;
use crate::analysis::syntax::ra::LineIndex;
use ra_ap_syntax::{AstNode, ast, ast::HasName};

/// Item pieces a generated file is built from. `{` opens a scope that a
/// later `}` closes, so modules and functions nest in fns, impls and each
/// other; names repeat so that same-named functions, including two on one
/// line, are common.
const OPEN: &[&str] = &[
    "mod a {",
    "mod b {",
    "pub(crate) mod r#c {",
    "fn f() {",
    "fn g(x: u8) {",
    "fn r#f() {",
    "impl S {",
    "trait T {",
    "#[cfg(test)] mod tests {",
];
const LEAF: &[&str] = &[
    "fn f() {}",
    "fn g() {}",
    "fn f() {} fn f() {}",
    "fn h<T>(t: T) { let y = 1; let _ = |z: u8| z; }",
    "use crate::a::f;",
    "use super::*;",
    "use crate::b::{g as k, h};",
    "extern crate core;",
    "const C: u8 = 1;",
    "static D: u8 = 2;",
    "struct S;",
    "struct P(u8);",
    "enum E { A }",
    "type Y = u8;",
    "mod m;",
    "#[test] fn t() { assert_eq!(f(), ()); }",
    "#[test]\nfn t() { { use crate::a::f; f(); } }",
];

/// A balanced source from a stream of choices.
fn source_from(choices: &[u8]) -> String {
    let mut source = String::new();
    let mut depth = 0usize;
    for choice in choices {
        let pick = usize::from(*choice);
        match pick % 4 {
            0 if depth < 6 => {
                source.push_str(OPEN[pick / 4 % OPEN.len()]);
                depth += 1;
            }
            1 if depth > 0 => {
                source.push('}');
                depth -= 1;
            }
            _ => source.push_str(LEAF[pick / 4 % LEAF.len()]),
        }
        source.push('\n');
    }
    source.push_str(&"}\n".repeat(depth));
    source
}

/// Every `(name, line)` a caller could ask about, plus near misses.
fn questions(tree: &ast::SourceFile, lines: &LineIndex) -> Vec<(String, usize)> {
    let mut asked = Vec::new();
    for function in tree.syntax().descendants().filter_map(ast::Fn::cast) {
        let (Some(token), Some(name)) = (function.fn_token(), function.name()) else {
            continue;
        };
        let line = lines.line(token.text_range().start());
        asked.push((name.text().to_string(), line));
        asked.push((name.text().to_string(), line + 1));
        asked.push(("absent".to_string(), line));
    }
    asked
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config {
        cases: 2000,
        ..proptest::test_runner::Config::default()
    })]

    #[test]
    fn index_answers_equal_the_per_call_reading_for_every_function(
        choices in proptest::collection::vec(proptest::num::u8::ANY, 0..80)
    ) {
        let source = source_from(&choices);
        let Some(parse) = parse_clean_source_file(&source) else {
            return Err(proptest::test_runner::TestCaseError::fail(format!(
                "generated source does not parse cleanly:\n{source}"
            )));
        };
        let tree = parse.tree();
        let lines = LineIndex::new(&source);
        let index = TreeScopeIndex::new(&tree, &lines);
        for (name, line) in questions(&tree, &lines) {
            proptest::prop_assert_eq!(
                index.scopes_for(&tree, &lines, line, &name),
                name_scopes_in_tree(&tree, &lines, line, &name),
                "`{}` on line {} of\n{}",
                name,
                line,
                source
            );
        }
    }
}

/// The equality above cannot hold on `None` alone: fixed sources reach a
/// shared module scope, a duplicate on one line, a nested fn and a miss.
#[test]
fn index_reaches_scoped_shared_duplicate_and_missing_answers() -> Result<(), String> {
    let source = "use crate::x::f;\nmod tests {\n    use super::*;\n    fn a() { fn inner() {} }\n    fn b() {}\n    fn d() {} fn d() {}\n}\n";
    let parse = parse_clean_source_file(source).ok_or("fixture parses")?;
    let tree = parse.tree();
    let lines = LineIndex::new(source);
    let index = TreeScopeIndex::new(&tree, &lines);

    let a = index.scopes_for(&tree, &lines, 4, "a");
    assert_eq!(a, name_scopes_in_tree(&tree, &lines, 4, "a"));
    let a = a.ok_or("`a` has scopes")?;
    // body, module `tests`, file
    assert_eq!(a.scopes.len(), 3);
    assert!(a.scopes[0].fn_body && a.scopes[0].defines.contains("inner"));
    assert_eq!(a.scopes[1].modules, vec!["tests".to_string()]);
    assert_eq!(a.scopes[1].globs, vec![Some(vec!["super".to_string()])]);
    assert!(a.scopes[2].imports_of("f").next().is_some());

    let b = index
        .scopes_for(&tree, &lines, 5, "b")
        .ok_or("`b` has scopes")?;
    // Two tests of one module share its scope instead of copying it.
    assert!(std::sync::Arc::ptr_eq(&a.scopes[1], &b.scopes[1]));
    assert!(std::sync::Arc::ptr_eq(&a.scopes[2], &b.scopes[2]));

    let inner = index.scopes_for(&tree, &lines, 4, "inner");
    assert_eq!(inner, name_scopes_in_tree(&tree, &lines, 4, "inner"));
    assert_eq!(inner.map(|scopes| scopes.scopes.len()), Some(4));

    assert_eq!(index.scopes_for(&tree, &lines, 6, "d"), None);
    assert_eq!(name_scopes_in_tree(&tree, &lines, 6, "d"), None);
    assert_eq!(index.scopes_for(&tree, &lines, 5, "a"), None);
    Ok(())
}

/// One file with 20,000 tests in one module: each test's scopes come from the
/// index without re-reading the file, so asking for all of them stays inside
/// a deadline that a per-test re-read of the module misses by far.
#[test]
fn index_answers_every_test_of_a_large_module_in_linear_time() -> Result<(), String> {
    const TESTS: usize = 20_000;
    let mut source = String::from("mod tests {\n    use super::*;\n");
    for at in 0..TESTS {
        source.push_str(&format!(
            "    #[test]\n    fn t{at}() {{ assert_eq!(f({at}), {at}); }}\n"
        ));
    }
    source.push_str("}\n");
    let parse = parse_clean_source_file(&source).ok_or("fixture parses")?;
    let tree = parse.tree();
    let lines = LineIndex::new(&source);
    let started = std::time::Instant::now();
    let index = TreeScopeIndex::new(&tree, &lines);
    let answered = (0..TESTS)
        .filter(|at| {
            index
                .scopes_for(&tree, &lines, 4 + 2 * at, &format!("t{at}"))
                .is_some_and(|scopes| scopes.scopes.len() == 3)
        })
        .count();
    let elapsed = started.elapsed();
    if answered != TESTS {
        return Err(format!(
            "{answered} of {TESTS} tests got their three scopes"
        ));
    }
    if elapsed > std::time::Duration::from_secs(20) {
        return Err(format!("{TESTS} tests took {elapsed:?}"));
    }
    Ok(())
}
