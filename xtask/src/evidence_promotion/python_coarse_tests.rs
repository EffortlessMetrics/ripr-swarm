//! Challenge the actual coarse-projection fixture contracts without rewriting reports.

use super::*;

#[test]
fn python_coarse_contracts_reject_exposed_and_empty_reports() -> Result<(), String> {
    let root = crate::repo_root()?;
    let corpus: Value = serde_json::from_str(&read_text_lossy(
        &root.join(EVIDENCE_PROMOTION_HONESTY_CORPUS),
    )?)
    .map_err(|error| error.to_string())?;
    let cases = corpus["cases"].as_array().ok_or("missing corpus cases")?;
    for (id, owner) in [
        ("python_coarse_sign", "python:coarse/fees.py::sign_fee"),
        (
            "python_coarse_residue",
            "python:coarse/fees.py::residue_fee",
        ),
        (
            "python_coarse_text_length",
            "python:coarse/fees.py::length_fee",
        ),
    ] {
        let case = cases
            .iter()
            .find(|case| case["id"] == id)
            .ok_or_else(|| format!("missing coarse control {id}"))?;
        let source = case["source_fixture"]
            .as_str()
            .ok_or("missing source fixture")?;
        let assertions = case["assertions"]
            .as_array()
            .ok_or("missing assertions")?
            .iter()
            .enumerate()
            .map(|(index, assertion)| evidence_promotion_parse_assertion(id, index, assertion))
            .collect::<Result<Vec<_>, _>>()?;
        let fixture = root.join(source);
        let mut report: Value =
            serde_json::from_str(&read_text_lossy(&fixture.join("expected/check.json"))?)
                .map_err(|error| error.to_string())?;
        let human = read_text_lossy(&fixture.join("expected/human-full.txt"))?;
        let judge = |report: &Value| {
            evidence_promotion_semantic_violations(
                id,
                Some(source),
                &assertions,
                report,
                Some(&human),
                true,
            )
        };
        assert!(
            assertions.iter().any(|assertion| matches!(
                assertion,
                EvidencePromotionSemanticAssertion::ExpectedFindingOwner { owner: pinned }
                    if pinned.as_str() == owner
            )),
            "{id}: case must pin the changed owner {owner}"
        );
        assert!(judge(&report).is_empty(), "{id}: {:?}", judge(&report));
        let findings = report["findings"]
            .as_array_mut()
            .ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "{id}: expected one changed owner");
        findings[0]["classification"] = serde_json::json!("exposed");
        let promoted = judge(&report);
        assert!(
            promoted
                .iter()
                .any(|error| error.contains("must_not_promote")),
            "{id}: false promotion escaped the independent assertion: {promoted:?}"
        );
        assert!(
            promoted
                .iter()
                .any(|error| error.contains("expected_class")),
            "{id}: false class escaped the literal class pin: {promoted:?}"
        );
        // The weak finding must belong to the changed owner: a wrong-owner
        // finding with the same class and count is not an equivalent report.
        let findings = report["findings"]
            .as_array_mut()
            .ok_or("missing findings")?;
        findings[0]["classification"] = serde_json::json!("weakly_exposed");
        findings[0]["probe"]["owner"] = serde_json::json!("python:coarse/fees.py::other_fee");
        let wrong_owner = judge(&report);
        assert!(
            wrong_owner
                .iter()
                .any(|error| error.contains("expected_finding_owner")),
            "{id}: wrong-owner finding escaped the owner pin: {wrong_owner:?}"
        );
        report["findings"] = serde_json::json!([]);
        let empty = judge(&report);
        assert!(
            empty
                .iter()
                .any(|error| error.contains("expected_finding_count")),
            "{id}: missing finding escaped the count pin: {empty:?}"
        );
    }
    Ok(())
}
