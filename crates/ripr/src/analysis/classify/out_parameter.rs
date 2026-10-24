//! Out-parameter observation (RIPR-SPEC-0351, #8999).
//!
//! `OffsetFormat { .. }.format(&mut s, off).unwrap(); assert_eq!(s, "+09:30");`
//! passes a local to the changed method as `&mut` and then asserts on that
//! local. The assertion never names the method, so lexical matching leaves
//! only the builder statement as the test's oracle. This module admits the
//! assertion for the call when every link is syntactically exact:
//!
//! - the owner is a method with a `&mut` parameter it actually uses;
//! - the asserted operand is a bare local, and that local is bound once by
//!   `let mut local = <call or record literal>;`, passed once as
//!   `&mut local` to a method call named like the owner, and mentioned by no
//!   other statement before the assertion;
//! - the definition, the call and the assertion are top-level statements of
//!   the test body, in that order;
//! - the call's receiver resolves to the owner's impl type by the same
//!   receiver rules as the relation layer (a type-qualified path, a struct
//!   literal, or a `let` typed by a literal or annotation).
//!
//! Anything else (a rebinding, a shadow, a loop, closure or macro mention,
//! an unresolved receiver, an alias, a second asserted operand that is also
//! a candidate) stays uncredited. The helper decides only that the
//! assertion reads the call's out-parameter; it grants no observation
//! confirmation, strength or reach.

use crate::analysis::classify::OwnerPinSyntax;
use crate::analysis::classify::reveal::assertion_comparison_operands;
use crate::analysis::classify::{
    impl_self_type_name, owner_dispatch_trait, resolves_method_in_prefix,
};
use crate::analysis::extract::mask_comments_and_strings;
use crate::analysis::rust_index::{FunctionSummary, OracleFact, RustIndex, TestSummary};
use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::AstToken;
use ra_ap_syntax::{
    AstNode, SyntaxKind, SyntaxNode,
    ast::{self, HasArgList, HasName},
};

/// The owner parameter name when `assertion` is a top-level `assert_eq!` on a
/// local that the test passed as `&mut` to a call of `owner` before it, under
/// the rules above.
pub(in crate::analysis) fn assertion_out_parameter(
    owner: &FunctionSummary,
    index: &RustIndex,
    test: &TestSummary,
    assertion: &OracleFact,
    syntax: &OwnerPinSyntax,
) -> Option<String> {
    // Cheap text gates first: this runs per (probe, test, assertion).
    if !assertion.text.trim_start().starts_with("assert_eq!")
        || !owner.body.contains("&mut")
        || !test.body.contains("&mut")
    {
        return None;
    }
    if !(syntax.admits(test, assertion, index)
        && syntax.targeted_macro_binding_is_unambiguous(test, assertion.line, "assert_eq", index))
    {
        return None;
    }
    let macro_names: &[String] = if owner.body.contains('!') {
        syntax.workspace_macro_names(index)
    } else {
        &[]
    };
    let out_parameters = owner_out_parameters(owner, macro_names)?;
    let impl_type = impl_self_type_name(&owner.id.0)?;
    if !owner_item_context_is_inert(owner, index, &impl_type) {
        return None;
    }
    // A user type, alias or glob import named like the std buffers could make
    // `push` a no-op, in the owner's file or the test's.
    if [&owner.file, &test.file].into_iter().any(|file| {
        index
            .files()
            .get(file)
            .is_none_or(|facts| file_shadows_buffer_types(&facts.source))
    }) {
        return None;
    }
    let impl_trait = owner_dispatch_trait(owner);
    let [left, right] = assertion_comparison_operands(&assertion.text)?;
    // Exactly one operand may be the candidate local that a matching call
    // wrote through; `assert_eq!(a, b)` with both written is ambiguous.
    let mut admitted = [left, right]
        .into_iter()
        .map(compact)
        .filter(|operand| is_plain_ident(operand))
        .filter_map(|operand| {
            let other = if compact(left) == operand {
                right
            } else {
                left
            };
            if mentions_word(&compact(other), &operand) {
                return None;
            }
            local_written_by_owner_call(
                owner,
                &out_parameters,
                (impl_type.as_str(), impl_trait.as_deref(), macro_names),
                test,
                assertion,
                &operand,
                &compact(other),
            )
        });
    let parameter = admitted.next()?;
    admitted.next().is_none().then_some(parameter)
}

