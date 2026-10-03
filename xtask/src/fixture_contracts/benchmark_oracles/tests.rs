//! Synthetic contract models only. No native commands run here, and these
//! temporary objects must never be retained as real upstream observations.

use std::path::PathBuf;

use serde_json::json;
use sha2::{Digest, Sha256};

use super::*;

const TEST_ID: &str = "synthetic::expected_empty";
const CORRECTED: &str = "fn synthetic() {\n    assert!(boundary());\n    assert!(ascii_boundary());\n    assert!(neighbor());\n}\n";
const ORIGINAL: &str = "fn synthetic() {\n    assert!(!boundary());\n    assert!(!ascii_boundary());\n    assert!(neighbor());\n}\n";
const WEAK: &str = "fn synthetic() {\n    assert!(neighbor());\n}\n";

struct SyntheticEvidence {
    root: PathBuf,
    case: Value,
    key: Value,
    pairing: Value,
}

impl Drop for SyntheticEvidence {
    fn drop(&mut self) {
        if let Ok(()) = fs::remove_dir_all(&self.root) {}
    }
}

impl SyntheticEvidence {
    fn file(&self, name: &str, bytes: &[u8]) -> Result<Value, String> {
        let path = self.root.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(path, bytes).map_err(|error| error.to_string())?;
        Ok(
            json!({"path": name, "bytes": bytes.len(), "sha256": format!("{:x}", Sha256::digest(bytes))}),
        )
    }

    fn persist(&mut self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.key).map_err(|error| error.to_string())?;
        self.case["semantic_oracle"]["answer_key"] = self.file("key.json", &bytes)?;
        self.pairing["answer_key_sha256"] =
            self.case["semantic_oracle"]["answer_key"]["sha256"].clone();
        self.persist_pairing()
    }

    fn persist_pairing(&mut self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.pairing).map_err(|error| error.to_string())?;
        self.case["semantic_oracle"]["native_pairing"] = self.file("pairing.json", &bytes)?;
        Ok(())
    }

    fn new() -> Result<Self, String> {
        let mut fixture = Self {
            root: crate::tests::temp_dir("synthetic-semantic-oracle-contract"),
            case: json!({"id": "synthetic-oracle-contract", "semantic_oracle": {"status": "valid", "variant": "corrected"}}),
            key: json!({}),
            pairing: json!({}),
        };
        fixture.key = json!({
            "case_id": "synthetic-oracle-contract", "claim": "Synthetic independent contract model",
            "test_id": TEST_ID, "package": "synthetic", "test_source_path": "src/lib.rs",
            "independent_review": {"disposition": "accepted", "reviewer": "synthetic reviewer", "rationale": "Synthetic expected-value review, not real execution"},
            "basis": [{"url": "https://example.invalid/synthetic-contract", "artifact": fixture.file("basis.txt", b"Synthetic independent expected behavior")?}],
            "sources": {"fixed": fixture.file("fixed.rs", b"fn boundary() -> bool { true }")?, "broken": fixture.file("broken.rs", b"fn boundary() -> bool { false }")?},
            "tests": {"corrected": fixture.file("corrected.rs", CORRECTED.as_bytes())?, "original": fixture.file("original.rs", ORIGINAL.as_bytes())?, "weak": fixture.file("weak.rs", WEAK.as_bytes())?},
            "boundary_assertions": [
                {"corrected": "assert!(boundary());", "original": "assert!(!boundary());"},
                {"corrected": "assert!(ascii_boundary());", "original": "assert!(!ascii_boundary());"}
            ],
            "failure_lines": {"broken_corrected": 2, "fixed_original": 2}
        });
        let lock = fixture.file("Cargo.lock", b"# Synthetic lock model\nversion = 4\n")?;
        let mut observations = Vec::new();
        for (variant, source, test, passes) in PAIRS {
            let verdict = if *passes { "ok" } else { "FAILED" };
            let failure = if *passes {
                String::new()
            } else {
                let expression = if *test == "original" {
                    "!boundary()"
                } else {
                    "boundary()"
                };
                format!(
                    "failures:\n---- {TEST_ID} stdout ----\nthread '{TEST_ID}' panicked at src/lib.rs:2:5:\nassertion failed: {expression}\n"
                )
            };
            let stdout = format!(
                "running 1 test\ntest {TEST_ID} ... {verdict}\n{failure}test result: {verdict}. {} passed; {} failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n",
                u64::from(*passes),
                u64::from(!*passes)
            );
            let source_sha = &fixture.key["sources"][*source]["sha256"];
            let test_sha = &fixture.key["tests"][*test]["sha256"];
            observations.push(json!({
                "variant": variant, "phase": "completed", "timed_out": false, "compile_failed": false, "process_failed": false,
                "source_sha256": source_sha, "source_sha256_after": source_sha,
                "test_sha256": test_sha, "test_sha256_after": test_sha,
                "lock_sha256": lock["sha256"], "lock_sha256_after": lock["sha256"],
                "counts": {"intended": 1, "discovered": 1, "selected": 1, "executed": 1, "passed": u64::from(*passes), "failed": u64::from(!*passes), "ignored": 0},
                "test_ids": [TEST_ID], "exit_code": if *passes { 0 } else { 101 },
                "runner": {"version": "synthetic cargo", "compiler_version": "synthetic rustc", "cwd": "synthetic/workspace", "target_dir": "synthetic/target", "build_dir": "synthetic/build", "artifact_sha256": "a".repeat(64), "executable": "/synthetic/bin/cargo", "executable_sha256": "b".repeat(64), "argv": ["/synthetic/bin/cargo", "test", "--locked", "--offline", "--manifest-path", "Cargo.toml", "-p", "synthetic", "--lib", TEST_ID, "--", "--exact"]},
                "stdout": fixture.file(&format!("{variant}.stdout"), stdout.as_bytes())?,
                "stderr": fixture.file(&format!("{variant}.stderr"), b"")?
            }));
        }
        fixture.pairing = json!({"case_id": "synthetic-oracle-contract", "lock": lock, "observations": observations});
        fixture.persist()?;
        Ok(fixture)
    }

    fn validate(&self) -> Result<&'static str, String> {
        validate_case(&self.root, &self.case)
    }
}

