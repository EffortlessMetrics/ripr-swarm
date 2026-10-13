use super::{run_command, run_git, unique_temp_workspace};
use serde_json::Value;
use std::fs;

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn python_unittest_selectors_preserve_literal_file_identity() -> TestResult {
    let scratch = unique_temp_workspace("unittest-literal-selectors");
    let result = (|| -> TestResult {
        for (name, path, expected) in [
            ("dotted-file", "tests/test_checkout.v2.py", None),
            ("dotted-dir", "tests.v2/test_checkout.py", None),
            (
                "ordinary",
                "tests/test_checkout.py",
                Some("python -m unittest tests.test_checkout.CheckoutTests.test_same"),
            ),
            (
                "apostrophe",
                "tests/o'brien.py",
                Some("python -m unittest 'tests.o'\\''brien'.CheckoutTests.test_same"),
            ),
            (
                "pytest-dotted-file",
                "tests/test_checkout.v2.py",
                Some("python -m pytest tests/test_checkout.v2.py::test_same"),
            ),
            (
                "pytest-dotted-dir",
                "tests.v2/test_checkout.py",
                Some("python -m pytest tests.v2/test_checkout.py::test_same"),
            ),
        ] {
            let root = scratch.join(name);
            fs::create_dir_all(root.join("src"))?;
            let test_file = root.join(path);
            fs::create_dir_all(test_file.parent().ok_or("test parent missing")?)?;
            fs::write(
                root.join("ripr.toml"),
                "[languages]\nenabled = [\"rust\", \"python\"]\n",
            )?;
            fs::write(
                root.join("src/pricing.py"),
                "def calculate_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
            )?;
            fs::write(
                test_file,
                if name.starts_with("pytest-") {
                    "from src.pricing import calculate_discount\ndef test_same():\n    assert calculate_discount(150, 100)\n"
                } else {
                    "import unittest\nfrom src.pricing import calculate_discount\nclass CheckoutTests(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(calculate_discount(150, 100))\n"
                },
            )?;
            fs::write(
                root.join("change.patch"),
                "diff --git a/src/pricing.py b/src/pricing.py\n--- a/src/pricing.py\n+++ b/src/pricing.py\n@@ -1,4 +1,4 @@\n def calculate_discount(amount, threshold):\n-    if amount > threshold:\n+    if amount >= threshold:\n         return amount - 10\n     return amount\n",
            )?;
            run_git(&root, &["init", "-b", "main"])?;
            let output = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&root),
                &[
                    "check",
                    "--root",
                    ".",
                    "--diff",
                    "change.patch",
                    "--mode",
                    "fast",
                    "--format",
                    "json",
                ],
            )?;
            assert!(output.status.success(), "{name}: {output:?}");
            let report: Value = serde_json::from_slice(&output.stdout)?;
            let findings = report["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(findings.len(), 1, "{name}: {report}");
            let finding = &findings[0];
            assert_eq!(finding["classification"], "weakly_exposed");
            let related = finding["related_tests"]
                .as_array()
                .ok_or("missing related")?;
            assert_eq!(related.len(), 1, "{name}: {finding}");
            assert_eq!(related[0]["file"], path);
            assert_eq!(related[0]["name"], "test_same");
            let evidence = finding["evidence"].as_array().ok_or("missing evidence")?;
            let commands: Vec<_> = evidence
                .iter()
                .filter_map(Value::as_str)
                .filter(|entry| entry.starts_with("test_verify_command: "))
                .collect();
            match expected {
                Some(command) => {
                    assert_eq!(
                        commands,
                        [format!("test_verify_command: {command} (test_same)")]
                    );
                    assert_eq!(finding["repair_placement"]["verify_command"], command);
                    assert_eq!(finding["python_repair_card"]["verify"]["command"], command);
                }
                None => {
                    assert!(commands.is_empty(), "{name}: {commands:?}");
                    assert!(finding["repair_placement"].is_null(), "{name}: {finding}");
                    assert!(finding["python_repair_card"].is_null(), "{name}: {finding}");
                }
            }
        }
        Ok(())
    })();
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
    }
    result
}

