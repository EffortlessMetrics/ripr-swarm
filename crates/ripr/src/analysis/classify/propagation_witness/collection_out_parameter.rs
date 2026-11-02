//! Free-function out-parameter alias for a pushed collection (#11402).
//!
//! `pub fn log_line(buf: &mut Vec<String>, msg: &str) { buf.push(..); }` is
//! called by a test as `let mut b = Vec::new(); log_line(&mut b, "hi");
//! assert_eq!(b.len(), 1);`. The pushed value lands in `b`, so the changed
//! value is followed to something the test holds, even though a count read
//! cannot tell a changed pushed value apart.
//!
//! This module only says the value is followed. It grants no assertion
//! strength: a count read stays a weak check. Every link must be exact, and
//! anything else fails closed:
//! - the owner is a plain free function (not async, generic or a method)
//!   whose only statement is one unconditional `receiver.push(..)` into a
//!   `&mut Vec<..>` or `&mut String` parameter;
//! - the test is not `#[ignore]` or `#[should_panic]`, and at its top level
//!   binds `let mut b = Vec::new();` (or `String::new()`, or a `default()`),
//!   then calls the owner exactly once, resolved to the owner and not a
//!   namesake, with `&mut b` at the pushed parameter's position;
//! - no other statement mentions `b` before the call, no other argument
//!   mentions it, and after the call every statement that mentions `b` is a
//!   recorded `assert!`, `assert_eq!` or `assert_ne!` whose every mention of
//!   `b` is `b.len()` or `b.is_empty()`. Any other read may already check
//!   the contents, so it is not this module's to grade.

use crate::analysis::classify::OwnerPinSyntax;
use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::targeted_free_call_reaches_owner;
use crate::analysis::rust_index::{FunctionSummary, RustIndex, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode, SyntaxToken,
    ast::{self, HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasName},
};

/// Whether `test` follows the value the owner pushes into `receiver` to a
/// local it holds and reads only through its length, under the rules above.
pub(in crate::analysis) fn test_counts_pushed_collection_argument(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    syntax: &OwnerPinSyntax,
) -> bool {
    followed_by_count(receiver, owner, index, test, syntax).is_some()
}

fn followed_by_count(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    syntax: &OwnerPinSyntax,
) -> Option<()> {
    // Cheap text gates first: this runs per (seam, related test).
    if owner.item.has_self_param
        || !owner.body.contains("&mut")
        || !test.body.contains("&mut")
        || test.assertions.is_empty()
        || test_may_skip(test)
    {
        return None;
    }
    let parameter = sole_pushed_parameter(receiver, owner)?;
    if [&owner.file, &test.file].into_iter().any(|file| {
        index.files().get(file).is_none_or(|facts| {
            crate::analysis::classify::out_parameter::file_shadows_buffer_types(&facts.source)
        })
    }) {
        return None;
    }
    let tree = parse_clean_source_file(&test.body)?.tree();
    let function = tree.syntax().descendants().find_map(ast::Fn::cast)?;
    if function.async_token().is_some() {
        return None;
    }
    let statements: Vec<ast::Stmt> = function.body()?.stmt_list()?.statements().collect();
    // The one top-level call of the owner, with `&mut <local>` at the pushed
    // parameter's position.
    let calls: Vec<(usize, ast::CallExpr)> = statements
        .iter()
        .enumerate()
        .filter_map(|(at, statement)| match statement_expression(statement)? {
            ast::Expr::CallExpr(call) => Some((at, call)),
            _ => None,
        })
        .filter(|(_, call)| call_last_segment(call).is_some_and(|name| name == owner.name))
        .collect();
    let [(call_at, call)] = calls.as_slice() else {
        return None;
    };
    if call.attrs().next().is_some() || mentions_word(function.syntax(), &owner.name) != 1 {
        return None;
    }
    if !call_resolves_to_owner(call, owner, index, test, syntax) {
        return None;
    }
    let arguments: Vec<ast::Expr> = call.arg_list()?.args().collect();
    if arguments.len() != parameter.count {
        return None;
    }
    let ast::Expr::RefExpr(argument) = arguments.get(parameter.at)? else {
        return None;
    };
    if argument.mut_token().is_none() || argument.raw_token().is_some() {
        return None;
    }
    let ast::Expr::PathExpr(path) = argument.expr()? else {
        return None;
    };
    let local = compact(&path.syntax().text().to_string());
    if !super::is_bare_ident(&local)
        || arguments
            .iter()
            .enumerate()
            .any(|(at, other)| at != parameter.at && mentions_word(other.syntax(), &local) > 0)
    {
        return None;
    }
    // Before the call, only the binding's own fresh, empty definition.
    let before: Vec<&ast::Stmt> = statements
        .get(..*call_at)?
        .iter()
        .filter(|statement| mentions_word(statement.syntax(), &local) > 0)
        .collect();
    let [ast::Stmt::LetStmt(definition)] = before.as_slice() else {
        return None;
    };
    if !fresh_empty_definition(definition, &local, parameter.string) {
        return None;
    }
    // After the call, every mention is a count read inside a recorded check.
    let mut reads = 0;
    for statement in statements.get(call_at + 1..)? {
        if mentions_word(statement.syntax(), &local) == 0 {
            continue;
        }
        if !count_read_assertion(statement, &local, test, index, syntax) {
            return None;
        }
        reads += 1;
    }
    (reads > 0).then_some(())
}

