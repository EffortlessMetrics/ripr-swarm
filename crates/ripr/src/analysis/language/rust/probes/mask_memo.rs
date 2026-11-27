//! A per-thread memo for the comment-and-string mask of long texts.
//!
//! Evidence for every seam of an owner masks the same owner body (or the
//! same predicate line) several times, so an owner with N seams cost
//! O(N x body) masking work: a 1,900-arm `else if` chain spent most of a
//! 20-second debug check re-masking one 50 KB line (#15189). The mask
//! depends only on the text, so recent long texts keep their answer and a
//! repeat question costs one comparison and one copy.

use std::cell::Cell;

/// Texts shorter than this are masked directly: the scan is already
/// cheaper than keeping a copy of the text.
const MEMO_MIN_BYTES: usize = 1024;

/// Distinct texts remembered per thread. One owner's checks interleave its
/// body, its predicate line and a few slices of them.
const MEMO_ENTRIES: usize = 4;

/// One remembered text and its mask.
struct Masked {
    text: String,
    masked: String,
}

thread_local! {
    // Most recently used first. `Cell::take` keeps this free of borrow
    // panics even if a scan were ever to re-enter the memo.
    static RECENT: Cell<Vec<Masked>> = const { Cell::new(Vec::new()) };
}

/// The mask of `text`, from the memo when this thread masked it recently.
pub(super) fn masked(text: &str, scan: fn(&str) -> String) -> String {
    if text.len() < MEMO_MIN_BYTES {
        return scan(text);
    }
    let mut recent = RECENT.with(Cell::take);
    let entry = match recent.iter().position(|entry| entry.text == text) {
        Some(index) => recent.remove(index),
        None => Masked {
            text: text.to_string(),
            masked: scan(text),
        },
    };
    let answer = entry.masked.clone();
    recent.insert(0, entry);
    recent.truncate(MEMO_ENTRIES);
    RECENT.with(|cell| cell.set(recent));
    answer
}

#[cfg(test)]
mod tests;
