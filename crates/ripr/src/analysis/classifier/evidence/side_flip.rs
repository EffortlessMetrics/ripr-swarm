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
//! Decision 5 also reads `assert_eq!(owner(..).ok(), None)` as that `Err`
//! assertion, written directly, through a constant-row table row whose
//! expected cell is `None`, or through a check helper the test calls with
//! `None`.
//!
//! Like the tuple-match producer this refines only a weak discriminator and
//! never repairs reach or propagation. Any unsupported shape leaves the
//! existing evidence unchanged, so the finding keeps its gap.

use super::tuple_match::{parsed, same_current_file};
use crate::analysis::classify::ProbeContext;
use crate::analysis::facts::cfg_predicates::{
    attribute_requires_test, attribute_test_build_availability,
};
use crate::analysis::rust_index::find_file_facts;
use crate::domain::{Confidence, ProbeFamily, RelationReason, StageEvidence, StageState};
use ra_ap_syntax::ast::{
    HasArgList, HasAttrs, HasGenericArgs, HasGenericParams, HasLoopBody, HasName,
};
use ra_ap_syntax::{AstNode, SyntaxKind, SyntaxNode, ast};

pub(super) mod guard;
pub(super) mod mock_count;

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
        // Decision 5 reads a check helper's assertion, so a test that
        // reaches the owner through a helper is examined too; the
        // structural checks below decide whether its call is read.
        if !matches!(
            reason,
            RelationReason::DirectOwnerCall | RelationReason::HelperOwnerCall
        ) {
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
        if asserts_owner_err(test_root.syntax(), &test_function, &owner.name) {
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
                && (attribute.excl_token().is_none() || is_test_build_gate(&text))
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
fn asserts_owner_err(root: &SyntaxNode, function: &ast::Fn, owner: &str) -> bool {
    asserts_owner_err_inner(root, function, owner).unwrap_or(false)
}

fn asserts_owner_err_inner(root: &SyntaxNode, function: &ast::Fn, owner: &str) -> Option<bool> {
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
    // A file-level `const owner: fn(..) = ..`, `static`, `struct` or
    // `as owner` rename makes `owner(..)` something other than the owner.
    // A second `fn owner` already fails the unique-function lookups.
    // Another function's parameter or local binding cannot reach this
    // test; the test body and helper scans read their own bindings.
    if root.descendants().filter_map(ast::Name::cast).any(|bound| {
        bound.text() == owner
            && !bound.syntax().parent().is_some_and(|parent| {
                ast::Fn::can_cast(parent.kind()) || ast::IdentPat::can_cast(parent.kind())
            })
    }) {
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
            && (!call.path().is_some_and(|path| {
                ASSERTION_MACROS.contains(&path.syntax().text().to_string().as_str())
            }) || token_tree_may_exit(&call, false))
        {
            return None;
        }
        // A local binding, block item (`const owner: fn(..) = ..`) or
        // import (`use x::owner`, `use x as owner`) of the owner's name
        // makes `owner(..)` call something else.
        if binds_name(&node, owner) {
            return None;
        }
    }
    let statements = body.stmt_list()?;
    // Decision 5 reads the text `None`; a file that can rebind that name
    // (`const None`, `use x::None`, a variant named `None`) gives none of
    // its three forms.
    let none_is_prelude = !file_binds_name(root, "None", None);
    let observes = |expression: &ast::Expr| {
        // `#[cfg(..)] assert!(..);` may never compile.
        !expression
            .syntax()
            .descendants()
            .any(|node| ast::Attr::can_cast(node.kind()))
            && if ok_side_expected(expression, owner).is_some() {
                none_is_prelude && observes_err_side(expression, owner)
            } else {
                observes_err_side(expression, owner)
                    || (none_is_prelude
                        && (table_row_observes_err(expression, function, owner)
                            || helper_call_observes_err(expression, root, function, owner)))
            }
    };
    Some(
        statements
            .statements()
            .filter_map(|statement| match statement {
                ast::Stmt::ExprStmt(statement)
                    if !statement
                        .syntax()
                        .children()
                        .any(|node| ast::Attr::can_cast(node.kind())) =>
                {
                    statement.expr()
                }
                _ => None,
            })
            // A loop that ends the body is its tail expression.
            .chain(statements.tail_expr())
            .any(|expression| observes(&expression)),
    )
}

/// RIPR-SPEC-0227 decision 5: `assert_eq!(owner(..).ok(), want)` (either
/// operand order) returns the expected operand's text when the other is
/// `.ok()` with no arguments on a direct owner call. A passing comparison
/// with `None` proves that call returned `Err`, exactly as `is_err()` does.
fn ok_side_expected(expression: &ast::Expr, owner: &str) -> Option<String> {
    let arguments = macro_arguments(expression, "assert_eq")?;
    let [left, right, ..] = arguments.as_slice() else {
        return None;
    };
    let ok_side = |text: &str| {
        parse_expression(text).is_some_and(|operand| match operand {
            ast::Expr::MethodCallExpr(call) => {
                call.name_ref().is_some_and(|name| name.text() == "ok")
                    && call
                        .arg_list()
                        .is_some_and(|list| list.args().next().is_none())
                    && call
                        .receiver()
                        .is_some_and(|receiver| is_direct_owner_call(&receiver, owner))
            }
            _ => false,
        })
    };
    if ok_side(left) {
        Some(right.trim().to_string())
    } else if ok_side(right) {
        Some(left.trim().to_string())
    } else {
        None
    }
}

/// Decision 5 through a constant-row table (#5328): a top-level unlabeled
/// `for` whose body holds `assert_eq!(owner(..).ok(), want)` as a direct
/// statement, nothing in the body can skip a row, and `want` is a column
/// of the table with a `None` cell. The loop runs that row, so its call
/// returned `Err`.
fn table_row_observes_err(expression: &ast::Expr, function: &ast::Fn, owner: &str) -> bool {
    let ast::Expr::ForExpr(for_expr) = expression else {
        return false;
    };
    if for_expr.label().is_some() {
        return false;
    }
    let Some(body) = for_expr.loop_body().and_then(|body| body.stmt_list()) else {
        return false;
    };
    if body.syntax().descendants().any(|node| {
        ast::BreakExpr::can_cast(node.kind())
            || ast::ContinueExpr::can_cast(node.kind())
            // `use consts::want;` in the body hides the row binding.
            || is_block_item(&node)
    }) {
        return false;
    }
    // The column must be bound by this loop's own pattern, not by another
    // table loop of the test.
    let binds_in_pattern = |name: &str| {
        for_expr.pat().is_some_and(|pat| {
            pat.syntax()
                .descendants()
                .filter_map(ast::IdentPat::cast)
                .any(|pattern| pattern.name().is_some_and(|bound| bound.text() == name))
        })
    };
    let fn_text = function.syntax().text().to_string();
    body.statements()
        .filter_map(|statement| match statement {
            ast::Stmt::ExprStmt(statement) => statement.expr(),
            _ => None,
        })
        .chain(body.tail_expr())
        .any(|assertion| {
            ok_side_expected(&assertion, owner).is_some_and(|expected| {
                is_plain_identifier(&expected)
                    && binds_in_pattern(&expected)
                    && crate::analysis::syntax::constant_table_column(&fn_text, &expected)
                        .is_some_and(|cells| cells.iter().any(|cell| cell.trim() == "None"))
            })
        })
}

/// Decision 5 through a check helper: a top-level call `helper(.., None, ..)`
/// to a unique free function declared beside the test, in the same module,
/// whose body asserts `assert_eq!(owner(..).ok(), want)` as a direct
/// statement, where `want` is the plain, never-rebound parameter that call
/// fills with `None`. No other item, import or binding in the file may
/// carry the helper's name, so the call resolves to that function. The
/// helper body may not return early (including through `?`), hold a
/// closure, nested item or other macro, or bind the owner's or a
/// parameter's name.
fn helper_call_observes_err(
    expression: &ast::Expr,
    root: &SyntaxNode,
    test: &ast::Fn,
    owner: &str,
) -> bool {
    let ast::Expr::CallExpr(call) = expression else {
        return false;
    };
    let Some(name) = call.expr().map(|callee| callee.syntax().text().to_string()) else {
        return false;
    };
    if name == owner || !is_plain_identifier(&name) {
        return false;
    }
    let Some(arguments) = call.arg_list().map(|list| list.args().collect::<Vec<_>>()) else {
        return false;
    };
    if arguments.iter().any(|argument| {
        argument.syntax().descendants().any(|node| {
            ast::MacroCall::can_cast(node.kind())
                || ast::ClosureExpr::can_cast(node.kind())
                || ast::BlockExpr::can_cast(node.kind())
                || ast::CallExpr::can_cast(node.kind())
                || ast::MethodCallExpr::can_cast(node.kind())
        })
    }) {
        return false;
    }
    let Some(helper) = unique_function(root, &name) else {
        return false;
    };
    if helper.syntax().parent() != test.syntax().parent()
        || file_binds_name(root, &name, helper.name().as_ref())
    {
        return false;
    }
    let nested = helper.syntax().ancestors().skip(1).any(|node| {
        ast::Impl::can_cast(node.kind())
            || ast::Trait::can_cast(node.kind())
            || ast::Fn::can_cast(node.kind())
    });
    if nested
        || helper.attrs().next().is_some()
        || helper.async_token().is_some()
        || helper.generic_param_list().is_some()
    {
        return false;
    }
    let Some(parameters) = helper.param_list().map(|list| {
        list.params()
            .map(|parameter| match parameter.pat() {
                Some(ast::Pat::IdentPat(pattern))
                    if pattern.mut_token().is_none() && pattern.ref_token().is_none() =>
                {
                    pattern.name().map(|name| name.text().to_string())
                }
                _ => None,
            })
            .collect::<Option<Vec<_>>>()
    }) else {
        return false;
    };
    let Some(parameters) = parameters else {
        return false;
    };
    if helper
        .param_list()
        .is_some_and(|list| list.self_param().is_some())
        || parameters.len() != arguments.len()
        || parameters.iter().any(|parameter| parameter == owner)
    {
        return false;
    }
    let Some(body) = helper.body() else {
        return false;
    };
    for node in body.syntax().descendants() {
        if ast::ReturnExpr::can_cast(node.kind())
            || ast::ClosureExpr::can_cast(node.kind())
            || ast::Fn::can_cast(node.kind())
            || ast::BreakExpr::can_cast(node.kind())
            || ast::ContinueExpr::can_cast(node.kind())
            || ast::TryExpr::can_cast(node.kind())
            // A block item (`const`, `use`, `static`) can rebind the
            // owner's or a parameter's name, and `#[cfg(..)]` can compile
            // the assertion out.
            || is_block_item(&node)
            || ast::Attr::can_cast(node.kind())
        {
            return false;
        }
        if let Some(macro_call) = ast::MacroCall::cast(node.clone())
            && (!macro_call.path().is_some_and(|path| {
                ASSERTION_MACROS.contains(&path.syntax().text().to_string().as_str())
            }) || token_tree_may_exit(&macro_call, true))
        {
            return false;
        }
        if ast::IdentPat::cast(node).is_some_and(|pattern| {
            pattern.name().is_some_and(|bound| {
                let bound = bound.text();
                bound == owner || parameters.iter().any(|parameter| *parameter == bound)
            })
        }) {
            return false;
        }
    }
    let Some(statements) = body.stmt_list() else {
        return false;
    };
    statements
        .statements()
        .filter_map(|statement| match statement {
            ast::Stmt::ExprStmt(statement) => statement.expr(),
            _ => None,
        })
        .chain(statements.tail_expr())
        .any(|assertion| {
            ok_side_expected(&assertion, owner).is_some_and(|expected| {
                parameters
                    .iter()
                    .position(|parameter| *parameter == expected)
                    .and_then(|position| arguments.get(position))
                    .is_some_and(|argument| argument.syntax().text() == "None")
            })
        })
}

/// Whether anything in the file other than `except` can bind `name`: an
/// item, variant, field-less declaration, `as` rename or local binding of
/// that name, or a `use` whose last segment is that name. A bare
/// identifier pattern reads as a binding too, so `None =>` in a `match`
/// refuses as well (fail closed).
fn file_binds_name(root: &SyntaxNode, name: &str, except: Option<&ast::Name>) -> bool {
    root.descendants().any(|node| {
        if let Some(bound) = ast::Name::cast(node.clone()) {
            return bound.text() == name && except.is_none_or(|except| *except != bound);
        }
        binds_name(&node, name)
    })
}

/// An assertion macro whose operands hold `return`, `break` or
/// `continue` (and, in a helper, `?`) can leave before its comparison or a
/// later one runs: `assert!({ return; true })`. Its operands are a token
/// tree, so the AST scans above never see them.
fn token_tree_may_exit(call: &ast::MacroCall, try_exits: bool) -> bool {
    call.token_tree().is_some_and(|tree| {
        tree.syntax()
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| match token.kind() {
                SyntaxKind::RETURN_KW | SyntaxKind::BREAK_KW | SyntaxKind::CONTINUE_KW => true,
                SyntaxKind::QUESTION => try_exits,
                _ => false,
            })
    })
}

/// An item declared inside a block (`const`, `static`, `use`, `struct`,
/// ..). Macro calls parse as items too and are judged separately.
fn is_block_item(node: &SyntaxNode) -> bool {
    ast::Item::can_cast(node.kind()) && !ast::MacroCall::can_cast(node.kind())
}

/// Whether this one node binds `name`: a `Name` (item, variant, `as`
/// rename or binding) or a `use` whose last segment is `name`.
fn binds_name(node: &SyntaxNode, name: &str) -> bool {
    if let Some(bound) = ast::Name::cast(node.clone()) {
        return bound.text() == name;
    }
    ast::UseTree::cast(node.clone()).is_some_and(|tree| {
        tree.rename().is_none()
            && tree
                .path()
                .and_then(|path| path.segment())
                .and_then(|segment| segment.name_ref())
                .is_some_and(|segment| segment.text() == name)
    })
}

fn is_plain_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

/// A `cfg` gate that requires a test build and is enabled in one
/// (`#[cfg(test)]`), read through the shared cfg-predicate authority.
fn is_test_build_gate(attribute: &str) -> bool {
    attribute_requires_test(attribute) && attribute_test_build_availability(attribute) == Some(true)
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
                .all(|attribute| is_test_build_gate(&attribute.syntax().text().to_string()))
        })
}