/// Positions (call-argument indexes, receiver excluded) of the owner's
/// `&mut` parameters that its body uses, or `None` when the owner is not a
/// plain method (no receiver, async, unparseable).
fn owner_out_parameters(
    owner: &FunctionSummary,
    macro_names: &[String],
) -> Option<Vec<(usize, String)>> {
    if !owner.item.has_self_param {
        return None;
    }
    let tree = parse_clean_source_file(&owner.body)?.tree();
    let function = tree.syntax().descendants().find_map(ast::Fn::cast)?;
    if function.async_token().is_some() {
        return None;
    }
    let body = function.body()?;
    let positions: Vec<(usize, String)> = function
        .param_list()?
        .params()
        .enumerate()
        .filter(|(_, parameter)| {
            let Some(ast::Pat::IdentPat(pattern)) = parameter.pat() else {
                return false;
            };
            let Some(ast::Type::RefType(ty)) = parameter.ty() else {
                return false;
            };
            // A generic or trait-typed `T` could be anything with a no-op
            // `push`: only the std growable buffers are accepted.
            let concrete = match ty.ty() {
                Some(ast::Type::PathType(path)) => path
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|name| {
                        matches!(strip_raw(name.text()), "String" | "Vec" | "VecDeque")
                    }),
                _ => false,
            };
            if !concrete {
                return false;
            }
            pattern.ref_token().is_none()
                && pattern.pat().is_none()
                && ty.mut_token().is_some()
                && pattern.name().is_some_and(|name| {
                    mentions_token(body.syntax(), name.text())
                        && !name_rebound_in(body.syntax(), name.text())
                        && owner_allows_out_parameter(&body, name.text(), macro_names)
                })
        })
        .filter_map(|(at, parameter)| match parameter.pat() {
            Some(ast::Pat::IdentPat(pattern)) => {
                Some((at, strip_raw(pattern.name()?.text()).to_string()))
            }
            _ => None,
        })
        .collect();
    (!positions.is_empty()).then_some(positions)
}

/// Whether the owner body can rebind or reassign `name` (a `let`, any
/// pattern binding including closure and match-arm parameters, a plain
/// assignment, or a binding keyword before the name inside a macro), so a
/// later use may no longer be the `&mut` parameter. Fail closed.
fn name_rebound_in(body: &SyntaxNode, name: &str) -> bool {
    let bound = body.descendants().any(|node| {
        ast::IdentPat::cast(node.clone())
            .and_then(|pattern| pattern.name())
            .is_some_and(|bound| strip_raw(bound.text()) == name)
            || ast::BinExpr::cast(node).is_some_and(|binary| {
                matches!(binary.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                    && binary.lhs().is_some_and(|lhs| {
                        strip_raw(&compact(&lhs.syntax().text().to_string())) == name
                    })
            })
    });
    bound || {
        let masked = mask_comments_and_strings(&body.text().to_string());
        let words: Vec<&str> = masked
            .split(|ch: char| !(ch == '_' || ch.is_ascii_alphanumeric()))
            .filter(|word| !word.is_empty())
            .collect();
        words
            .windows(2)
            .any(|pair| pair[1] == name && matches!(pair[0], "let" | "mut" | "ref"))
    }
}

/// Whether the changed expression itself writes through `parameter` (the
/// same rule as the owner-side check), so a mention of the name in an
/// unrelated expression is not confirmation.
pub(in crate::analysis) fn expression_writes_parameter(expression: &str, parameter: &str) -> bool {
    let source = format!("fn __probe() {{ {expression}; }}");
    let Some(parse) = parse_clean_source_file(&source) else {
        return false;
    };
    let tree = parse.tree();
    let Some(body) = tree
        .syntax()
        .descendants()
        .find_map(ast::Fn::cast)
        .and_then(|function| function.body())
    else {
        return false;
    };
    let Some(statements) = top_level_statements(&body) else {
        return false;
    };
    let [statement] = statements.as_slice() else {
        return false;
    };
    // The changed write must execute unconditionally: no branch, loop,
    // closure or early exit anywhere in it, macro arguments included.
    let conditional = statement
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .any(|token| {
            matches!(
                token.text(),
                "if" | "match" | "for" | "while" | "loop" | "return" | "break" | "|" | "||" | "?"
            )
        });
    !conditional
        && classify_statement(statement, parameter, false)
            .is_some_and(|(kind, _)| kind == StatementKind::Write)
}

/// Whether `expected` is clearly a non-empty value (a non-empty string or
/// byte-string literal, a non-zero integer literal, `lit.to_string()` and
/// kin, `String::from(lit)`, or a non-empty `vec![..]`). Anything else,
/// including every empty spelling and every local, is refused: an expected
/// value that may equal the local's initial state cannot notice a write.
fn is_clearly_nonempty(expected: &str) -> bool {
    let source = format!("fn __expected() {{ let _ = {expected}; }}");
    let Some(parse) = parse_clean_source_file(&source) else {
        return false;
    };
    let tree = parse.tree();
    let Some(initializer) = tree
        .syntax()
        .descendants()
        .find_map(ast::LetStmt::cast)
        .and_then(|statement| statement.initializer())
    else {
        return false;
    };
    nonempty_expr(&initializer)
}

/// The local starts as a recognized empty collection, so a clearly non-empty
/// expected value cannot equal its initial state.
fn is_empty_constructor(initial: &str) -> bool {
    matches!(
        initial,
        "String::new()"
            | "String::default()"
            | "Vec::new()"
            | "Vec::default()"
            | "VecDeque::new()"
            | "VecDeque::default()"
    )
}

fn nonempty_expr(expression: &ast::Expr) -> bool {
    match expression {
        ast::Expr::Literal(literal) => match literal.kind() {
            ast::LiteralKind::String(text) => text.value().is_ok_and(|value| !value.is_empty()),
            ast::LiteralKind::ByteString(text) => text.value().is_ok_and(|value| !value.is_empty()),
            ast::LiteralKind::IntNumber(number) => number.value().is_ok_and(|value| value != 0),
            _ => false,
        },
        ast::Expr::MethodCallExpr(call) => {
            call.arg_list()
                .is_some_and(|args| args.args().next().is_none())
                && call.name_ref().is_some_and(|name| {
                    matches!(
                        strip_raw(name.text()),
                        "to_string" | "to_owned" | "into" | "to_vec"
                    )
                })
                && call
                    .receiver()
                    .is_some_and(|receiver| nonempty_expr(&receiver))
        }
        ast::Expr::CallExpr(call) => {
            let callee = call
                .expr()
                .map(|callee| compact(&callee.syntax().text().to_string()));
            let arguments: Vec<_> = call
                .arg_list()
                .map(|args| args.args().collect())
                .unwrap_or_default();
            callee.as_deref() == Some("String::from")
                && matches!(arguments.as_slice(), [only] if nonempty_expr(only))
        }
        ast::Expr::MacroExpr(mac) => mac.macro_call().is_some_and(|mac| {
            mac.path()
                .is_some_and(|path| compact(&path.syntax().text().to_string()) == "vec")
                && mac.token_tree().is_some_and(|tree| {
                    let text = compact(&tree.syntax().text().to_string());
                    let inner = text
                        .strip_prefix(['[', '(', '{'])
                        .and_then(|rest| rest.strip_suffix([']', ')', '}']))
                        .unwrap_or_default();
                    !inner.is_empty() && !inner.contains(';')
                })
        }),
        _ => false,
    }
}

/// Whether a `format!`-style template certainly produces text: it holds a
/// non-placeholder character, or a placeholder with an explicit width.
fn template_is_nonempty(template: &str) -> bool {
    let chars: Vec<char> = template.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        match chars[at] {
            '{' if chars.get(at + 1) == Some(&'{') => return true,
            '}' if chars.get(at + 1) == Some(&'}') => return true,
            '{' => {
                let mut end = at + 1;
                while end < chars.len() && chars[end] != '}' {
                    end += 1;
                }
                let inner: String = chars[at + 1..end.min(chars.len())].iter().collect();
                if let Some((_, spec)) = inner.split_once(':') {
                    let mut spec: Vec<char> = spec.chars().collect();
                    if spec.len() >= 2 && matches!(spec[1], '<' | '^' | '>') {
                        spec.drain(..2);
                    } else if spec.first().is_some_and(|ch| matches!(ch, '<' | '^' | '>')) {
                        spec.remove(0);
                    }
                    while spec.first().is_some_and(|ch| matches!(ch, '+' | '-' | '#')) {
                        spec.remove(0);
                    }
                    if spec.first() == Some(&'0') {
                        spec.remove(0);
                    }
                    if spec.first().is_some_and(|ch| ('1'..='9').contains(ch)) {
                        return true;
                    }
                }
                at = end + 1;
            }
            _ => return true,
        }
    }
    false
}

