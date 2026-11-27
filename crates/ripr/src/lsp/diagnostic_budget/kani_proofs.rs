//! Kani bounded-model-checking harnesses for diagnostic delivery budgeting.
//!
//! Compiled only under `cfg(kani)`; normal builds never see this module.
//! Run with `cargo kani -p ripr --harness <name> --default-unwind 6`.
//!
//! `evaluate_diagnostic_budget` itself is not tractable for Kani (Strings,
//! `BTreeMap`/`BTreeSet`, `sort_by`), so the budget rule was factored into the
//! scalar-only `BudgetCursor::decide`/`commit`, which the production walk calls
//! for every item. These harnesses drive that production code over a bounded
//! walk of four items spread across two documents.
//!
//! Bounds (documented `kani::assume`s): four items; each is in document 0 or 1
//! (per-document counts are tracked by the harness exactly as the production
//! walk tracks them in its map); item-count limits are in `1..=3`; byte limits
//! are non-zero; payload and inline-detail sizes are arbitrary `usize`.

use super::*;

const STEPS: usize = 4;

fn symbolic_budget() -> DiagnosticBudget {
    let budget = DiagnosticBudget {
        max_items_per_document: kani::any(),
        max_items_per_workspace_response: kani::any(),
        max_serialized_bytes: kani::any(),
        max_inline_detail_bytes: kani::any(),
    };
    kani::assume((1..=3).contains(&budget.max_items_per_document));
    kani::assume((1..=3).contains(&budget.max_items_per_workspace_response));
    kani::assume(budget.max_items_per_document <= budget.max_items_per_workspace_response);
    kani::assume(budget.max_serialized_bytes > 0);
    kani::assume(budget.max_inline_detail_bytes > 0);
    // Every budget generated in this bounded domain must pass validation.
    assert!(budget.validate().is_ok());
    budget
}

/// Count, byte and inline-detail pressure compose in the same production walk.
/// The independent widened arithmetic pins exact decisions and reason priority,
/// not only upper bounds (an implementation that refuses everything must fail).
#[kani::proof]
#[kani::unwind(6)]
fn budget_item_limits_hold_over_walk() {
    let budget = symbolic_budget();
    let mut cursor = BudgetCursor::default();
    let mut per_doc = [0usize; 2];
    let mut omitted = 0usize;
    let mut exact = 0u128;
    let mut saw_document_omission = false;
    let mut saw_workspace_omission = false;
    let mut saw_byte_omission = false;
    for _ in 0..STEPS {
        let doc = usize::from(kani::any::<bool>());
        let payload: usize = kani::any();
        let detail: usize = kani::any();
        let next_exact = exact + payload as u128;
        let expected = if per_doc[doc] == budget.max_items_per_document {
            saw_document_omission = true;
            BudgetDecision::Omit {
                reason: OmittedDiagnosticReason::DocumentItemLimit,
                overflow: DiagnosticOverflowReason::DocumentItemLimit,
            }
        } else if cursor.selected_len == budget.max_items_per_workspace_response {
            saw_workspace_omission = true;
            BudgetDecision::Omit {
                reason: OmittedDiagnosticReason::WorkspaceItemLimit,
                overflow: DiagnosticOverflowReason::WorkspaceItemLimit,
            }
        } else if cursor.selected_len > 0 && next_exact > budget.max_serialized_bytes as u128 {
            saw_byte_omission = true;
            BudgetDecision::Omit {
                reason: OmittedDiagnosticReason::SerializedByteLimit,
                overflow: DiagnosticOverflowReason::SerializedByteLimit,
            }
        } else {
            BudgetDecision::Select {
                byte_overflow: next_exact > budget.max_serialized_bytes as u128,
                inline_detail_omitted: detail > budget.max_inline_detail_bytes,
            }
        };
        let decision = cursor.decide(&budget, per_doc[doc], payload, detail);
        assert!(decision == expected);
        match decision {
            BudgetDecision::Select { .. } => {
                cursor.commit(payload);
                exact = next_exact;
                per_doc[doc] += 1;
            }
            BudgetDecision::Omit { .. } => omitted += 1,
        }
        assert!(per_doc[0] <= budget.max_items_per_document);
        assert!(per_doc[1] <= budget.max_items_per_document);
        assert!(cursor.selected_len <= budget.max_items_per_workspace_response);
        assert!(cursor.selected_len == per_doc[0] + per_doc[1]);
        assert!(cursor.selected_bytes as u128 == exact);
        if cursor.selected_len > 1 {
            assert!(exact <= budget.max_serialized_bytes as u128);
        }
    }
    assert!(cursor.selected_len + omitted == STEPS);
    kani::cover!(omitted > 0 && cursor.selected_len > 1);
    kani::cover!(saw_document_omission && saw_byte_omission);
    kani::cover!(saw_workspace_omission && saw_byte_omission);
    kani::cover!(budget.max_serialized_bytes == usize::MAX && saw_byte_omission);
}

