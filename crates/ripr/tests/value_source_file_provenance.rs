//! Public-path source identity controls for #7805.
//! Source identity is metadata and cannot strengthen oracle admission.

use ripr::{CheckInput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const OWNER: &str = r#"pub fn coverage_summary_receipt(present: bool) -> serde_json::Value {
    let text: Option<String> = if present {
        Some(String::new())
    } else {
        None
    };
    serde_json::json!({"status": if text.is_some() { "collected" } else { "not_collected" }})
}
#[test]
fn json_indexed_receipt() {
    let summary = coverage_summary_receipt(false);
    assert_eq!(summary["status"], "not_collected");
}
#[cfg(test)]
mod decoy;
"#;

const PATCH: &str = r#"diff --git a/src/lib.rs b/src/lib.rs
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -2,6 +2,6 @@
     let text: Option<String> = if present {
         Some(String::new())
     } else {
-        Some(String::new())
+        None
     };
     serde_json::json!({"status": if text.is_some() { "collected" } else { "not_collected" }})
"#;

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    #[cfg(unix)]
    fn native_collision() -> Result<Self, String> {
        let fixture = Self::new("assert_eq!(summary[\"status\"], \"not_collected\");")?;
        let assertion = "assert_eq!(summary[\"status\"], \"not_collected\");";
        let source = std::fs::read_to_string(fixture.root.join("src/decoy.rs"))
            .map_err(|error| error.to_string())?
            .replace(
                "    let shared_context = text;",
                "    let summary = serde_json::json!({\"status\": \"not_collected\"});",
            );
        assert_eq!(
            source.lines().nth(11),
            Some(format!("    {assertion}").as_str())
        );
        std::fs::remove_file(fixture.root.join("src/decoy.rs"))
            .map_err(|error| error.to_string())?;
        std::fs::create_dir_all(fixture.root.join("src/odd")).map_err(|error| error.to_string())?;
        fixture.write(
            "src/odd\\receipt.rs",
            &source.replace(
                "unrelated_coverage_summary_receipt_text",
                "backslash_coverage_summary_receipt_text",
            ),
        )?;
        fixture.write(
            "src/odd/receipt.rs",
            &source.replace(
                "unrelated_coverage_summary_receipt_text",
                "directory_coverage_summary_receipt_text",
            ),
        )?;
        fixture.write("src/lib.rs", &OWNER.replace("mod decoy;", "#[path = \"odd\\\\receipt.rs\"]\nmod backslash;\n#[cfg(test)]\n#[path = \"odd/receipt.rs\"]\nmod directory;"))?;
        Ok(fixture)
    }

    fn new(decoy_assertion: &str) -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-7805-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
        let fixture = Self { root };
        fixture.write(
            "Cargo.toml",
            "[package]\nname = \"source-provenance\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dependencies]\nserde_json = \"1\"\n",
        )?;
        fixture.write("src/lib.rs", OWNER)?;
        fixture.write("diff.patch", PATCH)?;
        // Keep both real and unrelated assertions at exactly line 12.
        let decoy = format!(
            "#[test]\nfn unrelated_coverage_summary_receipt_text() {{\n    let text = \"## UB Ledger Context\";\n    let shared_context = text;\n\n\n\n\n\n\n\n    {decoy_assertion}\n}}\n"
        );
        assert_eq!(
            decoy.lines().nth(11),
            Some(format!("    {decoy_assertion}").as_str())
        );
        fixture.write("src/decoy.rs", &decoy)?;
        Ok(fixture)
    }

    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        std::fs::write(self.root.join(path), text).map_err(|error| error.to_string())
    }

    fn finding(&self) -> Result<serde_json::Value, String> {
        let output = check_workspace(CheckInput {
            root: self.root.clone(),
            diff_file: Some(self.root.join("diff.patch")),
            mode: Mode::Draft,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        let selected = output
            .findings
            .iter()
            .filter(|finding| finding.probe.expression == "None")
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1, "the changed None must be analyzed");
        assert_eq!(selected[0].class, ExposureClass::WeaklyExposed);
        let rendered = ripr::output::json::render(&output);
        let report: serde_json::Value =
            serde_json::from_str(&rendered).map_err(|error| error.to_string())?;
        let finding = report["findings"]
            .as_array()
            .and_then(|findings| {
                findings
                    .iter()
                    .find(|finding| finding["probe"]["expression"] == "None")
            })
            .cloned()
            .ok_or_else(|| "changed None is missing from rendered findings".to_owned())?;
        assert_eq!(
            finding["observed_values"],
            finding["activation"]["observed_values"]
        );
        let sarif: serde_json::Value =
            serde_json::from_str(&ripr::render_check(&output, &OutputFormat::Sarif)?)
                .map_err(|error| error.to_string())?;
        let sarif_values = sarif["runs"][0]["results"]
            .as_array()
            .and_then(|results| {
                results
                    .iter()
                    .find(|result| result["properties"]["finding_id"] == finding["id"])
            })
            .map(|result| &result["properties"]["observed_values"])
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "changed None is missing from SARIF".to_owned())?;
        let json_values = finding["observed_values"]
            .as_array()
            .ok_or_else(|| "JSON observed values are not an array".to_owned())?;
        assert_eq!(sarif_values.len(), json_values.len());
        for (sarif, json) in sarif_values.iter().zip(json_values) {
            assert_eq!(sarif["source_file"], json["source_file"]);
            assert_eq!(sarif["line"], json["line"]);
            assert_eq!(sarif["value"], json["value"]);
            assert_eq!(sarif["context"], json["context"]);
        }
        Ok(finding)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn sources_for_value(finding: &serde_json::Value, value: &str) -> Vec<serde_json::Value> {
    finding["observed_values"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|fact| fact["line"] == 12 && fact["value"] == value)
        .map(|fact| fact["source_file"].clone())
        .collect()
}

#[test]
fn a_same_line_decoy_cannot_replace_the_owner_values_source_file() -> Result<(), String> {
    let fixture = Fixture::new("assert!(shared_context.contains(\"## UB Ledger Context\"));")?;
    let finding = fixture.finding()?;
    assert_eq!(finding["ripr"]["discriminate"]["state"], "weak");
    assert_eq!(
        sources_for_value(&finding, "\"not_collected\""),
        vec![serde_json::json!("src/lib.rs")]
    );
    assert_eq!(
        sources_for_value(&finding, "\"## UB Ledger Context\""),
        vec![serde_json::json!("src/decoy.rs")]
    );
    Ok(())
}

#[test]
fn removing_only_the_decoy_assertion_preserves_owner_source_identity() -> Result<(), String> {
    let fixture = Fixture::new("let _ = shared_context.contains(\"## UB Ledger Context\");")?;
    let finding = fixture.finding()?;
    assert_eq!(
        sources_for_value(&finding, "\"not_collected\""),
        vec![serde_json::json!("src/lib.rs")]
    );
    assert!(sources_for_value(&finding, "\"## UB Ledger Context\"").is_empty());
    assert_eq!(finding["ripr"]["discriminate"]["state"], "weak");
    Ok(())
}

#[test]
fn identical_assertion_text_in_two_files_keeps_two_source_identities() -> Result<(), String> {
    let fixture = Fixture::new("assert_eq!(summary[\"status\"], \"not_collected\");")?;
    let decoy = std::fs::read_to_string(fixture.root.join("src/decoy.rs"))
        .map_err(|error| error.to_string())?
        .replace(
            "    let shared_context = text;",
            "    let summary = serde_json::json!({\"status\": \"not_collected\"});",
        );
    fixture.write("src/decoy.rs", &decoy)?;
    let finding = fixture.finding()?;
    let mut sources = sources_for_value(&finding, "\"not_collected\"");
    sources.sort_by_key(serde_json::Value::to_string);
    assert_eq!(
        sources,
        vec![
            serde_json::json!("src/decoy.rs"),
            serde_json::json!("src/lib.rs")
        ]
    );
    assert_eq!(finding["ripr"]["discriminate"]["state"], "weak");
    Ok(())
}

#[test]
fn a_renamed_decoy_keeps_its_own_indexed_file() -> Result<(), String> {
    let fixture = Fixture::new("assert!(shared_context.contains(\"## UB Ledger Context\"));")?;
    std::fs::rename(
        fixture.root.join("src/decoy.rs"),
        fixture.root.join("src/ledger_decoy.rs"),
    )
    .map_err(|error| error.to_string())?;
    fixture.write(
        "src/lib.rs",
        &OWNER.replace("mod decoy;", "mod ledger_decoy;"),
    )?;
    let finding = fixture.finding()?;
    assert_eq!(
        sources_for_value(&finding, "\"not_collected\""),
        vec![serde_json::json!("src/lib.rs")]
    );
    assert_eq!(
        sources_for_value(&finding, "\"## UB Ledger Context\""),
        vec![serde_json::json!("src/ledger_decoy.rs")]
    );
    assert_eq!(finding["ripr"]["discriminate"]["state"], "weak");
    Ok(())
}

#[test]
fn repeated_and_relocated_fixtures_keep_relative_source_identity() -> Result<(), String> {
    let first = Fixture::new("assert!(shared_context.contains(\"## UB Ledger Context\"));")?;
    let second = Fixture::new("assert!(shared_context.contains(\"## UB Ledger Context\"));")?;
    assert_ne!(first.root, second.root);
    let first_finding = first.finding()?;
    assert_eq!(first_finding, first.finding()?);
    let second_finding = second.finding()?;
    for value in ["\"not_collected\"", "\"## UB Ledger Context\""] {
        assert_eq!(
            sources_for_value(&first_finding, value),
            sources_for_value(&second_finding, value)
        );
    }
    Ok(())
}

#[test]
fn legacy_facts_keep_unknown_source_identity_without_changing_serialization() -> Result<(), String>
{
    let legacy = serde_json::json!({
        "line": 12,
        "text": "assert_eq!(summary[\"status\"], \"not_collected\");",
        "value": "\"not_collected\"",
        "context": "assertion_argument",
    });
    let fact: ripr::domain::ValueFact =
        serde_json::from_value(legacy.clone()).map_err(|error| error.to_string())?;
    assert!(fact.source_file.is_none());
    assert_eq!(
        serde_json::to_value(&fact).map_err(|error| error.to_string())?,
        legacy
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn native_backslash_and_directory_assertions_keep_distinct_public_sources() -> Result<(), String> {
    let fixture = Fixture::native_collision()?;
    let finding = fixture.finding()?;
    assert_eq!(finding["ripr"]["discriminate"]["state"], "weak");
    assert_eq!(
        sources_for_value(&finding, "\"not_collected\""),
        vec![
            serde_json::json!("src/lib.rs"),
            serde_json::json!("src/odd%5Creceipt.rs"),
            serde_json::json!("src/odd/receipt.rs"),
        ]
    );
    assert_eq!(finding, fixture.finding()?);
    let relocated = Fixture::native_collision()?.finding()?;
    assert_eq!(
        sources_for_value(&finding, "\"not_collected\""),
        sources_for_value(&relocated, "\"not_collected\"")
    );
    // An unambiguous filename is an independent representation control. Only
    // the file spelling changes; test names, bodies, lines and calls stay put.
    std::fs::rename(
        fixture.root.join("src/odd\\receipt.rs"),
        fixture.root.join("src/plain_receipt.rs"),
    )
    .map_err(|error| error.to_string())?;
    let owner = std::fs::read_to_string(fixture.root.join("src/lib.rs"))
        .map_err(|error| error.to_string())?;
    fixture.write(
        "src/lib.rs",
        &owner.replace("odd\\\\receipt.rs", "plain_receipt.rs"),
    )?;
    let unambiguous = fixture.finding()?;
    for field in [
        "classification",
        "ripr",
        "oracle_kind",
        "oracle_strength",
        "recommended_next_step",
        "suggested_next_action",
    ] {
        assert!(
            finding.get(field).is_some(),
            "control must inspect real {field}"
        );
        assert_eq!(
            finding[field], unambiguous[field],
            "filename identity must preserve {field}"
        );
    }
    let oracles =
        |report: &serde_json::Value| -> Result<Vec<serde_json::Value>, String> {
            let tests = report["related_tests"]
                .as_array()
                .ok_or("missing related tests")?;
            assert_eq!(tests.len(), 3, "must retain all three related tests");
            Ok(tests.iter().map(|test| serde_json::json!({
            "name": test["name"], "oracle": test["oracle"],
            "oracle_kind": test["oracle_kind"], "oracle_strength": test["oracle_strength"],
        })).collect())
        };
    assert_eq!(oracles(&finding)?, oracles(&unambiguous)?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn native_collision_fixture_compiles_and_runs_all_three_real_tests() -> Result<(), String> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let fixture = Fixture::native_collision()?;
    let stdout = fixture.root.join("runtime.stdout");
    let stderr = fixture.root.join("runtime.stderr");
    let mut command = std::process::Command::new(env!("CARGO"));
    command
        .args(["test", "--offline", "--lib"])
        .current_dir(&fixture.root)
        .env("CARGO_TARGET_DIR", fixture.root.join("target"))
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).map_err(|error| error.to_string())?)
        .stderr(std::fs::File::create(&stderr).map_err(|error| error.to_string())?);
    let mut child =
        ripr::process_owner::OwnedProcess::spawn(command).map_err(|error| error.to_string())?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if started.elapsed() >= Duration::from_mins(2) {
            child.terminate_tree()?;
            return Err("native collision fixture exceeded 120-second runtime budget".to_owned());
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = std::fs::read_to_string(stdout).map_err(|error| error.to_string())?;
    let errors = std::fs::read_to_string(stderr).map_err(|error| error.to_string())?;
    assert!(status.success(), "fixture failed: {output}\n{errors}");
    assert!(
        output.contains("3 passed; 0 failed"),
        "must execute all three real tests: {output}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn removing_each_native_collision_assertion_removes_only_its_source_value() -> Result<(), String> {
    for (removed, retained) in [
        ("src/odd\\receipt.rs", "src/odd/receipt.rs"),
        ("src/odd/receipt.rs", "src/odd%5Creceipt.rs"),
    ] {
        let fixture = Fixture::native_collision()?;
        let before = fixture.finding()?;
        let source = std::fs::read_to_string(fixture.root.join(removed))
            .map_err(|error| error.to_string())?;
        fixture.write(
            removed,
            &source.replace(
                "assert_eq!(summary[\"status\"], \"not_collected\");",
                "let _ = summary[\"status\"] == \"not_collected\";",
            ),
        )?;
        let after = fixture.finding()?;
        assert_eq!(
            sources_for_value(&after, "\"not_collected\""),
            vec![serde_json::json!("src/lib.rs"), serde_json::json!(retained)]
        );
        for field in [
            "classification",
            "ripr",
            "recommended_next_step",
            "suggested_next_action",
        ] {
            assert!(
                before.get(field).is_some(),
                "control must inspect real {field}"
            );
            assert_eq!(
                before[field], after[field],
                "metadata/removal must preserve {field}"
            );
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn quoted_percent_and_newline_native_origin_survives_public_projection() -> Result<(), String> {
    let fixture = Fixture::new("assert!(shared_context.contains(\"## UB Ledger Context\"));")?;
    let native = "src/quoted\"%\nreceipt.rs";
    std::fs::rename(fixture.root.join("src/decoy.rs"), fixture.root.join(native))
        .map_err(|error| error.to_string())?;
    fixture.write(
        "src/lib.rs",
        &OWNER.replace(
            "mod decoy;",
            "#[path = \"quoted\\\"%\\nreceipt.rs\"]\nmod decoy;",
        ),
    )?;
    assert_eq!(
        sources_for_value(&fixture.finding()?, "\"## UB Ledger Context\""),
        vec![serde_json::json!("src/quoted\"%25\nreceipt.rs")]
    );
    Ok(())
}
