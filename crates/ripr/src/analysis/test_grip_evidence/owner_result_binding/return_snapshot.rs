//! Subject-only admission for direct free-owner return snapshots (#3161).
//! Expected bytes, serializer settings and field projections remain separate
//! authorities. This narrows existing credit; it does not promote strength.

use super::*;
use crate::analysis::facts::{OracleFact, TestSummary};
use ra_ap_syntax::ast::HasAttrs;

pub(in crate::analysis::test_grip_evidence) fn snapshot_observes_owner(
    test: &TestSummary,
    owner: &FunctionSummary,
    oracle: &OracleFact,
    context: &CompactGripContext<'_>,
    syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> bool {
    if !context
        .index
        .drop_in_manifests
        .verified(&test.file, "insta")
    {
        return false;
    }
    if context
        .unique_evidence_function(&test.file, &test.name, test.start_line)
        .is_none()
    {
        return false;
    }
    let Some(parsed) = context.parsed_test_file(&test.file) else {
        return false;
    };
    let Some(function) = parsed.unique_test_fn(&test.name, test.start_line) else {
        return false;
    };
    if function.attrs().any(|attr| {
        crate::analysis::syntax::attribute_settles_test_outcome(&attr.syntax().text().to_string())
    }) {
        return false;
    }
    let calls: Vec<_> = statement_snapshots(&function)
        .into_iter()
        .filter(|call| parsed.lines.line(call.syntax().text_range().start()) == oracle.line)
        .collect();
    // A line-only fact must not select a neighbouring invocation on that line.
    let [call] = calls.as_slice() else {
        return false;
    };
    let Some(path) = call.path() else {
        return false;
    };
    let path: String = path
        .syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect();
    if !context.snapshot_macro_binding_is_unambiguous(&path) {
        return false;
    }
    let Some(subject) = snapshot_subject(call).and_then(|text| value_expression(&text)) else {
        return false;
    };
    let cutoff = call.syntax().text_range().start();
    if function
        .syntax()
        .descendants()
        .any(|node| node.text_range().start() < cutoff && ast::ReturnExpr::can_cast(node.kind()))
    {
        return false;
    }
    let Some(body_start) = function.fn_token().map(|token| token.text_range().start()) else {
        return false;
    };
    let Some(delimiter) = call
        .token_tree()
        .map(|tree| tree.syntax().text_range().start())
    else {
        return false;
    };
    let snapshot_syntax = crate::analysis::classify::OwnerPinSyntax::for_snapshot_operand(
        &test.file, delimiter, body_start,
    );
    if owner_call(&subject, test, owner, context, syntax, &snapshot_syntax) {
        return true;
    }
    direct_result_bindings(&function, &parsed.lines, |initializer| {
        owner_call(initializer, test, owner, context, syntax, &snapshot_syntax)
    })
    .iter()
    .any(|binding| {
        path_is_binding(&subject, &binding.name)
            && !binding_invalidated_before_offset(&function, binding, cutoff)
            && !binding_escapes_before(&function, binding, cutoff)
    })
}

/// Do not treat a local lookalike macro or `insta` module as the external
/// assertion. This is a conservative source boundary, not macro expansion.
pub(in crate::analysis::test_grip_evidence) fn snapshot_macro_is_unambiguous(
    text: &str,
    context: &CompactGripContext<'_>,
) -> bool {
    let leaf = text.rsplit("::").next().unwrap_or_default();
    if crate::analysis::classify::targeted_name_is_alias(context.index, leaf)
        || crate::analysis::classify::targeted_name_is_alias(context.index, "insta")
    {
        return false;
    }
    context.index.files().iter().all(|(file, facts)| {
        if facts.used_lexical_fallback {
            return false;
        }
        if !facts.source.contains(leaf) && !facts.source.contains("insta") {
            return true;
        }
        let Some(parsed) = context.parsed_test_file(file) else {
            return false;
        };
        let tree = parsed.parse.tree();
        let mut allowed_imports = std::collections::BTreeSet::new();
        for import in tree.syntax().descendants().filter_map(ast::Use::cast) {
            for item in import.syntax().descendants().filter_map(ast::UseTree::cast) {
                if let Some(rename) = item.rename() {
                    if rename
                        .name()
                        .is_some_and(|name| name.text().trim_start_matches("r#") == leaf)
                    {
                        return false;
                    }
                    continue;
                }
                if item.use_tree_list().is_some() || item.star_token().is_some() {
                    continue;
                }
                let Some(path) = item.path() else { continue };
                let path: String = path
                    .syntax()
                    .text()
                    .to_string()
                    .split_whitespace()
                    .collect();
                if path
                    .rsplit("::")
                    .next()
                    .unwrap_or_default()
                    .trim_start_matches("r#")
                    != leaf
                {
                    continue;
                }
                let mut paths: Vec<_> = item
                    .syntax()
                    .ancestors()
                    .filter_map(ast::UseTree::cast)
                    .filter_map(|item| item.path())
                    .map(|path| {
                        path.syntax()
                            .text()
                            .to_string()
                            .split_whitespace()
                            .collect::<String>()
                    })
                    .collect();
                paths.reverse();
                // Every leaf import must be valid in its own syntax. Another
                // module's genuine import on the same line supplies no waiver.
                if paths.join("::").trim_start_matches("::") != format!("insta::{leaf}") {
                    return false;
                }
                allowed_imports.insert(parsed.lines.line(import.syntax().text_range().start()));
            }
        }
        let sites = crate::analysis::syntax::trusted_macro_binding_sites(
            &facts.source,
            &context.index.package_names,
            &[leaf],
            &|_, _| false,
            &|_| false,
        );
        sites.iter().all(|(_, site)| {
            matches!(site.kind, crate::analysis::syntax::MacroBindingKind::Import)
                && allowed_imports.contains(&site.line)
        }) && (!tree
            .syntax()
            .descendants()
            .filter_map(ast::Module::cast)
            .filter_map(|item| item.name())
            .any(|name| name.text().trim_start_matches("r#") == "insta")
            && !crate::analysis::syntax::source_declares_value_name(&facts.source, "insta"))
    })
}

fn statement_snapshots(function: &ast::Fn) -> Vec<ast::MacroCall> {
    let Some(body) = function.body().and_then(|body| body.stmt_list()) else {
        return Vec::new();
    };
    let mut calls = Vec::new();
    for statement in body.statements() {
        let call = match statement {
            ast::Stmt::ExprStmt(statement) => statement.expr().and_then(expression_macro),
            ast::Stmt::Item(ast::Item::MacroCall(call)) => Some(call),
            _ => None,
        };
        if let Some(call) = call.filter(|call| snapshot_subject(call).is_some()) {
            calls.push(call);
        }
    }
    if let Some(call) = body.tail_expr().and_then(expression_macro)
        && snapshot_subject(&call).is_some()
    {
        calls.push(call);
    }
    calls
}

fn expression_macro(expression: ast::Expr) -> Option<ast::MacroCall> {
    ast::MacroExpr::cast(expression.syntax().clone())?.macro_call()
}

/// Select the value operand, never the snapshot name or inline expectation.
/// Dynamic names, redaction maps and other overloads stay unsupported here.
fn snapshot_subject(call: &ast::MacroCall) -> Option<String> {
    let path: String = call
        .path()?
        .syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect();
    let name = path.strip_prefix("::").unwrap_or(&path);
    let name = name.strip_prefix("insta::").unwrap_or(name);
    if !matches!(
        name,
        "assert_snapshot"
            | "assert_debug_snapshot"
            | "assert_json_snapshot"
            | "assert_compact_debug_snapshot"
            | "assert_compact_json_snapshot"
            | "assert_yaml_snapshot"
            | "assert_ron_snapshot"
            | "assert_toml_snapshot"
            | "assert_csv_snapshot"
            | "assert_display_snapshot"
    ) {
        return None;
    }
    let arguments = snapshot_arguments(&call.token_tree()?)?;
    let first = arguments.first()?;
    if arguments.len() == 1 {
        return Some(first.clone());
    }
    if arguments.len() == 2 && inline_expected(&arguments[1]) {
        return Some(first.clone());
    }
    if !string_literal(first) {
        return None;
    }
    match arguments.len() {
        2 => Some(arguments[1].clone()),
        3 if inline_expected(&arguments[2]) => Some(arguments[1].clone()),
        _ => None,
    }
}

/// Rust token trees keep quoted commas, raw strings and nested groups intact.
/// Empty operands refuse; a final comma is accepted by Insta.
fn snapshot_arguments(tree: &ast::TokenTree) -> Option<Vec<String>> {
    let mut elements: Vec<_> = tree.syntax().children_with_tokens().collect();
    if elements.len() < 2 {
        return None;
    }
    elements.remove(0);
    elements.pop();
    let mut arguments = Vec::new();
    let mut current = String::new();
    for element in elements {
        if element.kind() == ra_ap_syntax::SyntaxKind::COMMA {
            if current.trim().is_empty() {
                return None;
            }
            arguments.push(current.trim().to_owned());
            current.clear();
        } else {
            current.push_str(&element.to_string());
        }
    }
    if !current.trim().is_empty() {
        arguments.push(current.trim().to_owned());
    }
    Some(arguments)
}

fn string_literal(text: &str) -> bool {
    matches!(value_expression(text), Some(ast::Expr::Literal(literal))
        if matches!(literal.kind(), ast::LiteralKind::String(_)))
}

fn inline_expected(text: &str) -> bool {
    text.trim()
        .strip_prefix('@')
        .is_some_and(|value| string_literal(value.trim()))
}

fn value_expression(text: &str) -> Option<ast::Expr> {
    let parsed = parse_clean_source_file(&format!("fn __ripr_snapshot_value() {{ {text} }}"))?;
    let function = parsed
        .tree()
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)?;
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    let mut expression = body.tail_expr()?;
    loop {
        expression = match expression {
            ast::Expr::ParenExpr(paren) => paren.expr()?,
            ast::Expr::RefExpr(reference) if reference.mut_token().is_none() => reference.expr()?,
            expression => return Some(expression),
        };
    }
}

