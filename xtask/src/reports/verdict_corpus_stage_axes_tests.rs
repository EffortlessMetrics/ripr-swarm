//! #12922: tagging a case must not manufacture emitted-class or runtime coverage.
use super::*;

#[test]
fn stage_axes_match_pinned_classes_and_partial_runtime_truth() -> Result<(), String> {
    for language in ["python", "typescript"] {
        let dir = crate::dogfood::repo_rooted_fixture_path(&format!(
            "fixtures/{language}-verdict-corpus"
        ));
        let corpus = load_corpus(&dir)?;
        let report = expected_report(&dir)?;
        for class in [
            "reachable_unrevealed",
            "infection_unknown",
            "propagation_unknown",
        ] {
            let waived = language == "python" || class == "propagation_unknown";
            let tagged: Vec<_> = corpus
                .cases
                .iter()
                .filter(|case| case.axes.iter().any(|axis| axis == class))
                .collect();
            if waived {
                assert!(
                    tagged.is_empty(),
                    "{language}: waived {class} is not coverage"
                );
                assert!(
                    report.rows.iter().all(|row| !row
                        .observed_classifications
                        .iter()
                        .any(|value| value == class)),
                    "{language}: {class} now emits; replace its waiver with runtime cases and a positive floor"
                );
            } else {
                assert!(
                    tagged.len() >= 2,
                    "{language}: {class} needs two runtime cases"
                );
                for case in tagged {
                    let row = report
                        .rows
                        .iter()
                        .find(|row| row.case_id == case.case_id)
                        .ok_or_else(|| format!("missing expected row {}", case.case_id))?;
                    assert!(
                        row.observed_classifications
                            .iter()
                            .any(|value| value == class),
                        "{}: tag cannot substitute for the emitted class",
                        case.case_id
                    );
                    assert!(
                        !row.observed_classifications
                            .iter()
                            .any(|value| value == "exposed"),
                        "{}: unknown/unrevealed case must not be promoted",
                        case.case_id
                    );
                    assert!(
                        case.truth
                            .mutants
                            .iter()
                            .any(|mutant| mutant.outcome == MutantOutcome::TestsPassed),
                        "{}: negative control needs a runtime survivor",
                        case.case_id
                    );
                }
            }
        }
        if language == "python" {
            let partial: Vec<_> = corpus
                .cases
                .iter()
                .filter(|case| {
                    case.axes
                        .iter()
                        .any(|axis| axis == "partially_discriminated")
                })
                .collect();
            assert!(
                partial.len() >= 15,
                "Python needs fifteen runtime partial cases"
            );
            for case in partial {
                assert_eq!(
                    case.truth.state,
                    TruthState::PartiallyDiscriminated,
                    "{}: partial tag must follow runtime truth",
                    case.case_id
                );
                assert!(
                    case.truth
                        .mutants
                        .iter()
                        .any(|mutant| mutant.outcome == MutantOutcome::TestsFailed)
                );
                assert!(
                    case.truth
                        .mutants
                        .iter()
                        .any(|mutant| mutant.outcome == MutantOutcome::TestsPassed)
                );
            }
        }
    }
    Ok(())
}
