use super::module_literal_slice_consts;
use crate::analysis::facts::LiteralSliceConst;
use crate::analysis::syntax::{constant_table_column, parse_clean_source_file};
use ra_ap_syntax::{AstNode, ast};

fn consts_of(source: &str) -> Vec<LiteralSliceConst> {
    // A fixture that fails to parse reads as no consts; each test asserts
    // the consts it expects, so a broken fixture cannot pass silently.
    let Some(parse) = parse_clean_source_file(source) else {
        return Vec::new();
    };
    let Some(function) = parse.tree().syntax().descendants().find_map(ast::Fn::cast) else {
        return Vec::new();
    };
    module_literal_slice_consts(&function)
}

fn skip_list(name: &str, items: &[&str]) -> LiteralSliceConst {
    LiteralSliceConst {
        name: name.to_string(),
        items: items.iter().map(|item| (*item).to_string()).collect(),
    }
}

fn table(guard: &str) -> String {
    format!(
        "fn t() {{ for (scheme, want) in [(\"http\", 80), (\"ftp\", 21), (\"gopher\", 71)] {{ {guard} assert_eq!(port(scheme), want); }} }}"
    )
}

fn column(guard: &str, name: &str, consts: &[LiteralSliceConst]) -> Option<Vec<String>> {
    constant_table_column(&table(guard), name, consts)
}

#[test]
fn module_consts_are_read_beside_the_test() {
    let found = consts_of(
        "mod tests {\n    const SKIP: &[&str] = &[\"gopher\", \"ws\"];\n    const NUMS: &[u16] = &[1, 2];\n    #[test]\n    fn t() {}\n}",
    );
    assert_eq!(
        found,
        vec![
            skip_list("SKIP", &["\"gopher\"", "\"ws\""]),
            skip_list("NUMS", &["1", "2"])
        ]
    );
}

#[test]
fn module_consts_that_are_not_plain_literal_slices_are_left_out() {
    for source in [
        "const SKIP: &[&str] = &[name()];\nfn t() {}",
        "const SKIP: &[&str] = &[\"a\"; 3];\nfn t() {}",
        "const SKIP: Vec<&str> = Vec::new();\nfn t() {}",
        "#[cfg(any())]\nconst SKIP: &[&str] = &[\"a\"];\nfn t() {}",
        "const SKIP: &mut [&str] = &mut [\"a\"];\nfn t() {}",
        "const SKIP: &[&str] = &[\"a\"];\nconst SKIP: &[&str] = &[\"b\"];\nfn t() {}",
    ] {
        assert!(consts_of(source).is_empty(), "{source}");
    }
}

#[test]
fn a_listed_row_is_dropped_from_every_column() {
    let skip = [skip_list("SKIP", &["\"gopher\""])];
    let guard = "if SKIP.contains(&scheme) { continue; }";
    assert_eq!(
        column(guard, "scheme", &skip),
        Some(vec!["\"http\"".into(), "\"ftp\"".into()])
    );
    assert_eq!(
        column(guard, "want", &skip),
        Some(vec!["80".into(), "21".into()])
    );
}

#[test]
fn an_equality_guard_drops_its_row() {
    assert_eq!(
        column("if scheme == \"ftp\" { continue; }", "want", &[]),
        Some(vec!["80".into(), "71".into()])
    );
}

#[test]
fn an_undecided_guard_keeps_the_table_unreadable() {
    let skip = [skip_list("SKIP", &["\"gopher\""])];
    for (guard, consts) in [
        // The list is not a module `const` of literals.
        ("if SKIP.contains(&scheme) { continue; }", &[][..]),
        // The list's name is spelled twice: a local may shadow it.
        (
            "let SKIP = [\"http\"]; if SKIP.contains(&scheme) { continue; }",
            &skip[..],
        ),
        // Not the loop's own variable.
        ("if SKIP.contains(&other) { continue; }", &skip[..]),
        // More than a bare `continue`.
        (
            "if SKIP.contains(&scheme) { continue; } else { }",
            &skip[..],
        ),
        (
            "if SKIP.contains(&scheme) { println!(); continue; }",
            &skip[..],
        ),
        ("if SKIP.contains(&scheme) { break; }", &skip[..]),
        // Not a statement of its own.
        (
            "let _ = SKIP.contains(&scheme) && f(); if scheme == \"ftp\" { continue; }",
            &skip[..],
        ),
    ] {
        assert_eq!(column(guard, "scheme", consts), None, "{guard}");
    }
}

#[test]
fn a_guard_that_skips_every_row_leaves_nothing_to_credit() {
    let skip = [skip_list("SKIP", &["\"http\"", "\"ftp\"", "\"gopher\""])];
    assert_eq!(
        column("if SKIP.contains(&scheme) { continue; }", "scheme", &skip),
        None
    );
}

#[test]
fn spellings_that_could_differ_in_value_are_not_compared() {
    // `"\x66tp"` is `"ftp"`; text equality cannot say so.
    let escaped = [skip_list("SKIP", &["\"\\x66tp\""])];
    assert_eq!(
        column(
            "if SKIP.contains(&scheme) { continue; }",
            "scheme",
            &escaped
        ),
        None
    );
    let numbers = "fn t() { for (n, want) in [(21, 1), (0x15, 2)] { if SKIP.contains(&n) { continue; } assert_eq!(f(n), want); } }";
    assert_eq!(
        constant_table_column(numbers, "want", &[skip_list("SKIP", &["21"])]),
        None
    );
}
