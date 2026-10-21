//! #8389: the slice-cardinality witness fact names the test file that holds
//! the witnessing call. `ValueFact::file` is serde-skipped, so this reads the
//! typed `check_workspace` output rather than JSON.

use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn slice_table_witness_names_its_test_file() -> Result<(), String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-witness-file-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let scratch = Scratch(root);
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust_slice_table_boundary");
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::copy(
        fixture.join("input/Cargo.toml"),
        scratch.0.join("Cargo.toml"),
    )
    .map_err(|error| error.to_string())?;
    // Move the changed boundary to length 2, which the table does supply, so
    // the boundary is a witnessed input instead of a missing one.
    let source = std::fs::read_to_string(fixture.join("input/src/lib.rs"))
        .map_err(|error| error.to_string())?;
    let source = source.replacen("samples.len() == 0", "samples.len() == 2", 1);
    assert!(
        source.contains("samples.len() == 2"),
        "fixture predicate edited"
    );
    let diff_file = scratch.0.join("change.diff");
    std::fs::write(
        &diff_file,
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -18,1 +18,1 @@\n-    if samples.len() == 0 {\n+    if samples.len() == 2 {\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(scratch.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let output = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(diff_file),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let witnesses = output
        .findings
        .iter()
        .flat_map(|finding| finding.activation.observed_values.iter())
        .filter(|fact| fact.text.contains("slice table cardinality"))
        .collect::<Vec<_>>();
    assert!(
        !witnesses.is_empty(),
        "the slice witness fact is produced: {:?}",
        output
            .findings
            .iter()
            .map(|f| (
                f.activation
                    .observed_values
                    .iter()
                    .filter(|v| v.value.contains("=="))
                    .collect::<Vec<_>>(),
                &f.activation.missing_discriminators
            ))
            .collect::<Vec<_>>()
    );
    for fact in witnesses {
        assert_eq!(
            fact.file.as_deref(),
            Some(Path::new("src/lib.rs")),
            "{fact:?}"
        );
    }
    Ok(())
}

/// #8389: exercise real classification before either consumer applies its cap.
#[cfg(feature = "lang-python")]
#[test]
fn cross_file_boundary_facts_reach_human_and_json_windows() -> Result<(), String> {
    let scratch = Scratch(
        std::env::temp_dir().join(format!("ripr-cross-file-values-{}", std::process::id())),
    );
    std::fs::create_dir_all(scratch.0.join("src/pkg")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(scratch.0.join("tests")).map_err(|e| e.to_string())?;
    for (path, text) in [
        ("pyproject.toml", "[project]\nname = \"value-facts\"\n"),
        ("src/pkg/__init__.py", ""),
        (
            "src/pkg/mod.py",
            "def price(x):\n    if x > 5:\n        return 1\n    return 0\n",
        ),
        (
            "change.diff",
            "diff --git a/src/pkg/mod.py b/src/pkg/mod.py\n--- a/src/pkg/mod.py\n+++ b/src/pkg/mod.py\n@@ -2 +2 @@\n-    if x >= 5:\n+    if x > 5:\n",
        ),
    ] {
        std::fs::write(scratch.0.join(path), text).map_err(|e| e.to_string())?;
    }
    let header = "from pkg.mod import price\n\ndef test_price():\n";
    let mut config = ripr::config::RiprConfig::default();
    config.languages.enabled = vec![ripr::domain::LanguageId::Python];
    for (calls, second_file, expected_total) in [(12, false, 12), (12, true, 13), (32, true, 33)] {
        let body = (10..10 + calls)
            .map(|value| format!("    assert price({value}) == 1\n"))
            .collect::<String>();
        std::fs::write(scratch.0.join("tests/test_a.py"), format!("{header}{body}"))
            .map_err(|e| e.to_string())?;
        if second_file {
            std::fs::write(
                scratch.0.join("tests/test_b.py"),
                format!("{header}    assert price(10) == 1\n"),
            )
            .map_err(|e| e.to_string())?;
        }
        let output = ripr::app::check_workspace_with_config(
            CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(scratch.0.join("change.diff")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                include_unchanged_tests: true,
                ..CheckInput::default()
            },
            &config,
        )?;
        let predicates = output
            .findings
            .iter()
            .filter(|finding| finding.probe.family == ripr::domain::ProbeFamily::Predicate)
            .collect::<Vec<_>>();
        assert_eq!(
            predicates.len(),
            1,
            "one changed Python predicate: {output:?}"
        );
        let finding = predicates[0];
        let facts = &finding.activation.observed_values;
        assert_eq!(facts.len(), expected_total, "{facts:?}");
        let repeated_locations = facts
            .iter()
            .filter(|fact| fact.value == "x = 10")
            .map(|fact| (fact.file.as_deref(), fact.line))
            .collect::<Vec<_>>();
        let mut expected = vec![(Some(Path::new("tests/test_a.py")), 4)];
        if second_file {
            expected.push((Some(Path::new("tests/test_b.py")), 4));
        }
        assert_eq!(repeated_locations, expected);

        let human = ripr::output::human::render_finding(finding);
        let source_lines = human
            .lines()
            .filter(|line| line.contains("source function argument value"))
            .collect::<Vec<_>>();
        assert_eq!(source_lines.len(), 8, "{human}");
        if second_file {
            for file in ["tests/test_a.py", "tests/test_b.py"] {
                assert!(
                    human.contains(&format!(
                        "source function argument value x = 10 at {file}:4"
                    )),
                    "{human}"
                );
            }
        } else {
            assert!(
                human.contains("source function argument value x = 10 at line 4"),
                "{human}"
            );
        }
        assert!(
            human.contains(&format!(
                "showing 8 of {expected_total} read from related tests"
            )),
            "{human}"
        );
        assert!(
            human.contains(if expected_total > 32 {
                "--format json keeps a ranked 32"
            } else {
                "full list in --format json"
            }),
            "{human}"
        );

        let json: serde_json::Value = serde_json::from_str(&ripr::output::json::render(&output))
            .map_err(|e| e.to_string())?;
        let rendered = json["findings"]
            .as_array()
            .and_then(|findings| findings.iter().find(|item| item["id"] == finding.id))
            .ok_or_else(|| "classified predicate missing from JSON".to_string())?;
        for values in [
            &rendered["observed_values"],
            &rendered["activation"]["observed_values"],
        ] {
            let values = values
                .as_array()
                .ok_or_else(|| "missing observed values array".to_string())?;
            assert_eq!(values.len(), expected_total.min(32));
            assert!(values.iter().all(|value| value.get("file").is_none()));
            if second_file {
                assert_eq!(
                    values[0], values[1],
                    "distinct files retain identical wire facts"
                );
            }
        }
        if expected_total > 32 {
            assert_eq!(rendered["observed_values_total"], expected_total);
        } else {
            assert!(rendered.get("observed_values_total").is_none());
        }
    }
    Ok(())
}
