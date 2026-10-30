//! #10994: the import cleanup must not remove the production corpus gate.

use super::validate_assistant_loop_health_fixture_corpus_at;
use serde_json::Value;
use std::fs;
use std::path::Path;

const CORPUS_DIR: &str = "fixtures/boundary_gap/expected/assistant-loop-health";
const KIND_ERROR: &str = "assistant-loop-health corpus kind must be assistant_loop_health_corpus";

#[test]
fn assistant_loop_health_stays_in_the_production_fixture_contract_gate() -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    // Reuse the existing isolated-cwd helper and full-fixture setup used by
    // the benchmark-oracle command tests. Never mutate the checkout's corpus.
    crate::tests::with_temp_cwd("assistant-loop-health-production-gate", |root| {
        crate::tests::copy_dir_recursive(&repo.join("fixtures"), &root.join("fixtures"))?;
        let base = root.join(CORPUS_DIR);
        let path = base.join("corpus.json");
        let original = fs::read(&path).map_err(|error| error.to_string())?;
        let mut corpus: Value =
            serde_json::from_slice(&original).map_err(|error| error.to_string())?;
        assert!(
            corpus
                .get("cases")
                .and_then(Value::as_array)
                .is_some_and(|cases| !cases.is_empty()),
            "the committed corpus must exercise real cases"
        );
        let kind = corpus
            .get_mut("kind")
            .ok_or_else(|| "committed corpus has no kind".to_string())?;
        assert_eq!(kind.as_str(), Some("assistant_loop_health_corpus"));
        *kind = Value::String("wrong_corpus_kind".to_string());

        let mut violations = Vec::new();
        validate_assistant_loop_health_fixture_corpus_at(&base, &mut violations)?;
        assert!(violations.is_empty(), "valid corpus: {violations:?}");
        crate::fixture_contracts::check_fixture_contracts()?;
        let report_path = root.join("target/ripr/reports/fixture-contracts.md");
        let valid_report = fs::read_to_string(&report_path).map_err(|error| error.to_string())?;
        assert!(!valid_report.contains(KIND_ERROR));

        fs::write(
            &path,
            serde_json::to_vec(&corpus).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        validate_assistant_loop_health_fixture_corpus_at(&base, &mut violations)?;
        assert_eq!(
            violations,
            vec![KIND_ERROR.to_string()],
            "only kind changed from the independently accepted corpus; unexpected diagnostics: {violations:?}"
        );

        // The direct _at result is not passed to the production gate. Its own
        // call must detect the mutation and emit a fresh, specific report.
        fs::remove_file(&report_path).map_err(|error| error.to_string())?;
        let result = crate::fixture_contracts::check_fixture_contracts();
        // Removing the production validator call makes this assertion fail,
        // even though the separate _at invocation above still rejects it.
        assert!(result.is_err(), "production gate accepted a malformed corpus");
        let invalid_report = fs::read_to_string(&report_path).map_err(|error| error.to_string())?;
        assert!(invalid_report.contains(KIND_ERROR), "{invalid_report}");

        fs::write(&path, original).map_err(|error| error.to_string())?;
        violations.clear();
        validate_assistant_loop_health_fixture_corpus_at(&base, &mut violations)?;
        assert!(violations.is_empty(), "restored corpus: {violations:?}");
        fs::remove_file(&report_path).map_err(|error| error.to_string())?;
        crate::fixture_contracts::check_fixture_contracts()?;
        let restored_report = fs::read_to_string(&report_path).map_err(|error| error.to_string())?;
        assert!(!restored_report.contains(KIND_ERROR));
        Ok(())
    })
}
