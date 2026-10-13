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
                "import unittest\nfrom src.pricing import calculate_discount\nclass CheckoutTests(unittest.TestCase):\n    def test_same(self):\n        self.assertTrue(calculate_discount(150, 100))\n",
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