/// `lit`, `&lit` or `&format!(template, ..)` that certainly yields text, or
/// a plain identifier (data-dependent: a named limit).
fn nonempty_text_argument(argument: &ast::Expr) -> bool {
    let mut expression = argument.clone();
    while let ast::Expr::RefExpr(reference) = &expression {
        let Some(inner) = reference.expr() else {
            return false;
        };
        expression = inner;
    }
    match &expression {
        ast::Expr::Literal(literal) => match literal.kind() {
            ast::LiteralKind::String(text) => text.value().is_ok_and(|value| !value.is_empty()),
            ast::LiteralKind::ByteString(text) => text.value().is_ok_and(|value| !value.is_empty()),
            _ => false,
        },
        ast::Expr::PathExpr(path) => is_plain_ident(&compact(&path.syntax().text().to_string())),
        ast::Expr::MacroExpr(mac) => mac.macro_call().is_some_and(|mac| {
            mac.path()
                .is_some_and(|path| compact(&path.syntax().text().to_string()) == "format")
                && mac.token_tree().is_some_and(|tree| {
                    tree.syntax()
                        .children_with_tokens()
                        .filter_map(|element| element.into_token())
                        .find(|token| token.kind() == SyntaxKind::STRING)
                        .and_then(ast::String::cast)
                        .and_then(|text| {
                            text.value().ok().map(|value| template_is_nonempty(&value))
                        })
                        .unwrap_or(false)
                })
        }),
        _ => false,
    }
}

fn nonempty_collection_argument(argument: &ast::Expr) -> bool {
    let mut expression = argument.clone();
    while let ast::Expr::RefExpr(reference) = &expression {
        let Some(inner) = reference.expr() else {
            return false;
        };
        expression = inner;
    }
    match &expression {
        ast::Expr::ArrayExpr(array) => {
            array.semicolon_token().is_none() && array.exprs().next().is_some()
        }
        ast::Expr::MethodCallExpr(call) => {
            call.arg_list()
                .is_some_and(|args| args.args().next().is_none())
                && call
                    .name_ref()
                    .is_some_and(|name| matches!(strip_raw(name.text()), "chars" | "bytes"))
                && call.receiver().is_some_and(|receiver| {
                    matches!(&receiver, ast::Expr::Literal(_)) && nonempty_text_argument(&receiver)
                })
        }
        _ => false,
    }
}

