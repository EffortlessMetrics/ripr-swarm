//! Kani bounded-model-checking harnesses for unified-diff hunk arithmetic.
//! They call the production functions; they do not re-implement them.
//! Compiled only under `cargo kani`.
//!
//! - Header: `parse_hunk_number` and `parse_range` on symbolic ASCII input.
//!   Bounds: ASCII byte strings of a fixed small length. Longer or non-ASCII
//!   headers are outside the proof; a non-ASCII byte cannot be a digit, so it
//!   only ever reaches the rejection path.
//! - Body (#13681): `hunk_step` (line coordinates) and `hunk_account`
//!   (remaining header counts). Each harness states its own domain. How the
//!   parser state machine records, skips or settles a hunk around these
//!   functions is covered by ordinary parser tests, not by these proofs.

use super::{BodyKind, HunkStep, hunk_account, hunk_step, parse_hunk_number, parse_range};

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

// ---------------------------------------------------------------------------
// Hunk-body line mapping (#13681): the pure coordinate and accounting core
// that `ParserState::consume_hunk_line` and `account_hunk_line` delegate to.
// Expected values come from independent arithmetic in each harness. Driving
// the whole parser state (map, paths, line text) through CBMC produced no
// verdict within 25 minutes, so the proofs target this core and the existing
// ordinary parser tests keep covering how the state machine uses it.
// ---------------------------------------------------------------------------

fn any_kind() -> BodyKind {
    match kani::any::<u8>() % 3 {
        0 => BodyKind::Added,
        1 => BodyKind::Removed,
        _ => BodyKind::Context,
    }
}

/// For every coordinate pair, one body line either records exactly the
/// cursor's line and advances that side by one, is refused when its side is
/// at the empty-hunk coordinate 0, or exhausts the hunk when its counter is at
/// `usize::MAX`. No recorded line or new-side anchor is ever 0, and an
/// empty new side (`+S,0`, #13098) anchors a removal one past the cursor.
#[kani::proof]
fn hunk_step_records_the_cursor_line_and_never_line_zero() {
    let old: usize = kani::any();
    let new: usize = kani::any();
    let new_side_empty: bool = kani::any();
    let kind = any_kind();

    let refused = match kind {
        BodyKind::Added => new == 0,
        BodyKind::Removed => old == 0,
        BodyKind::Context => false,
    };
    let exhausted = !refused
        && match kind {
            BodyKind::Added => new == usize::MAX,
            BodyKind::Removed => old == usize::MAX,
            BodyKind::Context => old == usize::MAX || new == usize::MAX,
        };

    let step = hunk_step(old, new, new_side_empty, kind);
    assert!((step == HunkStep::Refused) == refused);
    assert!((step == HunkStep::Exhausted) == exhausted);
    match step {
        HunkStep::Added { line, next_new } => {
            assert!(kind == BodyKind::Added);
            assert!(line == new && line >= 1 && next_new == line + 1);
        }
        HunkStep::Removed {
            line,
            new_side_line,
            next_old,
        } => {
            assert!(kind == BodyKind::Removed);
            assert!(line == old && line >= 1 && next_old == line + 1);
            assert!(new_side_line >= 1);
            if new < usize::MAX {
                let anchor = if new_side_empty { new + 1 } else { new };
                assert!(new_side_line == anchor.max(1));
            }
        }
        HunkStep::Context { next_old, next_new } => {
            assert!(kind == BodyKind::Context);
            assert!(next_old == old + 1 && next_new == new + 1);
        }
        HunkStep::Refused | HunkStep::Exhausted => {}
    }
    kani::cover!(
        matches!(
            step,
            HunkStep::Removed {
                new_side_line: 1,
                ..
            }
        ) && new == 0,
        "top-of-file deletion anchors at line 1"
    );
    kani::cover!(
        step == HunkStep::Exhausted,
        "counter at usize::MAX exhausts"
    );
}

/// Feeding the core's own `next_*` values back as the cursor, a hunk whose
/// body agrees with its header maps each added line to `new_start` plus the
/// added and context lines before it, and each removed line to `old_start`
/// plus the removed and context lines before it. This is the coordinate every
/// finding points at; starts range over all of `usize` short of exhaustion.
#[kani::proof]
#[kani::unwind(5)]
fn hunk_body_maps_each_line_to_its_exact_coordinate() {
    let old_start: usize = kani::any();
    let new_start: usize = kani::any();
    // Domain: real one-based starts with room for three body lines; a zero
    // start and counter exhaustion are proved in the single-step harness.
    kani::assume(old_start >= 1 && old_start <= usize::MAX - 3);
    kani::assume(new_start >= 1 && new_start <= usize::MAX - 3);
    let kinds = [any_kind(), any_kind(), any_kind()];
    let new_side_empty = kinds.iter().all(|kind| *kind == BodyKind::Removed);

    let (mut old, mut new) = (old_start, new_start);
    let (mut olds_before, mut news_before) = (0usize, 0usize);
    for kind in kinds {
        let step = hunk_step(old, new, new_side_empty, kind);
        // A valid start inside the domain is never refused or exhausted.
        assert!(!matches!(step, HunkStep::Refused | HunkStep::Exhausted));
        match step {
            HunkStep::Added { line, next_new } => {
                assert!(line == new_start + news_before);
                new = next_new;
            }
            HunkStep::Removed {
                line,
                new_side_line,
                next_old,
            } => {
                assert!(line == old_start + olds_before);
                let bias = usize::from(new_side_empty);
                assert!(new_side_line == new_start + news_before + bias);
                old = next_old;
            }
            HunkStep::Context { next_old, next_new } => {
                old = next_old;
                new = next_new;
            }
            HunkStep::Refused | HunkStep::Exhausted => {}
        }
        if kind != BodyKind::Added {
            olds_before += 1;
        }
        if kind != BodyKind::Removed {
            news_before += 1;
        }
    }
    kani::cover!(
        kinds.contains(&BodyKind::Added) && kinds.contains(&BodyKind::Removed),
        "mixed add and remove hunk"
    );
    kani::cover!(new_side_empty, "pure-deletion hunk");
}

/// Charging a body of up to four lines against any stated `(old, new)` counts
/// settles at `(0, 0)` with no overrun exactly when the body's own old/new
/// line counts equal the header's. Any other body is accounted malformed,
/// whether it over-delivers or stops short.
#[kani::proof]
#[kani::unwind(6)]
fn hunk_account_settles_exactly_when_body_matches_header() {
    let stated: (usize, usize) = (kani::any(), kani::any());
    let kinds = [any_kind(), any_kind(), any_kind(), any_kind()];
    let len: usize = kani::any();
    // Domain: bodies of zero to four lines against unbounded stated counts.
    kani::assume(len <= kinds.len());

    let mut remaining = Some(stated);
    let (mut body_old, mut body_new) = (0usize, 0usize);
    for kind in &kinds[..len] {
        remaining = remaining.and_then(|left| hunk_account(left, *kind));
        if *kind != BodyKind::Added {
            body_old += 1;
        }
        if *kind != BodyKind::Removed {
            body_new += 1;
        }
    }
    let settled = remaining == Some((0, 0));
    assert!(settled == (stated == (body_old, body_new)));
    kani::cover!(settled && len == 4, "four-line hunk settles");
    kani::cover!(remaining.is_none(), "over-delivering body overruns");
    kani::cover!(
        remaining.is_some() && !settled,
        "short body leaves counts owed"
    );
}