fn owner_call(
    expression: &ast::Expr,
    test: &TestSummary,
    owner: &FunctionSummary,
    context: &CompactGripContext<'_>,
    syntax: &crate::analysis::classify::OwnerPinSyntax,
    snapshot_syntax: &crate::analysis::classify::OwnerPinSyntax,
) -> bool {
    let ast::Expr::CallExpr(call) = expression else {
        return false;
    };
    // Macro operands are separately parsed: the test AST cannot see an early
    // return (or opaque macro expansion) hidden inside this call's arguments.
    if expression.syntax().descendants().any(|node| {
        ast::ReturnExpr::can_cast(node.kind())
            || ast::BreakExpr::can_cast(node.kind())
            || ast::ContinueExpr::can_cast(node.kind())
            || ast::TryExpr::can_cast(node.kind())
            || ast::MacroExpr::can_cast(node.kind())
    }) {
        return false;
    }
    let Some(ast::Expr::PathExpr(path)) = call.expr() else {
        return false;
    };
    let Some(path) = path.path() else {
        return false;
    };
    let text: String = path
        .syntax()
        .text()
        .to_string()
        .split_whitespace()
        .collect();
    let (prefix, name) = text
        .rsplit_once("::")
        .map_or((None, text.as_str()), |(prefix, name)| (Some(prefix), name));
    name == owner.name
        && (crate::analysis::classify::targeted_free_call_reaches_owner(
            test,
            owner,
            prefix,
            context.index,
            syntax,
        ) || crate::analysis::classify::targeted_free_call_reaches_owner(
            test,
            owner,
            prefix,
            context.index,
            snapshot_syntax,
        ))
}

fn binding_escapes_before(
    function: &ast::Fn,
    binding: &OwnerResultBinding,
    cutoff: ra_ap_syntax::TextSize,
) -> bool {
    function.syntax().descendants().any(|node| {
        let start = node.text_range().start();
        if start < binding.range_end || start >= cutoff {
            return false;
        }
        if let Some(call) = ast::MacroCall::cast(node.clone()) {
            // Earlier simple snapshots may observe another value. Opaque
            // macros can replace or mutate this binding without parsed writes.
            return !snapshot_subject(&call)
                .and_then(|text| value_expression(&text))
                .is_some_and(|value| {
                    matches!(value, ast::Expr::PathExpr(_) | ast::Expr::Literal(_))
                });
        }
        // Even an immutable alias can mutate Cell/RefCell-style return values.
        // Until alias provenance is proved, any intervening use escapes this
        // narrow direct-binding admission (earlier simple snapshots above are
        // token trees and do not manufacture parsed path expressions).
        ast::PathExpr::cast(node).is_some_and(|path| {
            direct_name(&ast::Expr::PathExpr(path)).as_deref() == Some(binding.name.as_str())
        })
    })
}