/// Positive allowlist: a method call writes only when its arguments certainly
/// add data. Anything else (including identifier-valued collections) is not
/// credited.
fn call_certainly_writes(method: &str, call: &ast::MethodCallExpr) -> bool {
    let Some(list) = call.arg_list() else {
        return false;
    };
    let arguments: Vec<ast::Expr> = list.args().collect();
    match (method, arguments.as_slice()) {
        ("push" | "push_back" | "push_front" | "write_char", [_]) => true,
        ("push_str" | "write_str", [text]) => nonempty_text_argument(text),
        ("insert", [_, _]) => true,
        ("insert_str", [_, text]) => nonempty_text_argument(text),
        ("write_all", [bytes]) => {
            nonempty_text_argument(bytes) || nonempty_collection_argument(bytes)
        }
        ("extend" | "extend_from_slice", [items]) => nonempty_collection_argument(items),
        _ => false,
    }
}

const ADDITIVE_WRITERS: &[&str] = &[
    "push",
    "push_back",
    "push_front",
    "write_char",
    "push_str",
    "write_str",
    "insert",
    "insert_str",
    "write_all",
    "extend",
    "extend_from_slice",
];

const READ_ONLY_METHODS: &[&str] = &[
    "len",
    "is_empty",
    "capacity",
    "as_str",
    "as_bytes",
    "chars",
    "bytes",
    "iter",
    "contains",
    "starts_with",
    "ends_with",
    "get",
    "first",
    "last",
    "to_string",
    "to_owned",
    "clone",
];

#[derive(Clone, Copy, PartialEq)]
enum StatementKind {
    Write,
    Clear,
}

/// A top-level statement that is exactly an additive write (or a `clear()`)
/// on the bare identifier `name`, plus the text range of that identifier.
fn classify_statement(
    node: &SyntaxNode,
    name: &str,
    write_macro_shadowed: bool,
) -> Option<(StatementKind, ra_ap_syntax::TextRange)> {
    let mut expression = match ast::ExprStmt::cast(node.clone()) {
        Some(statement) => statement.expr()?,
        None => ast::Expr::cast(node.clone())?,
    };
    loop {
        match expression {
            ast::Expr::TryExpr(inner) => expression = inner.expr()?,
            ast::Expr::MethodCallExpr(call) => {
                let method = call.name_ref()?;
                let method = strip_raw(method.text());
                if let Some(receiver) = bare_name_receiver(&call, name) {
                    if ADDITIVE_WRITERS.contains(&method) && call_certainly_writes(method, &call) {
                        return Some((StatementKind::Write, receiver));
                    }
                    if method == "clear" && call.arg_list()?.args().next().is_none() {
                        return Some((StatementKind::Clear, receiver));
                    }
                    return None;
                }
                if !matches!(method, "unwrap" | "expect") {
                    return None;
                }
                expression = call.receiver()?;
            }
            ast::Expr::MacroExpr(mac) => {
                if write_macro_shadowed {
                    return None;
                }
                let mac = mac.macro_call()?;
                let path = compact(&mac.path()?.syntax().text().to_string());
                if !matches!(
                    path.as_str(),
                    "write"
                        | "writeln"
                        | "std::write"
                        | "std::writeln"
                        | "core::write"
                        | "core::writeln"
                ) {
                    return None;
                }
                let tree = mac.token_tree()?;
                let tokens: Vec<_> = tree
                    .syntax()
                    .children_with_tokens()
                    .filter_map(|element| element.into_token())
                    .filter(|token| !token.kind().is_trivia())
                    .collect();
                let first = tokens
                    .iter()
                    .find(|token| token.kind() == SyntaxKind::IDENT)?;
                if strip_raw(first.text()) != name {
                    return None;
                }
                // `writeln!` always adds a newline; `write!` needs a template
                // that certainly yields text.
                let writes = path.ends_with("writeln")
                    || tokens
                        .iter()
                        .find(|token| token.kind() == SyntaxKind::STRING)
                        .and_then(|token| ast::String::cast(token.clone()))
                        .and_then(|text| {
                            text.value().ok().map(|value| template_is_nonempty(&value))
                        })
                        .unwrap_or(false);
                return writes.then_some((StatementKind::Write, first.text_range()));
            }
            _ => return None,
        }
    }
}

fn bare_name_receiver(call: &ast::MethodCallExpr, name: &str) -> Option<ra_ap_syntax::TextRange> {
    let ast::Expr::PathExpr(path) = call.receiver()? else {
        return None;
    };
    (compact(&path.syntax().text().to_string()) == name).then(|| path.syntax().text_range())
}

fn top_level_statements(body: &ast::BlockExpr) -> Option<Vec<SyntaxNode>> {
    let list = body.stmt_list()?;
    Some(
        list.statements()
            .map(|statement| statement.syntax().clone())
            .chain(list.tail_expr().map(|tail| tail.syntax().clone()))
            .collect(),
    )
}

