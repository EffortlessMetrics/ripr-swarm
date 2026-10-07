//! RIPR-SPEC-0227 rule 3b: a result-side oracle that observes a `?` side flip.
//!
//! A `?` probe is an `error_path` probe, and a broad oracle (`is_err()`,
//! `matches!(.., Err(_))`, `unwrap_err()`) never confirms one under
//! RIPR-SPEC-0107. Rule 3b makes one exception: when the test input provably
//! reaches the `?` call's `Err` and the version without the `?` provably
//! returns `Ok` on that input, the side alone tells the two apart.
//!
//! This producer establishes both conditions without evaluating the input.
//! When the `?` is the owner's only possible source of `Err` (no other `?`,
//! no `return`, no `Err` constructor, an `Ok(..)` tail, and nothing a closure
//! or macro could hide), an owner call that a passing test asserts is `Err`
//! must have taken that `?` (condition 2), and swallowing it leaves no way to
//! return `Err` (condition 3). The `?` operand must not shape the error
//! (`map_err`, `ok_or`, a variant path): a side oracle never confirms which
//! error was returned (rule 1).
//!
//! Like the tuple-match producer this refines only a weak discriminator and
//! never repairs reach or propagation. Any unsupported shape leaves the
//! existing evidence unchanged, so the finding keeps its gap.

use super::tuple_match::{parsed, same_current_file};
use crate::analysis::classify::ProbeContext;
use crate::analysis::rust_index::find_file_facts;
use crate::domain::{Confidence, ProbeFamily, RelationReason, StageEvidence, StageState};
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasGenericArgs, HasName};
use ra_ap_syntax::{AstNode, SyntaxNode, ast};

pub(super) const QUESTION_MARK_SIDE_FLIP: &str = "Result-side oracle observes the `?` side flip: this `?` is the owner's only source of `Err`, and a test asserts the owner call returns `Err` (RIPR-SPEC-0227 rule 3b)";

/// Error-shaping adapters choose which error the `?` returns. A side oracle
/// cannot tell a changed error apart, so their presence refuses the flip.
const ERROR_SHAPING_METHODS: &[&str] = &[
    "map_err",
    "ok_or",
    "ok_or_else",
    "or",
    "or_else",
    "context",
    "with_context",
    "map_or",
    "map_or_else",
];

/// The assertion macros the test-side matcher reads by name. Any other
/// macro in a test body, or an import naming one of these, refuses.
const ASSERTION_MACROS: &[&str] = &["assert", "assert_eq", "assert_ne", "matches"];

