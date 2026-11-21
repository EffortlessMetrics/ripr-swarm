//! CLI regression for one related-test selection rule (#11228).
//!
//! A weak direct call is listed before a boundary assertion through a barrel
//! re-export. The human headline, full preview card and JSON oracle summary
//! name that row when it exercises the changed boundary. The off-boundary
//! assertion is strong in its own row but cannot establish observation.

#![cfg(feature = "lang-typescript")]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static WORKSPACE_COUNTER: AtomicU64 = AtomicU64::new(0);

struct WorkspaceGuard {
    root: PathBuf,
}

impl Drop for WorkspaceGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_file(root: &Path, relative: &str, body: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    }
    fs::write(&path, body).map_err(|error| format!("write {}: {error}", path.display()))
}

const OLD: &str = "export function discount(total: number): number {\n  if (total >= 100) {\n    return total - 20;\n  }\n  return total;\n}\n";
const NEW: &str = "export function discount(total: number): number {\n  if (total > 100) {\n    return total - 20;\n  }\n  return total;\n}\n";

fn workspace() -> Result<WorkspaceGuard, String> {
    let call = WORKSPACE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-one-selection-{}-{call}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).map_err(|error| format!("mkdir {}: {error}", root.display()))?;
    let guard = WorkspaceGuard { root };
    let root = &guard.root;
    write_file(
        root,
        "ripr.toml",
        "[languages]\nenabled = [\"typescript\"]\n",
    )?;
    write_file(
        root,
        "package.json",
        "{\"name\":\"discount-lib\",\"devDependencies\":{\"jest\":\"^29.0.0\"},\"scripts\":{\"test\":\"jest\"}}\n",
    )?;
    write_file(root, "src/pricing.ts", NEW)?;
    write_file(
        root,
        "src/index.ts",
        "export { discount } from './pricing';\n",
    )?;
    write_file(
        root,
        "tests/pricing.test.ts",
        "import { discount } from '../src/pricing';\nit('direct weak', () => { expect(discount(200)).toBeTruthy(); });\n",
    )?;
    write_file(
        root,
        "tests/barrel.test.ts",
        "import { discount } from '../src/index';\nit('barrel strong', () => { expect(discount(100)).toBe(80); });\n",
    )?;
    let mut diff = String::from(
        "diff --git a/src/pricing.ts b/src/pricing.ts\n--- a/src/pricing.ts\n+++ b/src/pricing.ts\n@@ -1,6 +1,6 @@\n",
    );
    for line in OLD.lines() {
        if line.contains(">= 100") {
            diff.push_str(&format!("-{line}\n+{}\n", line.replace(">= 100", "> 100")));
        } else {
            diff.push_str(&format!(" {line}\n"));
        }
    }
    write_file(root, "change.diff", &diff)?;
    Ok(guard)
}

fn check(root: &Path, format: &str) -> Result<String, String> {
    let output: Output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["check", "--quiet", "--format", format, "--root"])
        .arg(root)
        .arg("--diff")
        .arg(root.join("change.diff"))
        .output()
        .map_err(|error| format!("spawn ripr check: {error}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn headline_card_and_json_summary_name_the_strong_row() -> Result<(), String> {
    let guard = workspace()?;

    let human = check(&guard.root, "human")?;
    let headline = human
        .lines()
        .find(|line| line.trim_start().starts_with("Related test"))
        .ok_or_else(|| format!("no Related test headline:\n{human}"))?;
    assert!(headline.contains("barrel strong"), "{headline}\n{human}");
    assert!(!headline.contains("direct weak"), "{headline}");

    let json: serde_json::Value =
        serde_json::from_str(&check(&guard.root, "json")?).map_err(|error| error.to_string())?;
    let finding = json
        .pointer("/findings/0")
        .ok_or_else(|| format!("no finding: {json}"))?;
    let rows = finding
        .get("related_tests")
        .and_then(|value| value.as_array())
        .ok_or_else(|| format!("no related_tests: {finding}"))?;
    assert!(
        rows.len() >= 2,
        "the control needs both rows to be related: {finding}"
    );
    let position = |name: &str| rows.iter().position(|row| row.to_string().contains(name));
    let (weak_at, strong_at) = (position("direct weak"), position("barrel strong"));
    assert!(
        matches!((weak_at, strong_at), (Some(weak), Some(strong)) if weak < strong),
        "the control needs the weak row listed before the strong row: {finding}"
    );
    let strong = strong_at
        .and_then(|index| rows.get(index))
        .ok_or("missing barrel row")?;
    assert_eq!(
        strong["relation_reason"], "re_export_chain_followed",
        "{strong}"
    );
    assert_eq!(strong["oracle_strength"], "strong", "{strong}");
    assert_eq!(
        finding.get("oracle_strength").and_then(|v| v.as_str()),
        Some("strong"),
        "the summary describes the strong row the headline names: {finding}"
    );

    let full = check(&guard.root, "human-full")?;
    assert!(
        full.lines()
            .any(|line| line.trim() == "related test: tests/barrel.test.ts:2 barrel strong"),
        "{full}"
    );

    // The same exact assertion off the boundary cannot observe >= becoming >.
    write_file(
        &guard.root,
        "tests/barrel.test.ts",
        "import { discount } from '../src/index';\nit('barrel strong', () => { expect(discount(50)).toBe(50); });\n",
    )?;
    let off_boundary = check(&guard.root, "human")?;
    let off_headline = off_boundary
        .lines()
        .find(|line| line.trim_start().starts_with("Related test"))
        .ok_or_else(|| format!("missing advisory row: {off_boundary}"))?;
    assert!(off_headline.contains("direct weak"), "{off_headline}");
    assert!(
        off_headline.contains("assertion too weak"),
        "{off_headline}"
    );
    let off_json: serde_json::Value =
        serde_json::from_str(&check(&guard.root, "json")?).map_err(|error| error.to_string())?;
    let off_finding = &off_json["findings"][0];
    assert_eq!(off_finding["oracle_strength"], "smoke");
    let off_strong = off_finding["related_tests"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["name"] == "barrel strong"))
        .ok_or("off-boundary barrel assertion was not retained")?;
    assert_eq!(off_strong["oracle_strength"], "strong");
    assert_eq!(off_strong["miss"], "observation_unconfirmed");
    Ok(())
}