/// Fail-closed allowlist for an owner `&mut` parameter `name`: every use of
/// the bare identifier is either a top-level additive write statement (or a
/// `clear()` before the first write), a std `write!`/`writeln!` into it, or a
/// read-only method call; any other use (passing it onward, reborrowing,
/// aliasing, deref or parenthesized receivers, closure capture, unknown
/// methods, other macros) refuses. The writes are straight-line top-level
/// statements with no `return`, `break`, `continue` or `?` before the last
/// one, so the credited write executes for any input.
fn owner_allows_out_parameter(body: &ast::BlockExpr, name: &str, macro_names: &[String]) -> bool {
    // Any attribute (`#[cfg(..)]` and kin) can remove a statement.
    if body
        .syntax()
        .descendants()
        .any(|node| node.kind() == SyntaxKind::ATTR)
    {
        return false;
    }
    let shadowed = macro_names
        .iter()
        .any(|macro_name| matches!(macro_name.as_str(), "write" | "writeln"));
    let Some(statements) = top_level_statements(body) else {
        return false;
    };
    let kinds: Vec<Option<(StatementKind, ra_ap_syntax::TextRange)>> = statements
        .iter()
        .map(|node| classify_statement(node, name, shadowed))
        .collect();
    let Some(first_write) = kinds
        .iter()
        .position(|kind| matches!(kind, Some((StatementKind::Write, _))))
    else {
        return false;
    };
    let last_write = kinds
        .iter()
        .rposition(|kind| matches!(kind, Some((StatementKind::Write, _))))
        .unwrap_or(first_write);
    if kinds
        .iter()
        .enumerate()
        .any(|(at, kind)| matches!(kind, Some((StatementKind::Clear, _))) && at > first_write)
    {
        return false;
    }
    for (at, node) in statements.iter().enumerate() {
        let accounted = kinds[at].map(|(_, range)| range);
        for token in node
            .descendants_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::IDENT && strip_raw(token.text()) == name)
        {
            if accounted == Some(token.text_range()) || is_read_only_use(&token) {
                continue;
            }
            return false;
        }
        if at <= last_write {
            if !straight_line(node, macro_names) {
                return false;
            }
            let diverges = node.descendants().any(|inner| {
                matches!(
                    inner.kind(),
                    SyntaxKind::RETURN_EXPR | SyntaxKind::BREAK_EXPR | SyntaxKind::CONTINUE_EXPR
                ) || (at < last_write && inner.kind() == SyntaxKind::TRY_EXPR)
            });
            if diverges {
                return false;
            }
        }
    }
    true
}

fn is_read_only_use(token: &ra_ap_syntax::SyntaxToken) -> bool {
    let Some(path_expr) = token.parent_ancestors().find_map(ast::PathExpr::cast) else {
        return false;
    };
    let Some(call) = path_expr
        .syntax()
        .parent()
        .and_then(ast::MethodCallExpr::cast)
    else {
        return false;
    };
    call.receiver()
        .is_some_and(|receiver| receiver.syntax() == path_expr.syntax())
        && call
            .name_ref()
            .is_some_and(|method| READ_ONLY_METHODS.contains(&strip_raw(method.text())))
}

/// Names of the workspace's `macro_rules!` macros (any file).
pub(in crate::analysis) fn workspace_macro_names(index: &RustIndex) -> Vec<String> {
    let mut names = Vec::new();
    for (_, facts) in index.files().iter() {
        if !facts.source.contains("macro_rules") {
            continue;
        }
        let text = mask_comments_and_strings(&facts.source);
        let mut rest = text.as_str();
        while let Some(at) = rest.find("macro_rules") {
            rest = &rest[at + "macro_rules".len()..];
            let after = rest.trim_start();
            let Some(after) = after.strip_prefix('!') else {
                continue;
            };
            let name: String = after
                .trim_start()
                .chars()
                .take_while(|ch| *ch == '_' || ch.is_ascii_alphanumeric())
                .collect();
            if !name.is_empty() {
                names.push(name);
            }
        }
    }
    names
}

const PURE_MACROS: &[&str] = &[
    "format",
    "vec",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "matches",
    "stringify",
    "concat",
    "write",
    "writeln",
];

/// A statement that cannot attach a condition or an early exit to the writes
/// after it: no attribute, no macro outside a small pure std list (and none
/// the workspace redefines), no `exit`/`abort`/`panic`-family call. Macro
/// calls inside other macros' arguments are read from the tokens.
fn straight_line(node: &SyntaxNode, macro_names: &[String]) -> bool {
    if node
        .descendants()
        .any(|inner| inner.kind() == SyntaxKind::ATTR)
    {
        return false;
    }
    let tokens: Vec<_> = node
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    // A trailing `?` ends the statement; any other `?` can leave the owner.
    let mut end = tokens.len();
    while end > 0 && tokens[end - 1].kind() == SyntaxKind::SEMICOLON {
        end -= 1;
    }
    for (at, token) in tokens.iter().enumerate() {
        let text = token.text();
        if matches!(
            text,
            "return"
                | "break"
                | "continue"
                | "loop"
                | "while"
                | "for"
                | "if"
                | "match"
                | "else"
                | "async"
                | "await"
                | "|"
                | "||"
        ) || (text == "?" && at + 1 != end)
        {
            return false;
        }
        if token.kind() == SyntaxKind::IDENT
            && matches!(
                strip_raw(text),
                "exit" | "abort" | "panic_any" | "resume_unwind"
            )
        {
            return false;
        }
        let previous = at.checked_sub(1).and_then(|before| tokens.get(before));
        if token.kind() == SyntaxKind::BANG {
            let Some(previous) = previous else {
                return false;
            };
            let leaf = strip_raw(previous.text());
            if previous.kind() != SyntaxKind::IDENT
                || !PURE_MACROS.contains(&leaf)
                || macro_names.iter().any(|name| name == leaf)
            {
                return false;
            }
        }
        // A call to a bare identifier (possibly an alias of `exit`, or a
        // workspace function that diverges) is not followed: refuse it. Method
        // and path calls, `Some`/`Ok`/`Err` and macros are not bare calls.
        if token.kind() == SyntaxKind::IDENT
            && tokens
                .get(at + 1)
                .is_some_and(|next| next.kind() == SyntaxKind::L_PAREN)
            && !matches!(text, "Some" | "Ok" | "Err")
            && !previous
                .is_some_and(|before| matches!(before.kind(), SyntaxKind::DOT | SyntaxKind::COLON2))
        {
            return false;
        }
    }
    true
}