pub(super) fn discrimination(
    context: &ProbeContext<'_>,
    observe: &StageEvidence,
    current: &StageEvidence,
) -> Option<StageEvidence> {
    if context.probe.family != ProbeFamily::ErrorPath
        || observe.state != StageState::Yes
        || current.state != StageState::Weak
        || !context.workspace_complete
        || !context.probe.expression.contains('?')
    {
        return None;
    }
    // Rule 3b condition 1, narrowed: the credit covers swallowing this `?`,
    // so the edit must keep the same `?` call on both sides. A removed-line
    // probe (no `after`) or a changed operand (a new call, `.trim()` added,
    // another fallible function) is a change a side oracle cannot vouch for.
    let after = context.probe.after.as_deref()?;
    let before = context.probe.before.as_deref()?;
    let statement = canonical_try_statement(after)?;
    if canonical_try_statement(before)? != statement {
        return None;
    }
    let owner = context.owner_fn?;
    if context.probe.owner.as_ref() != Some(&owner.id)
        || context
            .index
            .functions()
            .iter()
            .filter(|function| function.name == owner.name)
            .count()
            != 1
    {
        return None;
    }
    let facts = find_file_facts(context.index, &owner.file)?;
    let same_source = find_file_facts(context.index, &context.probe.location.file)
        .is_some_and(|probe_facts| std::ptr::eq(facts.data(), probe_facts.data()))
        || same_current_file(
            context,
            &owner.file,
            &context.probe.location.file,
            &facts.source,
        );
    if !same_source || facts.used_lexical_fallback {
        return None;
    }
    let root = parsed(&facts.source)?;
    let function = unique_free_function(root.syntax(), &owner.name)?;
    let try_line = sole_error_source_line(&facts.source, &function)?;
    if try_line != context.probe.location.line {
        return None;
    }

    for (test, reason) in &context.related_tests {
        if *reason != RelationReason::DirectOwnerCall {
            continue;
        }
        let Some(test_facts) = find_file_facts(context.index, &test.file) else {
            continue;
        };
        // A parent file can define the `assert!` a child module sees, and a
        // file-local `macro_rules!` or inner `cfg` can turn the assertion
        // into a no-op or compile the test out.
        if test_facts.used_lexical_fallback
            || !test_facts.role_provenance.edges.is_empty()
            || test_facts
                .role_provenance
                .earliest_unresolved_reason
                .is_some()
            || context.test_file_imports_foreign_callee_name(
                &test.file,
                &test_facts.source,
                &owner.name,
            )
        {
            continue;
        }
        let Some(test_root) = parsed(&test_facts.source) else {
            continue;
        };
        if !test_file_macros_and_inner_cfg_are_plain(test_root.syntax()) {
            continue;
        }
        let Some(test_function) = unique_function(test_root.syntax(), &test.name) else {
            continue;
        };
        if asserts_owner_err(&test_function, &owner.name) {
            return Some(StageEvidence::new(
                StageState::Yes,
                Confidence::High,
                QUESTION_MARK_SIDE_FLIP,
            ));
        }
    }
    None
}

/// The changed statement, normalized so only a behavior-preserving
/// respelling of the same `?` statement compares equal: whitespace is
/// ignored, parentheses that wrap the whole `?` operand are dropped
/// (`(digit(c))?` is `digit(c)?`), and a single turbofish on the `?` call
/// of a `let` without a type reads as that type's annotation
/// (`let p = s.parse::<u16>()?` is `let p: u16 = s.parse()?`). Every other
/// token stays, so a changed callee, generic argument, annotation or
/// enclosing call does not compare equal (rule 1). `None` unless the text
/// is one statement with exactly one `?`.
fn canonical_try_statement(line: &str) -> Option<String> {
    let line = line.trim();
    let source = if line.ends_with(';') || line.ends_with('}') {
        format!("fn __try() {{ {line} }}")
    } else {
        format!("fn __try() {{ {line}; }}")
    };
    let root = parsed(&source)?;
    let function = root.syntax().children().find_map(ast::Fn::cast)?;
    let body = function.body()?.stmt_list()?;
    let statements = body.statements().collect::<Vec<_>>();
    let [statement] = statements.as_slice() else {
        return None;
    };
    if body.tail_expr().is_some() {
        return None;
    }
    let mut tries = statement
        .syntax()
        .descendants()
        .filter_map(ast::TryExpr::cast);
    let try_expr = tries.next()?;
    if tries.next().is_some() {
        return None;
    }
    if let ast::Stmt::LetStmt(binding) = statement {
        // The rebuilt form below drops attributes, so refuse them.
        if binding.let_else().is_some() || binding.attrs().next().is_some() {
            return None;
        }
        let pattern = canonical_tokens(binding.pat()?.syntax(), None);
        let initializer = binding.initializer()?;
        let (ty, moved) = match binding.ty() {
            Some(ty) => (Some(canonical_tokens(ty.syntax(), None)), None),
            None => match turbofish_type(&initializer, &try_expr) {
                Some((ty, list)) => (Some(ty), Some(list)),
                None => (None, None),
            },
        };
        let initializer = canonical_tokens(initializer.syntax(), moved.as_ref());
        return Some(match ty {
            Some(ty) => format!("let {pattern}: {ty} = {initializer};"),
            None => format!("let {pattern} = {initializer};"),
        });
    }
    Some(canonical_tokens(statement.syntax(), None))
}

