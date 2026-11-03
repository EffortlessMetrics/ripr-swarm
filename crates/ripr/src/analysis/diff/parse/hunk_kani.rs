//! Kani bounded-model-checking harnesses for unified-diff hunk-header
//! arithmetic. They call the production parsers (`parse_hunk_number`
//! and `parse_range`) on symbolic ASCII input; they do not
//! re-implement them. Compiled only under `cargo kani`.
//!
//! Bounds: inputs are ASCII byte strings of a fixed small length. Longer or
//! non-ASCII headers are outside the proof; a non-ASCII byte cannot be a
//! digit, so it only ever reaches the rejection path.

use super::{parse_hunk_number, parse_range};

#[kani::proof]
#[kani::unwind(8)]
fn hunk_number_accepts_only_nonempty_ascii_digits() {
    let buf: [u8; 4] = kani::any();
    kani::assume(buf.iter().all(|b| *b < 128));
    let Ok(text) = std::str::from_utf8(&buf) else {
        return;
    };
    match parse_hunk_number(text) {
        Some(value) => {
            assert!(text.bytes().all(|b| b.is_ascii_digit()));
            kani::cover!(value > 0, "accepted a positive number");
        }
        None => {
            kani::cover!(
                !text.bytes().all(|b| b.is_ascii_digit()),
                "rejected non-digit input"
            );
        }
    }
}

#[kani::proof]
#[kani::unwind(8)]
fn hunk_range_rejects_positive_span_at_line_zero() {
    let buf: [u8; 5] = kani::any();
    kani::assume(buf.iter().all(|b| *b < 128));
    let Ok(text) = std::str::from_utf8(&buf) else {
        return;
    };
    if let Some((start, count)) = parse_range(text) {
        // A positive span names real one-based lines.
        assert!(count == 0 || start >= 1);
        kani::cover!(count > 0 && start >= 1, "non-empty range accepted");
        kani::cover!(count == 0 && start == 0, "empty range at line 0 accepted");
    }
}