/// The serialized-byte budget is never exceeded except by a lone first item
/// (which is flagged as an overflow), and the running byte total never wraps:
/// it is always exactly the sum of selected payloads (computed in u128).
#[kani::proof]
#[kani::unwind(6)]
fn budget_bytes_never_exceed_limit_or_wrap() {
    let budget = symbolic_budget();
    let mut cursor = BudgetCursor::default();
    let mut exact: u128 = 0;
    let mut saw_byte_omission = false;
    for _ in 0..STEPS {
        let payload: usize = kani::any();
        let decision = cursor.decide(&budget, 0, payload, 0);
        match decision {
            BudgetDecision::Select { byte_overflow, .. } => {
                if byte_overflow {
                    // Only the very first selected item may overshoot.
                    assert!(cursor.selected_len == 0);
                    assert!(payload > budget.max_serialized_bytes);
                }
                cursor.commit(payload);
                exact += payload as u128;
            }
            BudgetDecision::Omit { reason, overflow } => {
                if reason == OmittedDiagnosticReason::SerializedByteLimit {
                    saw_byte_omission = true;
                    assert!(overflow == DiagnosticOverflowReason::SerializedByteLimit);
                    assert!(cursor.selected_len > 0);
                    assert!(exact + payload as u128 > budget.max_serialized_bytes as u128);
                }
            }
        }
        // No wrap: reported bytes equal the exact sum while it fits.
        assert!(cursor.selected_bytes as u128 == exact.min(usize::MAX as u128));
        // Budget respected unless a lone first item overshot.
        if cursor.selected_len > 1 {
            assert!(exact <= budget.max_serialized_bytes as u128);
        }
    }
    kani::cover!(cursor.selected_len >= 2);
    kani::cover!(saw_byte_omission);
    kani::cover!(budget.max_serialized_bytes == usize::MAX && saw_byte_omission);
    kani::cover!(cursor.selected_len == 1 && exact > budget.max_serialized_bytes as u128);
}

/// Inline detail is flagged omitted exactly when it exceeds its limit.
#[kani::proof]
fn budget_inline_detail_flag_is_exact() {
    let budget = symbolic_budget();
    let cursor = BudgetCursor::default();
    let detail: usize = kani::any();
    let decision = cursor.decide(&budget, 0, 0, detail);
    if let BudgetDecision::Select {
        inline_detail_omitted,
        ..
    } = decision
    {
        assert!(inline_detail_omitted == (detail > budget.max_inline_detail_bytes));
    } else {
        // Empty cursor with zero payload always selects.
        assert!(false);
    }
    kani::cover!(detail > budget.max_inline_detail_bytes);
    kani::cover!(detail <= budget.max_inline_detail_bytes);
}

/// Budget validation: accepted budgets have no zero limit and a per-document
/// limit that does not exceed the workspace limit.
#[kani::proof]
fn budget_validate_accepts_only_sane_limits() {
    let budget = DiagnosticBudget {
        max_items_per_document: kani::any(),
        max_items_per_workspace_response: kani::any(),
        max_serialized_bytes: kani::any(),
        max_inline_detail_bytes: kani::any(),
    };
    let ok = budget.validate().is_ok();
    if ok {
        assert!(budget.max_items_per_document > 0);
        assert!(budget.max_items_per_workspace_response > 0);
        assert!(budget.max_serialized_bytes > 0);
        assert!(budget.max_inline_detail_bytes > 0);
        assert!(budget.max_items_per_document <= budget.max_items_per_workspace_response);
    }
    kani::cover!(ok);
    kani::cover!(!ok);
}
