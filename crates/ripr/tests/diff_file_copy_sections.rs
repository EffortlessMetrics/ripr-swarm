//! #15121: a file a patch creates with `copy from`/`copy to` is new code.
//!
//! Git copy detection (`git diff -C`) writes a copied file without its
//! copied lines. `ripr check --diff` then reported the run complete with no
//! changed behavior, while the same change written without copy detection
//! produced findings. The plain patch is the reference result here.

#![cfg(feature = "lang-rust")]

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static WORKSPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// The repo's `[env] TEMP=target` redirect puts fixtures inside the outer git
/// worktree; remove the root on scope exit, success or failure.
struct FixtureGuard {
    root: PathBuf,
}

impl Drop for FixtureGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const PRICE: &str = "pub fn discounted_total(amount: i64, threshold: i64) -> i64 {\n    if amount >= threshold {\n        amount - 10\n    } else {\n        amount\n    }\n}\n";
const TEST: &str = "#[test]\nfn discount_applies() {\n    assert_eq!(demo::price::discounted_total(100, 50), 90);\n}\n";
const LIB_HUNK: &str = "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -1 +1,2 @@\n pub mod price;\n+pub mod fee;\n";

/// A workspace whose `src/price.rs` is tested and whose `src/fee.rs`, when
/// `fee` is given, is the file the patch creates.
fn write_workspace(fee: Option<&str>) -> Result<PathBuf, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = std::env::temp_dir();
    fs::create_dir_all(&temp).map_err(|error| format!("mkdir temp root: {error}"))?;
    let root = temp.join(format!("ripr-copy-section-{}-{call}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for dir in ["src", "tests"] {
        fs::create_dir_all(root.join(dir)).map_err(|error| format!("mkdir: {error}"))?;
    }
    let mut files = vec![
        (
            "Cargo.toml",
            "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        ),
        ("src/lib.rs", "pub mod price;\npub mod fee;\n"),
        ("src/price.rs", PRICE),
        ("tests/price.rs", TEST),
    ];
    if let Some(fee) = fee {
        files.push(("src/fee.rs", fee));
    }
    for (path, text) in files {
        fs::write(root.join(path), text).map_err(|error| format!("write {path}: {error}"))?;
    }
    Ok(root)
}

/// The typed outcome kind, each limitation's `(kind, path)`, and each
/// finding's `(file, line, classification)`.
type Observed = (String, Vec<(String, String)>, Vec<(String, u64, String)>);

fn check(fee: Option<&str>, diff: &str) -> Result<Observed, String> {
    let root = write_workspace(fee)?;
    let _guard = FixtureGuard { root: root.clone() };
    fs::write(root.join("change.diff"), diff).map_err(|error| format!("write diff: {error}"))?;
    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(&root)
        .args([
            "check",
            "--root",
            ".",
            "--diff",
            "change.diff",
            "--format",
            "json",
            "--quiet",
        ])
        .output()
        .map_err(|error| format!("spawn ripr failed: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let document: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|error| {
        format!(
            "check output is not JSON: {error}\nstdout: {stdout}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let text = |value: Option<&serde_json::Value>, what: &str| {
        value
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("missing {what}\nstdout: {stdout}"))
    };
    let kind = text(document.pointer("/analysis_outcome/outcome/kind"), "kind")?;
    let limitations = document
        .pointer("/analysis_outcome/outcome/limitations")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("missing limitations\nstdout: {stdout}"))?
        .iter()
        .map(|limitation| {
            Ok((
                text(limitation.get("kind"), "limitation kind")?,
                text(limitation.get("path"), "limitation path")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut findings = document
        .get("findings")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| format!("missing findings\nstdout: {stdout}"))?
        .iter()
        .map(|finding| {
            Ok((
                text(finding.pointer("/probe/file"), "probe file")?,
                finding
                    .pointer("/probe/line")
                    .and_then(serde_json::Value::as_u64)
                    .ok_or_else(|| format!("missing probe line\nstdout: {stdout}"))?,
                text(finding.get("classification"), "classification")?,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    findings.sort();
    Ok((kind, limitations, findings))
}

/// The patch git writes for a new file without copy detection.
fn added_file(path: &str, source: &str) -> String {
    let body = source
        .lines()
        .map(|line| format!("+{line}\n"))
        .collect::<String>();
    format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n{body}",
        source.lines().count()
    )
}

#[test]
fn a_copied_file_gets_the_findings_of_the_same_file_added_plainly() -> Result<(), String> {
    // Pure copy: no hunk at all.
    let reference = check(
        Some(PRICE),
        &format!("{}{LIB_HUNK}", added_file("src/fee.rs", PRICE)),
    )?;
    assert_eq!(reference.0, "complete_with_findings", "{reference:?}");
    assert!(reference.1.is_empty(), "{reference:?}");
    assert!(
        reference.2.len() >= 2
            && reference
                .2
                .iter()
                .all(|(file, _, _)| file.ends_with("src/fee.rs")),
        "the plain patch must name changed behavior in the new file: {reference:?}"
    );
    let pure = check(
        Some(PRICE),
        &format!(
            "diff --git a/src/price.rs b/src/fee.rs\nsimilarity index 100%\ncopy from src/price.rs\ncopy to src/fee.rs\n{LIB_HUNK}"
        ),
    )?;
    assert_eq!(pure, reference);

    // Copy with one edited line: the hunk names the edit, the copied
    // boundary on line 2 is new code all the same.
    let edited_fee = PRICE.replace("amount - 10", "amount - 20");
    let reference = check(
        Some(&edited_fee),
        &format!("{}{LIB_HUNK}", added_file("src/fee.rs", &edited_fee)),
    )?;
    assert!(
        reference.2.iter().any(|(_, line, _)| *line == 2),
        "{reference:?}"
    );
    let edited = check(
        Some(&edited_fee),
        &format!(
            "diff --git a/src/price.rs b/src/fee.rs\nsimilarity index 85%\ncopy from src/price.rs\ncopy to src/fee.rs\n--- a/src/price.rs\n+++ b/src/fee.rs\n@@ -3 +3 @@\n-        amount - 10\n+        amount - 20\n{LIB_HUNK}"
        ),
    )?;
    assert_eq!(edited, reference);
    Ok(())
}

#[test]
fn a_rename_stays_moved_code_and_a_missing_copy_is_disclosed() -> Result<(), String> {
    // A pure rename onto the same destination: the code moved, nothing new.
    let rename = check(
        Some(PRICE),
        "diff --git a/src/old.rs b/src/fee.rs\nsimilarity index 100%\nrename from src/old.rs\nrename to src/fee.rs\n",
    )?;
    assert!(rename.2.is_empty(), "{rename:?}");

    // The copy destination is not in the working tree: no file is read and
    // the run says so instead of reporting a complete analysis.
    let missing = check(
        None,
        "diff --git a/src/price.rs b/src/fee.rs\nsimilarity index 100%\ncopy from src/price.rs\ncopy to src/fee.rs\n",
    )?;
    assert_eq!(missing.0, "partial_with_limitations", "{missing:?}");
    assert!(
        missing.1.contains(&(
            "changed_file_absent_from_worktree".to_string(),
            "src/fee.rs".to_string()
        )),
        "{missing:?}"
    );
    assert!(missing.2.is_empty(), "{missing:?}");
    Ok(())
}