#[test]
fn benchmark_semantic_oracle_legacy_absent_is_unreviewed() -> Result<(), String> {
    assert_eq!(
        validate_case(Path::new("."), &json!({"id": "legacy"}))?,
        "unreviewed"
    );
    assert_eq!(
        validate_case(
            Path::new("."),
            &json!({"semantic_oracle": {"status": "unreviewed"}})
        )?,
        "unreviewed"
    );
    for value in [
        Value::Null,
        json!({}),
        json!({"status": "unexpected"}),
        json!({"status": "valid"}),
    ] {
        assert!(validate_case(Path::new("."), &json!({"semantic_oracle": value})).is_err());
    }
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_polarity_is_separate_from_discrimination() -> Result<(), String> {
    let mut fixture = SyntheticEvidence::new()?;
    assert_eq!(fixture.validate()?, "valid");
    fixture.case["semantic_oracle"]["variant"] = json!("original");
    assert!(fixture.validate().is_err());
    fixture.case["semantic_oracle"]["status"] = json!("invalid");
    assert_eq!(fixture.validate()?, "invalid");
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_requires_independent_basis_and_exact_removal() -> Result<(), String> {
    for field in [
        "basis",
        "self_basis",
        "review",
        "polarity",
        "weak_neighbor",
        "failure_line",
    ] {
        let mut fixture = SyntheticEvidence::new()?;
        match field {
            "basis" => fixture.key["basis"] = json!([]),
            "self_basis" => {
                fixture.key["basis"][0]["artifact"] = fixture.key["tests"]["corrected"].clone()
            }
            "review" => fixture.key["independent_review"]["rationale"] = json!(" "),
            "polarity" => {
                fixture.key["boundary_assertions"][0]["original"] = json!("assert!(boundary());")
            }
            "weak_neighbor" => {
                fixture.key["tests"]["weak"] = fixture.file("weak.rs", b"fn synthetic() {}\n")?
            }
            _ => fixture.key["failure_lines"]["broken_corrected"] = json!(4),
        }
        fixture.persist()?;
        assert!(fixture.validate().is_err(), "accepted {field}");
    }
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_rejects_missing_repeated_and_wrong_pairing() -> Result<(), String> {
    for field in ["missing", "duplicate", "wrong_case", "wrong_answer_key"] {
        let mut fixture = SyntheticEvidence::new()?;
        match field {
            "missing" => {
                if let Some(rows) = fixture.pairing["observations"].as_array_mut() {
                    let _ = rows.pop();
                }
            }
            "duplicate" => {
                fixture.pairing["observations"][5] = fixture.pairing["observations"][0].clone()
            }
            "wrong_case" => fixture.pairing["case_id"] = json!("neighbor"),
            _ => {}
        }
        fixture.persist()?;
        if field == "wrong_answer_key" {
            fixture.pairing["answer_key_sha256"] = json!("f".repeat(64));
            fixture.persist_pairing()?;
        }
        assert!(fixture.validate().is_err(), "accepted {field}");
    }
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_rejects_zero_subjects_instrument_and_stale_inputs()
-> Result<(), String> {
    for (field, value) in [
        ("phase", json!("NOT_RUN")),
        ("timed_out", json!(true)),
        ("compile_failed", json!(true)),
        ("process_failed", json!(true)),
        ("exit_code", json!(1)),
        ("test_ids", json!(["neighbor"])),
        ("source_sha256_after", json!("stale")),
        ("test_sha256", json!("stale")),
        ("lock_sha256_after", json!("stale")),
    ] {
        let mut fixture = SyntheticEvidence::new()?;
        fixture.pairing["observations"][1][field] = value;
        fixture.persist()?;
        assert!(fixture.validate().is_err(), "accepted {field}");
    }
    for field in ["intended", "discovered", "selected", "executed"] {
        let mut fixture = SyntheticEvidence::new()?;
        fixture.pairing["observations"][0]["counts"][field] = json!(0);
        fixture.persist()?;
        assert!(fixture.validate().is_err(), "accepted zero {field}");
    }
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_rejects_wrong_failure_and_incomplete_output() -> Result<(), String> {
    for output in [
        "setup failed",
        "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out; finished in 0.01s\n",
    ] {
        let mut fixture = SyntheticEvidence::new()?;
        fixture.pairing["observations"][1]["stdout"] =
            fixture.file("bad.stdout", output.as_bytes())?;
        fixture.persist()?;
        assert!(fixture.validate().is_err());
    }
    for (from, to) in [
        (
            "assertion failed: boundary()",
            "assertion failed: neighbor()",
        ),
        ("src/lib.rs:2:", "src/lib.rs:4:"),
        ("finished in 0.01s", "unfinished"),
    ] {
        let mut fixture = SyntheticEvidence::new()?;
        let original = retained_text(&fixture.root, &fixture.pairing["observations"][1]["stdout"])?;
        fixture.pairing["observations"][1]["stdout"] =
            fixture.file("bad.stdout", original.replace(from, to).as_bytes())?;
        fixture.persist()?;
        assert!(fixture.validate().is_err(), "accepted {from}");
    }
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_rejects_artifact_drift_and_escaping_paths() -> Result<(), String> {
    let mut fixture = SyntheticEvidence::new()?;
    fs::write(fixture.root.join("basis.txt"), "changed basis")
        .map_err(|error| error.to_string())?;
    assert!(fixture.validate().is_err());
    for path in [
        "../outside",
        "/outside",
        "nested/../outside",
        "nested\\outside",
    ] {
        fixture.case["semantic_oracle"]["answer_key"]["path"] = json!(path);
        assert!(fixture.validate().is_err(), "accepted {path}");
    }
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_command_discloses_absent_corpus() -> Result<(), String> {
    crate::tests::with_temp_cwd("semantic-oracle-command-disclosure", |root| {
        crate::fixture_contracts::check_fixture_contracts()?;
        let report = fs::read_to_string(root.join("target/ripr/reports/fixture-contracts.md"))
            .map_err(|error| error.to_string())?;
        assert!(report.contains("Benchmark semantic-oracle scope"));
        assert!(report.contains("NOT_ESTABLISHED"));
        assert!(!report.contains("valid=0"));
        Ok(())
    })
}

#[test]
fn benchmark_semantic_oracle_unavailable_corpus_has_no_zero_count() -> Result<(), String> {
    let fixture = SyntheticEvidence::new()?;
    let mut violations = Vec::new();
    let disclosure =
        super::super::general_validators::validate_evidence_quality_benchmark_fixture_corpus_at(
            &fixture.root.join("missing-corpus.json"),
            &mut violations,
        )?;
    assert!(!violations.is_empty());
    assert!(disclosure.intro.contains("NOT_ESTABLISHED"));
    assert!(disclosure.items.is_empty());
    Ok(())
}

#[test]
fn benchmark_semantic_oracle_production_reader_discloses_rejected_and_unreviewed()
-> Result<(), String> {
    let mut fixture = SyntheticEvidence::new()?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut corpus = read_json(&repo.join("fixtures/evidence-quality-benchmark/corpus.json"))?;
    let rows = corpus["cases"]
        .as_array_mut()
        .ok_or_else(|| "missing cases".to_string())?;
    let total = rows.len();
    let first = rows.first_mut().ok_or_else(|| "no cases".to_string())?;
    first["id"] = fixture.case["id"].clone();
    first["semantic_oracle"] = fixture.case["semantic_oracle"].clone();
    let path = fixture.root.join("corpus.json");
    for valid in [true, false] {
        if !valid {
            fixture.pairing["observations"][1]["counts"]["executed"] = json!(0);
            fixture.persist()?;
            corpus["cases"][0]["semantic_oracle"] = fixture.case["semantic_oracle"].clone();
        }
        fs::write(
            &path,
            serde_json::to_vec(&corpus).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let mut violations = Vec::new();
        let disclosure = super::super::general_validators::validate_evidence_quality_benchmark_fixture_corpus_at(&path, &mut violations)?;
        assert_eq!(violations.is_empty(), valid);
        let body = crate::policy_report_body(
            &crate::PolicyReportSpec {
                report_file: "fixture-contracts.md",
                check: "check-fixture-contracts",
                why_it_matters: "Synthetic fixture acceptance contract",
                fix_kind: crate::FixKind::AuthorDecisionRequired,
                recommended_fixes: &[],
                rerun_command: "cargo xtask check-fixture-contracts",
                exception_template: None,
            },
            &violations,
            &[disclosure],
        );
        assert!(body.contains("Benchmark semantic-oracle scope"));
        assert!(body.contains(&format!(
            "unreviewed={} (legacy absent={})",
            total - 1,
            total - 1
        )));
        assert!(body.contains(if valid {
            "valid=1, invalid=0"
        } else {
            "valid=0, invalid=0"
        }));
        assert!(body.contains(if valid { "rejected=0" } else { "rejected=1" }));
        assert!(body.contains("do not change static discrimination"));
    }
    Ok(())
}
