//! Ordinary production-path parity for the bounded diagnostic-budget claims.

use super::*;

fn item(id: &str, document: &str, payload_bytes: usize) -> DiagnosticBudgetItem {
    DiagnosticBudgetItem {
        canonical_id: id.to_string(),
        document: document.to_string(),
        payload_bytes,
        inline_detail_bytes: 0,
        eligibility: DiagnosticBudgetEligibility::Actionable,
        selection_key: DiagnosticSelectionKey {
            repair_route_rank: 0,
            causal_rank: 0,
            evidence_rank: 0,
        },
    }
}

#[test]
fn maximum_byte_budget_refuses_sum_overflow_and_preserves_exact_fit() -> Result<(), String> {
    let result = evaluate_diagnostic_budget(
        [
            item("a", "one", usize::MAX),
            item("b", "two", 1),
            item("c", "two", 0),
        ],
        &DiagnosticBudget {
            max_serialized_bytes: usize::MAX,
            ..DiagnosticBudget::default()
        },
        "snapshot",
        "evidence",
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(result.selected_ids().collect::<Vec<_>>(), ["a", "c"]);
    assert_eq!(result.selected_bytes, usize::MAX);
    assert_eq!(
        result.omitted,
        [OmittedDiagnosticItem {
            canonical_id: "b".to_string(),
            reason: OmittedDiagnosticReason::SerializedByteLimit,
        }]
    );
    assert_eq!(
        result.overflow_reasons,
        BTreeSet::from([DiagnosticOverflowReason::SerializedByteLimit])
    );
    Ok(())
}

#[test]
fn document_and_byte_pressure_compose_without_charging_omitted_payloads() -> Result<(), String> {
    let result = evaluate_diagnostic_budget(
        [
            item("a", "one", 1),
            item("b", "one", usize::MAX),
            item("c", "two", 2),
            item("d", "two", 1),
        ],
        &DiagnosticBudget {
            max_items_per_document: 1,
            max_items_per_workspace_response: 2,
            max_serialized_bytes: 2,
            max_inline_detail_bytes: 1,
        },
        "snapshot",
        "evidence",
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(result.selected_ids().collect::<Vec<_>>(), ["a", "d"]);
    assert_eq!(result.selected_bytes, 2);
    assert_eq!(
        result.omitted,
        [
            OmittedDiagnosticItem {
                canonical_id: "b".to_string(),
                reason: OmittedDiagnosticReason::DocumentItemLimit
            },
            OmittedDiagnosticItem {
                canonical_id: "c".to_string(),
                reason: OmittedDiagnosticReason::SerializedByteLimit
            },
        ]
    );
    assert_eq!(
        result.overflow_reasons,
        BTreeSet::from([
            DiagnosticOverflowReason::DocumentItemLimit,
            DiagnosticOverflowReason::SerializedByteLimit,
        ])
    );
    Ok(())
}

#[test]
fn oversized_first_item_does_not_admit_a_zero_byte_second_item() -> Result<(), String> {
    let result = evaluate_diagnostic_budget(
        [item("a", "one", 3), item("b", "two", 0)],
        &DiagnosticBudget {
            max_serialized_bytes: 2,
            ..DiagnosticBudget::default()
        },
        "snapshot",
        "evidence",
    )
    .map_err(|error| error.to_string())?;
    assert_eq!(result.selected_ids().collect::<Vec<_>>(), ["a"]);
    assert_eq!(result.selected_bytes, 3);
    assert_eq!(
        result.omitted,
        [OmittedDiagnosticItem {
            canonical_id: "b".to_string(),
            reason: OmittedDiagnosticReason::SerializedByteLimit,
        }]
    );
    assert!(result.overflowed);
    Ok(())
}

#[test]
fn bounded_walk_matches_independent_widened_accounting() {
    let payloads = [0usize, 1, 2, usize::MAX];
    let mut walks = 0usize;
    let mut observed_reasons = BTreeSet::new();
    let mut maximum_budget_omitted_bytes = false;
    for document_limit in 1..=3 {
        for workspace_limit in document_limit..=3 {
            for byte_limit in [1, 2, usize::MAX] {
                let budget = DiagnosticBudget {
                    max_items_per_document: document_limit,
                    max_items_per_workspace_response: workspace_limit,
                    max_serialized_bytes: byte_limit,
                    max_inline_detail_bytes: 1,
                };
                // Four steps, two documents, four representative payload sizes.
                for encoding in 0usize..4096 {
                    let mut choices = encoding;
                    let mut counts = [0usize; 2];
                    let mut selected = 0usize;
                    let mut exact = 0u128;
                    let mut cursor = BudgetCursor::default();
                    for step in 0..4 {
                        let doc = choices % 2;
                        choices /= 2;
                        let payload = payloads[choices % 4];
                        choices /= 4;
                        let detail = step;
                        let next_exact = exact + payload as u128;
                        let reason = if counts[doc] == document_limit {
                            Some((
                                OmittedDiagnosticReason::DocumentItemLimit,
                                DiagnosticOverflowReason::DocumentItemLimit,
                            ))
                        } else if selected == workspace_limit {
                            Some((
                                OmittedDiagnosticReason::WorkspaceItemLimit,
                                DiagnosticOverflowReason::WorkspaceItemLimit,
                            ))
                        } else if selected > 0 && next_exact > byte_limit as u128 {
                            Some((
                                OmittedDiagnosticReason::SerializedByteLimit,
                                DiagnosticOverflowReason::SerializedByteLimit,
                            ))
                        } else {
                            None
                        };
                        let expected = match reason {
                            Some((reason, overflow)) => BudgetDecision::Omit { reason, overflow },
                            None => BudgetDecision::Select {
                                byte_overflow: next_exact > byte_limit as u128,
                                inline_detail_omitted: detail > 1,
                            },
                        };
                        let decision = cursor.decide(&budget, counts[doc], payload, detail);
                        assert_eq!(
                            decision, expected,
                            "budget={budget:?}, encoding={encoding}, step={step}"
                        );
                        match decision {
                            BudgetDecision::Select { .. } => {
                                selected += 1;
                                counts[doc] += 1;
                                exact = next_exact;
                                cursor.commit(payload);
                            }
                            BudgetDecision::Omit { reason, .. } => {
                                observed_reasons.insert(reason);
                                maximum_budget_omitted_bytes |= byte_limit == usize::MAX
                                    && reason == OmittedDiagnosticReason::SerializedByteLimit;
                            }
                        }
                        assert_eq!(cursor.selected_len, selected);
                        assert_eq!(cursor.selected_bytes as u128, exact);
                        assert_eq!(selected, counts[0] + counts[1]);
                        assert!(counts.iter().all(|count| *count <= document_limit));
                        assert!(selected <= workspace_limit);
                        assert!(selected <= 1 || exact <= byte_limit as u128);
                    }
                    walks += 1;
                }
            }
        }
    }
    assert_eq!(walks, 73_728);
    assert_eq!(
        observed_reasons,
        BTreeSet::from([
            OmittedDiagnosticReason::DocumentItemLimit,
            OmittedDiagnosticReason::WorkspaceItemLimit,
            OmittedDiagnosticReason::SerializedByteLimit,
        ])
    );
    assert!(maximum_budget_omitted_bytes);
}