/// `assert!(owner(..).is_err())`, `assert!(!owner(..).is_ok())`,
/// `assert!(matches!(owner(..), Err(_)))`, `assert_eq!(owner(..).ok(),
/// None)` (decision 5) or a statement `owner(..).unwrap_err()` /
/// `.expect_err(..)`.
fn observes_err_side(expression: &ast::Expr, owner: &str) -> bool {
    if ok_side_expected(expression, owner).is_some_and(|expected| expected == "None") {
        return true;
    }
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
        parsed(source).is_some_and(|root| {
            unique_function(root.syntax(), test)
                .is_some_and(|function| asserts_owner_err(root.syntax(), &function, owner))
        })
    }

    #[test]
    fn ok_compared_with_none_observes_the_flip() {
        // RIPR-SPEC-0227 decision 5: `.ok() == None` is `is_err()`, written
        // directly, through a constant-row table row or a check helper.
        let direct = "#[test]\nfn t() {\n    assert_eq!(port(\"http\").ok(), None);\n}\n";
        assert!(test_observes(direct, "t", "port"));
        let swapped =
            "#[test]\nfn t() {\n    assert_eq!(None, port(\"http\").ok(), \"words\");\n}\n";
        assert!(test_observes(swapped, "t", "port"));
        let table = "#[test]\nfn t() {\n    let cases = [(\"50%\", Some(50)), (\"lots\", None)];\n    for (text, want) in cases {\n        assert_eq!(port(text).ok(), want, \"{text}\");\n    }\n}\n";
        assert!(test_observes(table, "t", "port"));
        let helper = "fn check(text: &str, want: Option<u16>) {\n    assert_eq!(port(text).ok(), want, \"{text}\");\n}\n#[test]\nfn t() {\n    check(\"80\", Some(80));\n    check(\"http\", None);\n}\n";
        assert!(test_observes(helper, "t", "port"));
        let unrelated_parameter = "fn display(port: u16) -> u16 {\n    port\n}\n#[test]\nfn t() {\n    assert_eq!(port(\"http\").ok(), None);\n}\n";
        assert!(test_observes(unrelated_parameter, "t", "port"));
    }

    #[test]
    fn ok_compared_with_some_or_unrun_does_not_observe_the_flip() {
        for (label, source) in [
            (
                "a Some expectation",
                "#[test]\nfn t() {\n    assert_eq!(port(\"80\").ok(), Some(80));\n}\n",
            ),
            (
                "assert_ne",
                "#[test]\nfn t() {\n    assert_ne!(port(\"80\").ok(), None);\n}\n",
            ),
            (
                "no None row",
                "#[test]\nfn t() {\n    let cases = [(\"50\", Some(50)), (\"7\", Some(7))];\n    for (text, want) in cases {\n        assert_eq!(port(text).ok(), want);\n    }\n}\n",
            ),
            (
                "a computed table",
                "#[test]\nfn t() {\n    let cases = rows();\n    for (text, want) in cases {\n        assert_eq!(port(text).ok(), want);\n    }\n}\n",
            ),
            (
                "a row that may be skipped",
                "#[test]\nfn t() {\n    let cases = [(\"lots\", None)];\n    for (text, want) in cases {\n        if text.is_empty() { continue; }\n        assert_eq!(port(text).ok(), want);\n    }\n}\n",
            ),
            (
                "a conditional assertion in the loop",
                "#[test]\nfn t() {\n    let cases = [(\"lots\", None)];\n    for (text, want) in cases {\n        if text.len() > 9 {\n            assert_eq!(port(text).ok(), want);\n        }\n    }\n}\n",
            ),
            (
                "a helper called with Some only",
                "fn check(text: &str, want: Option<u16>) {\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"80\", Some(80));\n}\n",
            ),
            (
                "a helper that rebinds its parameter",
                "fn check(text: &str, want: Option<u16>) {\n    let want = Some(1);\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper that may return first",
                "fn check(text: &str, want: Option<u16>) {\n    if text.is_empty() { return; }\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper asserting another function",
                "fn check(text: &str, want: Option<u16>) {\n    assert_eq!(other(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper call inside a closure",
                "fn check(text: &str, want: Option<u16>) {\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    let f = || check(\"http\", None);\n}\n",
            ),
            (
                "a shadowing const None",
                "const None: Option<u16> = Some(3);\n#[test]\nfn t() {\n    assert_eq!(port(\"80\").ok(), None);\n}\n",
            ),
            (
                "an imported None in a table",
                "use other::None;\n#[test]\nfn t() {\n    let cases = [(\"lots\", None)];\n    for (text, want) in cases {\n        assert_eq!(port(text).ok(), want);\n    }\n}\n",
            ),
            (
                "a None variant passed to a helper",
                "enum E { None }\nfn check(text: &str, want: Option<u16>) {\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper name that resolves to an import",
                "use other::check;\nmod dead {\n    fn check(text: &str, want: Option<u16>) {\n        assert_eq!(port(text).ok(), want);\n    }\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper name renamed onto a struct",
                "struct Check(&'static str, Option<u16>);\nuse Check as check;\nmod dead {\n    pub fn check(text: &str, want: Option<u16>) {\n        assert_eq!(port(text).ok(), want);\n    }\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper called with a non-None constant",
                "fn check(text: &str, want: Option<u16>) {\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"80\", WANT);\n}\n",
            ),
            (
                "None filling a parameter the helper does not compare",
                "fn check(text: &str, want: Option<u16>, note: Option<u16>) {\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"80\", WANT, None);\n}\n",
            ),
            (
                "a test-local binding of the helper's name",
                "fn check(text: &str, want: Option<u16>) {\n    assert_eq!(port(text).ok(), want);\n}\nfn skip(_: &str, _: Option<u16>) {}\n#[test]\nfn t() {\n    let check = skip;\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper-local const rebinding the owner",
                "fn check(text: &str, want: Option<u16>) {\n    const port: fn(&str) -> Result<u16, ()> = other;\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a test-local const rebinding the owner",
                "#[test]\nfn t() {\n    const port: fn(&str) -> Result<u16, ()> = other;\n    assert_eq!(port(\"80\").ok(), None);\n}\n",
            ),
            (
                "a test-local import renamed onto the owner",
                "#[test]\nfn t() {\n    use crate::other as port;\n    assert_eq!(port(\"80\").ok(), None);\n}\n",
            ),
            (
                "a bare None pattern in the file (fail closed)",
                "fn kind(v: Option<u16>) -> u8 {\n    match v {\n        None => 0,\n        Some(_) => 1,\n    }\n}\n#[test]\nfn t() {\n    assert_eq!(port(\"http\").ok(), None);\n}\n",
            ),
            (
                "a helper-local const hiding the compared parameter",
                "fn check(text: &str, want: Option<u16>) {\n    const want: Option<u16> = Some(3);\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a helper parameter named like the owner",
                "fn check(text: &str, want: Option<u16>, port: fn(&str) -> Result<u16, ()>) {\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None, fake);\n}\n",
            ),
            (
                "a cfg-gated helper assertion",
                "fn check(text: &str, want: Option<u16>) {\n    #[cfg(any())]\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "an import hiding the row binding in the loop",
                "#[test]\nfn t() {\n    let cases = [(\"12\", None), (\"3\", Some(7))];\n    for (text, want) in cases {\n        use consts::want;\n        assert_eq!(port(text).ok(), want);\n    }\n}\n",
            ),
            (
                "a None column bound by a different loop",
                "#[test]\nfn t() {\n    let cases = [(\"12\", None)];\n    for (text, want) in cases {\n        let _ = (text, want);\n    }\n    for _row in [1] {\n        assert_eq!(port(\"12\").ok(), want);\n    }\n}\n",
            ),
            (
                "a file-level const rebinding the owner",
                "const port: fn(&str) -> Result<u16, ()> = fake;\n#[test]\nfn t() {\n    assert_eq!(port(\"80\").ok(), None);\n}\n",
            ),
            (
                "a file-level struct named like the owner",
                "struct port<'a>(&'a str);\n#[test]\nfn t() {\n    assert_eq!(port(\"80\").ok(), None);\n}\n",
            ),
            (
                "a helper whose assertion operand returns",
                "fn check(text: &str, want: Option<u16>) {\n    assert!({ return; true });\n    assert_eq!(port(text).ok(), want);\n}\n#[test]\nfn t() {\n    check(\"http\", None);\n}\n",
            ),
            (
                "a test whose assertion operand returns",
                "#[test]\nfn t() {\n    assert!({ return; true });\n    assert_eq!(port(\"http\").ok(), None);\n}\n",
            ),
            (
                "a helper that may leave through ?",
                "fn check(text: &str, want: Option<u16>) -> Option<()> {\n    text.chars().next()?;\n    assert_eq!(port(text).ok(), want);\n    Some(())\n}\n#[test]\nfn t() {\n    check(\"\", None);\n}\n",
            ),
        ] {
            assert!(
                !test_observes(source, "t", "port"),
                "{label} must not observe the flip"
            );
        }
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
