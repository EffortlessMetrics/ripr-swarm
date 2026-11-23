//! #14816: an `if` condition replaced by a literal `true` or `false`.
//!
//! Stryker's ConditionalExpression operator and an ordinary "force this
//! branch" edit turn `if x > 0 { 1 } else { 2 }` into `if true { 1 } else {
//! 2 }`. The literal spells no boundary, so the predicate lens read the
//! change as `infection_unknown` even where a related test pins an input
//! that took the other branch on the base side.
//!
//! The reading here does not trust the probe's paired removed line
//! (`probe.before`). Like the unary-tail reading (#14815) it reads the diff
//! itself: inside the owner's span the diff must replace exactly the
//! condition's line and nothing else, and the base-side item that
//! [`crate::analysis::diff::verified_base_owner`] establishes as the same
//! function (not `cfg`-gated) must be the current item with that one line
//! restored. Both versions are parsed, and the two lines must differ only by
//! the condition of one `if`: a bool literal on the current side, any other
//! single-line condition without a `let` on the base side.
//!
//! Whether a test input took the other branch is never assumed from the
//! condition's text. The infection stage folds both owner versions at the
//! related tests' literal calls
//! ([`crate::analysis::test_grip_evidence::owner_change_visibility`]), so a
//! condition or body the fold cannot read stays unknown.
//!
//! Every other shape is `None` and keeps today's reading.

use super::unary_tail::{
    SoleReplacedLine, another_definition_may_answer_the_name, base_item_is_the_restored_owner,
    sole_replaced_line,
};
use crate::analysis::diff::{ChangedFile, ModuleDeclarations};
use crate::analysis::facts::{FunctionImplContext, FunctionSummary};
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::parse_clean_source_file;
use crate::domain::{Probe, ProbeFamily};
use ra_ap_syntax::{
    AstNode, SyntaxKind,
    ast::{self, HasName},
};

/// An `if` condition the diff replaced by a bool literal, with the base-side
/// owner text the diff establishes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::analysis) struct LiteralConditionChange {
    /// The literal the condition now is.
    pub forced: bool,
    /// The base-side condition text (`x > 0`).
    pub before: String,
    /// The owner's text on the base side: the current owner with the
    /// condition's line restored.
    pub baseline: String,
}

/// The change when `probe` is a predicate on a line whose only edit, inside
/// an owner the diff changes nowhere else, replaced one `if` condition by
/// `true` or `false`. `None` for every other shape.
pub(in crate::analysis) fn literal_condition_change(
    probe: &Probe,
    owner: &FunctionSummary,
    changed: &ChangedFile,
    index: &RustIndex,
    modules: &ModuleDeclarations,
) -> Option<LiteralConditionChange> {
    if probe.family != ProbeFamily::Predicate
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
    let current = IfCondition::read(owner.body.as_str(), offset, &owner.name)?;
    let base = IfCondition::read(&baseline, offset, &owner.name)?;
    let forced = bool_literal(&current.condition)?;
    if bool_literal(&base.condition).is_some()
        || base
            .condition
            .syntax()
            .descendants()
            .any(|node| node.kind() == SyntaxKind::LET_EXPR)
    {
        return None;
    }
    // The two lines differ only by the condition: the current line with the
    // literal's span replaced by the base condition is the old line.
    let base_text = base.condition.syntax().text().to_string();
    let literal_end = current.column + usize::from(current.condition.syntax().text().len());
    let restored = format!(
        "{}{}{}",
        current_line.get(..current.column)?,
        base_text,
        current_line.get(literal_end..)?
    );
    if restored != old_line || base.column != current.column {
        return None;
    }
    if !base_item_is_the_restored_owner(
        owner,
        changed,
        index,
        modules,
        line,
        &current_line,
        &old_line,
    ) || another_definition_may_answer_the_name(owner, index)
    {
        return None;
    }
    Some(LiteralConditionChange {
        forced,
        before: base_text,
        baseline,
    })
}

