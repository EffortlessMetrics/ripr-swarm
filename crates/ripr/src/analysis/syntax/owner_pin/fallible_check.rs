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
}
