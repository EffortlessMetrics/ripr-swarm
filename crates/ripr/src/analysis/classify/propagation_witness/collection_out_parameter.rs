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
//! - the owner is a plain free function (not async, const, unsafe, generic
//!   or a method, read from its full declaration)
//!   whose only statement is one unconditional `receiver.push(..)` into a
//!   `&mut Vec<..>` or `&mut String` parameter;
//! - the test's only attribute is a plain `#[test]`, and at its top level
//!   binds `let mut b = Vec::new();` (or `String::new()`, or a `default()`),
//!   then calls the owner exactly once, resolved to the owner and not a
//!   namesake, with `&mut b` at the pushed parameter's position;
//! - no other statement mentions `b` before the call, no other argument
//!   mentions it, and after the call every statement that mentions `b` is a
//!   recorded `assert!`, `assert_eq!` or `assert_ne!` whose every mention of
//!   `b` is `b.len()` or `b.is_empty()`, and the body's tail expression
//!   does not mention `b`. Any other read may already check the contents,
//!   so it is not this module's to grade.

use crate::analysis::classify::OwnerPinSyntax;
use crate::analysis::classify::call_identity::{CallTarget, shared_owner_call_identity};
use crate::analysis::classify::targeted_free_call_reaches_owner;
use crate::analysis::facts::OracleFact;
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

/// Whether `test` is followed under the rules above and one of its count
/// reads pins the single push: `assert_eq!(b.len(), N)` (either order) with
/// a positive integer `N`. One push into a fresh, empty local leaves it
/// non-empty, and a deleted push leaves it empty, so such a read notices the
/// deletion. It still cannot tell a changed pushed value apart. Only this
/// exact equality pins: both the diff side and the repository side grade it
/// a strong exact check, so they agree that the deletion is noticed. A
/// boolean read such as `assert!(!b.is_empty())` grades weaker there, so it
/// pins nothing here either.
pub(in crate::analysis) fn test_count_pins_pushed_collection(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    syntax: &OwnerPinSyntax,
) -> bool {
    followed_by_count(receiver, owner, index, test, syntax)
        .is_some_and(|reads| reads.iter().any(|read| read.pins_push))
}

/// Whether `assertion` is one of `test`'s count reads that pins the single
/// push (see [`test_count_pins_pushed_collection`]). The one rule both the
/// diff-side deleted-call probe and the repository-side call-presence seam
/// read, so they grade a deleted push the same way.
pub(in crate::analysis) fn assertion_pins_pushed_collection_count(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &OwnerPinSyntax,
) -> bool {
    // Cheap text gates first: this runs per (probe, test, assertion).
    if !assertion.text.contains(".len()") {
        return false;
    }
    followed_by_count(receiver, owner, index, test, syntax).is_some_and(|reads| {
        reads.iter().any(|read| {
            read.pins_push
                && read.line == assertion.line
                && read.text == compact(&assertion.text).trim_end_matches(';')
        })
    })
}

/// One count read of the followed local: its line, its compact text
/// without the trailing `;`, and whether it pins the single push.
struct CountRead {
    line: usize,
    text: String,
    pins_push: bool,
}

fn followed_by_count(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    syntax: &OwnerPinSyntax,
) -> Option<Vec<CountRead>> {
    // Cheap text gates first: this runs per (seam, related test).
    if owner.item.has_self_param
        || !owner.body.contains("&mut")
        || !test.body.contains("&mut")
        || test.assertions.is_empty()
        || !only_plain_test_attribute(test)
    {
        return None;
    }
    let parameter = sole_pushed_parameter(receiver, owner, index)?;
    if [&owner.file, &test.file].into_iter().any(|file| {
        index.files().get(file).is_none_or(|facts| {
            crate::analysis::classify::out_parameter::file_shadows_buffer_types(&facts.source)
        })
    }) {
        return None;
    }
    let tree = parse_clean_source_file(&test.body)?.tree();
    let function = tree.syntax().descendants().find_map(ast::Fn::cast)?;
    let body = function.body()?.stmt_list()?;
    // An inner attribute (`#![cfg(..)]`) may remove the whole body.
    if body.attrs().next().is_some() {
        return None;
    }
    let statements: Vec<ast::Stmt> = body.statements().collect();
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
    // A final check written without `;` is the block's tail, not a
    // statement, so it is refused rather than left unread.
    if body
        .tail_expr()
        .is_some_and(|tail| mentions_word(tail.syntax(), &local) > 0)
    {
        return None;
    }
    let mut reads = Vec::new();
    let mut last_read = *call_at;
    for (at, statement) in statements.iter().enumerate().skip(call_at + 1) {
        if mentions_word(statement.syntax(), &local) == 0 {
            continue;
        }
        let line = count_read_assertion(statement, &local, test, index, syntax)?;
        let text = compact(&statement.syntax().text().to_string())
            .trim_end_matches(';')
            .to_string();
        reads.push(CountRead {
            pins_push: pins_single_push(&text, &local),
            line,
            text,
        });
        last_read = at;
    }
    // Straight line up to the last read: nothing but the definition, the
    // call and the count reads, so no `return`, `?`, branch or other
    // statement can end the test before a read or stand between them.
    let straight = statements
        .get(..=last_read)?
        .iter()
        .enumerate()
        .all(|(at, statement)| {
            at == *call_at
                || matches!(statement, ast::Stmt::LetStmt(definition)
                    if fresh_empty_definition(definition, &local, parameter.string))
                || (at > *call_at && mentions_word(statement.syntax(), &local) > 0)
        });
    (!reads.is_empty() && straight).then_some(reads)
}

