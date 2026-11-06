//! Kani harnesses for feedback vocabulary totality and reference
//! classification. Run: `cargo kani -p ripr --harness <name>`.

use super::*;

fn any_reason() -> FeedbackReason {
    // Bound derived from the closed taxonomy itself, so a new reason widens it.
    let all = FeedbackReason::all();
    let index: usize = kani::any();
    kani::assume(index < all.len());
    all[index]
}

/// Every reason maps to exactly one judgment (total), only `other` requires a
/// note, and `other` carries the non-committal `unclear` default.
#[kani::proof]
fn reason_judgment_mapping_is_total_and_note_rule_holds() {
    let reason = any_reason();
    let _ = reason.judgment();
    assert_eq!(reason.requires_note(), reason == FeedbackReason::Other);
    if reason == FeedbackReason::Other {
        assert_eq!(reason.judgment(), FeedbackJudgment::Unclear);
    }
    kani::cover!(reason == FeedbackReason::Other);
    kani::cover!(reason.judgment() == FeedbackJudgment::Expensive);
}

/// Wire labels round-trip through `parse` for the judgment vocabulary.
#[kani::proof]
#[kani::unwind(25)]
fn judgment_label_roundtrip() {
    // Concrete iteration (symbolic string comparison is intractable).
    let mut seen_last = false;
    for judgment in [
        FeedbackJudgment::Useful,
        FeedbackJudgment::Incorrect,
        FeedbackJudgment::Unclear,
        FeedbackJudgment::Expensive,
        FeedbackJudgment::IntentionalNoAction,
    ] {
        assert_eq!(FeedbackJudgment::parse(judgment.as_str()), Ok(judgment));
        if judgment == FeedbackJudgment::IntentionalNoAction {
            seen_last = true;
        }
    }
    kani::cover!(seen_last);
}

/// Wire labels round-trip through `parse` for all 25 reasons.
#[kani::proof]
#[kani::unwind(30)]
fn reason_label_roundtrip() {
    // Concrete iteration over the closed taxonomy (symbolic string comparison
    // across 25 labels is intractable); each reason is fully checked.
    let mut seen_other = false;
    for reason in FeedbackReason::all() {
        assert_eq!(FeedbackReason::parse(reason.as_str()), Ok(reason));
        if reason == FeedbackReason::Other {
            seen_other = true;
        }
    }
    kani::cover!(seen_other);
}

fn pick(choice: u8) -> Option<String> {
    match choice % 3 {
        0 => None,
        1 => Some("a".to_string()),
        _ => Some("b".to_string()),
    }
}

fn any_identity() -> ResultIdentity {
    ResultIdentity {
        snapshot_id: if kani::any() { "s1" } else { "s2" }.to_string(),
        canonical_item: pick(kani::any()),
        route_digest: pick(kani::any()),
        attempt_id: pick(kani::any()),
        receipt_id: pick(kani::any()),
    }
}

/// `classify_reference` is total and fail-closed: no live subject or an equal
/// identity is `Current`; `Historical` requires a differing identity whose
/// snapshot, item and route all match; anything else is `Mismatched`, so a
/// different snapshot/item/route is never reported current or historical.
#[kani::proof]
#[kani::unwind(5)]
fn classify_reference_is_total_and_fail_closed() {
    let stored = any_identity();
    let live = if kani::any() {
        Some(any_identity())
    } else {
        None
    };
    let state = classify_reference(&stored, live.as_ref());
    match &live {
        None => {
            assert_eq!(state, ReferenceState::Current);
        }
        Some(live) => {
            let same_result = stored.snapshot_id == live.snapshot_id
                && stored.canonical_item == live.canonical_item
                && stored.route_digest == live.route_digest;
            match state {
                ReferenceState::Current => {
                    assert!(stored == *live);
                }
                ReferenceState::Historical => {
                    assert!(same_result && stored != *live);
                }
                ReferenceState::Mismatched => {
                    assert!(!same_result);
                }
            }
        }
    }
    kani::cover!(state == ReferenceState::Current && live.is_some());
    kani::cover!(state == ReferenceState::Historical);
    kani::cover!(state == ReferenceState::Mismatched);
}