/// Attributes that cannot remove or redirect an item.
fn attribute_is_inert(attribute: &ast::Attr) -> bool {
    attribute.meta().is_some_and(|meta| {
        meta.path().is_some_and(|path| {
            matches!(
                compact(&path.syntax().text().to_string()).as_str(),
                "doc" | "inline" | "must_use" | "allow" | "track_caller" | "cold" | "deprecated"
            )
        })
    })
}

/// The owner fn, its impl and every enclosing module carry only inert
/// attributes, and no second method of the same name exists in an impl of the
/// same type in the file (a `cfg`-gated twin could be the active one).
fn owner_item_context_is_inert(
    owner: &FunctionSummary,
    index: &RustIndex,
    impl_type: &str,
) -> bool {
    use ra_ap_syntax::ast::HasAttrs;
    let Some(facts) = index.files().get(&owner.file) else {
        return false;
    };
    let Some(parse) = parse_clean_source_file(&facts.source) else {
        return false;
    };
    let tree = parse.tree();
    let line_of = |offset: usize| {
        1 + facts.source[..offset.min(facts.source.len())]
            .matches('\n')
            .count()
    };
    let functions: Vec<ast::Fn> = tree
        .syntax()
        .descendants()
        .filter_map(ast::Fn::cast)
        .filter(|function| {
            function
                .name()
                .is_some_and(|name| strip_raw(name.text()) == owner.name)
        })
        .collect();
    let Some(function) = functions
        .iter()
        .filter(|function| {
            let range = function.syntax().text_range();
            line_of(usize::from(range.start())) <= owner.start_line
                && owner.start_line <= line_of(usize::from(range.end()))
        })
        .min_by_key(|function| function.syntax().text_range().len())
    else {
        return false;
    };
    let mut twins = 0;
    for impl_item in tree.syntax().descendants().filter_map(ast::Impl::cast) {
        let self_ty = impl_item
            .self_ty()
            .map(|ty| ty.syntax().text().to_string())
            .unwrap_or_default();
        if word_occurrences(&self_ty, impl_type) == 0 {
            continue;
        }
        twins += impl_item
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast)
            .filter(|candidate| {
                candidate
                    .name()
                    .is_some_and(|name| strip_raw(name.text()) == owner.name)
            })
            .count();
    }
    if twins != 1 {
        return false;
    }
    let inert_all = |attrs: ast::AstChildren<ast::Attr>| {
        attrs.into_iter().all(|attr| attribute_is_inert(&attr))
    };
    if !inert_all(function.attrs()) {
        return false;
    }
    function.syntax().ancestors().skip(1).all(|ancestor| {
        if let Some(item) = ast::Impl::cast(ancestor.clone()) {
            return inert_all(item.attrs());
        }
        if let Some(item) = ast::Module::cast(ancestor.clone()) {
            return inert_all(item.attrs());
        }
        if let Some(item) = ast::Trait::cast(ancestor) {
            return inert_all(item.attrs());
        }
        true
    })
}

/// A `let` of a plain value or an assertion macro; nothing else may sit
/// between the test's start and its assertion.
fn simple_test_statement(node: &SyntaxNode, macro_names: &[String]) -> bool {
    if !straight_line(node, macro_names) {
        return false;
    }
    match ast::Stmt::cast(node.clone()) {
        Some(ast::Stmt::LetStmt(definition)) => {
            definition.let_else().is_none()
                && definition
                    .initializer()
                    .is_some_and(|initializer| simple_value(&initializer))
        }
        Some(ast::Stmt::ExprStmt(statement)) => {
            matches!(statement.expr(), Some(ast::Expr::MacroExpr(_)))
        }
        _ => false,
    }
}

