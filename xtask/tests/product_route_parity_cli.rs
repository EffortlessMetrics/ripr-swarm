//! Exercise the corpus boundary through the real xtask command.
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::{Value, json};

#[path = "common/xtask_bin.rs"]
mod xtask_bin;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("product-parity-cli-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(Self(root))
    }

    fn run(&self, body: &str) -> Result<Output, String> {
        let corpus = self.0.join("corpus.json");
        std::fs::write(&corpus, body).map_err(|e| e.to_string())?;
        xtask_bin::output(|command| {
            command
                .current_dir(&self.0)
                .arg("check-product-route-parity")
                .arg("--corpus")
                .arg(&corpus)
                .arg("--json");
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn corpus_text() -> Result<String, String> {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../fixtures/product-route-parity/corpus.json"),
    )
    .map_err(|e| e.to_string())
}

#[test]
fn unknown_and_duplicate_terminal_keys_are_rejected_at_the_cli() -> Result<(), String> {
    let original = corpus_text()?;
    for key in [
        "product.unknown.required",
        "",
        "product.rust.clippyy",
        "product.evidence.pr_evidenc",
    ] {
        let mut corpus: Value = serde_json::from_str(&original).map_err(|e| e.to_string())?;
        corpus["subjects"][0]["terminals"][key] = json!("fail");
        let fixture = Fixture::new()?;
        let output = fixture.run(&serde_json::to_string(&corpus).map_err(|e| e.to_string())?)?;
        assert!(!output.status.success(), "unknown key {key:?} was accepted");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains("unknown terminal gate"), "{error}");
        assert!(error.contains("full-route-missing-selectors"), "{error}");
        assert!(
            !fixture
                .0
                .join("target/ripr/reports/product-route-parity.json")
                .exists()
        );
    }
    let needle = "\"product.rust.workspace_tests\": \"not_proven\",";
    let duplicate = original.replacen(needle, "\"product.rust.workspace_tests\": \"not_proven\",\n        \"product.rust.workspace_tests\": \"fail\",", 1);
    assert_ne!(
        duplicate, original,
        "fixture must contain the key being duplicated"
    );
    let fixture = Fixture::new()?;
    let output = fixture.run(&duplicate)?;
    assert!(
        !output.status.success(),
        "duplicate terminal key was accepted"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("duplicate terminal gate `product.rust.workspace_tests`")
    );
    let duplicated_field =
        original.replacen("\"terminals\": {", "\"terminals\": {}, \"terminals\": {", 1);
    assert_ne!(duplicated_field, original);
    let fixture = Fixture::new()?;
    let output = fixture.run(&duplicated_field)?;
    assert!(
        !output.status.success(),
        "duplicate terminals field was accepted"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate field `terminals`"));
    Ok(())
}

#[test]
fn canonical_atomic_composite_and_omitted_terminals_remain_supported() -> Result<(), String> {
    let original = corpus_text()?;
    for omit_terminals in [false, true] {
        let mut corpus: Value = serde_json::from_str(&original).map_err(|e| e.to_string())?;
        if omit_terminals {
            corpus["subjects"][0]
                .as_object_mut()
                .ok_or("subject is not an object")?
                .remove("terminals");
        } else {
            corpus["subjects"][0]["terminals"]["product.rust.formatting"] = json!("fail");
            corpus["subjects"][0]["terminals"]["product.evidence.pr_evidence"] =
                json!("unavailable");
        }
        let fixture = Fixture::new()?;
        let output = fixture.run(&serde_json::to_string(&corpus).map_err(|e| e.to_string())?)?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())?;
        assert_eq!(report["proof_equivalence_verdict"], "not_proven");
        assert!(
            report["limitations"]
                .as_array()
                .ok_or("missing limitations")?
                .iter()
                .any(|item| item
                    .as_str()
                    .is_some_and(|text| text.contains("not recorded by this report")))
        );
        assert_eq!(
            report["subjects"][0]["matched_propositions"].as_u64(),
            Some(repo_policy::product_gate_plan::product_gate_definitions().len() as u64)
        );
    }
    Ok(())
}
