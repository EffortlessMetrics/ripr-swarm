//! The `return` exits of an owner body as the scalar early-return gate reads
//! them, read once per owner text in a run scope (#11738).
//!
//! `scalar_early_return_gate` asks, for one changed `return <lit>;`, whether
//! every `return` of the owner builds a whole decimal literal, which of them
//! build the changed literal, and what the tail builds. Everything but the
//! changed literal and its line is a pure function of the owner's text, yet
//! each `return` was located, collapsed and parsed again for every changed
//! line and every pin establishment of that line, so one long owner with many
//! changed `return`s paid its length times its `return` count. The text half
//! is read here once per scope; only the changed literal's exits are asked
//! per call, in the same order and with the same line check.

use super::super::boundary_pairing::decimal_integer_value;
use super::{collapse_whitespace, top_level_tail_starts, whole_word_offsets};
use crate::analysis::syntax::run_memo::{self, Product};
use std::collections::HashMap;
use std::rc::Rc;

/// The exits of one owner body whose every `return` builds a whole decimal
/// literal.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct ScalarExits {
    /// Each `return`'s `(offset, value end)` in the body's inner text, by the
    /// canonical value of the literal it builds, in source order.
    by_value: HashMap<String, Vec<(usize, usize)>>,
    /// The tail's canonical value, `None` when the tail is not one whole
    /// decimal literal.
    tail: Option<String>,
}

impl ScalarExits {
    /// Whether the changed literal `changed` (a whole decimal literal) is
    /// built by exactly one `return`, every `return` building it spans the
    /// changed line, and the tail builds a different whole decimal literal:
    /// the answer the per-call loop gave.
    pub(super) fn changed_is_only_source(
        &self,
        changed: &str,
        spans_changed_line: &dyn Fn(usize, usize) -> bool,
    ) -> bool {
        let Some(changed_value) = decimal_integer_value(changed) else {
            return false;
        };
        let same = self
            .by_value
            .get(&changed_value)
            .map_or(&[][..], Vec::as_slice);
        // Only the changed line may produce this literal; a second `return`
        // of it elsewhere is a second source. The line check is pure, so
        // asking it only for a lone exit gives the loop's answer.
        let [(offset, value_end)] = same else {
            return false;
        };
        spans_changed_line(*offset, *value_end)
            && self
                .tail
                .as_ref()
                .is_some_and(|tail| *tail != changed_value)
    }
}

/// The scalar exits of the body whose block interior is `inner` (comments and
/// strings masked) and `inner_text` (as written). `None` when a `return` has
/// no `;` after it, or builds anything but a whole decimal literal: a
/// parameter, const or computed exit may alias the changed literal under an
/// input this static read does not evaluate.
pub(super) fn scalar_exits(inner: &str, inner_text: &str) -> Rc<Option<ScalarExits>> {
    run_memo::memoize_with(Product::ScalarReturnExits, 0, inner, inner_text, || {
        Rc::new(read(inner, inner_text))
    })
}

fn read(inner: &str, inner_text: &str) -> Option<ScalarExits> {
    #[cfg(test)]
    SCALAR_EXIT_READS.with(|reads| reads.set(reads.get() + 1));
    let mut by_value: HashMap<String, Vec<(usize, usize)>> = HashMap::new();
    for offset in whole_word_offsets(inner, "return") {
        let value_start = offset + "return".len();
        let value_end = value_start + inner[value_start..].find(';')?;
        let value = collapse_whitespace(&inner_text[value_start..value_end]);
        let value = decimal_integer_value(value.trim())?;
        by_value.entry(value).or_default().push((offset, value_end));
    }
    let (after_semicolon, after_brace) = top_level_tail_starts(inner);
    let tail = collapse_whitespace(&inner_text[after_semicolon.max(after_brace)..]);
    Some(ScalarExits {
        by_value,
        tail: decimal_integer_value(tail.trim()),
    })
}

#[cfg(test)]
thread_local! {
    /// How many owner bodies this thread has read for scalar exits.
    static SCALAR_EXIT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod tests;
