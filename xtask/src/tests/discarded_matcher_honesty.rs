//! Independent observer contracts for #5713, evaluated by the required honesty owner.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::EvidencePromotionSemanticAssertion as Assertion;

const CORPUS: &str = "fixtures/evidence-promotion-honesty-corpus/corpus.json";
const REPORTS: &str = "fixtures/evidence-promotion-honesty-corpus/discarded-matcher-reports";
const SHAPES: [(&str, &str, &str, &str); 7] = [
    ("bare-wildcard", "reachable_unrevealed", "unknown", "none"),
    ("bare-exact", "reachable_unrevealed", "unknown", "none"),
    ("bare-guarded", "reachable_unrevealed", "unknown", "none"),
    ("bound-exact", "reachable_unrevealed", "unknown", "none"),
    (
        "wrapped-wildcard",
        "weakly_exposed",
        "relational_check",
        "weak",
    ),
    ("wrapped-exact", "exposed", "exact_value", "strong"),
    ("wrapped-guarded", "exposed", "exact_value", "strong"),
];

fn workspace() -> Result<&'static Path, String> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask must have a workspace parent".to_string())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn expectations(class: &str, kind: &str, strength: &str) -> Vec<Assertion> {
    let mut assertions = vec![
        Assertion::MustNotReportClean,
        Assertion::ExpectedFindingCount { count: 1 },
        Assertion::ExpectedClass {
            class: class.to_string(),
        },
        Assertion::ExpectedOracle {
            kind: kind.to_string(),
            strength: strength.to_string(),
        },
        Assertion::ExpectedRelatedTest {
            name: "observes_score".to_string(),
            file: "src/lib.rs".to_string(),
            line: 8,
            kind: kind.to_string(),
            strength: strength.to_string(),
            relation_reason: None,
        },
    ];
    if class == "exposed" {
        assertions.push(Assertion::MustPromote);
    } else {
        assertions.push(Assertion::MustNotPromote);
        assertions.push(Assertion::MaximumClass {
            class: class.to_string(),
        });
    }
    assertions
}

#[test]
fn discarded_matcher_reports_reject_false_credit_missed_credit_and_vacuity() -> Result<(), String> {
    // score(1)==2 is established by the source/patch controls, not inferred
    // from these captured predictions. A bare boolean cannot fail a test;
    // asserting exact/guarded wrappers can, while wildcard wrappers stay weak.
    let mut judged = 0;
    for (shape, class, kind, strength) in SHAPES {
        for variant in ["original", "wrong"] {
            let id = format!("{shape}-{variant}");
            let path = workspace()?.join(REPORTS).join(&id).join("check.json");
            let original_bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
            let report = read_json(&path)?;
            assert_eq!(report["findings"].as_array().map(Vec::len), Some(1), "{id}");
            let assertions = expectations(class, kind, strength);
            let judge = |candidate: &Value| {
                crate::evidence_promotion_semantic_violations(
                    &id,
                    Some(REPORTS),
                    &assertions,
                    candidate,
                    None,
                    false,
                )
            };
            assert!(judge(&report).is_empty(), "{id}: baseline must be accepted");

            let mut wrong_credit = report.clone();
            let (wrong_class, wrong_kind, wrong_strength, required_violation) =
                if class == "exposed" {
                    ("reachable_unrevealed", "unknown", "none", "must_promote")
                } else {
                    ("exposed", "exact_value", "strong", "must_not_promote")
                };
            wrong_credit["findings"][0]["classification"] = wrong_class.into();
            wrong_credit["findings"][0]["oracle_kind"] = wrong_kind.into();
            wrong_credit["findings"][0]["oracle_strength"] = wrong_strength.into();
            wrong_credit["findings"][0]["related_tests"][0]["oracle_kind"] = wrong_kind.into();
            wrong_credit["findings"][0]["related_tests"][0]["oracle_strength"] =
                wrong_strength.into();
            let violations = judge(&wrong_credit);
            assert!(
                violations
                    .iter()
                    .any(|line| line.contains(required_violation))
                    && violations
                        .iter()
                        .any(|line| line.contains("expected_class")),
                "{id}: wrong credit must fail its semantic contract, got {violations:?}"
            );

            if class == "reachable_unrevealed" {
                // MustNotPromote alone permits weak credit. The absent observer
                // contract must reject that escape as well as exposed/strong.
                let mut weak_credit = report.clone();
                weak_credit["findings"][0]["classification"] = "weakly_exposed".into();
                weak_credit["findings"][0]["oracle_kind"] = "relational_check".into();
                weak_credit["findings"][0]["oracle_strength"] = "weak".into();
                weak_credit["findings"][0]["related_tests"][0]["oracle_kind"] =
                    "relational_check".into();
                weak_credit["findings"][0]["related_tests"][0]["oracle_strength"] = "weak".into();
                let violations = judge(&weak_credit);
                assert!(
                    violations
                        .iter()
                        .any(|line| line.contains("expected_class"))
                        && violations
                            .iter()
                            .any(|line| line.contains("expected_oracle")),
                    "{id}: weak false credit must fail exact absent-oracle expectations, got {violations:?}"
                );
            }

            let mut empty = report.clone();
            empty["findings"] = serde_json::json!([]);
            empty["summary"]["findings"] = 0.into();
            let violations = judge(&empty);
            assert!(
                violations
                    .iter()
                    .any(|line| line
                        .contains("`expected_finding_count` requires 1 finding(s), found 0")),
                "{id}: an empty re-bless must fail the nonempty denominator, got {violations:?}"
            );

            let mut removed_consumer = report.clone();
            removed_consumer["findings"][0]["related_tests"] = serde_json::json!([]);
            let violations = judge(&removed_consumer);
            assert!(
                violations
                    .iter()
                    .any(|line| line.contains("expected_related_test")
                        && line.contains("src/lib.rs:8 observes_score")),
                "{id}: removal of the intended observer must fail, got {violations:?}"
            );
            assert_eq!(
                std::fs::read(&path).map_err(|error| error.to_string())?,
                original_bytes
            );
            judged += 1;
        }
    }
    assert_eq!(judged, 14, "the authored static denominator is explicit");
    Ok(())
}

