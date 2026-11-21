//! Validation for the bounded Python real-mutant calibration receipt (#12901).
//!
//! The receipt is produced by `benchmarks/mutation_spot_check/run_python_calibration.py`,
//! which executes each listed mutant against the authored subject's own
//! pytest suite and scores the same edit through `ripr check --diff`.
//! These tests re-derive the summary from the per-mutant rows so a hand edit
//! cannot inflate precision/recall, and they pin the honesty rules: at least
//! ten mutants across at least two operator families, runtime outcomes only
//! `caught`/`missed`/`unscoreable`, and unknown/weak verdicts never scored.

#[cfg(test)]
mod tests {
    use serde_json::Value;

    fn receipt() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../benchmarks/mutation_spot_check/python-calibration-receipt.json"
        );
        let text = std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path}: {err}"));
        serde_json::from_str(&text).expect("receipt is valid JSON")
    }

    fn rows(receipt: &Value) -> &[Value] {
        receipt["mutants"].as_array().expect("mutants array")
    }

    fn rate(num: u64, den: u64) -> Option<f64> {
        (den > 0).then(|| num as f64 / den as f64)
    }

    #[test]
    fn receipt_has_at_least_ten_mutants_across_two_operators() {
        let receipt = receipt();
        let rows = rows(&receipt);
        assert!(rows.len() >= 10, "got {} mutants", rows.len());
        let operators: std::collections::BTreeSet<&str> = rows
            .iter()
            .filter_map(|r| r["operator_family"].as_str())
            .collect();
        assert!(operators.len() >= 2, "operators: {operators:?}");
        let subjects: std::collections::BTreeSet<&str> =
            rows.iter().filter_map(|r| r["subject"].as_str()).collect();
        assert!(subjects.len() >= 2, "subjects: {subjects:?}");
        assert_eq!(
            receipt["summary"]["mutants_total"].as_u64(),
            Some(rows.len() as u64)
        );
    }

    #[test]
    fn runtime_outcomes_are_never_static_classes() {
        let receipt = receipt();
        for row in rows(&receipt) {
            let outcome = row["runtime_outcome"].as_str().unwrap_or("");
            assert!(
                ["caught", "missed", "unscoreable"].contains(&outcome),
                "{} has runtime outcome {outcome}",
                row["id"]
            );
            // A static classification must not be copied into the runtime field.
            assert_ne!(outcome, row["ripr_classification"].as_str().unwrap_or("\0"));
        }
        assert_eq!(receipt["summary"]["unknowns_are_successes"], false);
    }

    #[test]
    fn summary_is_rederived_from_rows_and_unknowns_are_unscored() {
        let receipt = receipt();
        let rows = rows(&receipt);
        let scored: Vec<&Value> = rows.iter().filter(|r| r["scored"] == true).collect();
        // Only strongly_gripped / ungripped claim pairs may be scored.
        for row in &scored {
            assert!(
                ["strongly_gripped", "ungripped"]
                    .contains(&row["grip_class"].as_str().unwrap_or("")),
                "{} scored with grip {}",
                row["id"],
                row["grip_class"]
            );
            assert!(
                ["caught", "missed"].contains(&row["runtime_outcome"].as_str().unwrap_or("")),
                "{} scored with runtime {}",
                row["id"],
                row["runtime_outcome"]
            );
        }
        for row in rows {
            if row["grip_class"]
                .as_str()
                .is_some_and(|g| ["weakly_gripped", "activation_unknown", "opaque"].contains(&g))
            {
                assert_eq!(
                    row["scored"], false,
                    "{} unknown/weak must be unscored",
                    row["id"]
                );
                assert_eq!(row["agreement"], "unscored");
            }
        }
        let count = |grip: &str, outcome: &str| {
            scored
                .iter()
                .filter(|r| r["grip_class"] == grip && r["runtime_outcome"] == outcome)
                .count() as u64
        };
        let (tp, fp, fn_, tn) = (
            count("strongly_gripped", "caught"),
            count("strongly_gripped", "missed"),
            count("ungripped", "caught"),
            count("ungripped", "missed"),
        );
        let summary = &receipt["summary"];
        assert_eq!(
            summary["scored_claim_pairs"].as_u64(),
            Some(scored.len() as u64)
        );
        assert_eq!(
            summary["confusion_scored_only"]["strong_caught"].as_u64(),
            Some(tp)
        );
        assert_eq!(
            summary["confusion_scored_only"]["strong_missed_overclaim"].as_u64(),
            Some(fp)
        );
        assert_eq!(
            summary["confusion_scored_only"]["ungripped_caught_false_gap"].as_u64(),
            Some(fn_)
        );
        assert_eq!(
            summary["confusion_scored_only"]["ungripped_missed"].as_u64(),
            Some(tn)
        );
        let check_rate =
            |key: &str, num: u64, den: u64| match (summary[key].as_f64(), rate(num, den)) {
                (None, None) => {}
                (Some(got), Some(want)) => {
                    assert!((got - want).abs() < 1e-9, "{key}: {got} != {want}")
                }
                other => panic!("{key} mismatch: {other:?} vs recomputed {num}/{den}"),
            };
        check_rate("discriminator_precision", tp, tp + fp);
        check_rate("missed_recall_strict", tn, tn + fp);
        check_rate("caught_recall", tp, tp + fn_);
    }

    #[test]
    fn baselines_passed_so_runtime_outcomes_are_meaningful() {
        let receipt = receipt();
        let baselines = receipt["baselines"].as_object().expect("baselines");
        assert!(!baselines.is_empty());
        for (name, baseline) in baselines {
            assert_eq!(baseline["passed"], true, "baseline {name} did not pass");
        }
    }
}
