//! #6578: the part of a changed one-expression line that actually differs
//! from its old text. In `lines.iter().map(|l| l.cents).sum::<u64>() + 50`
//! → `.. + 25 * 2` only `25 * 2` changed; the unchanged closure runs the
//! same way in both versions, so it neither hides the change from the
//! return nor decides whether the changed part runs.

use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{AstNode, NodeOrToken, SyntaxNode, TextRange, TextSize, ast};

/// The changed expression parsed as one tail expression, and the smallest
/// node covering the text between its longest common prefix and suffix
/// with `before`.
pub(super) struct ChangedSpan {
    pub(super) tail: SyntaxNode,
    pub(super) covering: SyntaxNode,
}

const WRAPPER: &str = "fn ripr_tail() { ";

impl ChangedSpan {
    /// `None` when `before` is absent, empty or equal to `after`, or when
    /// `after` does not parse as exactly one tail expression: callers then
    /// read the whole expression as they did before.
    pub(super) fn of(after: &str, before: Option<&str>) -> Option<Self> {
        let after = collapse(after);
        let before = collapse(before?);
        if before.is_empty() || before == after {
            return None;
        }
        let prefix = after
            .char_indices()
            .zip(before.chars())
            .take_while(|((_, a), b)| a == b)
            .last()
            .map_or(0, |((offset, character), _)| offset + character.len_utf8());
        let suffix = after[prefix..]
            .chars()
            .rev()
            .zip(before.get(prefix..).unwrap_or("").chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(character, _)| character.len_utf8())
            .sum::<usize>();
        let end = (after.len() - suffix).max(prefix);
        let parse = parse_clean_source_file(&format!("{WRAPPER}{after} }}"))?;
        let tail = parse
            .tree()
            .syntax()
            .descendants()
            .find_map(ast::Fn::cast)?
            .body()?
            .stmt_list()
            .filter(|list| list.statements().next().is_none())?
            .tail_expr()?;
        // A name bound in the tail outside every closure (`{ let k = 3;
        // it.map(|x| x * k).sum() }`, a `match` arm, `if let`, `for`) can
        // carry the change into a closure that may never run.
        if binds_outside_closures(tail.syntax()) {
            return None;
        }
        let offset = |at: usize| TextSize::try_from(WRAPPER.len() + at).ok();
        let range = TextRange::new(offset(prefix)?, offset(end)?);
        let covering = match tail.syntax().covering_element(range) {
            NodeOrToken::Node(node) => node,
            NodeOrToken::Token(token) => token.parent()?,
        };
        Some(Self {
            tail: tail.syntax().clone(),
            covering,
        })
    }

    /// Each node enclosing the covering node, up to and including the tail,
    /// paired with the child it was reached from.
    pub(super) fn enclosing(&self) -> Vec<(SyntaxNode, SyntaxNode)> {
        let mut pairs = Vec::new();
        if self.covering == self.tail {
            return pairs;
        }
        let mut child = self.covering.clone();
        for ancestor in self.covering.ancestors().skip(1) {
            let last = ancestor == self.tail;
            pairs.push((ancestor.clone(), child));
            if last {
                break;
            }
            child = ancestor;
        }
        pairs
    }

    /// Whether the changed text sits in, or itself holds, a closure.
    pub(super) fn under_closure(&self) -> bool {
        self.covering
            .descendants()
            .chain(self.enclosing().into_iter().map(|(ancestor, _)| ancestor))
            .chain(std::iter::once(self.covering.clone()))
            .any(|node| ast::ClosureExpr::can_cast(node.kind()))
    }
}

fn binds_outside_closures(node: &SyntaxNode) -> bool {
    node.children().any(|child| {
        if ast::ClosureExpr::can_cast(child.kind()) {
            return false;
        }
        ast::LetStmt::can_cast(child.kind())
            || ast::LetExpr::can_cast(child.kind())
            || ast::MatchArm::can_cast(child.kind())
            || ast::ForExpr::can_cast(child.kind())
            || binds_outside_closures(&child)
    })
}