/// The single type argument of a turbofish on the method call that is the
/// whole `?` operand of a `let` initializer, with the list to skip.
fn turbofish_type(
    initializer: &ast::Expr,
    try_expr: &ast::TryExpr,
) -> Option<(String, ast::GenericArgList)> {
    if initializer.syntax() != try_expr.syntax() {
        return None;
    }
    let ast::Expr::MethodCallExpr(call) = unwrap_parens(try_expr.expr()?) else {
        return None;
    };
    let list = call.generic_arg_list()?;
    let arguments = list.generic_args().collect::<Vec<_>>();
    let [ast::GenericArg::TypeArg(argument)] = arguments.as_slice() else {
        return None;
    };
    Some((canonical_tokens(argument.syntax(), None), list))
}

/// A postfix or atomic operand: `(x)?` and `x?` parse the same.
fn binds_as_tightly_as_try(expression: ast::Expr) -> bool {
    matches!(
        unwrap_parens(expression),
        ast::Expr::CallExpr(_)
            | ast::Expr::MethodCallExpr(_)
            | ast::Expr::PathExpr(_)
            | ast::Expr::FieldExpr(_)
            | ast::Expr::IndexExpr(_)
            | ast::Expr::Literal(_)
            | ast::Expr::AwaitExpr(_)
    )
}

fn unwrap_parens(mut expression: ast::Expr) -> ast::Expr {
    while let ast::Expr::ParenExpr(paren) = &expression {
        let Some(inner) = paren.expr() else {
            break;
        };
        expression = inner;
    }
    expression
}

fn canonical_tokens(node: &SyntaxNode, skip: Option<&ast::GenericArgList>) -> String {
    let mut text = String::new();
    for element in node.descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        if token.kind().is_trivia() {
            continue;
        }
        let Some(parent) = token.parent() else {
            continue;
        };
        if skip.is_some_and(|list| parent.ancestors().any(|node| node == *list.syntax())) {
            continue;
        }
        // Only parentheses that wrap the whole `?` operand, around an
        // operand that already binds as tightly as `?`, are a respelling;
        // elsewhere they can change the callee or precedence
        // (`(cfg.parse)(s)` is not `cfg.parse(s)`, `(a + b)?` is not
        // `a + b?`).
        if matches!(token.text(), "(" | ")")
            && ast::ParenExpr::can_cast(parent.kind())
            && parent
                .ancestors()
                .skip(1)
                .find(|node| !ast::ParenExpr::can_cast(node.kind()))
                .is_some_and(|node| ast::TryExpr::can_cast(node.kind()))
            && ast::ParenExpr::cast(parent.clone())
                .is_some_and(|paren| binds_as_tightly_as_try(ast::Expr::ParenExpr(paren)))
        {
            continue;
        }
        if !text.is_empty() && needs_space(&text, token.text()) {
            text.push(' ');
        }
        text.push_str(token.text());
    }
    text
}

/// Keep adjacent words apart (`let mut d`), nothing else.
fn needs_space(text: &str, next: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.ends_with(word) && next.starts_with(word)
}

/// Nothing in the test file can shadow `assert!` or `matches!`: no
/// `macro_rules!`, no `#[macro_use]`, no `use` naming `assert` or
/// `matches`, and no glob import other than `super::*`, `self::*` or
/// `crate::*`. No inner attribute other than `#![cfg(test)]` either: an
/// inner `cfg` compiles the test out without touching its function or
/// modules.
fn test_file_macros_and_inner_cfg_are_plain(root: &SyntaxNode) -> bool {
    root.descendants().all(|node| {
        if ast::MacroRules::can_cast(node.kind()) {
            return false;
        }
        if let Some(tree) = ast::UseTree::cast(node.clone()) {
            let is_assertion_name = |name: &str| ASSERTION_MACROS.contains(&name);
            let names_macro = tree
                .path()
                .and_then(|path| path.segment())
                .is_some_and(|segment| is_assertion_name(&segment.syntax().text().to_string()))
                || tree
                    .rename()
                    .and_then(|rename| rename.name())
                    .is_some_and(|name| is_assertion_name(name.text()));
            let foreign_glob = tree.star_token().is_some()
                && !tree.path().is_some_and(|path| {
                    matches!(
                        path.syntax().text().to_string().as_str(),
                        "super" | "self" | "crate"
                    )
                });
            return !names_macro && !foreign_glob;
        }
        ast::Attr::cast(node).is_none_or(|attribute| {
            let text = attribute.syntax().text().to_string();
            !text.contains("macro_use")
                && (attribute.excl_token().is_none() || text == "#![cfg(test)]")
        })
    })
}

