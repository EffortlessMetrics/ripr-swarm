//! Literal JSON import authority stays local to one opaque operand query.
use super::*;

fn parsed(source: &str) -> ra_ap_syntax::Parse<ast::SourceFile> {
    let parse = ast::SourceFile::parse(source, ra_ap_syntax::Edition::CURRENT);
    assert!(parse.errors().is_empty(), "{:?}", parse.errors());
    parse
}

fn function_body(
    parse: &ra_ap_syntax::Parse<ast::SourceFile>,
    name: &str,
) -> Result<SyntaxNode, &'static str> {
    Ok(parse
        .tree()
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .find(|function| function.name().is_some_and(|found| found.text() == name))
        .and_then(|function| function.body())
        .ok_or("named fixture function body")?
        .syntax()
        .clone())
}

#[test]
fn literal_json_authority_is_lazy_and_once_per_operand() -> Result<(), &'static str> {
    let cases = [
        ("assert_eq!(1, 1)", true, (false, 0)),
        ("assert_eq!(format!(\"x\"), \"x\")", true, (false, 0)),
        ("assert_eq!({return;}, 1)", true, (true, 0)),
        ("assert_eq!(value()?, 1)", true, (true, 0)),
        ("assert_eq!(unknown!(), 1)", true, (true, 0)),
        ("assert_eq!(serde_json::json!(1), 1)", true, (true, 0)),
        ("assert_eq!(serde_json :: json!(1), 1)", true, (true, 0)),
        ("assert_eq!(r#json!(1), 1)", true, (true, 0)),
        ("assert_eq!(json!(1), json!(1))", true, (false, 1)),
        ("assert_eq!(json!(1), json!(1))", false, (true, 1)),
        ("assert_eq!(json!(value), 1)", true, (true, 1)),
        ("assert_eq!(json!(1 . 0), 1)", true, (true, 1)),
    ];
    for (code, authority, expected) in cases {
        let parse = parsed(&format!("fn case() {{ {code}; }}"));
        let call = function_body(&parse, "case")?
            .descendants()
            .find_map(ast::MacroCall::cast)
            .ok_or("outer fixture macro")?;
        let tree = call.token_tree().ok_or("fixture macro operands")?;
        let mut calls = 0;
        let result = opaque_macro_operand_with_json(&tree, || {
            calls += 1;
            authority
        });
        assert_eq!((result, calls), expected, "{code}; authority={authority}");
    }
    Ok(())
}

#[test]
fn literal_json_context_preserves_complete_refusals() -> Result<(), &'static str> {
    let opaque = |path: &str| Some(AssertionContextRefusal::OpaqueMacro(path.into()));
    let operand = || {
        Some(AssertionContextRefusal::MacroOperandExit(
            "assert_eq".into(),
        ))
    };
    let cases = [
        ("", "assert_eq!(1, 1);", true, None),
        ("", "assert_eq!(format!(\"x\"), \"x\");", true, None),
        (
            "use serde_json::json;",
            "assert_eq!({return;}, 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(value()?, 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(unknown!(), 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(serde_json::json!(1), 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(serde_json :: json!(1), 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(json!(1), 1);",
            true,
            None,
        ),
        (
            "use serde_json::json;",
            "assert_eq!(json!({\"a\": 1}), json!({\"a\": 1}));",
            true,
            None,
        ),
        ("", "assert_eq!(json!(1), 1);", true, operand()),
        ("", "assert_eq!(json!(1), json!(1));", true, operand()),
        (
            "use serde_json::json;",
            "assert_eq!(json!(1), 1);",
            false,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(json!(value), 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(json!(1 . 0), 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(r#json!(1), 1);",
            true,
            operand(),
        ),
        ("use serde_json::json;", "json!({\"a\": 1});", true, None),
        (
            "use serde_json::json;",
            "json!(value);",
            true,
            opaque("json"),
        ),
        ("", "json!(1);", true, opaque("json")),
        (
            "#[cfg(test)] use serde_json::json;",
            "assert_eq!(json!(1), 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json as json;",
            "assert_eq!(json!(1), 1);",
            true,
            operand(),
        ),
        (
            "use serde_json::json;",
            "assert_eq!(json!(unknown!()), 1);",
            true,
            operand(),
        ),
    ];
    for (imports, code, json, expected) in cases {
        let parse = parsed(&format!("{imports}\nfn case() {{ {code} }}"));
        let body = function_body(&parse, "case")?;
        assert!(
            body.descendants()
                .any(|node| ast::MacroCall::can_cast(node.kind()))
        );
        let mut trusted = TRUSTED_MACRO_NAMES.to_vec();
        if json {
            trusted.push("json");
        }
        assert_eq!(
            has_escape(&body, &trusted, &BTreeMap::new()),
            expected,
            "{imports} {code}"
        );
    }
    Ok(())
}

#[test]
fn literal_json_authority_follows_each_lexical_call() -> Result<(), &'static str> {
    let parse = parsed(
        "mod inside { use serde_json::json;
            fn visible() { assert_eq!(json!(1), 1); }
         }
         fn outside() { assert_eq!(json!(1), 1); }",
    );
    let mut trusted = TRUSTED_MACRO_NAMES.to_vec();
    trusted.push("json");
    for order in [["visible", "outside"], ["outside", "visible"]] {
        for name in order {
            let expected = (name == "outside")
                .then(|| AssertionContextRefusal::MacroOperandExit("assert_eq".into()));
            assert_eq!(
                has_escape(&function_body(&parse, name)?, &trusted, &BTreeMap::new()),
                expected,
                "{name}",
            );
        }
    }
    Ok(())
}
