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
use ra_ap_syntax::ast::{HasArgList, HasAttrs, HasName};
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
    let operand = sole_try_operand(after)?;
    if sole_try_operand(before)? != operand {
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

/// The `?` operand of a one-line statement or expression, normalized so a
/// behavior-preserving respelling compares equal: parentheses around an
/// expression and generic arguments (`parse::<u16>()`) are dropped, and
/// whitespace is ignored. `None` unless the text holds exactly one `?`.
fn sole_try_operand(line: &str) -> Option<String> {
    let line = line.trim();
    let source = if line.ends_with(';') || line.ends_with('}') {
        format!("fn __try() {{ {line} }}")
    } else {
        format!("fn __try() {{ {line}; }}")
    };
    let root = parsed(&source)?;
    let mut tries = root.syntax().descendants().filter_map(ast::TryExpr::cast);
    let try_expr = tries.next()?;
    if tries.next().is_some() {
        return None;
    }
    let operand = try_expr.expr()?;
    let mut text = String::new();
    for element in operand.syntax().descendants_with_tokens() {
        let Some(token) = element.into_token() else {
            continue;
        };
        if token.kind().is_trivia() {
            continue;
        }
        let parent = token.parent()?;
        if parent
            .ancestors()
            .any(|node| ast::GenericArgList::can_cast(node.kind()))
        {
            continue;
        }
        if ast::ParenExpr::can_cast(parent.kind()) && matches!(token.text(), "(" | ")") {
            continue;
        }
        text.push_str(token.text());
    }
    Some(text)
}

/// No `macro_rules!` anywhere in the test file (it can shadow `assert!` or
/// `matches!`), and no inner attribute other than `#![cfg(test)]` (an inner
/// `cfg` compiles the test out without touching its function or modules).
fn test_file_macros_and_inner_cfg_are_plain(root: &SyntaxNode) -> bool {
    root.descendants().all(|node| {
        if ast::MacroRules::can_cast(node.kind()) {
            return false;
        }
        ast::Attr::cast(node).is_none_or(|attribute| {
            attribute.excl_token().is_none() || attribute.syntax().text() == "#![cfg(test)]"
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

/// `owner(args)` by its bare name, with no argument that calls the owner.
fn is_direct_owner_call(expression: &ast::Expr, owner: &str) -> bool {
    let ast::Expr::CallExpr(call) = expression else {
        return false;
    };
    call.expr()
        .is_some_and(|callee| callee.syntax().text() == owner)
        && call.arg_list().is_some_and(|list| {
            list.args().all(|argument| {
                !argument
                    .syntax()
                    .descendants()
                    .filter_map(ast::PathExpr::cast)
                    .any(|path| path.syntax().text() == owner)
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
        ] {
            assert!(!test_observes(source, "t", "port"), "{label}");
        }
    }

    #[test]
    fn a_respelled_operand_compares_equal_and_a_changed_one_does_not() {
        let same = sole_try_operand("let port = text.trim().parse::<u16>()?;");
        assert_eq!(same.as_deref(), Some("text.trim().parse()"));
        assert_eq!(
            sole_try_operand("let port: u16 = text.trim().parse()?;"),
            same
        );
        assert_eq!(
            sole_try_operand("let d = (digit(c))?;"),
            sole_try_operand("let d = digit(c)?;")
        );
        for changed in [
            "let port: u16 = text.parse()?;",
            "let port: u16 = text.trim().parse_strict()?;",
            "let port: u16 = text.trim().parse().and_then(check)?;",
        ] {
            assert_ne!(sole_try_operand(changed), same, "{changed}");
        }
        assert_eq!(sole_try_operand("let d = digit(c).unwrap_or(0);"), None);
        assert_eq!(sole_try_operand("let d = a(c)? + b(c)?;"), None);
    }

    #[test]
    fn macro_definitions_and_inner_cfg_make_a_test_file_unreadable() {
        let plain =
            "#![cfg(test)]\nuse super::*;\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n";
        let root = parsed(plain).map(|root| root.syntax().clone());
        assert!(root.is_some_and(|root| test_file_macros_and_inner_cfg_are_plain(&root)));
        for source in [
            "macro_rules! assert { ($($t:tt)*) => {}; }\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
            "mod tests {\n    #![cfg(any())]\n    #[test]\n    fn t() { assert!(port(\"x\").is_err()); }\n}\n",
            "#![cfg(any())]\n#[test]\nfn t() { assert!(port(\"x\").is_err()); }\n",
        ] {
            let root = parsed(source).map(|root| root.syntax().clone());
            assert!(
                root.is_some_and(|root| !test_file_macros_and_inner_cfg_are_plain(&root)),
                "{source}"
            );
        }
    }
}
