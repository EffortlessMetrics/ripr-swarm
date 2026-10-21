//! SPEC-0197 rule 7a's closed owner/helper/test grammar.
//! No macro expansion or generic name/type inference is performed here.

use crate::analysis::facts::{FunctionSummary, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SyntaxNode, TextRange,
    ast::{self, HasAttrs, HasGenericParams, HasModuleItem, HasName},
};

#[derive(Clone, Debug)]
pub(crate) struct LiteralOwnerSyntax {
    pub value: u8,
    pub literal: TextRange,
    pub module_name: String,
    pub module_line: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct LiteralCheckSyntax {
    pub expected: u8,
    pub helper_name: String,
    pub helper_line: usize,
    pub assertion_line: usize,
    pub assertion_text: String,
    pub local: String,
    pub spans: Vec<(u32, u32)>,
}

fn compact(text: &str) -> String {
    crate::analysis::extract::mask_comments_and_strings(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

pub(crate) fn decimal_u8(text: &str) -> Option<u8> {
    let text = text.strip_suffix("u8").unwrap_or(text);
    if !text.starts_with(|c: char| c.is_ascii_digit())
        || !text.chars().all(|c| c.is_ascii_digit() || c == '_')
    {
        return None;
    }
    text.replace('_', "").parse().ok()
}

fn line(source: &str, offset: u32) -> usize {
    source
        .get(..offset as usize)
        .map_or(0, |s| 1 + s.bytes().filter(|b| *b == b'\n').count())
}

fn function_matches(
    source: &str,
    function: &ast::Fn,
    name: &str,
    start: usize,
    body: &str,
) -> bool {
    function.name().is_some_and(|n| n.text() == name)
        && function.fn_token().is_some_and(|token| {
            line(source, u32::from(token.text_range().start())) == start
                && source.get(
                    u32::from(token.text_range().start()) as usize
                        ..u32::from(function.syntax().text_range().end()) as usize,
                ) == Some(body)
        })
}

fn plain_function(function: &ast::Fn, return_type: &str) -> bool {
    function.async_token().is_none() && function.const_token().is_none()
        && function.unsafe_token().is_none() && function.generic_param_list().is_none()
        && function.where_clause().is_none()
        && function.param_list().is_some_and(|p| p.params().next().is_none() && p.self_param().is_none())
        && function.ret_type().and_then(|r| r.ty()).is_some_and(|t| compact(&t.syntax().text().to_string()) == return_type)
        // ABI and nonstandard qualifiers do not belong to this grammar.
        && function.fn_token().is_some_and(|token| {
            let prefix = function.syntax().text().to_string();
            let before = u32::from(token.text_range().start() - function.syntax().text_range().start()) as usize;
            let prefix = compact(prefix.get(..before).unwrap_or(""));
            prefix.is_empty() || prefix == "pub" || prefix == "pub(crate)"
                || (return_type == "Result<()>" && prefix == "#[test]")
        })
}

pub(crate) fn literal_owner_syntax(
    source: &str,
    owner: &FunctionSummary,
) -> Option<LiteralOwnerSyntax> {
    let parse = parse_clean_source_file(source)?;
    let file = parse.tree();
    if file
        .syntax()
        .children()
        .any(|n| ast::Attr::can_cast(n.kind()))
    {
        return None;
    }
    let items = file.items().collect::<Vec<_>>();
    if items.len() != 2 {
        return None;
    }
    let functions = items
        .iter()
        .filter_map(|i| ast::Fn::cast(i.syntax().clone()))
        .collect::<Vec<_>>();
    let modules = items
        .iter()
        .filter_map(|i| ast::Module::cast(i.syntax().clone()))
        .collect::<Vec<_>>();
    let ([function], [module]) = (functions.as_slice(), modules.as_slice()) else {
        return None;
    };
    if !function_matches(source, function, &owner.name, owner.start_line, &owner.body)
        || function.attrs().next().is_some()
        || !plain_function(function, "u8")
        || module.item_list().is_some()
    {
        return None;
    }
    let attrs = module.attrs().collect::<Vec<_>>();
    if attrs.len() != 2
        || !attrs
            .iter()
            .any(|a| compact(&a.syntax().text().to_string()) == "#[cfg(test)]")
        || !attrs
            .iter()
            .any(|a| a.path().is_some_and(|p| p.syntax().text() == "path"))
    {
        return None;
    }
    let module_name = module.name()?.text().to_string();
    if ["anyhow", "Result", "ensure", "Ok", owner.name.as_str()].contains(&module_name.as_str())
        || module_name.starts_with("r#")
    {
        return None;
    }
    let statements = function.body()?.stmt_list()?;
    if statements.statements().next().is_some() {
        return None;
    }
    let ast::Expr::Literal(literal) = statements.tail_expr()? else {
        return None;
    };
    Some(LiteralOwnerSyntax {
        value: decimal_u8(&literal.syntax().text().to_string())?,
        literal: literal.syntax().text_range(),
        module_name,
        module_line: line(source, u32::from(module.mod_token()?.text_range().start())),
    })
}

pub(crate) fn literal_check_syntax(
    source: &str,
    test: &TestSummary,
    owner: &str,
) -> Option<LiteralCheckSyntax> {
    let parse = parse_clean_source_file(source)?;
    let file = parse.tree();
    if file
        .syntax()
        .children()
        .any(|n| ast::Attr::can_cast(n.kind()))
    {
        return None;
    }
    let items = file.items().collect::<Vec<_>>();
    if items.len() != 3 {
        return None;
    }
    let uses = items
        .iter()
        .filter_map(|i| ast::Use::cast(i.syntax().clone()))
        .collect::<Vec<_>>();
    let functions = items
        .iter()
        .filter_map(|i| ast::Fn::cast(i.syntax().clone()))
        .collect::<Vec<_>>();
    let ([import], [first, second]) = (uses.as_slice(), functions.as_slice()) else {
        return None;
    };
    let import = compact(&import.syntax().text().to_string());
    if import != "useanyhow::{Result,ensure};" && import != "useanyhow::{ensure,Result};" {
        return None;
    }
    let (collected, helper) =
        if function_matches(source, first, &test.name, test.start_line, &test.body) {
            (first, second)
        } else if function_matches(source, second, &test.name, test.start_line, &test.body) {
            (second, first)
        } else {
            return None;
        };
    let attrs = collected.attrs().collect::<Vec<_>>();
    if attrs.len() != 1
        || compact(&attrs[0].syntax().text().to_string()) != "#[test]"
        || !plain_function(collected, "Result<()>")
        || !plain_function(helper, "Result<()>")
        || helper.attrs().next().is_some()
    {
        return None;
    }
    let helper_name = helper.name()?.text().to_string();
    if ["anyhow", "Result", "ensure", "Ok", owner].contains(&test.name.as_str())
        || ["anyhow", "Result", "ensure", "Ok", owner].contains(&helper_name.as_str())
        || helper_name == test.name
        || helper_name.starts_with("r#")
        || test.name.starts_with("r#")
        || owner.starts_with("r#")
    {
        return None;
    }
    let test_body = collected.body()?.stmt_list()?;
    if test_body.statements().next().is_some()
        || compact(&test_body.tail_expr()?.syntax().text().to_string())
            != format!("{helper_name}()")
    {
        return None;
    }
    let helper_body = helper.body()?.stmt_list()?;
    let statements = helper_body.statements().collect::<Vec<_>>();
    let [binding, assertion] = statements.as_slice() else {
        return None;
    };
    let ast::Stmt::LetStmt(binding) = binding else {
        return None;
    };
    let ast::Pat::IdentPat(pattern) = binding.pat()? else {
        return None;
    };
    if pattern.mut_token().is_some() || pattern.ref_token().is_some() || pattern.pat().is_some() {
        return None;
    }
    let local = pattern.name()?.text().to_string();
    if [
        "anyhow",
        "Result",
        "ensure",
        "Ok",
        owner,
        helper_name.as_str(),
    ]
    .contains(&local.as_str())
        || local.starts_with("r#")
    {
        return None;
    }
    if compact(&binding.syntax().text().to_string()) != format!("let{local}=super::{owner}();")
        || compact(&helper_body.tail_expr()?.syntax().text().to_string()) != "Ok(())"
    {
        return None;
    }
    let text = assertion.syntax().text().to_string();
    let normalized = compact(&text);
    let expected = normalized
        .strip_prefix(&format!("ensure!({local}=="))?
        .strip_suffix(");")?;
    let expected = decimal_u8(expected)?;
    let macros = assertion
        .syntax()
        .descendants()
        .filter_map(ast::MacroCall::cast)
        .collect::<Vec<_>>();
    let [invocation] = macros.as_slice() else {
        return None;
    };
    // Macro token trees are not expression-parsed by the source parser.
    // Do not turn two tokens (`7 u8`) into one valid literal by compacting.
    let tokens = invocation
        .token_tree()?
        .syntax()
        .descendants_with_tokens()
        .filter_map(|e| e.into_token())
        .filter(|t| !t.kind().is_trivia())
        .collect::<Vec<_>>();
    let value = match tokens.as_slice() {
        [open, actual, equals, value, close]
            if open.text() == "("
                && actual.text() == local
                && equals.text() == "=="
                && close.text() == ")" =>
        {
            value
        }
        [open, actual, first, second, value, close]
            if open.text() == "("
                && actual.text() == local
                && first.text() == "="
                && second.text() == "="
                && first.text_range().end() == second.text_range().start()
                && close.text() == ")" =>
        {
            value
        }
        _ => return None,
    };
    if decimal_u8(value.text()) != Some(expected) {
        return None;
    }
    let span = |node: &SyntaxNode| {
        (
            u32::from(node.text_range().start()),
            u32::from(node.text_range().end()),
        )
    };
    Some(LiteralCheckSyntax {
        expected,
        helper_name,
        helper_line: line(source, u32::from(helper.fn_token()?.text_range().start())),
        assertion_line: line(source, u32::from(invocation.syntax().text_range().start())),
        assertion_text: text.trim().to_string(),
        local,
        spans: vec![
            span(helper.syntax()),
            span(collected.syntax()),
            span(binding.syntax()),
            span(assertion.syntax()),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primitive_u8_literal_refuses_non_decimal_or_non_u8_values() {
        for (text, value) in [
            ("7", Some(7)),
            ("7u8", Some(7)),
            ("2_55", Some(255)),
            ("256", None),
            ("0x7", None),
            ("7u32", None),
            ("-7", None),
            ("7.0", None),
        ] {
            assert_eq!(decimal_u8(text), value, "{text}");
        }
    }

    const OWNER: &str =
        "pub fn answer() -> u8 { 7 }\n\n#[cfg(test)]\n#[path = \"test_support.rs\"]\nmod tests;\n";
    const CHECK: &str = "use anyhow::{Result, ensure};\n\nfn check_answer() -> Result<()> {\n    let actual = super::answer();\n    ensure!(actual == 7);\n    Ok(())\n}\n\n#[test]\nfn observes_answer() -> Result<()> {\n    check_answer()\n}\n";

    fn owner_of(source: &str) -> Result<FunctionSummary, String> {
        let facts =
            crate::analysis::rust_index::summarize_file("src/lib.rs".into(), source.to_string());
        facts
            .functions
            .into_iter()
            .find(|f| f.name == "answer")
            .ok_or_else(|| "answer owner".to_string())
    }

    fn test_of(source: &str) -> Result<TestSummary, String> {
        let facts = crate::analysis::rust_index::summarize_file(
            "src/test_support.rs".into(),
            source.to_string(),
        );
        facts
            .tests
            .into_iter()
            .find(|t| t.name == "observes_answer")
            .ok_or_else(|| "observes_answer test".to_string())
    }

    #[test]
    fn literal_owner_syntax_accepts_only_the_closed_owner_shape() -> Result<(), String> {
        let owner = owner_of(OWNER)?;
        let syntax = literal_owner_syntax(OWNER, &owner).ok_or("canonical owner shape")?;
        assert_eq!(syntax.value, 7);
        assert_eq!(syntax.module_name, "tests");
        assert_eq!(syntax.module_line, 5);
        assert_eq!(
            &OWNER[u32::from(syntax.literal.start()) as usize
                ..u32::from(syntax.literal.end()) as usize],
            "7"
        );
        for (name, from, to) in [
            ("out_of_range_literal", "{ 7 }", "{ 300 }"),
            ("statement_before_tail", "{ 7 }", "{ let x = 1; 7 }"),
            ("non_literal_tail", "{ 7 }", "{ 3 + 4 }"),
            ("wrong_return_type", "-> u8", "-> u16"),
            ("takes_parameter", "answer()", "answer(x: u8)"),
            ("const_qualifier", "pub fn", "pub const fn"),
            ("extern_abi", "pub fn", "pub extern \"C\" fn"),
            ("owner_attribute", "pub fn", "#[inline]\npub fn"),
            (
                "file_attribute",
                "pub fn answer",
                "#![forbid(unsafe_code)]\npub fn answer",
            ),
            ("missing_path_attr", "#[path = \"test_support.rs\"]\n", ""),
            ("inline_module_body", "mod tests;", "mod tests {}"),
            ("not_cfg_test", "#[cfg(test)]", "#[cfg(not(test))]"),
            ("reserved_module_name", "mod tests;", "mod Result;"),
            ("module_named_owner", "mod tests;", "mod answer;"),
            ("extra_item", "mod tests;", "mod tests;\nfn other() {}"),
        ] {
            let source = OWNER.replace(from, to);
            // Control: the mutated source must still be a distinct subject.
            assert_ne!(source, OWNER, "{name}");
            let facts =
                crate::analysis::rust_index::summarize_file("src/lib.rs".into(), source.clone());
            let Some(owner) = facts.functions.into_iter().find(|f| f.name == "answer") else {
                return Err(format!("{name}: owner fact"));
            };
            assert!(literal_owner_syntax(&source, &owner).is_none(), "{name}");
        }
        // The summary must describe the same function text and line.
        let mut stale = owner_of(OWNER)?;
        stale.start_line += 1;
        assert!(literal_owner_syntax(OWNER, &stale).is_none());
        let mut renamed = owner_of(OWNER)?;
        renamed.name = "other".into();
        assert!(literal_owner_syntax(OWNER, &renamed).is_none());
        Ok(())
    }

    #[test]
    fn literal_check_syntax_accepts_both_assertion_spellings_and_function_orders()
    -> Result<(), String> {
        let test = test_of(CHECK)?;
        let check = literal_check_syntax(CHECK, &test, "answer").ok_or("canonical check")?;
        assert_eq!(check.expected, 7);
        assert_eq!(check.helper_name, "check_answer");
        assert_eq!(check.helper_line, 3);
        assert_eq!(check.assertion_line, 5);
        assert_eq!(check.assertion_text, "ensure!(actual == 7);");
        assert_eq!(check.local, "actual");
        assert_eq!(check.spans.len(), 4);
        // The collected test may precede its helper, and the import order is free.
        let reordered = "use anyhow::{ensure, Result};\n\n#[test]\nfn observes_answer() -> Result<()> {\n    check_answer()\n}\n\nfn check_answer() -> Result<()> {\n    let actual = super::answer();\n    ensure!(actual == 7u8);\n    Ok(())\n}\n";
        let test = test_of(reordered)?;
        let check = literal_check_syntax(reordered, &test, "answer").ok_or("reordered check")?;
        assert_eq!(check.expected, 7);
        assert_eq!(check.helper_line, 8);
        assert_eq!(check.assertion_line, 10);
        Ok(())
    }

    #[test]
    fn literal_check_syntax_refuses_outside_the_closed_check_grammar() -> Result<(), String> {
        for (name, from, to) in [
            (
                "foreign_import",
                "use anyhow::{Result, ensure};",
                "use anyhow::Result;",
            ),
            ("mismatched_literal", "actual == 7", "actual == 7u8 + 0"),
            ("split_suffix_tokens", "actual == 7", "actual == 7 u8"),
            (
                "different_local_in_assertion",
                "ensure!(actual == 7)",
                "ensure!(other == 7)",
            ),
            ("assertion_message", "actual == 7", "actual == 7, \"why\""),
            ("inequality", "actual == 7", "actual != 7"),
            ("mutable_binding", "let actual", "let mut actual"),
            ("wrong_owner_call", "super::answer()", "super::other()"),
            (
                "extra_statement",
                "    Ok(())\n}\n\n#[test]",
                "    let _ = 1;\n    Ok(())\n}\n\n#[test]",
            ),
            (
                "helper_not_tail_of_test",
                "    check_answer()\n",
                "    check_answer()?;\n    Ok(())\n",
            ),
            (
                "helper_takes_parameter",
                "fn check_answer()",
                "fn check_answer(x: u8)",
            ),
            ("helper_named_ensure", "check_answer", "ensure"),
            (
                "helper_shadows_test",
                "fn check_answer",
                "fn observes_answer",
            ),
            (
                "async_test",
                "fn observes_answer",
                "async fn observes_answer",
            ),
            (
                "helper_attribute",
                "fn check_answer",
                "#[inline]\nfn check_answer",
            ),
            ("test_extra_attribute", "#[test]", "#[test]\n#[ignore]"),
            (
                "file_attribute",
                "use anyhow",
                "#![forbid(unsafe_code)]\nuse anyhow",
            ),
            (
                "non_ok_tail",
                "    Ok(())\n}\n\n#[test]",
                "    Ok(1).map(|_| ())\n}\n\n#[test]",
            ),
            ("extra_item", "\n#[test]", "\nfn extra() {}\n#[test]"),
        ] {
            let source = CHECK.replace(from, to);
            assert_ne!(source, CHECK, "{name}");
            let facts = crate::analysis::rust_index::summarize_file(
                "src/test_support.rs".into(),
                source.clone(),
            );
            let Some(test) = facts
                .tests
                .into_iter()
                .find(|t| t.name == "observes_answer")
            else {
                return Err(format!("{name}: collected test"));
            };
            assert!(
                literal_check_syntax(&source, &test, "answer").is_none(),
                "{name}"
            );
        }
        // The owner name may not collide with the test or the helper.
        let test = test_of(CHECK)?;
        assert!(literal_check_syntax(CHECK, &test, "check_answer").is_none());
        assert!(literal_check_syntax(CHECK, &test, "observes_answer").is_none());
        assert!(literal_check_syntax(CHECK, &test, "r#answer").is_none());
        // The summary must describe the same test text and line.
        let mut stale = test_of(CHECK)?;
        stale.start_line += 1;
        assert!(literal_check_syntax(CHECK, &stale, "answer").is_none());
        Ok(())
    }
}
