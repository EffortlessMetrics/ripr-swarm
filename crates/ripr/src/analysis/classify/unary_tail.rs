//! #14815: a unary `-` or `!` deleted from, or added to, a returned name.
//!
//! cargo-mutants' UnaryOperator genre turns `-a` into `a` and `!a` into `a`.
//! When the owner's whole body is that one-line tail, the change reaches the
//! owner's return, but the tail text (`a`) carries no cue the flow reader
//! takes as a return and recurs throughout the owner (`fn f(a: i32)`), so the
//! sink stayed unknown.
//!
//! The decision here does not trust the probe's paired removed line
//! (`probe.before`), which can be any removed line the diff pairs by position
//! (#14852). It reads the diff itself: inside the owner's span the diff must
//! replace exactly the tail line and nothing else, and the base-side item
//! that [`crate::analysis::diff::verified_base_owner`] establishes as the same
//! function (not `cfg`-gated) must be the current item with that one line
//! restored. Both versions are parsed,
//! and each must be a statement-free body whose tail is the parameter name,
//! bare on one side and under the operator on the other.
//!
//! What the change does to the result depends on the operator:
//!
//! - `!` on `bool` or on a primitive integer changes the result at every
//!   input (`!a != a`), so any input that reaches the owner infects it.
//! - `-` on a signed integer leaves the result unchanged at `0`, so the
//!   infection stage credits only an asserted literal input at which the two
//!   versions fold to different values (see
//!   [`crate::analysis::test_grip_evidence::owner_change_visibility`]).
//!
//! Every other shape is `None` and keeps today's reading.

use super::owner_pin::{OwnerPinSyntax, builtin_type_unshadowed};
use crate::analysis::diff::{ChangedFile, ModuleDeclarations};
use crate::analysis::facts::{FunctionImplContext, FunctionSummary};
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasGenericArgs, HasGenericParams, HasName},
};

/// The prefix operator the change deleted or added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::analysis) enum UnaryOperator {
    /// `-a`: equal to `a` at `0`.
    Neg,
    /// `!a`: different from `a` at every `bool` or integer input.
    Not,
}

/// A whole-body tail that changed between `name` and `OP name`, with the
/// base-side owner text the diff establishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct UnaryTailChange {
    pub operator: UnaryOperator,
    /// The returned parameter.
    pub operand: String,
    /// The current tail text (`a` or `-a`), the return-value sink's text.
    pub tail: String,
    /// The tail's line in the current file.
    pub line: usize,
    /// The owner's text on the base side: the current owner with the tail
    /// line restored.
    pub baseline: String,
}

/// Parameter types whose `!` differs from the operand at every value.
const NOT_TYPES: [&str; 13] = [
    "bool", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
];

/// Parameter types whose `-` the integer fold evaluates at a declared width.
const NEG_TYPES: [&str; 4] = ["i8", "i16", "i32", "i64"];

/// The change when `probe` is the owner's whole one-line tail and the diff
/// changes nothing else in the owner but one prefix `-`/`!` on the returned
/// parameter. `None` for every other shape.
pub(in crate::analysis) fn unary_tail_change(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
    index: &RustIndex,
    syntax: &OwnerPinSyntax,
    modules: &ModuleDeclarations,
) -> Option<UnaryTailChange> {
    if probe.family != ProbeFamily::ReturnValue
        || !matches!(owner.impl_context, FunctionImplContext::Free)
    {
        return None;
    }
    let line = probe.location.line;
    let SoleReplacedLine {
        offset,
        current: current_line,
        old: old_line,
        baseline,
    } = sole_replaced_line(owner, changed, line)?;
    let (operator, inserted) = one_prefix_operator_apart(&old_line, &current_line)?;

    if !base_item_is_the_restored_owner(
        owner,
        changed,
        index,
        modules,
        line,
        &current_line,
        &old_line,
    ) {
        return None;
    }
    let current = OwnerTail::read(owner.body.as_str(), offset)?;
    let base = OwnerTail::read(&baseline, offset)?;
    if current.name != owner.name
        || base.name != owner.name
        || current.operand != base.operand
        || current.parameter_type != base.parameter_type
        || current.return_type != base.return_type
        || current.parameter_type != current.return_type
        || current.operator != inserted.then_some(operator)
        || base.operator != (!inserted).then_some(operator)
    {
        return None;
    }
    let admitted: &[&str] = match operator {
        UnaryOperator::Not => &NOT_TYPES,
        UnaryOperator::Neg => &NEG_TYPES,
    };
    if !admitted.contains(&current.parameter_type.as_str())
        || !builtin_type_unshadowed(&current.parameter_type, index, syntax)
    {
        return None;
    }
    if another_definition_may_answer_the_name(owner, index) {
        return None;
    }
    Some(UnaryTailChange {
        operator,
        operand: current.operand,
        tail: current.tail,
        line,
        baseline,
    })
}

