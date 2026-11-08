//! #6578: the part of a changed one-expression line that actually differs
//! from its old text. In `lines.iter().map(|l| l.cents).sum::<u64>() + 50`
//! → `.. + 25 * 2` only `25 * 2` changed; the unchanged closure runs the
//! same way in both versions, so it neither hides the change from the
//! return nor decides whether the changed part runs.

use crate::analysis::syntax::parse_clean_source_file;
use ra_ap_syntax::{
    AstNode, NodeOrToken, SyntaxKind, SyntaxNode, SyntaxToken, TextRange, TextSize, ast,
};

/// The changed expression parsed as one tail expression, and the smallest
/// node covering the text between its longest common prefix and suffix
/// with `before`.
pub(super) struct ChangedSpan {
    pub(super) tail: SyntaxNode,
    pub(super) covering: SyntaxNode,
    /// The old text between the same prefix and suffix: what the change
    /// removed.
    pub(super) removed: String,
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
        let parse = parse_clean_source_file(&format!("{WRAPPER}{after} }}"))?;
        let after_tree = parse.tree();
        let (prefix, end, removed) = token_changed_range(after_tree.syntax(), &after, &before)
            .or_else(|| char_changed_range(&after, &before))?;
        let end = end.max(prefix);
        let tail = after_tree
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
        // A comment beside the tail parses outside it, so a change that
        // reaches into one lies outside the tail and is read whole.
        if !tail.syntax().text_range().contains_range(range) {
            return None;
        }
        let covering = match tail.syntax().covering_element(range) {
            NodeOrToken::Node(node) => node,
            NodeOrToken::Token(token) => token.parent()?,
        };
        Some(Self {
            tail: tail.syntax().clone(),
            covering,
            removed,
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

/// Trimmed, without a trailing `;` or a leading `return `, as the
/// return-path gate reads a final `return <tail>;`. Inner whitespace in a
/// string literal (`"a b"` → `"a  b"`) stays a change: string tokens
/// compare by full text. Whitespace between tokens is skipped so a reformat
/// beside the change does not widen the covering node.
fn collapse(text: &str) -> String {
    let text = text.trim().trim_end_matches(';').trim_end();
    text.strip_prefix("return ").unwrap_or(text).to_string()
}

/// Byte range in collapsed `after`, plus the corresponding removed text in
/// `before`, from the first and last non-whitespace tokens that differ.
/// Comments stay in the stream: a comment beside the tail still lies
/// outside it. `None` when `before` does not parse cleanly, the token
/// streams match, or a differing token cannot be mapped back into `after`.
fn token_changed_range(
    after_root: &SyntaxNode,
    after: &str,
    before: &str,
) -> Option<(usize, usize, String)> {
    let before_parse = parse_clean_source_file(&format!("{WRAPPER}{before} }}"))?;
    let after_tokens = non_whitespace_tokens(after_root);
    let before_tokens = non_whitespace_tokens(before_parse.tree().syntax());
    let same = |left: &SyntaxToken, right: &SyntaxToken| {
        left.kind() == right.kind() && left.text() == right.text()
    };
    let mut prefix_tokens = 0;
    for (after_token, before_token) in after_tokens.iter().zip(before_tokens.iter()) {
        if !same(after_token, before_token) {
            break;
        }
        prefix_tokens += 1;
    }
    let mut suffix_tokens = 0;
    for (after_token, before_token) in after_tokens
        .iter()
        .skip(prefix_tokens)
        .rev()
        .zip(before_tokens.iter().skip(prefix_tokens).rev())
    {
        if !same(after_token, before_token) {
            break;
        }
        suffix_tokens += 1;
    }
    let after_end = after_tokens.len().checked_sub(suffix_tokens)?;
    let after_changed = after_tokens.get(prefix_tokens..after_end)?;
    let first = after_changed.first()?;
    let last = after_changed.last()?;
    let prefix = unwrapped_offset(first.text_range().start(), after.len())?;
    let end = unwrapped_offset(last.text_range().end(), after.len())?;
    if prefix > end {
        return None;
    }
    let before_end = before_tokens.len().checked_sub(suffix_tokens)?;
    let before_changed = before_tokens.get(prefix_tokens..before_end)?;
    let removed = match (before_changed.first(), before_changed.last()) {
        (Some(first_removed), Some(last_removed)) => {
            let start = unwrapped_offset(first_removed.text_range().start(), before.len())?;
            let stop = unwrapped_offset(last_removed.text_range().end(), before.len())?;
            before.get(start..stop)?.to_string()
        }
        _ => String::new(),
    };
    Some((prefix, end, removed))
}

fn non_whitespace_tokens(root: &SyntaxNode) -> Vec<SyntaxToken> {
    root.descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() != SyntaxKind::WHITESPACE)
        .collect()
}

fn unwrapped_offset(at: TextSize, text_len: usize) -> Option<usize> {
    let offset = usize::from(at).checked_sub(WRAPPER.len())?;
    (offset <= text_len).then_some(offset)
}

fn char_changed_range(after: &str, before: &str) -> Option<(usize, usize, String)> {
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
    let removed = before
        .get(prefix..(before.len() - suffix).max(prefix))?
        .to_string();
    Some((prefix, end, removed))
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
        // A final `return` on either side is not part of the change.
        assert_eq!(
            covering(&format!("return {};", tail("51")), &tail("50")).as_deref(),
            Some("51")
        );
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
    fn the_removed_text_and_string_whitespace_are_kept() {
        let span = ChangedSpan::of(
            "it.map(|l| l.cents).sum::<u64>() + 51",
            Some("it.map(|l| l.cents).sum::<u64>() + (|| 51)()"),
        );
        assert_eq!(span.map(|span| span.removed).as_deref(), Some("(|| 51)()"));
        // Whitespace inside a string literal is part of the change.
        let strings = covering(
            "\"a  b\".len() + it.map(|l| l.cents).sum::<usize>() + 1",
            "\"a b\".len() + it.map(|l| l.cents).sum::<usize>() + 2",
        );
        assert_ne!(strings.as_deref(), Some("1"), "{strings:?}");
    }

    #[test]
    fn no_span_without_an_old_text_or_one_tail_expression() {
        let after = "it.map(|l| l.cents).sum::<u64>() + 51";
        assert!(ChangedSpan::of(after, None).is_none());
        assert!(ChangedSpan::of(after, Some("")).is_none());
        assert!(ChangedSpan::of(after, Some(after)).is_none());
        assert!(ChangedSpan::of("let x = 1", Some("let x = 2")).is_none());
        assert!(ChangedSpan::of("a + (", Some("a + 1")).is_none());
        // A change that reaches into a comment beside the tail.
        for (after, before) in [
            (
                "it.map(|l| l.cents).sum::<u64>() + 51 /* fee v2 */",
                "it.map(|l| l.cents).sum::<u64>() + 50 /* fee v1 */",
            ),
            (
                "/* v2 */ it.map(|l| l.cents).sum::<u64>() + 51",
                "/* v1 */ it.map(|l| l.cents).sum::<u64>() + 50",
            ),
        ] {
            assert!(ChangedSpan::of(after, Some(before)).is_none(), "{after}");
        }
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
            (
                "{ let mut t = 0; while let Some(k) = Some(3) { t = it.map(|x| x * k).sum(); break; } t }",
                "{ let mut t = 0; while let Some(k) = Some(2) { t = it.map(|x| x * k).sum(); break; } t }",
            ),
            (
                "{ for k in [3] { return it.map(|x| x * k).sum(); } 0 }",
                "{ for k in [2] { return it.map(|x| x * k).sum(); } 0 }",
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

    #[test]
    fn a_whitespace_reformat_beside_the_change_does_not_widen_the_covering_node() {
        // #7956: `fns [0](50) + g()` → `fns[0](51) + g()` must cover `51`,
        // not the indexed call. A char-level prefix stops at `fns`.
        assert_eq!(
            covering("fns[0](51) + g()", "fns [0](50) + g()").as_deref(),
            Some("51")
        );
        assert_ne!(
            covering("fns[0](51) + g()", "fns [0](50) + g()").as_deref(),
            Some("fns[0](51)")
        );
        // A reformat-only difference beside the change keeps the same
        // covering node as the compact pair.
        let compact = covering("fns[0](51) + g()", "fns[0](50) + g()");
        assert_eq!(compact.as_deref(), Some("51"));
        assert_eq!(covering("fns [0](51) + g()", "fns[0](50) + g()"), compact);
        assert_eq!(covering("fns[0](51) + g()", "fns [0](50) + g()"), compact);
        assert_eq!(covering("fns [0](51) + g()", "fns [0](50) + g()"), compact);
    }
}
