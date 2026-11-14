//! Reject false-credit and empty Perl threshold projections independently of the adapter.

use crate::EvidencePromotionSemanticAssertion as Assertion;

#[test]
fn spaced_receiver_contract_rejects_false_credit_and_vacuity() -> Result<(), String> {
    let report: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/evidence-promotion-honesty-corpus/reports/perl-threshold-spaced-receiver-expected-projection.json"
    ))
    .map_err(|error| error.to_string())?;
    let assertions = [
        Assertion::MustNotReportClean,
        Assertion::ExpectedFindingCount { count: 1 },
        Assertion::MustNotPromote,
        Assertion::ExpectedClass {
            class: "static_unknown".to_string(),
        },
    ];
    let judge = |report: &serde_json::Value| {
        crate::evidence_promotion_semantic_violations_scoped(
            "perl_threshold_spaced_receiver",
            None,
            &assertions,
            report,
            None,
            false,
            Some("return_value"),
        )
    };
    let admitted = judge(&report);
    assert!(
        admitted.is_empty(),
        "the expected contract must be admitted: {admitted:?}"
    );

    let mut false_credit = report.clone();
    false_credit["findings"][0]["classification"] = "exposed".into();
    let false_credit_violations = judge(&false_credit);
    assert!(
        false_credit_violations.iter().any(|error| {
            error.contains("promoted to exposed") && error.contains("`must_not_promote`")
        }),
        "false credit must fail the promotion invariant: {false_credit_violations:?}"
    );

    let mut wrong_class = report.clone();
    wrong_class["findings"][0]["classification"] = "weakly_exposed".into();
    let wrong_class_violations = judge(&wrong_class);
    assert!(
        wrong_class_violations.iter().any(|error| {
            error.contains("classification `weakly_exposed`, expected_class `static_unknown`")
        }),
        "the exact unknown class must be enforced: {wrong_class_violations:?}"
    );

    let mut empty = report;
    empty["findings"] = serde_json::json!([]);
    empty["summary"]["findings"] = 0.into();
    let empty_violations = judge(&empty);
    assert!(
        empty_violations
            .iter()
            .any(|error| error.contains("`probe_family` `return_value` matched no findings")),
        "an empty result must fail the selected-subject check: {empty_violations:?}"
    );
    assert!(
        empty_violations
            .iter()
            .any(|error| error.contains("`expected_finding_count` requires 1 finding(s), found 0")),
        "an empty result must fail the finding-count check: {empty_violations:?}"
    );
    Ok(())
}