/// The one line of an owner the diff replaces, with the base-side owner text.
pub(super) struct SoleReplacedLine {
    /// The line's offset inside the owner's text.
    pub(super) offset: usize,
    /// The line's current text.
    pub(super) current: String,
    /// The text the diff replaced.
    pub(super) old: String,
    /// The owner's text with the old line restored.
    pub(super) baseline: String,
}

/// The replaced `line` when, inside `owner`'s span, the diff adds exactly
/// that line and removes exactly the one line it pairs with it. A removed
/// line anywhere else in the span (an old early return, a deleted statement)
/// makes the base-side owner something these readings do not see.
pub(super) fn sole_replaced_line(
    owner: &FunctionSummary,
    changed: &ChangedFile,
    line: usize,
) -> Option<SoleReplacedLine> {
    let span = owner.start_line..=owner.end_line;
    if !span.contains(&line) {
        return None;
    }
    let mut added = changed
        .added_lines
        .iter()
        .filter(|added| span.contains(&added.new_side_line));
    let (Some(added), None) = (added.next(), added.next()) else {
        return None;
    };
    let mut removed = changed
        .removed_lines
        .iter()
        .filter(|removed| span.contains(&removed.new_side_line));
    let (Some(removed), None) = (removed.next(), removed.next()) else {
        return None;
    };
    if added.new_side_line != line
        || changed
            .replaced_line_counterpart(line)
            .is_none_or(|counterpart| !std::ptr::eq(counterpart, removed))
    {
        return None;
    }
    let offset = line.checked_sub(owner.start_line)?;
    let lines: Vec<&str> = owner.body.as_str().split('\n').collect();
    let current = lines.get(offset)?.trim_end_matches('\r');
    if current != added.text.trim_end_matches('\r') {
        return None;
    }
    let old = removed.text.trim_end_matches('\r');
    let mut baseline_lines: Vec<String> = lines.iter().map(|line| (*line).to_string()).collect();
    *baseline_lines.get_mut(offset)? = old.to_string();
    Some(SoleReplacedLine {
        offset,
        current: current.to_string(),
        old: old.to_string(),
        baseline: baseline_lines.join("\n"),
    })
}

/// The base-side owner is the same item (#14815 review F1). The line pairing
/// inside the owner's span says nothing about which function the removed
/// line belonged to: swapping two same-name functions between modules, or
/// swapping their `#[cfg]`s, replaces one line inside the current owner's
/// span while the old line came from the other function. The shared owner
/// [`crate::analysis::diff::verified_base_owner`] establishes the base-side
/// item (same qualified path and attributes, unique under it, no changed
/// header, no changed `mod` or `#[path]` declaration on its module chain); a
/// gated item (a `cfg` on the function, an enclosing item, the file or a
/// `mod` declaration on its chain, or any `#[path]` declaration) refuses,
/// since a test may run a twin compiled in its place. The verified base item
/// must then be exactly the current item with the one replaced tail line
/// restored.
pub(super) fn base_item_is_the_restored_owner(
    owner: &FunctionSummary,
    changed: &ChangedFile,
    index: &RustIndex,
    modules: &ModuleDeclarations,
    line: usize,
    current_line: &str,
    old_line: &str,
) -> bool {
    let Some(facts) = crate::analysis::rust_index::find_file_facts(index, &owner.file) else {
        return false;
    };
    let Some(base) = crate::analysis::diff::verified_base_owner(
        &facts.source,
        &owner.file,
        changed,
        modules,
        &owner.name,
        owner.start_line,
        owner.end_line,
    ) else {
        return false;
    };
    if base.cfg_gated {
        return false;
    }
    let Some(offset) = line.checked_sub(base.new_first_line) else {
        return false;
    };
    let mut restored: Vec<&str> = base.new_text.split('\n').collect();
    match restored.get_mut(offset) {
        Some(slot) if slot.trim_end_matches('\r') == current_line => *slot = old_line,
        _ => return false,
    }
    restored.join("\n") == base.old_text
}

