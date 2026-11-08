use super::{call_name, future_is_dropped, identifier_count, plain_sync_test_attributes};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, SyntaxNode, ast};

/// Every owner call in `body` (the inside of a test fn) and whether each one
/// drops its future unpolled.
fn dropped(body: &str) -> Result<Vec<bool>, String> {
    let source = format!("fn t() {{\n{body}\n}}\n");
    let parse = parse_clean_source_file(&source).ok_or("premise: the test parses")?;
    let tree = parse.tree();
    let function = tree
        .syntax()
        .children()
        .find_map(ast::Fn::cast)
        .ok_or("premise: one fn")?;
    let block = function.body().ok_or("premise: a body")?;
    let code = block.syntax().text().to_string();
    let calls: Vec<SyntaxNode> = block
        .syntax()
        .descendants()
        .filter(|node| call_name(node).as_deref() == Some("fetch_len"))
        .collect();
    if calls.is_empty() {
        return Err(format!("premise: an owner call in {body:?}"));
    }
    Ok(calls
        .iter()
        .map(|call| future_is_dropped(call, &code))
        .collect())
}

#[test]
fn a_future_bound_to_an_unused_name_or_dropped_is_unpolled() -> Result<(), String> {
    for body in [
        "let _f = fetch_len(\"abc\");",
        "let _ = fetch_len(\"abc\");",
        "fetch_len(\"abc\");",
        "(fetch_len(\"abc\"));",
        "drop(fetch_len(\"abc\"));",
        "std::mem::drop(fetch_len(\"abc\"));",
        "let unused = fx::fetch_len(\"abc\");",
        "let _f = svc.fetch_len();",
    ] {
        assert_eq!(dropped(body)?, vec![true], "{body}");
    }
    Ok(())
}

#[test]
fn a_future_that_is_awaited_passed_on_or_used_again_is_not_read_as_unpolled() -> Result<(), String>
{
    for body in [
        // awaited (inside an async block the parser still sees the await)
        "let _n = async { fetch_len(\"abc\").await };",
        // driven by an executor
        "let _n = futures::executor::block_on(fetch_len(\"abc\"));",
        "let _n = rt.block_on(fetch_len(\"abc\"));",
        // held and polled later
        "let f = fetch_len(\"abc\"); let _n = block_on(f);",
        "let _f = fetch_len(\"abc\"); let _n = block_on(_f);",
        // passed to a helper that may poll it
        "drive(fetch_len(\"abc\"));",
        // a tail expression is the test's value, not a dropped statement
        "fetch_len(\"abc\")",
        // a destructuring pattern
        "let (a,) = (fetch_len(\"abc\"),);",
        // drop with a second argument is not `drop`
        "drop(fetch_len(\"abc\"), 1);",
        // a closure may run the call
        "let c = || fetch_len(\"abc\"); let _ = c;",
    ] {
        let read = dropped(body)?;
        assert!(read.iter().any(|dropped| !dropped), "{body}: {read:?}");
    }
    Ok(())
}

#[test]
fn only_plain_test_and_lint_attributes_are_synchronous() {
    let attrs = |list: &[&str]| list.iter().map(|attr| attr.to_string()).collect::<Vec<_>>();
    assert!(plain_sync_test_attributes(&attrs(&["#[test]"])));
    assert!(plain_sync_test_attributes(&attrs(&[
        "#[test]",
        "#[warn(missing_docs)]"
    ])));
    for list in [
        &["#[tokio::test]"][..],
        &["#[async_std::test]"],
        &["#[test]", "#[tokio::main]"],
        &["#[test]", "#[ignore]"],
        &["#[rstest]"],
        &["#[warn(missing_docs)]"],
        &[],
    ] {
        assert!(!plain_sync_test_attributes(&attrs(list)), "{list:?}");
    }
}

#[test]
fn identifiers_count_only_whole_words() {
    assert_eq!(
        identifier_count("fetch_len(x); fetch_len_x(); xfetch_len", "fetch_len"),
        1
    );
    assert_eq!(identifier_count("a.await; await", "await"), 2);
    assert_eq!(identifier_count("anything", ""), 0);
}
