//! Input availability must survive the real CLI's JSON, human and badge projections.
//! The retained behavioral patch has one finding before and after source restoration.
use super::{run_command, run_git, unique_temp_workspace};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
const SOURCE: &str = "def discount(total):\n    return total >= 100\n";
const PATCH: &str = "diff --git a/src/discount.py b/src/discount.py\nindex ec6175f..8f07f5e 100644\n--- a/src/discount.py\n+++ b/src/discount.py\n@@ -1,2 +1,2 @@\n def discount(total):\n-    return total > 100\n+    return total >= 100\n";

fn fixture(label: &str) -> TestResult<PathBuf> {
    let root = unique_temp_workspace(label);
    fs::create_dir_all(root.join("src"))?;
    fs::create_dir_all(root.join("tests"))?;
    fs::write(
        root.join("ripr.toml"),
        "[languages]\nenabled = [\"rust\", \"python\"]\n",
    )?;
    fs::write(root.join("src/discount.py"), SOURCE)?;
    fs::write(
        root.join("tests/test_discount.py"),
        "from src.discount import discount\n\ndef test_discount_boundary():\n    assert discount(100) is True\n",
    )?;
    fs::write(root.join("change.patch"), PATCH)?;
    run_git(&root, &["init", "-b", "main"])?;
    Ok(root)
}

fn check(root: &Path, format: &str) -> TestResult<String> {
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
            format,
        ],
    )?;
    assert!(
        output.status.success(),
        "{format}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?)
}

fn json(root: &Path) -> TestResult<Value> {
    Ok(serde_json::from_str(&check(root, "json")?)?)
}

fn outcome(report: &Value) -> &Value {
    &report["analysis_outcome"]["outcome"]
}

fn python_count(report: &Value) -> Option<u64> {
    report["summary"]["changed_files_by_language"]
        .as_array()?
        .iter()
        .find(|entry| entry["language"] == "python")?["files"]
        .as_u64()
}

fn assert_missing(report: &Value, path: &str) {
    assert_eq!(
        report["analysis_outcome"]["analysis_complete"], false,
        "{report}"
    );
    assert_eq!(outcome(report)["kind"], "partial_with_limitations");
    let limitations = &outcome(report)["limitations"];
    assert_eq!(
        limitations.as_array().map(Vec::len),
        Some(1),
        "{limitations}"
    );
    assert_eq!(limitations[0]["kind"], "changed_file_absent_from_worktree");
    assert_eq!(limitations[0]["path"], path);
    assert_eq!(limitations[0]["affected_items"], 1);
    assert_eq!(limitations[0]["recovery"]["kind"], "retry");
    assert!(
        limitations[0]["recovery"]["detail"]
            .as_str()
            .is_some_and(|s| s.contains("Check out")),
        "{limitations}"
    );
}