/// #14952: a test's bare `f(5)` reaches the changed owner only when nothing
/// else can be named `f` where the test runs. The shared identity reads `cfg`
/// gates on the item, its ancestors and its `mod` declarations, not a `cfg`
/// on a `pub use` that re-exports one of two same-name functions, an
/// `include!` that pulls in a twin, a `macro_rules!` that emits a module or a
/// function, or a `use .. as f` rename. So this reading also refuses when the
/// index holds another function of the owner's name anywhere, or when any
/// indexed file renames an import to that name, calls `include!`, or defines
/// a `macro_rules!` whose body emits a `mod`, a `fn $name` or a `fn` of the
/// owner's name. A file that matches the lexical prefilter but does not parse
/// refuses too.
pub(super) fn another_definition_may_answer_the_name(
    owner: &FunctionSummary,
    index: &RustIndex,
) -> bool {
    let name = owner.name.as_str();
    let mut same_name = index
        .functions()
        .iter()
        .filter(|function| function.name == name);
    let (Some(only), None) = (same_name.next(), same_name.next()) else {
        return true;
    };
    if only.file != owner.file || only.start_line != owner.start_line {
        return true;
    }
    index.files().iter().any(|(_, facts)| {
        let source: &str = &facts.data().source;
        if !(source.contains("macro_rules")
            || source.contains("include")
            || source.contains(" as "))
        {
            return false;
        }
        let Some(parse) = parse_clean_source_file(source) else {
            return true;
        };
        parse.tree().syntax().descendants().any(|node| {
            if let Some(rules) = ast::MacroRules::cast(node.clone()) {
                return macro_body_may_emit(&rules, name);
            }
            if let Some(call) = ast::MacroCall::cast(node.clone()) {
                return call
                    .path()
                    .and_then(|path| path.segment())
                    .and_then(|segment| segment.name_ref())
                    .is_some_and(|macro_name| macro_name.text() == "include");
            }
            ast::Rename::cast(node)
                .is_some_and(|rename| rename.name().is_some_and(|renamed| renamed.text() == name))
        })
    })
}

/// Whether a `macro_rules!` body may emit a module, a `fn $name` or a `fn`
/// named `name`. A body with no token tree refuses.
fn macro_body_may_emit(rules: &ast::MacroRules, name: &str) -> bool {
    let Some(body) = rules.token_tree() else {
        return true;
    };
    let tokens: Vec<_> = body
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    tokens
        .iter()
        .enumerate()
        .any(|(at, token)| match token.text() {
            "mod" => true,
            "fn" => tokens
                .get(at + 1)
                .is_none_or(|next| next.text() == "$" || next.text() == name),
            _ => false,
        })
}

/// The operator and whether `new` holds it, when `old` and `new` differ by
/// exactly one `-` or `!` written immediately before an identifier.
fn one_prefix_operator_apart(old: &str, new: &str) -> Option<(UnaryOperator, bool)> {
    let (short, long, inserted) = if new.len() == old.len() + 1 {
        (old, new, true)
    } else if old.len() == new.len() + 1 {
        (new, old, false)
    } else {
        return None;
    };
    let at = short
        .bytes()
        .zip(long.bytes())
        .take_while(|(left, right)| left == right)
        .count();
    if long.get(..at)? != short.get(..at)? || long.get(at + 1..)? != short.get(at..)? {
        return None;
    }
    let operator = match long.as_bytes().get(at)? {
        b'-' => UnaryOperator::Neg,
        b'!' => UnaryOperator::Not,
        _ => return None,
    };
    // `a-b` to `ab`, `a!` to `a`, `--a` to `-a`: the operator must be a
    // prefix of a name, not a binary operator or a repeated prefix. The
    // parse below checks the whole tail; this keeps the pairing honest.
    let starts_name = long
        .as_bytes()
        .get(at + 1)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_');
    let after_operand = long
        .get(..at)?
        .trim_end()
        .bytes()
        .next_back()
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b')' | b']'));
    (starts_name && !after_operand).then_some((operator, inserted))
}

/// One owner version read through the parser: a free `fn` whose body holds no
/// statement and whose tail, on line `offset` of the text and on that line
/// alone, is a parameter name, optionally under one prefix `-`/`!`.
struct OwnerTail {
    name: String,
    operand: String,
    operator: Option<UnaryOperator>,
    parameter_type: String,
    return_type: String,
    tail: String,
}