/// The `true`/`false` a condition is when it is a bare bool literal.
fn bool_literal(condition: &ast::Expr) -> Option<bool> {
    let ast::Expr::Literal(literal) = condition else {
        return None;
    };
    match literal.kind() {
        ast::LiteralKind::Bool(value) => Some(value),
        _ => None,
    }
}

/// The one `if` of an owner version whose single-line condition starts on
/// line `offset` of the text.
struct IfCondition {
    condition: ast::Expr,
    /// The condition's byte column on its line.
    column: usize,
}

impl IfCondition {
    fn read(text: &str, offset: usize, name: &str) -> Option<Self> {
        let parse = parse_clean_source_file(text)?;
        let mut functions = parse
            .tree()
            .syntax()
            .descendants()
            .filter_map(ast::Fn::cast);
        let (Some(function), None) = (functions.next(), functions.next()) else {
            return None;
        };
        if function.name()?.text() != name {
            return None;
        }
        let body = function.body()?;
        // An attribute anywhere in the body can compile out syntax this
        // reading compares, so any `#` in the body refuses.
        if body
            .syntax()
            .descendants_with_tokens()
            .any(|element| element.kind() == SyntaxKind::POUND)
        {
            return None;
        }
        let mut on_line = body
            .syntax()
            .descendants()
            .filter_map(ast::IfExpr::cast)
            .filter_map(|if_expr| if_expr.condition())
            .filter(|condition| {
                let start = usize::from(condition.syntax().text_range().start());
                text.get(..start)
                    .is_some_and(|before| before.matches('\n').count() == offset)
            });
        let (Some(condition), None) = (on_line.next(), on_line.next()) else {
            return None;
        };
        if condition.syntax().text().contains_char('\n') {
            return None;
        }
        let start = usize::from(condition.syntax().text_range().start());
        let line_start = text.get(..start)?.rfind('\n').map_or(0, |at| at + 1);
        Some(Self {
            condition,
            column: start - line_start,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str, offset: usize) -> Option<(String, usize)> {
        IfCondition::read(text, offset, "f")
            .map(|read| (read.condition.syntax().text().to_string(), read.column))
    }

    #[test]
    fn reads_the_one_if_condition_on_the_line() {
        assert_eq!(
            read(
                "pub fn f(x: i32) -> i32 {\n    if true { 1 } else { 2 }\n}",
                1
            ),
            Some(("true".to_string(), 7))
        );
        assert_eq!(
            read(
                "pub fn f(x: i32) -> i32 {\n    if x > 0 { 1 } else { 2 }\n}",
                1
            ),
            Some(("x > 0".to_string(), 7))
        );
        for (text, offset) in [
            // No `if` starts its condition on the line.
            (
                "pub fn f(x: i32) -> i32 {\n    if true { 1 } else { 2 }\n}",
                0,
            ),
            // Two conditions on one line, an attribute in the body, another
            // function's name.
            (
                "pub fn f(x: i32) -> i32 {\n    if true { 1 } else if x > 0 { 2 } else { 3 }\n}",
                1,
            ),
            (
                "pub fn f(x: i32) -> i32 {\n    #[cfg(any())]\n    if true { 1 } else { 2 }\n}",
                2,
            ),
            (
                "pub fn g(x: i32) -> i32 {\n    if true { 1 } else { 2 }\n}",
                1,
            ),
        ] {
            assert_eq!(read(text, offset), None, "{text}");
        }
    }

    #[test]
    fn only_a_bare_bool_literal_is_forced() {
        let condition = |text: &str| {
            IfCondition::read(
                &format!("pub fn f(x: bool) -> i32 {{\n    if {text} {{ 1 }} else {{ 2 }}\n}}"),
                1,
                "f",
            )
            .and_then(|read| bool_literal(&read.condition))
        };
        assert_eq!(condition("true"), Some(true));
        assert_eq!(condition("false"), Some(false));
        for text in ["x", "!true", "(true)", "TRUE", "x == true"] {
            assert_eq!(condition(text), None, "{text}");
        }
    }
}
