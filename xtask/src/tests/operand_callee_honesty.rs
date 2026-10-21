//! Reject false-credit re-blessing independently of the public-API producer.

use crate::EvidencePromotionSemanticAssertion as Assertion;

#[test]
fn later_binding_contract_rejects_false_credit_and_vacuity() -> Result<(), String> {
    let report: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/evidence-promotion-honesty-corpus/reports/operand-callee-later-binding.json"
    ))
    .map_err(|error| error.to_string())?;
    let assertions = [
        Assertion::MustNotReportClean,
        Assertion::ExpectedFindingCount { count: 1 },
        Assertion::MustNotPromote,
        Assertion::ExpectedClass {
            class: "weakly_exposed".to_string(),
        },
    ];
    let judge = |report: &serde_json::Value| {
        crate::evidence_promotion_semantic_violations_scoped(
            "rust_operand_callee_later_binding",
            None,
            &assertions,
            report,
            None,
            false,
            Some("field_construction"),
        )
    };
    assert!(
        judge(&report).is_empty(),
        "expected contract must be admitted"
    );
    let mut false_credit = report.clone();
    false_credit["findings"][0]["classification"] = "exposed".into();
    assert!(
        judge(&false_credit)
            .iter()
            .any(|error| error.contains("must_not_promote"))
    );
    let mut empty = report;
    empty["findings"] = serde_json::json!([]);
    empty["summary"]["findings"] = 0.into();
    assert!(
        judge(&empty)
            .iter()
            .any(|error| error.contains("matched no findings"))
    );
    Ok(())
}