#[test]
fn missing_changed_python_source_is_incomplete_and_restorable() -> TestResult {
    let root = fixture("python-source-restoration")?;
    let present = json(&root)?;
    let present_human = check(&root, "human")?;
    let present_badge: Value = serde_json::from_str(&check(&root, "badge-json")?)?;
    assert_eq!(
        present["summary"]["weakly_exposed"], 1,
        "positive fixture must exercise behavior"
    );
    assert_eq!(outcome(&present)["kind"], "complete_with_findings");
    assert_eq!(present["preview_languages"][0]["file_count"], 1);
    assert_eq!(present["preview_languages"][0]["analyzed"], true);
    assert!(present_human.contains("1 Python file analyzed"));

    // The patch, tests, selected root and process entry point stay identical.
    // Moving the source outside the walk also exercises a repeated same-root run.
    fs::rename(root.join("src/discount.py"), root.join("held-source.txt"))?;
    let absent = json(&root)?;
    let absent_human = check(&root, "human")?;
    let absent_badge: Value = serde_json::from_str(&check(&root, "badge-json")?)?;
    assert_missing(&absent, "src/discount.py");
    assert_eq!(absent["summary"]["findings"], 0);
    assert_eq!(python_count(&absent), Some(0));
    assert_eq!(outcome(&absent)["counts"]["changed_file_count"], 1);
    assert!(absent.get("preview_languages").is_none(), "{absent}");
    assert!(absent_human.contains("src/discount.py"));
    assert!(absent_human.contains("Check out the missing file"));
    assert!(!absent_human.contains("1 Python file analyzed"));
    assert_eq!(absent_badge["analysis_complete"], false);
    assert_eq!(absent_badge["analysis_outcome"], *outcome(&absent));
    assert_ne!(absent_badge["color"], "brightgreen");
    assert_ne!(absent_badge["status"], "pass");

    fs::rename(root.join("held-source.txt"), root.join("src/discount.py"))?;
    assert_eq!(fs::read_to_string(root.join("src/discount.py"))?, SOURCE);
    assert_eq!(json(&root)?, present);
    assert_eq!(check(&root, "human")?, present_human);
    assert_eq!(
        serde_json::from_str::<Value>(&check(&root, "badge-json")?)?,
        present_badge
    );
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn mixed_missing_python_source_preserves_available_findings_and_counts() -> TestResult {
    let root = fixture("python-source-mixed")?;
    let present = json(&root)?;
    assert_eq!(present["summary"]["weakly_exposed"], 1);
    let missing_patch = PATCH
        .replace("discount.py", "missing.py")
        .replace("discount(total)", "missing(total)");
    fs::write(root.join("change.patch"), format!("{PATCH}{missing_patch}"))?;
    let mixed = json(&root)?;
    assert_missing(&mixed, "src/missing.py");
    assert_eq!(mixed["findings"], present["findings"]);
    assert_eq!(python_count(&mixed), Some(1));
    assert_eq!(outcome(&mixed)["counts"]["changed_file_count"], 2);
    assert_eq!(mixed["preview_languages"][0]["file_count"], 1);
    assert_eq!(
        mixed["preview_languages"][0]["sample_paths"],
        serde_json::json!(["src/discount.py"])
    );
    let human = check(&root, "human")?;
    assert!(human.contains("1 Python file analyzed"));
    assert!(human.contains("src/missing.py"));
    let badge: Value = serde_json::from_str(&check(&root, "badge-json")?)?;
    assert_eq!(badge["analysis_complete"], false);
    assert_eq!(badge["analysis_outcome"], *outcome(&mixed));
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn python_source_admission_preserves_read_failure_and_honest_comment_zero() -> TestResult {
    let root = fixture("python-source-controls")?;
    fs::write(root.join("src/discount.py"), [0xff, b'\n'])?;
    let unreadable = json(&root)?;
    assert_eq!(unreadable["analysis_outcome"]["analysis_complete"], false);
    assert_eq!(
        outcome(&unreadable)["limitations"][0]["kind"],
        "language_scope_unsupported"
    );
    assert_eq!(
        outcome(&unreadable)["limitations"][0]["path"],
        "src/discount.py"
    );
    let badge: Value = serde_json::from_str(&check(&root, "badge-json")?)?;
    assert_ne!(badge["color"], "brightgreen");

    fs::write(
        root.join("src/discount.py"),
        format!("# New comment\n{SOURCE}"),
    )?;
    fs::write(
        root.join("change.patch"),
        "diff --git a/src/discount.py b/src/discount.py\n--- a/src/discount.py\n+++ b/src/discount.py\n@@ -1,3 +1,3 @@\n-# Old comment\n+# New comment\n def discount(total):\n     return total >= 100\n",
    )?;
    let comment = json(&root)?;
    assert_eq!(comment["analysis_outcome"]["analysis_complete"], true);
    assert_eq!(outcome(&comment)["kind"], "no_behavioral_candidates");
    assert_eq!(comment["summary"]["findings"], 0);
    assert_eq!(python_count(&comment), Some(1));
    let badge: Value = serde_json::from_str(&check(&root, "badge-json")?)?;
    assert_eq!(badge["color"], "brightgreen");
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn python_source_admission_preserves_exclusions_and_genuine_git_deletions() -> TestResult {
    let root = fixture("python-source-intentional-absence")?;
    for path in ["vendor/discount.py", "src/discount_pb2.py"] {
        fs::write(
            root.join("change.patch"),
            PATCH.replace("src/discount.py", path),
        )?;
        let excluded = json(&root)?;
        assert_eq!(excluded["summary"]["findings"], 0);
        assert_eq!(python_count(&excluded), Some(0));
        assert_eq!(excluded["analysis_outcome"]["analysis_complete"], false);
        assert!(
            !outcome(&excluded)["limitations"]
                .as_array()
                .is_some_and(|items| items
                    .iter()
                    .any(|item| item["kind"] == "changed_file_absent_from_worktree")),
            "{excluded}"
        );
    }
    fs::rename(root.join("src/discount.py"), root.join("held-source.txt"))?;
    fs::write(
        root.join("change.patch"),
        "diff --git a/src/discount.py b/src/discount.py\ndeleted file mode 100644\n--- a/src/discount.py\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-def discount(total):\n-    return total >= 100\n",
    )?;
    let deleted = json(&root)?;
    assert_eq!(
        deleted["analysis_outcome"]["analysis_complete"], true,
        "{deleted}"
    );
    assert_eq!(outcome(&deleted)["counts"]["changed_file_count"], 0);
    assert_eq!(outcome(&deleted)["limitations"], serde_json::json!([]));
    assert_eq!(deleted["summary"]["findings"], 0);
    fs::remove_dir_all(root)?;
    Ok(())
}
