//! A version-guarded conftest `collect_ignore` withholds only the files it
//! could ignore (#15631): a test in another file keeps its credit, a test in
//! the possibly ignored file does not, and the limit names the conftest.
#![cfg(feature = "lang-python")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const TEST_BODY: &str = "from app import price\n\n\ndef test_price():\n    assert price(3) == 2\n";

fn workspace(label: &str, tests: &[(&str, &str)]) -> Result<PathBuf, String> {
    let root = std::env::temp_dir().join(format!("ripr-py-guard-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("tests")).map_err(|error| format!("mkdir: {error}"))?;
    let files = [
        ("pyproject.toml", "[project]\nname = \"guard\"\n"),
        ("app.py", "def price(amount):\n    return amount - 1\n"),
        (
            "tests/conftest.py",
            "import sys\n\ncollect_ignore = []\nif sys.version_info < (3, 14):\n    collect_ignore.extend([\"test_ignored.py\"])\n",
        ),
        (
            "change.diff",
            "diff --git a/app.py b/app.py\nindex 0000000..1111111 100644\n--- a/app.py\n+++ b/app.py\n@@ -1,2 +1,2 @@\n def price(amount):\n-    return amount\n+    return amount - 1\n",
        ),
    ];
    for (path, text) in files.iter().chain(tests) {
        fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))?;
    }
    Ok(root)
}

fn check(root: &Path) -> Result<String, String> {
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args([
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--format",
            "human-full",
        ])
        .output()
        .map_err(|error| format!("spawn ripr: {error}"))?;
    let _ = fs::remove_dir_all(root);
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn test_outside_the_guarded_ignore_keeps_credit() -> Result<(), String> {
    let root = workspace("other", &[("tests/test_calc.py", TEST_BODY)])?;
    let out = check(&root)?;
    if out.contains("unevaluated conftest") || out.contains("decorator_indirection") {
        return Err(format!(
            "directory withheld by a guarded collect_ignore:\n{out}"
        ));
    }
    if !out.contains("Static exposure\n  exposed (") {
        return Err(format!("expected exposed:\n{out}"));
    }
    Ok(())
}

#[test]
fn test_in_the_possibly_ignored_file_is_not_credited() -> Result<(), String> {
    let root = workspace("ignored", &[("tests/test_ignored.py", TEST_BODY)])?;
    let out = check(&root)?;
    if out.contains("Static exposure\n  exposed (") {
        return Err(format!("possibly ignored test was credited:\n{out}"));
    }
    if out.contains("unevaluated conftest") || out.contains("uses `pytest.collection") {
        return Err(format!(
            "limit should name the conftest collect_ignore:\n{out}"
        ));
    }
    if !out.contains("collect_ignore") {
        return Err(format!("limit should name collect_ignore:\n{out}"));
    }
    Ok(())
}
