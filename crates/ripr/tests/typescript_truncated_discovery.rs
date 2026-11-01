//! #11767: when TypeScript workspace discovery stops at its entry cap, a
//! finding must not say "no test reaches this change" about tests the walk
//! never read. Real producer path through the CLI; the cap is passed to the
//! child process only, so no test mutates the shared environment.

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "export function applyDiscount(amount: number, threshold: number): number {
    if (amount >= threshold) {
        return amount - 10;
    }
    return amount;
}
";

const DIFF: &str = "diff --git a/src/discount.ts b/src/discount.ts
index 0000000..1111111 100644
--- a/src/discount.ts
+++ b/src/discount.ts
@@ -1,5 +1,5 @@
 export function applyDiscount(amount: number, threshold: number): number {
-    if (amount > threshold) {
+    if (amount >= threshold) {
         return amount - 10;
     }
     return amount;
";

/// Runs `ripr check --format json` on a workspace that has the changed source
/// and many unrelated entries but no test, with the walk cap set to `cap`.
fn check_json(cap: Option<&str>) -> Result<serde_json::Value, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let root: PathBuf = std::env::temp_dir().join(format!(
        "ripr-ts-truncated-discovery-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let write = |path: &str, text: &str| -> Result<(), String> {
        let file = root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("mkdir {path}: {error}"))?;
        }
        std::fs::write(&file, text).map_err(|error| format!("write {path}: {error}"))
    };
    write(
        "ripr.toml",
        "[languages]\nenabled = [\"rust\", \"typescript\"]\n",
    )?;
    write("src/discount.ts", SOURCE)?;
    for index in 0..12 {
        write(&format!("docs/page{index}.txt"), "unrelated\n")?;
    }
    write("diff.patch", DIFF)?;
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"));
    command
        .args(["check", "--root"])
        .arg(&root)
        .arg("--diff")
        .arg(root.join("diff.patch"))
        .args(["--mode", "fast", "--format", "json"]);
    if let Some(cap) = cap {
        command.env("RIPR_TS_MAX_WORKSPACE_FILES", cap);
    }
    let output = command
        .output()
        .map_err(|error| format!("run ripr: {error}"))?;
    let _ = std::fs::remove_dir_all(&root);
    if !output.status.success() {
        return Err(format!(
            "ripr check failed with {}:\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|error| format!("parse json: {error}"))
}

fn only_finding(report: &serde_json::Value) -> Result<&serde_json::Value, String> {
    let findings = report["findings"]
        .as_array()
        .ok_or("findings is not an array")?;
    match findings.as_slice() {
        [finding] => Ok(finding),
        other => Err(format!("expected one finding, got {}", other.len())),
    }
}

/// Control: a walk that finishes keeps the verdict this fixture always had.
#[test]
fn a_finished_walk_still_reports_no_static_path() -> Result<(), String> {
    let report = check_json(None)?;
    let finding = only_finding(&report)?;
    assert_eq!(finding["classification"], "no_static_path", "{finding}");
    assert!(
        finding
            .get("static_limit_kind")
            .is_none_or(|kind| kind.is_null())
    );
    Ok(())
}

/// The cap trips before the walk covers the workspace: the verdict is
/// withheld and the limit names the cap and the way out.
#[test]
fn a_truncated_walk_withholds_no_static_path() -> Result<(), String> {
    let report = check_json(Some("3"))?;
    let finding = only_finding(&report)?;
    assert_eq!(finding["classification"], "static_unknown", "{finding}");
    assert_eq!(
        finding["static_limit_kind"], "typescript_workspace_discovery_truncated",
        "{finding}"
    );
    let text = finding.to_string();
    assert!(text.contains("RIPR_TS_MAX_WORKSPACE_FILES"), "{text}");
    assert!(
        text.contains("3 TypeScript/JavaScript source files"),
        "{text}"
    );
    assert!(text.contains("15 directory entries"), "{text}");
    assert_eq!(report["analysis_outcome"]["analysis_complete"], false);
    Ok(())
}
