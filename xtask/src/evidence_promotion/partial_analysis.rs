//! Corpus guard for typed partial analysis, independent of preview advisories.

use super::{
    evidence_promotion_required_assertion_string, evidence_promotion_required_assertion_u64,
};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExpectedPartialAnalysis {
    limitation_kind: String,
    producer_stage: String,
    path: String,
    affected_items: u64,
    detail_contains: String,
}

pub(crate) fn parse(
    case_id: &str,
    index: usize,
    assertion: &Value,
) -> Result<ExpectedPartialAnalysis, String> {
    let string = |key| evidence_promotion_required_assertion_string(case_id, index, assertion, key);
    Ok(ExpectedPartialAnalysis {
        limitation_kind: string("limitation_kind")?,
        producer_stage: string("producer_stage")?,
        path: string("path")?,
        affected_items: evidence_promotion_required_assertion_u64(
            case_id,
            index,
            assertion,
            "affected_items",
        )?,
        detail_contains: string("detail_contains")?,
    })
}

pub(crate) fn matches(expected: &ExpectedPartialAnalysis, report: &Value) -> bool {
    let envelope = &report["analysis_outcome"];
    let outcome = &envelope["outcome"];
    envelope["analysis_complete"].as_bool() == Some(false)
        && outcome["kind"].as_str() == Some("partial_with_limitations")
        && outcome["limitations"].as_array().is_some_and(|limits| {
            limits
                .iter()
                .filter(|limit| {
                    limit["kind"].as_str() == Some(expected.limitation_kind.as_str())
                        && limit["producer_stage"].as_str()
                            == Some(expected.producer_stage.as_str())
                        && limit["path"].as_str() == Some(expected.path.as_str())
                        && limit["affected_items"].as_u64() == Some(expected.affected_items)
                        && limit["bounded_detail"]
                            .as_str()
                            .is_some_and(|detail| detail.contains(&expected.detail_contains))
                })
                .count()
                == 1
        })
}

#[cfg(test)]
mod tests {
    use super::super::{
        evidence_promotion_case_assertions, evidence_promotion_semantic_violations,
    };
    use serde_json::{Value, json};

    fn corpus_case() -> Result<Value, String> {
        let corpus: Value = serde_json::from_str(include_str!(
            "../../../fixtures/evidence-promotion-honesty-corpus/corpus.json"
        ))
        .map_err(|error| error.to_string())?;
        corpus["cases"]
            .as_array()
            .and_then(|cases| {
                cases
                    .iter()
                    .find(|case| case["id"] == "typescript_nested_private_method_partial")
            })
            .cloned()
            .ok_or_else(|| "nested private corpus case missing".to_string())
    }

    #[test]
    fn nested_private_corpus_rejects_lost_partial_outcome_despite_preview_advisory()
    -> Result<(), String> {
        let case = corpus_case()?;
        let assertions = evidence_promotion_case_assertions(&case)?;
        let report: Value = serde_json::from_str(include_str!(
            "../../../fixtures/typescript_nested_private_method_partial/expected/check.json"
        ))
        .map_err(|error| error.to_string())?;
        let violations = |report: &Value| {
            evidence_promotion_semantic_violations(
                "typescript_nested_private_method_partial",
                None,
                &assertions,
                report,
                None,
                false,
            )
        };
        assert!(violations(&report).is_empty());
        assert!(
            report["preview_languages"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
        );
        for (pointer, replacement) in [
            ("/analysis_outcome/analysis_complete", json!(true)),
            (
                "/analysis_outcome/outcome/kind",
                json!("complete_no_findings"),
            ),
            ("/analysis_outcome/outcome/limitations", json!([])),
            (
                "/analysis_outcome/outcome/limitations/0/affected_items",
                json!(0),
            ),
            (
                "/analysis_outcome/outcome/limitations/0/affected_items",
                json!(2),
            ),
            (
                "/analysis_outcome/outcome/limitations/0/path",
                json!("src/other.ts"),
            ),
            (
                "/analysis_outcome/outcome/limitations/0/kind",
                json!("other"),
            ),
            (
                "/analysis_outcome/outcome/limitations/0/producer_stage",
                json!("other"),
            ),
            (
                "/analysis_outcome/outcome/limitations/0/bounded_detail",
                json!("other"),
            ),
        ] {
            let mut mutated = report.clone();
            let field = mutated
                .pointer_mut(pointer)
                .ok_or_else(|| pointer.to_string())?;
            *field = replacement;
            assert!(
                violations(&mutated)
                    .iter()
                    .any(|v| v.contains("expected_partial_analysis")),
                "mutation escaped: {pointer}"
            );
        }
        let mut complete = report;
        complete["analysis_outcome"]["analysis_complete"] = json!(true);
        complete["analysis_outcome"]["outcome"]["kind"] = json!("complete_no_findings");
        complete["analysis_outcome"]["outcome"]["limitations"] = json!([]);
        assert!(
            violations(&complete)
                .iter()
                .any(|v| v.contains("expected_partial_analysis"))
        );
        Ok(())
    }

    #[test]
    fn partial_analysis_assertion_requires_every_identity_and_count_field() -> Result<(), String> {
        let case = corpus_case()?;
        let assertion = case["assertions"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["type"] == "expected_partial_analysis")
            })
            .ok_or_else(|| "typed partial assertion missing".to_string())?;
        super::parse("nested", 0, assertion)?;
        for field in [
            "limitation_kind",
            "producer_stage",
            "path",
            "affected_items",
            "detail_contains",
        ] {
            let mut missing = assertion.clone();
            missing
                .as_object_mut()
                .ok_or_else(|| "assertion is not an object".to_string())?
                .remove(field);
            if super::parse("nested", 0, &missing).is_ok() {
                return Err(format!("missing {field} accepted"));
            }
        }
        Ok(())
    }
}