fn validate(corpus: &Path) -> Result<Vec<String>, String> {
    let mut violations = Vec::new();
    // This is the exact validator called by check-evidence-promotion-honesty,
    // including manifest parsing, typed assertions and source_report routing.
    crate::validate_evidence_promotion_honesty_corpus_at(corpus, &mut violations)?;
    Ok(violations)
}

#[test]
fn discarded_matcher_canonical_gate_rejects_dishonest_and_empty_reblesses() -> Result<(), String> {
    let workspace = workspace()?;
    let root = super::temp_dir("discarded-matcher-honesty-5713");
    let result = (|| {
        let mut corpus = read_json(&workspace.join(CORPUS))?;
        let cases = corpus["cases"]
            .as_array_mut()
            .ok_or("missing canonical cases")?;
        let mut registrations = 0;
        let selected_id = "rust_discarded_matcher_bare_exact_wrong";
        let selected_report = workspace.join(REPORTS).join("bare-exact-wrong/check.json");
        let original_bytes = std::fs::read(&selected_report).map_err(|error| error.to_string())?;
        let controlled_report = root.join("controlled-check.json");
        std::fs::write(&controlled_report, &original_bytes).map_err(|error| error.to_string())?;
        let mut found_selected = false;
        for case in cases {
            if case["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("rust_discarded_matcher_"))
            {
                registrations += 1;
            }
            // Absolute input routes isolate this copied manifest from process
            // cwd without altering any captured producer JSON.
            for field in ["source_fixture", "source_report", "external_patch"] {
                if let Some(path) = case[field].as_str() {
                    let absolute: PathBuf = workspace.join(path);
                    case[field] = absolute.to_string_lossy().into_owned().into();
                }
            }
            if case["id"] == selected_id {
                assert_eq!(
                    case["source_report"],
                    selected_report.to_string_lossy().as_ref()
                );
                case["source_report"] = controlled_report.to_string_lossy().into_owned().into();
                found_selected = true;
            }
        }
        assert_eq!(
            registrations, 14,
            "file presence alone is not corpus registration"
        );
        assert!(
            found_selected,
            "the intended canonical gate subject must exist"
        );
        let manifest = root.join("corpus.json");
        std::fs::write(
            &manifest,
            serde_json::to_vec_pretty(&corpus).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        assert!(
            validate(&manifest)?.is_empty(),
            "the complete canonical baseline must pass"
        );

        let mut dishonest: Value =
            serde_json::from_slice(&original_bytes).map_err(|error| error.to_string())?;
        dishonest["findings"][0]["classification"] = "exposed".into();
        dishonest["findings"][0]["oracle_kind"] = "exact_value".into();
        dishonest["findings"][0]["oracle_strength"] = "strong".into();
        dishonest["findings"][0]["related_tests"][0]["oracle_kind"] = "exact_value".into();
        dishonest["findings"][0]["related_tests"][0]["oracle_strength"] = "strong".into();
        std::fs::write(
            &controlled_report,
            serde_json::to_vec_pretty(&dishonest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let violations = validate(&manifest)?;
        assert!(
            violations.iter().any(|line| line.contains(selected_id)
                && line.contains("must_not_promote")
                && line.contains("promoted to exposed")),
            "the required validator must reject the dishonest golden, got {violations:?}"
        );

        let mut weak_credit: Value =
            serde_json::from_slice(&original_bytes).map_err(|error| error.to_string())?;
        weak_credit["findings"][0]["classification"] = "weakly_exposed".into();
        weak_credit["findings"][0]["oracle_kind"] = "relational_check".into();
        weak_credit["findings"][0]["oracle_strength"] = "weak".into();
        weak_credit["findings"][0]["related_tests"][0]["oracle_kind"] = "relational_check".into();
        weak_credit["findings"][0]["related_tests"][0]["oracle_strength"] = "weak".into();
        std::fs::write(
            &controlled_report,
            serde_json::to_vec_pretty(&weak_credit).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let violations = validate(&manifest)?;
        assert!(
            violations
                .iter()
                .any(|line| line.contains(selected_id) && line.contains("expected_class"))
                && violations
                    .iter()
                    .any(|line| line.contains(selected_id) && line.contains("expected_oracle")),
            "the required validator must reject weak false credit, got {violations:?}"
        );

        let mut empty: Value =
            serde_json::from_slice(&original_bytes).map_err(|error| error.to_string())?;
        empty["findings"] = serde_json::json!([]);
        empty["summary"]["findings"] = 0.into();
        std::fs::write(
            &controlled_report,
            serde_json::to_vec_pretty(&empty).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let violations = validate(&manifest)?;
        assert!(
            violations.iter().any(|line| line.contains(selected_id)
                && line.contains("`expected_finding_count` requires 1 finding(s), found 0")),
            "the required validator must reject empty findings, got {violations:?}"
        );

        std::fs::write(&controlled_report, &original_bytes).map_err(|error| error.to_string())?;
        assert!(
            validate(&manifest)?.is_empty(),
            "restored canonical baseline must pass"
        );
        assert_eq!(
            std::fs::read(&selected_report).map_err(|error| error.to_string())?,
            original_bytes
        );
        Ok(())
    })();
    let cleanup = std::fs::remove_dir_all(&root).map_err(|error| error.to_string());
    result.and(cleanup)
}