fn collapse(text: &str) -> String {
    text.trim()
        .trim_end_matches(';')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn covering(after: &str, before: &str) -> Option<String> {
        ChangedSpan::of(after, Some(before)).map(|span| span.covering.text().to_string())
    }

    #[test]
    fn the_span_is_the_smallest_node_around_the_differing_text() {
        let tail = |fee: &str| format!("lines.iter().map(|l| l.cents).sum::<u64>() + {fee}");
        assert_eq!(
            covering(&tail("25 * 2"), &tail("50")).as_deref(),
            Some("25 * 2")
        );
        assert_eq!(
            covering(&tail("25 * 2"), &format!("{};", tail("50"))).as_deref(),
            Some("25 * 2"),
            "a trailing `;` and spacing are not part of the change"
        );
        // A change inside the closure covers the closure's body.
        let inside = covering(
            "lines.iter().map(|l| l.cents + 1).sum::<u64>()",
            "lines.iter().map(|l| l.cents + 2).sum::<u64>()",
        );
        assert_eq!(inside.as_deref(), Some("1"));
    }

    #[test]
    fn a_closure_is_seen_only_around_or_inside_the_change() {
        let span = |after: &str, before: &str| ChangedSpan::of(after, Some(before));
        let outside = span(
            "it.map(|l| l.cents).sum::<u64>() + 51",
            "it.map(|l| l.cents).sum::<u64>() + 50",
        );
        assert!(outside.is_some_and(|span| !span.under_closure()));
        let inside = span(
            "it.map(|l| l.cents + 1).sum::<u64>()",
            "it.map(|l| l.cents + 2).sum::<u64>()",
        );
        assert!(inside.is_some_and(|span| span.under_closure()));
        // The change replaces a closure-free operand with a closure call.
        let holds = span(
            "it.map(|l| l.cents).sum::<u64>() + (|| 50)()",
            "it.map(|l| l.cents).sum::<u64>() + 50",
        );
        assert!(holds.is_some_and(|span| span.under_closure()));
        // `||` is an operator, not a closure.
        let operator = span(
            "day % 7 == 6 || day % 7 == 0",
            "day % 7 == 0 || day % 7 == 6",
        );
        assert!(operator.is_some_and(|span| !span.under_closure()));
    }

    #[test]
    fn no_span_without_an_old_text_or_one_tail_expression() {
        let after = "it.map(|l| l.cents).sum::<u64>() + 51";
        assert!(ChangedSpan::of(after, None).is_none());
        assert!(ChangedSpan::of(after, Some("")).is_none());
        assert!(ChangedSpan::of(after, Some(after)).is_none());
        assert!(ChangedSpan::of("let x = 1", Some("let x = 2")).is_none());
        assert!(ChangedSpan::of("a + (", Some("a + 1")).is_none());
        // A name bound outside every closure can carry the change into one.
        for (after, before) in [
            (
                "{ let k = 3; it.map(|x| x * k).sum() }",
                "{ let k = 2; it.map(|x| x * k).sum() }",
            ),
            (
                "match 3 { k => it.map(|x| x * k).sum() }",
                "match 2 { k => it.map(|x| x * k).sum() }",
            ),
            (
                "if let Some(k) = Some(3) { it.map(|x| x * k).sum() } else { 0 }",
                "if let Some(k) = Some(2) { it.map(|x| x * k).sum() } else { 0 }",
            ),
        ] {
            assert!(ChangedSpan::of(after, Some(before)).is_none(), "{after}");
        }
        // A binding inside a closure stays the closure's own.
        assert!(
            ChangedSpan::of(
                "it.map(|l| { let c = l.cents; c }).sum::<u64>() + 51",
                Some("it.map(|l| { let c = l.cents; c }).sum::<u64>() + 50")
            )
            .is_some()
        );
    }
}
