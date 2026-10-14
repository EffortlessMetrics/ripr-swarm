//! Exact call-site projection binding for the existing collection observer.

use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::reveal::assertion_comparison_operands;
use crate::analysis::classify::targeted_free_call_reaches_owner;
use crate::analysis::rust_index::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList, HasAttrs, HasName},
};

/// A narrow extension of the bare collection observer: a sole, unconditional
/// parameter push, called with an exact mutable field projection, followed by
/// an equality on that unchanged projection and an independent literal vec.
/// Aliases, nested execution, unresolved calls and intervening uses fail closed.
pub(in crate::analysis) fn assertion_observes_collection_argument(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
) -> bool {
    observed_projection(receiver, owner, index, test, assertion).is_some()
}

fn observed_projection(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
) -> Option<()> {
    let syntax = crate::analysis::classify::OwnerPinSyntax::default();
    if !assertion.text.trim_start().starts_with("assert_eq!")
        || !syntax.admits(test, assertion, index)
        || !syntax.targeted_macro_binding_is_unambiguous(test, assertion.line, "vec", index)
    {
        return None;
    }
    let owner_tree = parse_clean_source_file(&owner.body)?.tree();
    let function = owner_tree.syntax().descendants().find_map(ast::Fn::cast)?;
    if function.async_token().is_some() || function.gen_token().is_some() {
        return None;
    }
    let parameters: Vec<_> = function.param_list()?.params().collect();
    let parameter = parameters.iter().position(|parameter| {
        let Some(ast::Pat::IdentPat(pattern)) = parameter.pat() else {
            return false;
        };
        let Some(ast::Type::RefType(ty)) = parameter.ty() else {
            return false;
        };
        pattern.name().is_some_and(|name| name.text() == receiver)
            && pattern.ref_token().is_none()
            && pattern.pat().is_none()
            && ty.mut_token().is_some()
    })?;
    let owner_statements = function.body()?.stmt_list()?;
    let expressions: Vec<_> = owner_statements
        .statements()
        .filter_map(statement_expression)
        .chain(owner_statements.tail_expr())
        .collect();
    if expressions.len() != 1 || owner_statements.statements().count() > 1 {
        return None;
    }
    let ast::Expr::MethodCallExpr(push) = &expressions[0] else {
        return None;
    };
    if push.attrs().next().is_some() {
        return None;
    }
    if push.name_ref()?.text() != "push" || push.receiver()?.syntax().text() != receiver {
        return None;
    }
    let [subject, expected] = assertion_comparison_operands(&assertion.text)?;
    let subject = compact(subject);
    let (binding, _) = subject.split_once('.')?;
    if !super::is_bare_ident(binding)
        || !subject.split('.').skip(1).all(|field| {
            super::is_bare_ident(field)
                || (!field.is_empty() && field.chars().all(|c| c.is_ascii_digit()))
        })
        || !literal_vec(expected)
    {
        return None;
    }
    let tree = parse_clean_source_file(&test.body)?.tree();
    let function = tree.syntax().descendants().find_map(ast::Fn::cast)?;
    let statements: Vec<_> = function.body()?.stmt_list()?.statements().collect();
    // The exact recorded assertion must be one top-level statement on its line.
    let assertions: Vec<_> = statements
        .iter()
        .enumerate()
        .filter(|(_, statement)| {
            let at = usize::from(statement.syntax().text_range().start());
            test.start_line + test.body[..at].matches('\n').count() == assertion.line
                && compact(&statement.syntax().text().to_string()).trim_end_matches(';')
                    == compact(&assertion.text).trim_end_matches(';')
        })
        .map(|(at, _)| at)
        .collect();
    let [assertion_at] = assertions.as_slice() else {
        return None;
    };
    let visible = &statements[..*assertion_at];
    let mentions: Vec<_> = visible
        .iter()
        .enumerate()
        .filter(|(_, statement)| mentions_binding(statement.syntax(), binding))
        .collect();
    let [(binding_at, definition), (call_at, call_statement)] = mentions.as_slice() else {
        return None;
    };
    if binding_at >= call_at {
        return None;
    }
    let ast::Stmt::LetStmt(definition) = definition else {
        return None;
    };
    let ast::Pat::IdentPat(pattern) = definition.pat()? else {
        return None;
    };
    if pattern.name()?.text() != binding
        || pattern.ref_token().is_some()
        || pattern.pat().is_some()
        || definition.let_else().is_some()
    {
        return None;
    }
    // Construction owns a value; a borrowed or renamed binding is an alias.
    let initializer = definition.initializer()?;
    if initializer
        .syntax()
        .descendants()
        .any(|node| ast::RefExpr::can_cast(node.kind()))
        || !matches!(
            initializer,
            ast::Expr::CallExpr(_) | ast::Expr::RecordExpr(_)
        )
    {
        return None;
    }
    let ast::Expr::CallExpr(call) = statement_expression((*call_statement).clone())? else {
        return None;
    };
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    if call.attrs().next().is_some() {
        return None;
    }
    let spelled = compact(&path.syntax().text().to_string());
    let mut segments: Vec<String> = spelled.split("::").map(str::to_string).collect();
    if segments.pop()?.as_str() != owner.name {
        return None;
    }
    let prefix = (!segments.is_empty()).then(|| format!("{}::", segments.join("::")));
    if !targeted_free_call_reaches_owner(test, owner, prefix.as_deref(), index, &syntax)
        || shared_owner_call_identity(index, owner).is_some_and(|identity| {
            identity.resolve_call(test, index, &segments) != CallTarget::Owner
        })
    {
        return None;
    }
    let arguments: Vec<_> = call.arg_list()?.args().collect();
    if arguments.len() != parameters.len() {
        return None;
    }
    let ast::Expr::RefExpr(argument) = arguments.get(parameter)? else {
        return None;
    };
    if argument.mut_token().is_none() || argument.raw_token().is_some() {
        return None;
    }
    let ast::Expr::FieldExpr(projection) = argument.expr()? else {
        return None;
    };
    if compact(&projection.syntax().text().to_string()) != subject {
        return None;
    }
    // Other call arguments cannot use, mutate or alias this same root.
    if arguments
        .iter()
        .enumerate()
        .any(|(at, argument)| at != parameter && mentions_binding(argument.syntax(), binding))
    {
        return None;
    }
    Some(())
}

fn statement_expression(statement: ast::Stmt) -> Option<ast::Expr> {
    match statement {
        ast::Stmt::ExprStmt(statement) => statement.expr(),
        _ => None,
    }
}

fn mentions_binding(node: &SyntaxNode, binding: &str) -> bool {
    node.descendants_with_tokens()
        .filter_map(|node| node.into_token())
        .any(|token| token.kind() == SyntaxKind::IDENT && token.text() == binding)
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn literal_vec(text: &str) -> bool {
    let text = compact(text);
    let Some(elements) = text
        .strip_prefix("vec![")
        .and_then(|text| text.strip_suffix(']'))
    else {
        return false;
    };
    elements.is_empty()
        || elements
            .trim_end_matches(',')
            .split(',')
            .all(|element| !element.is_empty() && element.chars().all(|ch| ch.is_ascii_digit()))
}
