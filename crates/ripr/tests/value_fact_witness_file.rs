//! #8389: value facts retain their source locations through classification.
//! Exercise producer-owned provenance, human windows and JSON's separate ranked cap;
//! the machine value objects omit the file field while retaining fact counts.

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
            output.findings.iter().find_map(|finding| {
                let index = finding
                    .activation
                    .observed_values
                    .iter()
                    .position(|value| std::ptr::eq(value, fact))?;
                output.source_value_files(finding)?.get(index)?.as_deref()
            }),
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
    for (calls, second_file, padding, expected_total) in [
        (12, false, 0, 12),
        (12, true, 0, 13),
        (12, true, 20, 13),
        (8, false, 0, 8),
        (8, true, 0, 9),
        (32, false, 0, 32),
        (32, true, 0, 33),
    ] {
        let body = (10..10 + calls)
            .map(|value| format!("    assert price({value}) == 1\n"))
            .collect::<String>();
        std::fs::write(scratch.0.join("tests/test_a.py"), format!("{header}{body}"))
            .map_err(|e| e.to_string())?;
        if second_file {
            std::fs::write(
                scratch.0.join("tests/test_b.py"),
                format!(
                    "{}{header}    assert price(10) == 1\n",
                    "\n".repeat(padding)
                ),
            )
            .map_err(|e| e.to_string())?;
        } else if scratch.0.join("tests/test_b.py").exists() {
            std::fs::remove_file(scratch.0.join("tests/test_b.py")).map_err(|e| e.to_string())?;
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
        let human = ripr::app::render_check(&output, &OutputFormat::HumanFull)?;
        let json: serde_json::Value = serde_json::from_str(&ripr::output::json::render(&output))
            .map_err(|e| e.to_string())?;
        let json_finding = json["findings"]
            .as_array()
            .and_then(|all| all.iter().find(|f| f["id"] == finding.id))
            .ok_or("missing JSON finding")?;
        assert_eq!(
            facts.len(),
            expected_total,
            "human:\n{human}\nJSON value counts: activation={:?}, promoted={:?}",
            json_finding["activation"]["observed_values"]
                .as_array()
                .map(Vec::len),
            json_finding["observed_values"].as_array().map(Vec::len)
        );
        let files = output
            .source_value_files(finding)
            .ok_or("missing producer value origins")?;
        assert_eq!(files.len(), facts.len());
        let repeated_locations = facts
            .iter()
            .zip(files)
            .filter(|(fact, _)| fact.value == "x = 10")
            .map(|(fact, file)| (file.as_deref(), fact.line))
            .collect::<Vec<_>>();
        let mut expected = vec![(Some(Path::new("tests/test_a.py")), 4)];
        if second_file {
            expected.push((Some(Path::new("tests/test_b.py")), 4 + padding));
        }
        assert_eq!(repeated_locations, expected);

        if calls == 12 && second_file && padding == 0 {
            // A detached four-field DTO retains counts but cannot invent paths.
            let bare = ripr::output::human::render_finding(finding);
            assert!(
                bare.contains("source function argument value x = 10 at line 4"),
                "{bare}"
            );
            assert!(
                !bare.contains("source function argument value x = 10 at tests/"),
                "{bare}"
            );
            assert_eq!(output.source_value_files(&finding.clone()), Some(files));
            let mut edited = output.clone();
            let original_id = finding.id.clone();
            let changed = edited
                .findings
                .iter_mut()
                .find(|f| f.id == original_id)
                .ok_or("missing cloned finding")?;
            changed.activation.observed_values[0].text.push(' ');
            let changed = edited
                .findings
                .iter()
                .find(|f| f.id == original_id)
                .ok_or("missing edited finding")?;
            assert!(edited.source_value_files(changed).is_none());
            let edited_human = ripr::app::render_check(&edited, &OutputFormat::HumanFull)?;
            assert!(
                !edited_human.contains("source function argument value x = 10 at tests/"),
                "{edited_human}"
            );
            // Fresh explain owns the context too; only artifact/detached routes fall back.
            let explained = ripr::app::explain_finding_with_config(
                CheckInput {
                    root: scratch.0.clone(),
                    diff_file: Some(scratch.0.join("change.diff")),
                    mode: Mode::Fast,
                    format: OutputFormat::HumanFull,
                    include_unchanged_tests: true,
                    ..CheckInput::default()
                },
                &finding.id,
                &config,
            )?;
            for file in ["tests/test_a.py", "tests/test_b.py"] {
                assert!(
                    explained.contains(&format!(
                        "source function argument value x = 10 at {file}:4"
                    )),
                    "{explained}"
                );
            }
        }
        let source_lines = human
            .lines()
            .filter(|line| line.contains("source function argument value"))
            .collect::<Vec<_>>();
        assert_eq!(source_lines.len(), 8, "{human}");
        if second_file && padding == 0 {
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
        if expected_total > 8 {
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
        } else {
            assert!(!human.contains("source values (showing"), "{human}");
        }
        if padding > 0 {
            assert!(
                source_lines.iter().all(|line| line.contains("at line ")),
                "{human}"
            );
        }

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
            if second_file && padding == 0 {
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

#[cfg(feature = "lang-python")]
#[test]
fn cross_file_guard_and_term_facts_reach_consumers() -> Result<(), String> {
    let mut config = ripr::config::RiprConfig::default();
    config.languages.enabled = vec![ripr::domain::LanguageId::Python];
    for (label, source, line, before, after, call, value, discriminator) in [
        (
            "guard",
            "def price(x):\n    if x > 5:\n        return 2\n    return 0\n",
            3,
            "        return 1",
            "        return 2",
            "assert price(0) == 0",
            "x = 0",
            "x > 5",
        ),
        (
            "term",
            "def shipping_fee(x):\n    if x >= 5:\n        return 0\n    return 1\n\ndef price(x):\n    return x\n",
            7,
            "    return x + shipping_fee(x)",
            "    return x",
            "assert price(10) == 10",
            "x = 10",
            "shipping_fee",
        ),
    ] {
        let scratch = Scratch(
            std::env::temp_dir().join(format!("ripr-cross-file-{label}-{}", std::process::id())),
        );
        std::fs::create_dir_all(scratch.0.join("src/pkg")).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(scratch.0.join("tests")).map_err(|e| e.to_string())?;
        for (path, text) in [
            ("pyproject.toml", "[project]\nname = \"value-facts\"\n"),
            ("src/pkg/__init__.py", ""),
            ("src/pkg/mod.py", source),
        ] {
            std::fs::write(scratch.0.join(path), text).map_err(|e| e.to_string())?;
        }
        std::fs::write(scratch.0.join("change.diff"), format!(
            "diff --git a/src/pkg/mod.py b/src/pkg/mod.py\n--- a/src/pkg/mod.py\n+++ b/src/pkg/mod.py\n@@ -{line} +{line} @@\n-{before}\n+{after}\n"
        )).map_err(|e| e.to_string())?;
        for file in ["tests/test_b.py", "tests/test_a.py"] {
            std::fs::write(
                scratch.0.join(file),
                format!("from pkg.mod import price\n\ndef test_price():\n    {call}\n"),
            )
            .map_err(|e| e.to_string())?;
        }
        let output = ripr::app::check_workspace_with_config(
            CheckInput {
                root: scratch.0.clone(),
                diff_file: Some(scratch.0.join("change.diff")),
                mode: Mode::Fast,
                format: OutputFormat::Json,
                ..CheckInput::default()
            },
            &config,
        )?;
        let finding = output
            .findings
            .iter()
            .find(|finding| {
                finding.probe.family == ripr::domain::ProbeFamily::ReturnValue
                    && finding.probe.location.line == line
            })
            .ok_or_else(|| format!("{label}: missing changed return finding: {output:?}"))?;
        assert!(
            finding
                .activation
                .missing_discriminators
                .iter()
                .any(|fact| format!("{fact:?}").contains(discriminator)),
            "{label}: {finding:?}"
        );
        let facts = &finding.activation.observed_values;
        assert_eq!(facts.len(), 2, "{label}: {facts:?}");
        let origins = output
            .source_value_files(finding)
            .ok_or("missing producer value origins")?;
        assert_eq!(origins.len(), facts.len());
        for ((fact, origin), file) in facts
            .iter()
            .zip(origins)
            .zip(["tests/test_a.py", "tests/test_b.py"])
        {
            assert_eq!(origin.as_deref(), Some(Path::new(file)));
            assert_eq!(fact.line, 4);
            assert_eq!(fact.value, value);
        }
        let human = ripr::app::render_check(&output, &OutputFormat::HumanFull)?;
        for file in ["tests/test_a.py", "tests/test_b.py"] {
            assert!(
                human.contains(&format!(
                    "source function argument value {value} at {file}:4"
                )),
                "{human}"
            );
        }
        let json: serde_json::Value = serde_json::from_str(&ripr::output::json::render(&output))
            .map_err(|e| e.to_string())?;
        let rendered = json["findings"]
            .as_array()
            .and_then(|items| items.iter().find(|item| item["id"] == finding.id))
            .ok_or_else(|| "changed return missing from JSON".to_string())?;
        for values in [
            &rendered["observed_values"],
            &rendered["activation"]["observed_values"],
        ] {
            let values = values
                .as_array()
                .ok_or_else(|| "missing observed values".to_string())?;
            assert_eq!(values.len(), 2);
            assert!(values.iter().all(|value| value.get("file").is_none()));
            assert_eq!(values[0], values[1]);
        }
        assert!(rendered.get("observed_values_total").is_none());
    }
    Ok(())
}

#[test]
fn classified_input_beyond_first_32_outranks_assertion_values_in_json() -> Result<(), String> {
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "ripr-ranked-classified-values-{}",
        std::process::id()
    )));
    std::fs::create_dir_all(scratch.0.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        scratch.0.join("Cargo.toml"),
        "[package]\nname = \"ranked-values\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .map_err(|e| e.to_string())?;
    let assertions = "        assert_eq!(0, 0);\n".repeat(32);
    std::fs::write(scratch.0.join("src/lib.rs"), format!(
        "pub fn price(x: i32) -> bool {{\n    x > 5\n}}\n\n#[cfg(test)]\nmod tests {{\n    use super::price;\n    #[test]\n    fn test_price() {{\n{assertions}        assert!(price(10));\n    }}\n}}\n"
    )).map_err(|e| e.to_string())?;
    std::fs::write(scratch.0.join("change.diff"),
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -2 +2 @@\n-    x >= 5\n+    x > 5\n"
    ).map_err(|e| e.to_string())?;
    let output = check_workspace(CheckInput {
        root: scratch.0.clone(),
        diff_file: Some(scratch.0.join("change.diff")),
        mode: Mode::Fast,
        format: OutputFormat::Json,
        ..CheckInput::default()
    })?;
    let finding = output
        .findings
        .iter()
        .find(|finding| finding.probe.family == ripr::domain::ProbeFamily::Predicate)
        .ok_or_else(|| "missing changed Rust predicate".to_string())?;
    let facts = &finding.activation.observed_values;
    assert!(facts.len() > 32, "{facts:?}");
    let (input_index, input) = facts
        .iter()
        .enumerate()
        .find(|(_, fact)| {
            fact.context == ripr::domain::ValueContext::FunctionArgument && fact.value == "x = 10"
        })
        .ok_or_else(|| format!("missing classified owner input: {facts:?}"))?;
    assert!(
        input_index >= 32,
        "ranked input must be outside a naive first-32 window"
    );
    assert!(
        facts
            .iter()
            .take(32)
            .all(|fact| fact.context == ripr::domain::ValueContext::AssertionArgument),
        "{facts:?}"
    );
    let json: serde_json::Value =
        serde_json::from_str(&ripr::output::json::render(&output)).map_err(|e| e.to_string())?;
    let rendered = json["findings"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == finding.id))
        .ok_or_else(|| "changed predicate missing from JSON".to_string())?;
    for values in [
        &rendered["observed_values"],
        &rendered["activation"]["observed_values"],
    ] {
        let values = values
            .as_array()
            .ok_or_else(|| "missing observed values".to_string())?;
        assert_eq!(values.len(), 32);
        assert!(
            values.iter().any(|value| value["value"] == input.value
                && value["context"] == "function_argument"
                && value["line"] == input.line),
            "ranked owner input must survive the JSON cap: {values:?}"
        );
    }
    assert_eq!(rendered["observed_values_total"], facts.len());
    let human = ripr::app::render_check(&output, &OutputFormat::HumanFull)?;
    assert!(
        human.contains(&format!(
            "showing 8 of {} read from related tests",
            facts.len()
        )),
        "{human}"
    );
    assert!(human.contains("--format json keeps a ranked 32"), "{human}");
    assert!(
        !human.contains("source function argument value x = 10 at"),
        "human keeps the first eight, independently of JSON ranking: {human}"
    );
    Ok(())
}
