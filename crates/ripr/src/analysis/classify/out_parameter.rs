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
    let out_parameters = owner_out_parameters(owner)?;
    let impl_type = impl_self_type_name(&owner.id.0)?;
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
                (impl_type.as_str(), impl_trait.as_deref()),
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
fn owner_out_parameters(owner: &FunctionSummary) -> Option<Vec<(usize, String)>> {
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
            pattern.ref_token().is_none()
                && pattern.pat().is_none()
                && ty.mut_token().is_some()
                && pattern.name().is_some_and(|name| {
                    mentions_token(body.syntax(), name.text())
                        && !name_rebound_in(body.syntax(), name.text())
                        && name_written_in(body.syntax(), name.text())
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

/// Whether the owner body visibly writes through `name`: a mutating method
/// called on it, a `write!`/`writeln!` into it, or an assignment through a
/// dereference, field or index of it. Passing it onward or only reading it is
/// not a write (fail closed).
fn name_written_in(body: &SyntaxNode, name: &str) -> bool {
    const WRITERS: &[&str] = &[
        "push",
        "push_str",
        "push_back",
        "push_front",
        "insert",
        "extend",
        "extend_from_slice",
        "append",
        "write_str",
        "write_char",
        "write_fmt",
        "write_all",
        "write",
        "clear",
        "truncate",
        "remove",
        "pop",
        "resize",
        "set",
        "replace_range",
        "retain",
        "drain",
        "sort",
        "swap",
        "fill",
    ];
    let is_name = |text: String| strip_raw(&compact(&text)) == name;
    body.descendants().any(|node| {
        if let Some(call) = ast::MethodCallExpr::cast(node.clone()) {
            return call
                .name_ref()
                .is_some_and(|method| WRITERS.contains(&strip_raw(method.text())))
                && call
                    .receiver()
                    .is_some_and(|receiver| is_name(receiver.syntax().text().to_string()));
        }
        if let Some(binary) = ast::BinExpr::cast(node.clone()) {
            return matches!(binary.op_kind(), Some(ast::BinaryOp::Assignment { .. }))
                && binary.lhs().is_some_and(|lhs| {
                    let text = compact(&lhs.syntax().text().to_string());
                    let root = text
                        .trim_start_matches('*')
                        .split(['.', '['])
                        .next()
                        .unwrap_or_default();
                    (text.starts_with('*') || text.contains('.') || text.contains('['))
                        && strip_raw(root) == name
                });
        }
        if let Some(mac) = ast::MacroCall::cast(node) {
            let is_write = mac.path().is_some_and(|path| {
                matches!(path.syntax().text().to_string().trim(), "write" | "writeln")
            });
            return is_write
                && mac.token_tree().is_some_and(|tree| {
                    let text = compact(&tree.syntax().text().to_string());
                    let inner = text.trim_start_matches(['(', '[', '{']);
                    inner
                        .split(',')
                        .next()
                        .is_some_and(|first| strip_raw(first) == name)
                });
        }
        false
    })
}

fn local_written_by_owner_call(
    owner: &FunctionSummary,
    out_parameters: &[(usize, String)],
    (impl_type, impl_trait): (&str, Option<&str>),
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
    // An expected value equal to the local's initial state (or an empty
    // literal) cannot notice the method writing through it.
    let initial = ast::Stmt::cast(items[*definition_at].clone())
        .and_then(|statement| match statement {
            ast::Stmt::LetStmt(definition) => definition.initializer(),
            _ => None,
        })
        .map(|initializer| compact(&initializer.syntax().text().to_string()))?;
    if expected == initial
        || matches!(
            expected,
            "\"\"" | "String::new()" | "Vec::new()" | "vec![]" | "Default::default()"
        )
    {
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