/// Whether the compact read `text` is `assert_eq!(b.len(), N)` (either
/// order) with a positive integer literal `N`.
fn pins_single_push(text: &str, local: &str) -> bool {
    let positive = |literal: &str| {
        !literal.is_empty()
            && literal.bytes().all(|byte| byte.is_ascii_digit())
            && literal.bytes().any(|byte| byte != b'0')
    };
    let count = format!("{local}.len()");
    let Some(operands) = text
        .strip_prefix("assert_eq!(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    match operands.split_once(',') {
        Some((left, right)) if left == count => positive(right),
        Some((left, right)) if right == count => positive(left),
        _ => false,
    }
}

/// The pushed parameter's position, the parameter count, and whether the
/// parameter is a `String` (else a `Vec`).
struct PushedParameter {
    at: usize,
    count: usize,
    string: bool,
}

fn sole_pushed_parameter(
    receiver: &str,
    owner: &FunctionSummary,
    index: &RustIndex,
) -> Option<PushedParameter> {
    // The full source declaration: the indexed body slice starts at `fn`
    // and drops qualifiers such as `async` or `const`.
    let function = index
        .source_syntax
        .get(index, &owner.file)?
        .declaration(owner)?;
    if function.async_token().is_some()
        || function.unsafe_token().is_some()
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
        || push.arg_list()?.args().any(|argument| {
            mentions_word(argument.syntax(), receiver) > 0 || !straight_line_argument(&argument)
        })
    {
        return None;
    }
    Some(PushedParameter {
        at,
        count: parameters.len(),
        string,
    })
}

/// Whether the pushed argument always finishes and reaches the push: no
/// `return`, `?`, `break`, `continue`, `.await`, `yield` or `let`, and no
/// macro, closure, block, `if`, `match` or loop whose contents could skip or
/// replace the push. Plain values, paths, field reads, operators and calls
/// remain; a call that panics fails the test instead of skipping the push.
fn straight_line_argument(argument: &ast::Expr) -> bool {
    std::iter::once(argument.syntax().clone())
        .chain(argument.syntax().descendants())
        .all(|node| {
            !matches!(
                node.kind(),
                SyntaxKind::RETURN_EXPR
                    | SyntaxKind::TRY_EXPR
                    | SyntaxKind::BREAK_EXPR
                    | SyntaxKind::CONTINUE_EXPR
                    | SyntaxKind::AWAIT_EXPR
                    | SyntaxKind::YIELD_EXPR
                    | SyntaxKind::YEET_EXPR
                    | SyntaxKind::BECOME_EXPR
                    | SyntaxKind::LET_EXPR
                    | SyntaxKind::MACRO_EXPR
                    | SyntaxKind::MACRO_CALL
                    | SyntaxKind::CLOSURE_EXPR
                    | SyntaxKind::BLOCK_EXPR
                    | SyntaxKind::IF_EXPR
                    | SyntaxKind::MATCH_EXPR
                    | SyntaxKind::LOOP_EXPR
                    | SyntaxKind::WHILE_EXPR
                    | SyntaxKind::FOR_EXPR
                    | SyntaxKind::ERROR
            )
        })
}