/// The only function of this name anywhere in the file.
fn unique_function(root: &SyntaxNode, name: &str) -> Option<ast::Fn> {
    let mut functions = root
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| function.name().is_some_and(|n| n.text() == name));
    let function = functions.next()?;
    functions.next().is_none().then_some(function)
}

/// A free function outside any `impl` or `trait`, with no attributes, whose
/// declared return type is a plain `Result<..>`.
fn unique_free_function(root: &SyntaxNode, name: &str) -> Option<ast::Fn> {
    let function = unique_function(root, name)?;
    let nested_in_item = function.syntax().ancestors().skip(1).any(|node| {
        ast::Impl::can_cast(node.kind())
            || ast::Trait::can_cast(node.kind())
            || ast::Fn::can_cast(node.kind())
    });
    let returns_result = function
        .ret_type()
        .and_then(|ret| ret.ty())
        .is_some_and(|ty| ty.syntax().text().to_string().starts_with("Result<"));
    (!nested_in_item
        && returns_result
        && function.attrs().next().is_none()
        && function.async_token().is_none()
        && function.const_token().is_none())
    .then_some(function)
}

/// The line of the owner's single `?` when nothing else in its body can
/// return `Err`. `None` for any shape this cannot establish.
fn sole_error_source_line(source: &str, function: &ast::Fn) -> Option<usize> {
    let body = function.body()?;
    let mut tries = Vec::new();
    for node in body.syntax().descendants() {
        if ast::ReturnExpr::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
            || ast::MacroCall::can_cast(node.kind())
            || ast::Fn::can_cast(node.kind())
            || ast::Item::can_cast(node.kind())
        {
            return None;
        }
        if let Some(block) = ast::BlockExpr::cast(node.clone())
            && block.async_token().is_some()
        {
            return None;
        }
        // Belt and braces: with no `return` and an `Ok(..)` tail an `Err`
        // value can leave only through the `?`, and an `Err` in the operand
        // is refused by `shapes_error`. `Result::Err` spellings are not read.
        if let Some(path) = ast::PathExpr::cast(node.clone())
            && path.syntax().text() == "Err"
        {
            return None;
        }
        if let Some(try_expr) = ast::TryExpr::cast(node) {
            tries.push(try_expr);
        }
    }
    let [try_expr] = tries.as_slice() else {
        return None;
    };
    if shapes_error(try_expr.expr()?.syntax()) {
        return None;
    }
    // Every path out of the body other than the `?` is the `Ok(..)` tail.
    let tail = ast::CallExpr::cast(body.stmt_list()?.tail_expr()?.syntax().clone())?;
    if tail.expr()?.syntax().text() != "Ok" {
        return None;
    }
    let offset = u32::from(try_expr.question_mark_token()?.text_range().start()) as usize;
    Some(
        source
            .get(..offset)?
            .bytes()
            .filter(|b| *b == b'\n')
            .count()
            + 1,
    )
}

/// Whether the `?` operand chooses its error: an error-shaping adapter or
/// any value path that names a type or variant (`E::Bad`, `Error::from`).
fn shapes_error(operand: &SyntaxNode) -> bool {
    std::iter::once(operand.clone())
        .chain(operand.descendants())
        .any(|node| {
            if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
                return call.name_ref().is_some_and(|name| {
                    ERROR_SHAPING_METHODS.contains(&name.text().to_string().as_str())
                });
            }
            ast::PathExpr::cast(node).is_some_and(|path| {
                path.syntax()
                    .text()
                    .to_string()
                    .split("::")
                    .any(|segment| segment.starts_with(|c: char| c.is_ascii_uppercase()))
            })
        })
}