fn simple_value(expression: &ast::Expr) -> bool {
    match expression {
        ast::Expr::Literal(_) | ast::Expr::PathExpr(_) => true,
        ast::Expr::RefExpr(reference) => reference.expr().is_some_and(|inner| simple_value(&inner)),
        ast::Expr::MacroExpr(_) => true,
        ast::Expr::RecordExpr(record) => record.record_expr_field_list().is_some_and(|list| {
            list.fields()
                .all(|field| field.expr().is_none_or(|value| simple_value(&value)))
                && list.spread().is_none()
        }),
        ast::Expr::CallExpr(call) => {
            let callee = call
                .expr()
                .map(|callee| compact(&callee.syntax().text().to_string()))
                .unwrap_or_default();
            matches!(
                callee.as_str(),
                "String::new"
                    | "String::from"
                    | "Vec::new"
                    | "VecDeque::new"
                    | "Some"
                    | "Ok"
                    | "Err"
            ) && call
                .arg_list()
                .is_some_and(|args| args.args().all(|argument| simple_value(&argument)))
        }
        _ => false,
    }
}

/// Whether `source` defines, imports or aliases a type named `String`, `Vec`
/// or `VecDeque`, or glob-imports from anywhere but `super`/`self`/`std`.
fn file_shadows_buffer_types(source: &str) -> bool {
    const NAMES: &[&str] = &["String", "Vec", "VecDeque"];
    if !NAMES.iter().any(|name| source.contains(name)) && !source.contains('*') {
        return false;
    }
    let Some(parse) = parse_clean_source_file(source) else {
        return true;
    };
    let tree = parse.tree();
    for node in tree.syntax().descendants() {
        if let Some(item) = ast::Use::cast(node.clone()) {
            let text = compact(&item.syntax().text().to_string());
            let std_rooted = text.starts_with("usestd::")
                || text.starts_with("usecore::")
                || text.starts_with("usealloc::");
            let names_buffer = NAMES.iter().any(|name| word_occurrences(&text, name) > 0);
            let glob = text.contains('*');
            let local_glob = matches!(text.as_str(), "usesuper::*;" | "useself::*;");
            if (!std_rooted && names_buffer) || (glob && !std_rooted && !local_glob) {
                return true;
            }
        }
    }
    let tokens: Vec<_> = tree
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens.windows(2).any(|pair| {
        pair[1].kind() == SyntaxKind::IDENT
            && NAMES.contains(&strip_raw(pair[1].text()))
            && matches!(
                pair[0].text(),
                "struct"
                    | "enum"
                    | "type"
                    | "trait"
                    | "union"
                    | "mod"
                    | "fn"
                    | "const"
                    | "static"
                    | "as"
            )
    })
}

fn local_written_by_owner_call(
    owner: &FunctionSummary,
    out_parameters: &[(usize, String)],
    (impl_type, impl_trait, macro_names): (&str, Option<&str>, &[String]),
    test: &TestSummary,
    assertion: &OracleFact,
    local: &str,
    expected: &str,
) -> Option<String> {
    let parse = parse_clean_source_file(&test.body)?;
    let tree = parse.tree();
    let function = tree.syntax().descendants().find_map(ast::Fn::cast)?;
    if function.async_token().is_some() {
        return None;
    }
    let list = function.body().and_then(|body| body.stmt_list())?;
    let items: Vec<SyntaxNode> = list
        .statements()
        .map(|statement| statement.syntax().clone())
        .chain(list.tail_expr().map(|tail| tail.syntax().clone()))
        .collect();
    let line_of = |node: &SyntaxNode| {
        let at = usize::from(node.text_range().start());
        test.start_line + test.body[..at].matches('\n').count()
    };
    let wanted = compact(&assertion.text);
    let wanted = wanted.trim_end_matches(';');
    let assertions: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            line_of(node) == assertion.line
                && compact(&node.text().to_string()).trim_end_matches(';') == wanted
        })
        .map(|(at, _)| at)
        .collect();
    let [assertion_at] = assertions.as_slice() else {
        return None;
    };
    let mentions: Vec<usize> = (0..*assertion_at)
        .filter(|at| mentions_token(&items[*at], local))
        .collect();
    let [definition_at, call_at] = mentions.as_slice() else {
        return None;
    };
    if !is_owned_local_definition(&items[*definition_at], local) {
        return None;
    }
    // Every other statement before the assertion must be a plain setup
    // statement: a helper that exits or panics there could stop the test
    // before the assertion runs.
    if items
        .iter()
        .enumerate()
        .take(*assertion_at)
        .any(|(at, node)| {
            at != *definition_at && at != *call_at && !simple_test_statement(node, macro_names)
        })
    {
        return None;
    }
    // An expected value equal to the local's initial state (or an empty
    // literal) cannot notice the method writing through it.
    let initial = ast::Stmt::cast(items[*definition_at].clone())
        .and_then(|statement| match statement {
            ast::Stmt::LetStmt(definition) => definition.initializer(),
            _ => None,
        })
        .map(|initializer| compact(&initializer.syntax().text().to_string()))?;
    if !is_empty_constructor(&initial) || expected == initial || !is_clearly_nonempty(expected) {
        return None;
    }
    let call = &items[*call_at];
    if call.kind() != SyntaxKind::EXPR_STMT
        || call
            .descendants()
            .any(|node| node.kind() == SyntaxKind::ATTR)
    {
        return None;
    }
    let call_expr = ast::ExprStmt::cast(call.clone()).and_then(|statement| statement.expr())?;
    let method_call = owner_method_call(call_expr, &owner.name)?;
    let parameter = passes_local_as_out_parameter(&method_call, call, out_parameters, local)?;
    // The receiver must resolve to the owner's impl type, read from the
    // masked source up to this call so an earlier call of the same method
    // name on another receiver cannot supply the resolution.
    let masked = mask_comments_and_strings(&test.body);
    let start = usize::from(call.text_range().start());
    let end = usize::from(call.text_range().end());
    let (Some(before), Some(statement)) = (masked.get(..start), masked.get(start..end)) else {
        return None;
    };
    if word_occurrences(statement, &owner.name) != 1 {
        return None;
    }
    let prefix = format!("{}{}", blank_word(before, &owner.name), &masked[start..end]);
    resolves_method_in_prefix(&prefix, &owner.name, impl_type, impl_trait).then_some(parameter)
}