/// The test's own attributes, read from the full declaration, are exactly
/// one plain `#[test]`: a `cfg`-gated, ignored, panic-expecting or
/// otherwise wrapped test may never run the read, and an `async` test needs
/// another harness attribute. No recorded attribute (the lexical fallback)
/// is refused too.
fn only_plain_test_attribute(test: &TestSummary) -> bool {
    matches!(test.attrs.as_slice(), [only] if compact(only) == "#[test]")
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

/// The line of a statement that is one recorded, admitted `assert!`,
/// `assert_eq!` or `assert_ne!` whose every mention of `local` is
/// `local.len()` or `local.is_empty()`; `None` for any other statement.
fn count_read_assertion(
    statement: &ast::Stmt,
    local: &str,
    test: &TestSummary,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
) -> Option<usize> {
    // An attribute on the check (`#[cfg(any())] assert_eq!(..)`) may
    // remove it.
    if statement
        .syntax()
        .descendants()
        .any(|node| node.kind() == SyntaxKind::ATTR)
    {
        return None;
    }
    let Some(ast::Expr::MacroExpr(expression)) = statement_expression(statement) else {
        return None;
    };
    let call = expression.macro_call()?;
    let name = compact(&call.path()?.syntax().text().to_string());
    if !matches!(name.as_str(), "assert" | "assert_eq" | "assert_ne") {
        return None;
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
    (recorded && every_mention_counts(statement.syntax(), local)).then_some(line)
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

#[cfg(test)]
mod tests {
    use super::{test_count_pins_pushed_collection, test_counts_pushed_collection_argument};
    use crate::analysis::classify::OwnerPinSyntax;
    use crate::analysis::facts::build_index;
    use std::path::PathBuf;

    const OWNER: &str = "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(msg.to_uppercase());\n}\n";

    struct Root(PathBuf);

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Whether the rule holds for `owner` (plus `extra` items) in
    /// `src/lib.rs`, with one inline test whose header is `header` and whose
    /// body is `body`. The owner is the `log_line`/`add_mark` defined first.
    fn follows(owner: &str, extra: &str, header: &str, body: &str) -> Result<bool, String> {
        Ok(judge(owner, extra, header, body)?.0)
    }

    /// Whether the rule holds, and whether a count read pins the push.
    fn judge(owner: &str, extra: &str, header: &str, body: &str) -> Result<(bool, bool), String> {
        let source = format!(
            "{owner}{extra}\n#[cfg(test)]\nmod tests {{\n    use super::*;\n\n{header}\n    fn t() {{\n{body}    }}\n}}\n"
        );
        let root = Root(crate::analysis::facts::drop_in::temp_workspace(
            "collection-out-parameter",
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", &source),
            ],
        )?);
        let index = build_index(&root.0, &[PathBuf::from("src/lib.rs")])?;
        let owner = index
            .functions()
            .iter()
            .filter(|function| function.body.contains(".push("))
            .min_by_key(|function| function.start_line)
            .ok_or("owner")?;
        let test = index
            .tests()
            .iter()
            .find(|test| test.name == "t")
            .ok_or("test")?;
        let receiver = "buf";
        let syntax = OwnerPinSyntax::default();
        Ok((
            test_counts_pushed_collection_argument(receiver, owner, &index, test, &syntax),
            test_count_pins_pushed_collection(receiver, owner, &index, test, &syntax),
        ))
    }

    const TEST: &str = "    #[test]";
    const CALL: &str = "        let mut b = Vec::new();\n        log_line(&mut b, \"hi\");\n";

    #[test]
    fn count_reads_of_a_fresh_vector_or_string_are_followed() -> Result<(), String> {
        assert!(follows(
            OWNER,
            "",
            TEST,
            &format!("{CALL}        assert_eq!(b.len(), 1);\n")
        )?);
        assert!(follows(
            OWNER,
            "",
            TEST,
            &format!("{CALL}        assert!(!b.is_empty());\n")
        )?);
        let string = "pub fn add_mark(buf: &mut String, c: char) {\n    buf.push(c.to_ascii_uppercase());\n}\n";
        assert!(follows(
            string,
            "",
            TEST,
            "        let mut s = String::default();\n        add_mark(&mut s, 'a');\n        assert_eq!(s.len(), 1);\n"
        )?);
        Ok(())
    }

    #[test]
    fn reads_calls_and_statements_that_break_the_straight_line_are_refused() -> Result<(), String> {
        for body in [
            // A read of the contents is not this rule's to grade.
            format!("{CALL}        assert_eq!(b.len(), 1);\n        assert_eq!(b[0], \"HI\");\n"),
            format!("{CALL}        assert_eq!(b, vec![\"HI\".to_string()]);\n"),
            // An inner attribute may remove the body.
            format!("        #![cfg(any())]\n{CALL}        assert_eq!(b.len(), 1);\n"),
            // A final contents check written without `;` is the tail.
            format!("{CALL}        assert_eq!(b.len(), 1);\n        assert_eq!(b[0], \"HI\")\n"),
            // An attribute may remove the check.
            format!("{CALL}        #[cfg(any())]\n        assert_eq!(b.len(), 1);\n"),
            // A return or branch between the definition and the read.
            "        let mut b = Vec::new();\n        log_line(&mut b, \"hi\");\n        if b.capacity() > 99 {\n            return;\n        }\n        assert_eq!(b.len(), 1);\n".to_string(),
            "        let mut b = Vec::new();\n        log_line(&mut b, \"hi\");\n        let _ = 1;\n        assert_eq!(b.len(), 1);\n".to_string(),
            "        let mut b = Vec::new();\n        return;\n        log_line(&mut b, \"hi\");\n        assert_eq!(b.len(), 1);\n".to_string(),
            // A second call, an owner passed as a value, another argument
            // naming the vector.
            format!("{CALL}        log_line(&mut b, \"again\");\n        assert_eq!(b.len(), 2);\n"),
            "        let mut b = Vec::new();\n        let f = log_line;\n        f(&mut b, \"hi\");\n        assert_eq!(b.len(), 1);\n".to_string(),
            "        let mut b = Vec::new();\n        log_line(&mut b, &b.len().to_string());\n        assert_eq!(b.len(), 1);\n".to_string(),
            // The namesake in another module.
            "        let mut b = Vec::new();\n        other::log_line(&mut b, \"hi\");\n        assert_eq!(b.len(), 1);\n".to_string(),
        ] {
            // The namesake module only where the body names it: its mere
            // presence already refuses the bare call, which would make every
            // other case here pass for the wrong reason.
            let other = "\npub mod other {\n    pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n        let _ = (buf, msg);\n    }\n}\n";
            let extra = if body.contains("other::") { other } else { "" };
            if follows(OWNER, extra, TEST, &body)? {
                return Err(format!("must refuse:\n{body}"));
            }
        }
        Ok(())
    }

    #[test]
    fn gated_tests_shadowed_buffers_and_skippable_pushes_are_refused() -> Result<(), String> {
        let read = format!("{CALL}        assert_eq!(b.len(), 1);\n");
        for header in [
            "    #[test]\n    #[ignore]",
            "    #[test]\n    #[should_panic]",
            "    #[test]\n    #[cfg(unix)]",
            // An attribute macro may replace the body; an async test needs
            // another harness.
            "    #[test]\n    #[skip_me]",
            "    #[tokio::test]\n    async",
        ] {
            if follows(OWNER, "", header, &read)? {
                return Err(format!("must refuse the test header:\n{header}"));
            }
        }
        // A file-level type named like the std vector.
        if follows(OWNER, "\npub struct Vec<T>(T);\n", TEST, &read)? {
            return Err("must refuse a shadowed Vec".to_string());
        }
        for owner in [
            "pub fn log_line(buf: &mut Vec<String>, msg: &str, skip: bool) {\n    buf.push(if skip { return } else { msg.to_uppercase() });\n}\n",
            "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(format!(\"{msg}\"));\n}\n",
            "pub fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push({ msg.to_uppercase() });\n}\n",
            // Never polled by the test, so nothing is pushed.
            "pub async fn log_line(buf: &mut Vec<String>, msg: &str) {\n    buf.push(msg.to_uppercase());\n}\n",
        ] {
            if follows(owner, "", TEST, &read)? {
                return Err(format!("must refuse the owner:\n{owner}"));
            }
        }
        Ok(())
    }

    #[test]
    fn only_an_exact_nonzero_count_pins_the_push() -> Result<(), String> {
        for read in [
            "assert_eq!(b.len(), 1);",
            "assert_eq!(1, b.len());",
            "assert_eq!(b.len(), 12);",
        ] {
            if judge(OWNER, "", TEST, &format!("{CALL}        {read}\n"))? != (true, true) {
                return Err(format!("must be followed and pin the push: {read}"));
            }
        }
        for read in [
            "assert_ne!(b.len(), 2);",
            "assert!(!b.is_empty());",
            "assert_eq!(b.len(), 0);",
            "assert_eq!(b.len(), 1, \"one line\");",
            "assert!(b.len() < 3);",
        ] {
            if judge(OWNER, "", TEST, &format!("{CALL}        {read}\n"))? != (true, false) {
                return Err(format!("must be followed without pinning the push: {read}"));
            }
        }
        Ok(())
    }
}