/// A plain `#[test]` whose body, at its top level, asserts that a direct
/// owner call returns `Err`, with nothing in the test able to skip the
/// assertion or rebind the owner's name.
fn asserts_owner_err(function: &ast::Fn, owner: &str) -> bool {
    asserts_owner_err_inner(function, owner).unwrap_or(false)
}

fn asserts_owner_err_inner(function: &ast::Fn, owner: &str) -> Option<bool> {
    let attributes = function.attrs().collect::<Vec<_>>();
    let [attribute] = attributes.as_slice() else {
        return None;
    };
    if attribute.syntax().text() != "#[test]"
        || function.async_token().is_some()
        || function.param_list()?.params().next().is_some()
        || !enclosing_modules_are_test_only(function)
    {
        return None;
    }
    let body = function.body()?;
    for node in body.syntax().descendants() {
        if ast::ReturnExpr::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
            || ast::Fn::can_cast(node.kind())
        {
            return None;
        }
        // Any other macro may expand to an early `return` (a skip macro)
        // before the assertion runs.
        if let Some(call) = ast::MacroCall::cast(node.clone())
            && !call.path().is_some_and(|path| {
                ASSERTION_MACROS.contains(&path.syntax().text().to_string().as_str())
            })
        {
            return None;
        }
        if ast::IdentPat::cast(node)
            .is_some_and(|pat| pat.name().is_some_and(|n| n.text() == owner))
        {
            return None;
        }
    }
    let statements = body.stmt_list()?;
    Some(statements.statements().any(|statement| {
        let ast::Stmt::ExprStmt(statement) = statement else {
            return false;
        };
        // `#[cfg(..)] assert!(..);` may never compile.
        if statement
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
        {
            return false;
        }
        statement
            .expr()
            .is_some_and(|expression| observes_err_side(&expression, owner))
    }))
}

/// Only `#[cfg(test)]` modules may enclose the test: any other attribute
/// could compile it out.
fn enclosing_modules_are_test_only(function: &ast::Fn) -> bool {
    function
        .syntax()
        .ancestors()
        .filter_map(ast::Module::cast)
        .all(|module| {
            module
                .attrs()
                .all(|attribute| attribute.syntax().text() == "#[cfg(test)]")
        })
}

/// `assert!(owner(..).is_err())`, `assert!(!owner(..).is_ok())`,
/// `assert!(matches!(owner(..), Err(_)))` or a statement
/// `owner(..).unwrap_err()` / `.expect_err(..)`.
fn observes_err_side(expression: &ast::Expr, owner: &str) -> bool {
    if let ast::Expr::MethodCallExpr(call) = expression {
        return call.name_ref().is_some_and(|name| {
            matches!(
                name.text().to_string().as_str(),
                "unwrap_err" | "expect_err"
            )
        }) && call
            .receiver()
            .is_some_and(|receiver| is_direct_owner_call(&receiver, owner));
    }
    let Some(arguments) = macro_arguments(expression, "assert") else {
        return false;
    };
    let Some(condition) = arguments.first() else {
        return false;
    };
    let Some(condition) = parse_expression(condition) else {
        return false;
    };
    condition_observes_err(&condition, owner)
}

fn condition_observes_err(condition: &ast::Expr, owner: &str) -> bool {
    match condition {
        ast::Expr::MethodCallExpr(call) => {
            call.name_ref().is_some_and(|name| name.text() == "is_err")
                && call
                    .arg_list()
                    .is_some_and(|list| list.args().next().is_none())
                && call
                    .receiver()
                    .is_some_and(|receiver| is_direct_owner_call(&receiver, owner))
        }
        ast::Expr::PrefixExpr(prefix) => {
            prefix.op_kind() == Some(ast::UnaryOp::Not)
                && prefix.expr().is_some_and(|inner| match inner {
                    ast::Expr::MethodCallExpr(call) => {
                        call.name_ref().is_some_and(|name| name.text() == "is_ok")
                            && call
                                .arg_list()
                                .is_some_and(|list| list.args().next().is_none())
                            && call
                                .receiver()
                                .is_some_and(|receiver| is_direct_owner_call(&receiver, owner))
                    }
                    _ => false,
                })
        }
        _ => {
            let Some(arguments) = macro_arguments(condition, "matches") else {
                return false;
            };
            let [scrutinee, pattern] = arguments.as_slice() else {
                return false;
            };
            matches!(pattern.trim(), "Err(_)" | "Err(..)")
                && parse_expression(scrutinee)
                    .is_some_and(|scrutinee| is_direct_owner_call(&scrutinee, owner))
        }
    }
}

