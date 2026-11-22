//! A per-thread memo for the changed-binding relation of one owner body.
//!
//! The activation pass asks, for every boundary seam, whether the compared
//! operand is a live local and so resolves the binding's uses against the
//! whole owner body; an owner with N seams cost O(N x body). A 1,900-arm
//! `else if` chain asks the same question 1,900 times (#15189). The answer
//! depends only on the question's text and lines, so one body's answers
//! are kept per thread and a repeat question costs one body comparison.

use super::BindingPredicateResolution;
use std::cell::Cell;
use std::collections::HashMap;

/// Bodies shorter than this are scanned directly: the scan is already
/// cheaper than keeping a copy of the body.
const MEMO_MIN_BYTES: usize = 1024;

/// One relation question, exactly as `resolve_changed_binding_uses` takes it.
#[derive(Clone, Copy)]
pub(super) struct Question<'a> {
    pub(super) binding: &'a str,
    pub(super) initializer: &'a str,
    pub(super) body: &'a str,
    pub(super) body_start_line: usize,
    pub(super) changed_line: usize,
}

/// The question less its body: the key within one body's answers.
type AnswerKey = (String, String, usize, usize);

/// One owner body and the answers already computed against it.
struct BodyAnswers {
    body: String,
    answers: HashMap<AnswerKey, BindingPredicateResolution>,
}

thread_local! {
    // Seams are evaluated owner by owner, so one body per thread is
    // enough; a different body replaces it. `Cell::take` keeps this free
    // of borrow panics.
    static LAST_BODY: Cell<Option<BodyAnswers>> = const { Cell::new(None) };
}

/// The uncached relation scan, in `resolve_changed_binding_uses` order.
pub(super) type Scan = fn(&str, &str, &str, usize, usize) -> BindingPredicateResolution;

/// The relation for `question`, from this thread's memo when it was
/// already answered against the same body.
pub(super) fn resolved(question: Question<'_>, scan: Scan) -> BindingPredicateResolution {
    let run = || {
        scan(
            question.binding,
            question.initializer,
            question.body,
            question.body_start_line,
            question.changed_line,
        )
    };
    if question.body.len() < MEMO_MIN_BYTES {
        return run();
    }
    let key = (
        question.binding.to_string(),
        question.initializer.to_string(),
        question.body_start_line,
        question.changed_line,
    );
    let mut entry = LAST_BODY
        .with(Cell::take)
        .filter(|entry| entry.body == question.body)
        .unwrap_or_else(|| BodyAnswers {
            body: question.body.to_string(),
            answers: HashMap::new(),
        });
    let answer = entry.answers.entry(key).or_insert_with(run).clone();
    LAST_BODY.with(|cell| cell.set(Some(entry)));
    answer
}

#[cfg(test)]
mod tests;