#[test]
fn python_repair_route_and_card_use_the_same_runnable_test() -> TestResult {
    let scratch = unique_temp_workspace("unittest-runnable-fallback");
    let result = (|| -> TestResult {
        for (name, tests, selected_file, selected_strength, command) in [
            (
                "fallback-weaker",
                vec![
                    (
                        "tests/test_checkout.v2.py",
                        "self.assertTrue(calculate_discount(150, 100))",
                    ),
                    ("tests/test_plain.py", "calculate_discount(150, 100)"),
                ],
                "tests/test_plain.py",
                "unknown",
                "python -m unittest tests.test_plain.CheckoutTests.test_same",
            ),
            (
                "ranked-runnable",
                vec![
                    (
                        "tests/test_checkout.v2.py",
                        "self.assertTrue(calculate_discount(150, 100))",
                    ),
                    ("tests/test_aunknown.py", "calculate_discount(150, 100)"),
                    (
                        "tests/test_zplain.py",
                        "self.assertTrue(calculate_discount(150, 100))",
                    ),
                ],
                "tests/test_zplain.py",
                "smoke",
                "python -m unittest tests.test_zplain.CheckoutTests.test_same",
            ),
            (
                "duplicate-ordinary",
                vec![
                    (
                        "tests/test_afirst.py",
                        "self.assertTrue(calculate_discount(150, 100))",
                    ),
                    (
                        "tests/test_zlast.py",
                        "self.assertTrue(calculate_discount(150, 100))",
                    ),
                ],
                "tests/test_afirst.py",
                "smoke",
                "python -m unittest tests.test_afirst.CheckoutTests.test_same",
            ),
        ] {
            let root = scratch.join(name);
            fs::create_dir_all(root.join("src"))?;
            fs::create_dir_all(root.join("tests"))?;
            fs::write(
                root.join("ripr.toml"),
                "[languages]\nenabled = [\"rust\", \"python\"]\n",
            )?;
            fs::write(
                root.join("src/pricing.py"),
                "def calculate_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
            )?;
            for (path, body) in &tests {
                fs::write(
                    root.join(path),
                    format!(
                        "import unittest\nfrom src.pricing import calculate_discount\nclass CheckoutTests(unittest.TestCase):\n    def test_same(self):\n        {body}\n"
                    ),
                )?;
            }
            fs::write(
                root.join("change.patch"),
                "diff --git a/src/pricing.py b/src/pricing.py\n--- a/src/pricing.py\n+++ b/src/pricing.py\n@@ -1,4 +1,4 @@\n def calculate_discount(amount, threshold):\n-    if amount > threshold:\n+    if amount >= threshold:\n         return amount - 10\n     return amount\n",
            )?;
            run_git(&root, &["init", "-b", "main"])?;
            let output = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&root),
                &[
                    "check",
                    "--root",
                    ".",
                    "--diff",
                    "change.patch",
                    "--mode",
                    "fast",
                    "--format",
                    "json",
                ],
            )?;
            assert!(output.status.success(), "{name}: {output:?}");
            let report: Value = serde_json::from_slice(&output.stdout)?;
            let findings = report["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(findings.len(), 1, "{name}: {report}");
            let finding = &findings[0];
            assert_eq!(finding["classification"], "weakly_exposed");
            assert_eq!(
                finding["related_tests"]
                    .as_array()
                    .ok_or("missing related")?
                    .len(),
                tests.len()
            );
            assert_eq!(
                finding["repair_placement"]["suggested_test_file"], selected_file,
                "{name}: {finding}"
            );
            assert_eq!(finding["repair_placement"]["verify_command"], command);
            let card = &finding["python_repair_card"];
            assert_eq!(card["suggested_location"]["test_file"], selected_file);
            assert_eq!(card["suggested_location"]["test_name"], "test_same");
            assert_eq!(card["verify"]["command"], command);
            let current = card["current_test_evidence"]
                .as_str()
                .ok_or("missing card evidence")?;
            assert!(
                current.starts_with(&format!("{selected_file}:4 test_same ")),
                "{name}: {current}"
            );
            assert!(
                current.contains(&format!("oracle_strength={selected_strength}")),
                "{name}: {current}"
            );
            let evidence = finding["evidence"].as_array().ok_or("missing evidence")?;
            assert!(
                !evidence
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|entry| entry.starts_with("test_verify_command: "))
                    .any(|entry| entry.contains("test_checkout.v2"))
            );
        }
        Ok(())
    })();
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
    }
    result
}