/// `owner(args)` by its bare name, with no argument that calls the owner
/// or could skip the assertion. The arguments sit inside the assertion's
/// token tree, which the test-body scan does not see, so they get the same
/// refusals here: no macro, `return`, closure or block.
fn is_direct_owner_call(expression: &ast::Expr, owner: &str) -> bool {
    let ast::Expr::CallExpr(call) = expression else {
        return false;
    };
    call.expr()
        .is_some_and(|callee| callee.syntax().text() == owner)
        && call.arg_list().is_some_and(|list| {
            list.args().all(|argument| {
                argument.syntax().descendants().all(|node| {
                    !ast::MacroCall::can_cast(node.kind())
                        && !ast::ReturnExpr::can_cast(node.kind())
                        && !ast::ClosureExpr::can_cast(node.kind())
                        && !ast::BlockExpr::can_cast(node.kind())
                        && ast::PathExpr::cast(node)
                            .is_none_or(|path| path.syntax().text() != owner)
                })
            })
        })
}

/// The top-level comma-separated arguments of `name!(..)`.
fn macro_arguments(expression: &ast::Expr, name: &str) -> Option<Vec<String>> {
    let ast::Expr::MacroExpr(expression) = expression else {
        return None;
    };
    let call = expression.macro_call()?;
    if call.path()?.syntax().text() != name {
        return None;
    }
    let tokens = call.token_tree()?.syntax().text().to_string();
    let inner = tokens.strip_prefix('(')?.strip_suffix(')')?;
    Some(split_top_level(inner))
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    let mut in_string = false;
    let mut escaped = false;
    for c in text.chars() {
        if in_string {
            current.push(c);
            match c {
                _ if escaped => escaped = false,
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

fn parse_expression(text: &str) -> Option<ast::Expr> {
    let source = format!("fn __side() {{ {} }}", text.trim());
    let root = parsed(&source)?;
    let function = root.syntax().children().find_map(ast::Fn::cast)?;
    let body = function.body()?.stmt_list()?;
    if body.statements().next().is_some() {
        return None;
    }
    body.tail_expr()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner_line(source: &str, name: &str) -> Option<usize> {
        let root = parsed(source)?;
        let function = unique_free_function(root.syntax(), name)?;
        sole_error_source_line(source, &function)
    }

    fn test_observes(source: &str, test: &str, owner: &str) -> bool {
        parsed(source)
            .and_then(|root| unique_function(root.syntax(), test))
            .is_some_and(|function| asserts_owner_err(&function, owner))
    }

    #[test]
    fn a_lone_passthrough_question_mark_is_the_sole_error_source() {
        let source = "pub fn port(text: &str) -> Result<u16, std::num::ParseIntError> {\n    let port: u16 = text.trim().parse()?;\n    Ok(port)\n}\n";
        assert_eq!(owner_line(source, "port"), Some(2));
        let looped = "pub fn total(s: &str) -> Result<u32, E> {\n    let mut sum = 0;\n    for c in s.chars() {\n        let d = (digit(c))?;\n        sum += d;\n    }\n    Ok(sum)\n}\n";
        assert_eq!(owner_line(looped, "total"), Some(4));
    }

    #[test]
    fn any_other_error_source_refuses_the_flip() {
        for (label, source) in [
            (
                "second ?",
                "pub fn total(s: &str) -> Result<u32, E> {\n    no_letters(s)?;\n    let d = digit(s)?;\n    Ok(d)\n}\n",
            ),
            (
                "early return",
                "pub fn total(s: &str) -> Result<u32, E> {\n    if s.is_empty() { return Ok(0); }\n    let d = digit(s)?;\n    Ok(d)\n}\n",
            ),
            (
                "Err tail",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = digit(s)?;\n    if d > 9 { Err(E::Bad) } else { Ok(d) }\n}\n",
            ),
            (
                "error-shaping operand",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = digit(s).map_err(|_| E::Bad)?;\n    Ok(d)\n}\n",
            ),
            (
                "variant operand",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = s.parse::<u32>().ok().ok_or(E::Bad)?;\n    Ok(d)\n}\n",
            ),
            (
                "macro body",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = digit(s)?;\n    check!(d);\n    Ok(d)\n}\n",
            ),
            (
                "lowercase error mapper",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = digit(s).map_err(convert)?;\n    Ok(d)\n}\n",
            ),
            (
                "lowercase ok_or argument",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = s.parse::<u32>().ok().ok_or(err)?;\n    Ok(d)\n}\n",
            ),
            (
                "type-path callee",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = E::parse(s)?;\n    Ok(d)\n}\n",
            ),
            (
                "non-Ok tail",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let d = digit(s)?;\n    validate(d)\n}\n",
            ),
            (
                "closure",
                "pub fn total(s: &str) -> Result<u32, E> {\n    let f = || digit(s);\n    let d = f()?;\n    Ok(d)\n}\n",
            ),
        ] {
            assert_eq!(owner_line(source, "total"), None, "{label}");
        }
    }

    #[test]
    fn methods_and_non_result_owners_are_out_of_scope() {
        let method = "impl P { pub fn port(text: &str) -> Result<u16, E> {\n    let p = text.parse()?;\n    Ok(p)\n} }\n";
        assert_eq!(owner_line(method, "port"), None);
        let option = "pub fn port(text: &str) -> Option<u16> {\n    let p = text.parse().ok()?;\n    Some(p)\n}\n";
        assert_eq!(owner_line(option, "port"), None);
    }

    #[test]
    fn err_side_assertions_observe_the_flip() {
        for body in [
            "assert!(port(\"http\").is_err());",
            "assert!(port(\"http\").is_err(), \"words are refused\");",
            "assert!(!port(\"http\").is_ok());",
            "assert!(matches!(port(\"http\"), Err(_)));",
            "port(\"http\").unwrap_err();",
            "port(\"http\").expect_err(\"refused\");",
        ] {
            let source = format!(
                "#[cfg(test)]\nmod tests {{\n    use super::*;\n    #[test]\n    fn refuses() {{\n        assert_eq!(port(\"8\"), Ok(8));\n        {body}\n    }}\n}}\n"
            );
            assert!(test_observes(&source, "refuses", "port"), "{body}");
        }
    }

    #[test]
    fn ok_side_or_unexecuted_assertions_do_not_observe_the_flip() {
        for (label, source) in [
            (
                "is_ok",
                "#[test]\nfn t() {\n    assert!(port(\"8\").is_ok());\n}\n",
            ),
            (
                "inside if",
                "#[test]\nfn t() {\n    if false { assert!(port(\"x\").is_err()); }\n}\n",
            ),
            (
                "should_panic",
                "#[test]\n#[should_panic]\nfn t() {\n    assert!(port(\"x\").is_err());\n}\n",
            ),
            (
                "shadowed owner",
                "#[test]\nfn t() {\n    let port = |_: &str| Err::<u16, ()>(());\n    assert!(port(\"x\").is_err());\n}\n",
            ),
            (
                "owner rebound",
                "#[test]\nfn t() {\n    let port = other_port;\n    assert!(port(\"x\").is_err());\n}\n",
            ),
            (
                "cfg on the statement",
                "#[test]\nfn t() {\n    #[cfg(any())]\n    assert!(port(\"x\").is_err());\n}\n",
            ),
            (
                "skip macro",
                "#[test]\nfn t() {\n    skip_if_offline!();\n    assert!(port(\"x\").is_err());\n}\n",
            ),
            (
                "early return",
                "#[test]\nfn t() {\n    return;\n    assert!(port(\"x\").is_err());\n}\n",
            ),
            (
                "wrapper call",
                "#[test]\nfn t() {\n    assert!(wrap(port(\"x\")).is_err());\n}\n",
            ),
            (
                "cfg-gated module",
                "#[cfg(any())]\nmod m {\n    #[test]\n    fn t() {\n        assert!(port(\"x\").is_err());\n    }\n}\n",
            ),
            (
                "Ok pattern",
                "#[test]\nfn t() {\n    assert!(matches!(port(\"x\"), Ok(_)));\n}\n",
            ),
            (
                "macro inside the asserted call",
                "#[test]\nfn t() {\n    assert!(port({ skip!(); \"x\" }).is_err());\n}\n",
            ),
            (
                "return inside the matches scrutinee",
                "#[test]\nfn t() {\n    assert!(matches!(port(if true { return } else { \"x\" }), Err(_)));\n}\n",
            ),
        ] {
            assert!(!test_observes(source, "t", "port"), "{label}");
        }
    }

    #[test]
    fn a_respelled_statement_compares_equal_and_a_changed_one_does_not() {
        let same = canonical_try_statement("let port = text.trim().parse::<u16>()?;");
        assert_eq!(
            same.as_deref(),
            Some("let port: u16 = text.trim().parse()?;")
        );
        assert_eq!(
            canonical_try_statement("let port: u16 = text.trim().parse()?;"),
            same
        );
        assert_eq!(
            canonical_try_statement("let d = (digit(c))?;"),
            canonical_try_statement("let d = digit(c)?;")
        );
        for changed in [
            "let port: u16 = text.parse()?;",
            "let port: u16 = text.trim().parse_strict()?;",
            "let port: u16 = text.trim().parse().and_then(check)?;",
            "let port = text.trim().parse::<u8>()?;",
            "let port: u8 = text.trim().parse()?;",
            "let port: u16 = (text.trim().parse)()?;",
        ] {
            assert_ne!(canonical_try_statement(changed), same, "{changed}");
        }
        assert_ne!(
            canonical_try_statement("push_u8(s.parse()?);"),
            canonical_try_statement("push_u16(s.parse()?);")
        );
        assert_ne!(
            canonical_try_statement("let d = (cfg.parse)(s)?;"),
            canonical_try_statement("let d = cfg.parse(s)?;")
        );
        assert_eq!(
            canonical_try_statement("let d = digit(c).unwrap_or(0);"),
            None
        );
        assert_eq!(canonical_try_statement("let d = a(c)? + b(c)?;"), None);
        // Parentheses around a looser operand carry precedence.
        assert_ne!(
            canonical_try_statement("let d = (a + b)?;"),
            canonical_try_statement("let d = a + b?;")
        );
        // An attribute on the `let` is not a respelling.
        assert_eq!(
            canonical_try_statement("#[cfg(feature = \"x\")] let p: u16 = s.parse()?;"),
            None
        );
    }

    #[test]
    fn macro_definitions_and_inner_cfg_make_a_test_file_unreadable() {
        let plain = "#![cfg(test)]\nuse super::*;\nuse std::fmt;\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n";
        let root = parsed(plain).map(|root| root.syntax().clone());
        assert!(root.is_some_and(|root| test_file_macros_and_inner_cfg_are_plain(&root)));
        for source in [
            "macro_rules! assert { ($($t:tt)*) => {}; }\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
            "mod tests {\n    #![cfg(any())]\n    #[test]\n    fn t() { assert!(port(\"x\").is_err()); }\n}\n",
            "#![cfg(any())]\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
            "use helpers::assert;\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
            "use helpers::noop as assert;\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
            "use helpers::assert_eq;\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
            "use helpers::*;\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
            "#[macro_use]\nmod helpers;\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
        ] {
            let root = parsed(source).map(|root| root.syntax().clone());
            assert!(
                root.is_some_and(|root| !test_file_macros_and_inner_cfg_are_plain(&root)),
                "{source}"
            );
        }
    }
}