impl OwnerTail {
    fn read(text: &str, offset: usize) -> Option<Self> {
        let parse = parse_clean_source_file(text)?;
        let mut functions = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast);
        let (Some(function), None) = (functions.next(), functions.next()) else {
            return None;
        };
        if function.async_token().is_some() || function.generic_param_list().is_some() {
            return None;
        }
        let body = function.body()?;
        // An attribute anywhere in the body (`#![cfg(any())]` on the block,
        // `#[cfg(..)]` on the tail) can compile out syntax this reading
        // compares, so any `#` in the body refuses.
        if body
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::POUND)
        {
            return None;
        }
        let list = body.stmt_list()?;
        if list.statements().next().is_some() {
            return None;
        }
        let tail = list.tail_expr()?;
        let tail_text = tail.syntax().text().to_string();
        let start = usize::from(tail.syntax().text_range().start());
        if tail_text.contains('\n') || text.get(..start)?.matches('\n').count() != offset {
            return None;
        }
        let (operator, operand) = match &tail {
            ast::Expr::PrefixExpr(prefix) => {
                let operator = match prefix.op_kind()? {
                    ast::UnaryOp::Neg => UnaryOperator::Neg,
                    ast::UnaryOp::Not => UnaryOperator::Not,
                    ast::UnaryOp::Deref => return None,
                };
                (Some(operator), prefix.expr()?)
            }
            other => (None, other.clone()),
        };
        let operand = bare_name(&operand)?;
        let mut parameter_type = None;
        let params = function.param_list()?;
        if params.self_param().is_some() {
            return None;
        }
        for param in params.params() {
            let ast::Pat::IdentPat(ident) = param.pat()? else {
                return None;
            };
            if ident.ref_token().is_some() || ident.pat().is_some() {
                return None;
            }
            if ident.name()?.text() == operand {
                parameter_type = Some(compact(&param.ty()?.syntax().text().to_string()));
            }
        }
        let return_type = compact(&function.ret_type()?.ty()?.syntax().text().to_string());
        Some(Self {
            name: function.name()?.text().to_string(),
            operand,
            operator,
            parameter_type: parameter_type?,
            return_type,
            tail: tail_text,
        })
    }
}

/// A single-segment path expression's name (`a`), nothing qualified.
fn bare_name(expression: &ast::Expr) -> Option<String> {
    let ast::Expr::PathExpr(path) = expression else {
        return None;
    };
    let path = path.path()?;
    if path.qualifier().is_some() {
        return None;
    }
    let segment = path.segment()?;
    if segment.generic_arg_list().is_some() {
        return None;
    }
    Some(segment.name_ref()?.text().to_string())
}

fn compact(text: &str) -> String {
    text.split_whitespace().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_only_one_prefix_operator_before_a_name() {
        assert_eq!(
            one_prefix_operator_apart("    -a", "    a"),
            Some((UnaryOperator::Neg, false))
        );
        assert_eq!(
            one_prefix_operator_apart("    a", "    !a"),
            Some((UnaryOperator::Not, true))
        );
        // A binary operator, a suffix, a digit operand or a wider edit is
        // not a prefix deletion on a name.
        for (old, new) in [
            ("    a-b", "    ab"),
            ("    a!", "    a"),
            ("    -1", "    1"),
            ("    -a", "    b"),
            ("    -ab", "    a"),
            ("    +a", "    a"),
        ] {
            assert_eq!(one_prefix_operator_apart(old, new), None, "{old} -> {new}");
        }
    }

    #[test]
    fn reads_a_statement_free_one_line_parameter_tail() {
        let read = |text: &str, offset: usize| {
            OwnerTail::read(text, offset)
                .map(|tail| (tail.operator, tail.operand, tail.parameter_type, tail.tail))
        };
        assert_eq!(
            read("pub fn f(a: i32) -> i32 {\n    -a\n}", 1),
            Some((
                Some(UnaryOperator::Neg),
                "a".to_string(),
                "i32".to_string(),
                "-a".to_string()
            ))
        );
        assert_eq!(
            read("pub fn f(a: bool) -> bool {\n    a\n}", 1),
            Some((None, "a".to_string(), "bool".to_string(), "a".to_string()))
        );
        for (text, offset) in [
            // The tail is not on the probe's line.
            ("pub fn f(a: i32) -> i32 {\n    -a\n}", 0),
            // A statement before the tail (an early return, a shadow).
            (
                "pub fn f(a: i32) -> i32 {\n    if a == 5 { return 5; }\n    -a\n}",
                2,
            ),
            ("pub fn f(a: i32) -> i32 {\n    let a = 0;\n    -a\n}", 2),
            // A double prefix, a non-parameter name, a field, a method.
            ("pub fn f(a: i32) -> i32 {\n    --a\n}", 1),
            ("pub fn f(a: i32) -> i32 {\n    -B\n}", 1),
            ("pub fn f(p: P) -> i32 {\n    -p.x\n}", 1),
            ("pub fn f(&self) -> i32 {\n    -self\n}", 1),
            // An attribute in the body: an inner one on the block, an outer
            // one on the tail.
            (
                "pub fn f(a: i32) -> i32 {\n    #![cfg(any())]\n    -a\n}",
                2,
            ),
            ("pub fn f(a: i32) -> i32 {\n    #[cfg(any())]\n    -a\n}", 2),
            // A generic or async owner.
            ("pub fn f<T>(a: i32) -> i32 {\n    -a\n}", 1),
            ("pub async fn f(a: i32) -> i32 {\n    -a\n}", 1),
        ] {
            assert_eq!(read(text, offset), None, "{text}");
        }
    }
}
