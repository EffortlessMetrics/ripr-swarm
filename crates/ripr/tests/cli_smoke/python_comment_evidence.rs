//! Adding inert comments must not manufacture Python assertion evidence.
use super::{run_command, unique_temp_workspace};
use serde_json::Value;
use std::fs;
use std::path::Path;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

fn check(root: &Path, body: &str) -> TestResult<Value> {
    fs::write(
        root.join("tests/test_fees.py"),
        format!("from src.fees import fee\n\ndef test_fee():\n{body}"),
    )?;
    let output = run_command(
        env!("CARGO_BIN_EXE_ripr"),
        Some(root),
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
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["summary"]["probes"], 1);
    let findings = report["findings"].as_array().ok_or("missing findings")?;
    assert_eq!(findings.len(), 1);
    let finding = findings[0].clone();
    assert_eq!(finding["probe"]["owner"], "python:src/fees.py::fee");
    assert_eq!(
        finding["related_tests"]
            .as_array()
            .ok_or("missing related tests")?
            .len(),
        1
    );
    Ok(finding)
}

#[test]
fn comments_do_not_increase_python_proof_confidence() -> TestResult {
    let root = unique_temp_workspace("python-comment-evidence");
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"python\"]\n",
    )?;
    fs::write(
        root.join("src/fees.py"),
        "def fee(value):\n    return value - 1\n",
    )?;
    fs::write(
        root.join("change.patch"),
        "diff --git a/src/fees.py b/src/fees.py\n--- a/src/fees.py\n+++ b/src/fees.py\n@@ -1,2 +1,2 @@\n def fee(value):\n-    return value + 1\n+    return value - 1\n",
    )?;
    for (plain, commented, expected) in [
        (
            "    fee(10)\n    other = 12\n    assert (other\n            == 12)\n",
            "    fee(10)\n    other = 12\n    assert (other # fee\n            == 12)\n",
            "weakly_exposed",
        ),
        (
            "    assert fee(10) == fee(10)\n",
            "    assert fee(10) == fee(\n        10 # irrelevant café\n    )\n",
            "weakly_exposed",
        ),
        (
            "    assert fee(10) == 9\n",
            "    assert fee(10) == (\n        9 # irrelevant café\n    )\n",
            "exposed",
        ),
    ] {
        let before = check(&root, plain)?;
        let after = check(&root, commented)?;
        assert_eq!(before["classification"], expected, "{plain}");
        assert_eq!(
            after["classification"], before["classification"],
            "{commented}"
        );
        assert_eq!(after["confidence"], before["confidence"], "{commented}");
        assert_eq!(after["probe"]["owner"], before["probe"]["owner"]);
        assert_eq!(
            after["related_tests"][0]["oracle_strength"],
            before["related_tests"][0]["oracle_strength"]
        );
        assert_eq!(
            after["related_tests"][0]["oracle_kind"],
            before["related_tests"][0]["oracle_kind"]
        );
    }
    fs::remove_dir_all(root)?;
    Ok(())
}