/// `let mut local = Type::ctor(..)` or `let mut local = Type { .. }`, with
/// no borrow in the initializer and no `let ... else`.
fn is_owned_local_definition(statement: &SyntaxNode, local: &str) -> bool {
    let Some(ast::Stmt::LetStmt(definition)) = ast::Stmt::cast(statement.clone()) else {
        return false;
    };
    let Some(ast::Pat::IdentPat(pattern)) = definition.pat() else {
        return false;
    };
    let Some(name) = pattern.name() else {
        return false;
    };
    if strip_raw(name.text()) != local
        || pattern.mut_token().is_none()
        || pattern.ref_token().is_some()
        || pattern.pat().is_some()
        || definition.let_else().is_some()
    {
        return false;
    }
    let Some(initializer) = definition.initializer() else {
        return false;
    };
    !initializer
        .syntax()
        .descendants()
        .any(|node| ast::RefExpr::can_cast(node.kind()))
        && matches!(
            initializer,
            ast::Expr::CallExpr(_) | ast::Expr::RecordExpr(_)
        )
}

/// The method call named `method` at the end of a `?` / `.unwrap()` /
/// `.expect(..)` chain; any other wrapper (a `match`, a closure, another
/// method) refuses.
fn owner_method_call(mut expression: ast::Expr, method: &str) -> Option<ast::MethodCallExpr> {
    loop {
        match expression {
            ast::Expr::TryExpr(inner) => expression = inner.expr()?,
            ast::Expr::MethodCallExpr(call) => {
                let name = call.name_ref()?;
                if strip_raw(name.text()) == method {
                    return Some(call);
                }
                if !matches!(name.text(), "unwrap" | "expect") {
                    return None;
                }
                expression = call.receiver()?;
            }
            _ => return None,
        }
    }
}

/// The call passes `&mut local` (exactly) at an out-parameter position, and
/// `local` appears nowhere else in the statement.
fn passes_local_as_out_parameter(
    call: &ast::MethodCallExpr,
    statement: &SyntaxNode,
    out_parameters: &[(usize, String)],
    local: &str,
) -> Option<String> {
    let arguments: Vec<_> = call.arg_list()?.args().collect();
    let exact: Vec<usize> = arguments
        .iter()
        .enumerate()
        .filter(|(_, argument)| {
            let ast::Expr::RefExpr(reference) = argument else {
                return false;
            };
            reference.mut_token().is_some()
                && reference.raw_token().is_none()
                && reference.expr().is_some_and(|inner| {
                    strip_raw(&compact(&inner.syntax().text().to_string())) == local
                })
        })
        .map(|(at, _)| at)
        .collect();
    let [position] = exact.as_slice() else {
        return None;
    };
    // The whole statement (receiver chain included) names the local once.
    if token_count(statement, local) != 1 {
        return None;
    }
    out_parameters
        .iter()
        .find(|(at, _)| at == position)
        .map(|(_, name)| name.clone())
}

fn mentions_token(node: &SyntaxNode, name: &str) -> bool {
    token_count(node, name) > 0
}

fn token_count(node: &SyntaxNode, name: &str) -> usize {
    node.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() == SyntaxKind::IDENT && strip_raw(token.text()) == name)
        .count()
}

fn strip_raw(text: &str) -> &str {
    text.strip_prefix("r#").unwrap_or(text)
}

fn compact(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

fn is_plain_ident(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && text
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

fn is_word_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric()
}

fn word_positions<'a>(text: &'a str, word: &'a str) -> impl Iterator<Item = usize> + 'a {
    let bytes = text.as_bytes();
    text.match_indices(word)
        .map(|(at, _)| at)
        .filter(move |at| {
            let after = at + word.len();
            (*at == 0 || !is_word_byte(bytes[at - 1]))
                && (after >= bytes.len() || !is_word_byte(bytes[after]))
        })
}

fn word_occurrences(text: &str, word: &str) -> usize {
    word_positions(text, word).count()
}

fn mentions_word(text: &str, word: &str) -> bool {
    word_occurrences(text, word) > 0
}

/// `text` with every whole-word occurrence of `word` replaced by spaces, so
/// byte offsets stay unchanged.
fn blank_word(text: &str, word: &str) -> String {
    let mut blanked = text.to_string();
    let positions: Vec<usize> = word_positions(text, word).collect();
    for at in positions {
        blanked.replace_range(at..at + word.len(), &" ".repeat(word.len()));
    }
    blanked
}