/// The pushed parameter's position, the parameter count, and whether the
/// parameter is a `String` (else a `Vec`).
struct PushedParameter {
    at: usize,
    count: usize,
    string: bool,
}

fn sole_pushed_parameter(receiver: &str, owner: &FunctionSummary) -> Option<PushedParameter> {
    let tree = parse_clean_source_file(&owner.body)?.tree();
    let function = tree.syntax().descendants().find_map(ast::Fn::cast)?;
    if function.async_token().is_some()
        || function.gen_token().is_some()
        || function.const_token().is_some()
        || function.generic_param_list().is_some()
        || function.where_clause().is_some()
        || function.param_list()?.self_param().is_some()
    {
        return None;
    }
    let parameters: Vec<ast::Param> = function.param_list()?.params().collect();
    let at = parameters.iter().position(|parameter| {
        matches!(parameter.pat(), Some(ast::Pat::IdentPat(pattern))
            if pattern.name().is_some_and(|name| name.text() == receiver)
                && pattern.ref_token().is_none()
                && pattern.mut_token().is_none()
                && pattern.pat().is_none())
    })?;
    let ast::Type::RefType(reference) = parameters.get(at)?.ty()? else {
        return None;
    };
    if reference.mut_token().is_none() || reference.lifetime().is_some() {
        return None;
    }
    let ast::Type::PathType(pointee) = reference.ty()? else {
        return None;
    };
    let pointee = compact(&pointee.syntax().text().to_string());
    let string = if pointee == "String" {
        true
    } else if pointee.starts_with("Vec<") && pointee.ends_with('>') {
        false
    } else {
        return None;
    };
    // The body is exactly one statement: the unconditional push.
    let body = function.body()?.stmt_list()?;
    let mut statements = body.statements();
    let (Some(only), None, None) = (statements.next(), statements.next(), body.tail_expr()) else {
        return None;
    };
    let ast::Expr::MethodCallExpr(push) = statement_expression(&only)? else {
        return None;
    };
    if push.attrs().next().is_some()
        || push.name_ref()?.text() != "push"
        || push.generic_arg_list().is_some()
        || compact(&push.receiver()?.syntax().text().to_string()) != receiver
        || push.arg_list()?.args().count() != 1
        || push
            .arg_list()?
            .args()
            .any(|argument| mentions_word(argument.syntax(), receiver) > 0)
    {
        return None;
    }
    Some(PushedParameter {
        at,
        count: parameters.len(),
        string,
    })
}

fn test_may_skip(test: &TestSummary) -> bool {
    test.attrs.iter().any(|attribute| {
        let attribute = compact(attribute);
        attribute.contains("ignore") || attribute.contains("should_panic")
    })
}

fn call_last_segment(call: &ast::CallExpr) -> Option<String> {
    let ast::Expr::PathExpr(path) = call.expr()? else {
        return None;
    };
    let spelled = compact(&path.syntax().text().to_string());
    spelled.rsplit("::").next().map(str::to_string)
}