#[test]
fn python_repair_card_distinguishes_mixed_framework_duplicate_names() -> TestResult {
    let scratch = unique_temp_workspace("python-mixed-method-selectors");
    let result = (|| -> TestResult {
        for (case, classes, selected_line, smoke_line) in [
            (
                "stronger-first",
                "class Bad(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(calculate_discount(150, 100))\nclass TestGood:\n    def test_same(self):\n        calculate_discount(150, 100)\n",
                7,
                Some(4),
            ),
            (
                "selected-first",
                "class TestGood:\n    def test_same(self):\n        calculate_discount(150, 100)\nclass Bad(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(calculate_discount(150, 100))\n",
                4,
                Some(7),
            ),
            (
                "unique-without-line",
                "class TestGood:\n    def test_same(self):\n        calculate_discount(150, 100)\n",
                4,
                None,
            ),
        ] {
            let root = scratch.join(case);
            fs::create_dir_all(root.join("src"))?;
            fs::create_dir_all(root.join("tests"))?;
            fs::write(
                root.join("ripr.toml"),
                "[languages]\nenabled = [\"rust\", \"python\"]\n",
            )?;
            fs::write(
                root.join("src/pricing.py"),
                "def calculate_discount(amount, threshold):\n    if amount >= threshold:\n        return amount - 10\n    return amount\n",
            )?;
            fs::write(
                root.join("tests/test_mixed.v2.py"),
                format!("import unittest\nfrom src.pricing import calculate_discount\n{classes}"),
            )?;
            fs::write(
                root.join("change.patch"),
                "diff --git a/src/pricing.py b/src/pricing.py\n--- a/src/pricing.py\n+++ b/src/pricing.py\n@@ -1,4 +1,4 @@\n def calculate_discount(amount, threshold):\n-    if amount > threshold:\n+    if amount >= threshold:\n         return amount - 10\n     return amount\n",
            )?;
            run_git(&root, &["init", "-b", "main"])?;
            let output = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&root),
                &[
                    "check",
                    "--root",
                    ".",
                    "--diff",
                    "change.patch",
                    "--mode",
                    "fast",
                    "--format",
                    "json",
                ],
            )?;
            assert!(output.status.success(), "{case}: {output:?}");
            let report: Value = serde_json::from_slice(&output.stdout)?;
            let findings = report["findings"].as_array().ok_or("missing findings")?;
            assert_eq!(findings.len(), 1, "{case}: {report}");
            let finding = &findings[0];
            assert_eq!(finding["classification"], "weakly_exposed");
            let related = finding["related_tests"]
                .as_array()
                .ok_or("missing related")?;
            assert_eq!(related.len(), if smoke_line.is_some() { 2 } else { 1 });
            let selected = related
                .iter()
                .find(|row| row["line"] == selected_line)
                .ok_or("selected literal row missing")?;
            assert_eq!(selected["name"], "test_same");
            assert_eq!(selected["oracle_strength"], "unknown");
            if let Some(line) = smoke_line {
                let competitor = related
                    .iter()
                    .find(|row| row["line"] == line)
                    .ok_or("competing literal row missing")?;
                assert_eq!(competitor["name"], "test_same");
                assert_eq!(competitor["oracle_strength"], "smoke");
            }
            let command = "python -m pytest tests/test_mixed.v2.py::TestGood::test_same";
            assert_eq!(finding["repair_placement"]["verify_command"], command);
            let card = &finding["python_repair_card"];
            assert_eq!(card["verify"]["command"], command);
            assert_eq!(
                card["suggested_location"]["pytest_node_id"],
                "tests/test_mixed.v2.py::TestGood::test_same"
            );
            assert_eq!(
                card["current_test_evidence"],
                format!(
                    "tests/test_mixed.v2.py:{selected_line} test_same currently has oracle_strength=unknown, oracle_kind=unknown"
                )
            );
            let evidence = finding["evidence"].as_array().ok_or("missing evidence")?;
            assert_eq!(
                evidence
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|entry| entry.starts_with("suggested_test_line:")),
                smoke_line.is_some()
            );
            if smoke_line.is_some() {
                assert!(
                    evidence
                        .iter()
                        .any(|entry| entry == &format!("suggested_test_line: {selected_line}"))
                );
            }

            // Public context must keep the literal selected class's weak row,
            // even when a same-file, same-name competitor has a stronger oracle.
            let context = run_command(
                env!("CARGO_BIN_EXE_ripr"),
                Some(&root),
                &[
                    "context",
                    "--root",
                    ".",
                    "--diff",
                    "change.patch",
                    "--at",
                    "src/pricing.py:2",
                ],
            )?;
            assert!(context.status.success(), "{case}: {context:?}");
            let packet: Value = serde_json::from_slice(&context.stdout)?;
            assert_eq!(packet["probe"]["id"], finding["id"]);
            assert_eq!(
                packet["related_tests"]
                    .as_array()
                    .ok_or("context related rows missing")?
                    .len(),
                related.len()
            );
            let site = &packet["witness"]["fix_site"];
            assert_eq!(site["file"], "tests/test_mixed.v2.py", "{case}: {packet}");
            assert_eq!(site["line"], selected_line, "{case}: {packet}");
            assert_eq!(site["test_name"], "test_same");
            assert_eq!(site["oracle_kind"], "unknown");
            assert_eq!(site["oracle_strength"], "unknown");
            assert!(site["current_oracle"].is_null());
            assert!(
                packet["witness"]["limitations"]
                    .as_array()
                    .ok_or("witness limitations missing")?
                    .iter()
                    .any(|limit| limit["kind"] == "oracle_text_unavailable")
            );
        }
        Ok(())
    })();
    if scratch.exists() {
        fs::remove_dir_all(&scratch)?;
    }
    result
}