/// The spelled path settles on the owner: the targeted free-call authority,
/// and the same-named-rival identity when the index has rivals.
fn call_resolves_to_owner(
    call: &ast::CallExpr,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    syntax: &OwnerPinSyntax,
) -> bool {
    let Some(ast::Expr::PathExpr(path)) = call.expr() else {
        return false;
    };
    let spelled = compact(&path.syntax().text().to_string());
    let mut segments: Vec<String> = spelled.split("::").map(str::to_string).collect();
    if segments.pop().as_deref() != Some(owner.name.as_str())
        || segments
            .iter()
            .any(|segment| !super::is_bare_ident(segment))
    {
        return false;
    }
    let prefix = (!segments.is_empty()).then(|| format!("{}::", segments.join("::")));
    targeted_free_call_reaches_owner(test, owner, prefix.as_deref(), index, syntax)
        && shared_owner_call_identity(index, owner).is_none_or(|identity| {
            identity.resolve_call(test, index, &segments) == CallTarget::Owner
        })
}

/// `let mut <local> = Vec::new();` (or `String::new()` for a `String`
/// parameter, or either's `default()`), with no type, pattern or `else`.
fn fresh_empty_definition(definition: &ast::LetStmt, local: &str, string: bool) -> bool {
    let Some(ast::Pat::IdentPat(pattern)) = definition.pat() else {
        return false;
    };
    let constructor = definition
        .initializer()
        .map(|initializer| compact(&initializer.syntax().text().to_string()));
    let expected: &[&str] = if string {
        &["String::new()", "String::default()"]
    } else {
        &["Vec::new()", "Vec::default()"]
    };
    pattern.name().is_some_and(|name| name.text() == local)
        && pattern.mut_token().is_some()
        && pattern.ref_token().is_none()
        && pattern.pat().is_none()
        && definition.ty().is_none()
        && definition.let_else().is_none()
        && definition.attrs().next().is_none()
        && constructor.is_some_and(|constructor| expected.contains(&constructor.as_str()))
}

/// A statement that is one recorded, admitted `assert!`, `assert_eq!` or
/// `assert_ne!` whose every mention of `local` is `local.len()` or
/// `local.is_empty()`.
fn count_read_assertion(
    statement: &ast::Stmt,
    local: &str,
    test: &TestSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> bool {
    let Some(ast::Expr::MacroExpr(expression)) = statement_expression(statement) else {
        return false;
    };
    let Some(call) = expression.macro_call() else {
        return false;
    };
    let Some(name) = call
        .path()
        .map(|path| compact(&path.syntax().text().to_string()))
    else {
        return false;
    };
    if !matches!(name.as_str(), "assert" | "assert_eq" | "assert_ne") {
        return false;
    }
    let text = compact(&statement.syntax().text().to_string());
    let line = test.start_line
        + test
            .body
            .get(..usize::from(statement.syntax().text_range().start()))
            .unwrap_or_default()
            .matches('\n')
            .count();
    let recorded = test.assertions.iter().any(|assertion| {
        assertion.line == line
            && compact(&assertion.text).trim_end_matches(';') == text.trim_end_matches(';')
            && syntax.admits(test, assertion, index)
            && syntax.targeted_macro_binding_is_unambiguous(test, line, &name, index)
    });
    recorded && every_mention_counts(statement.syntax(), local)
}

/// Whether each `local` token is followed by `.len()` or `.is_empty()`.
fn every_mention_counts(node: &SyntaxNode, local: &str) -> bool {
    let tokens: Vec<SyntaxToken> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens.iter().enumerate().all(|(at, token)| {
        if !is_word(token, local) {
            return true;
        }
        let next: Vec<&str> = tokens
            .iter()
            .skip(at + 1)
            .take(4)
            .map(SyntaxToken::text)
            .collect();
        let previous = at
            .checked_sub(1)
            .and_then(|before| tokens.get(before))
            .map(SyntaxToken::text);
        !matches!(previous, Some("." | "::" | "&" | "*"))
            && matches!(next.as_slice(), [".", "len" | "is_empty", "(", ")"])
    })
}

fn statement_expression(statement: &ast::Stmt) -> Option<ast::Expr> {
    match statement {
        ast::Stmt::ExprStmt(statement) => statement.expr(),
        _ => None,
    }
}

fn is_word(token: &SyntaxToken, word: &str) -> bool {
    token.kind() == SyntaxKind::IDENT
        && token.text().strip_prefix("r#").unwrap_or(token.text()) == word
}

fn mentions_word(node: &SyntaxNode, word: &str) -> usize {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| is_word(token, word))
        .count()
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}
