//! #11985: unread workspace sources cannot establish that no test reaches a
//! TypeScript change. Exercise the real CLI with child-local byte limits.

#![cfg(feature = "lang-typescript")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "export function discount(amount: number): number {
    if (amount >= 100) {
        return amount - 10;
    }
    return amount;
}
";

const TEST: &str = "import { expect, test } from 'vitest';
import { discount } from '../src/discount';
test('checks the threshold', () => {
    expect(discount(100)).toBe(90);
});
";

const DIFF: &str = "diff --git a/src/discount.ts b/src/discount.ts
index 0000000..1111111 100644
--- a/src/discount.ts
+++ b/src/discount.ts
@@ -1,5 +1,5 @@
 export function discount(amount: number): number {
-    if (amount > 100) {
+    if (amount >= 100) {
         return amount - 10;
     }
     return amount;
";

const SMALL_CAP: u64 = 512;
const LARGE_CAP: u64 = 65_536;

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-ts-read-budget-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let workspace = Self { root };
        workspace.write("ripr.toml", "[languages]\nenabled = [\"typescript\"]\n")?;
        workspace.write("src/discount.ts", SOURCE)?;
        workspace.write("diff.patch", DIFF)?;
        assert!(
            (SOURCE.len() + TEST.len()) as u64 <= SMALL_CAP,
            "the source and retained observer must fit both small budgets"
        );
        Ok(workspace)
    }

    fn write(&self, path: &str, text: &str) -> Result<(), String> {
        let file = self.root.join(path);
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|error| format!("mkdir {path}: {error}"))?;
        }
        std::fs::write(&file, text).map_err(|error| format!("write {path}: {error}"))
    }

    fn write_large(&self, path: &str, source: &str) -> Result<(), String> {
        let text = format!("// {}\n{source}", "padding".repeat(160));
        assert!(text.len() as u64 > SMALL_CAP);
        assert!(((text.len() + SOURCE.len() + TEST.len()) as u64) < LARGE_CAP);
        self.write(path, &text)
    }

    fn check_json(
        &self,
        file_cap: u64,
        workspace_budget: u64,
    ) -> Result<serde_json::Value, String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .args(["check", "--root"])
            .arg(&self.root)
            .arg("--diff")
            .arg(self.root.join("diff.patch"))
            .args(["--mode", "fast", "--format", "json"])
            .env("RIPR_TS_MAX_WORKSPACE_FILES", "20000")
            .env("RIPR_TS_MAX_FILE_READ_BYTES", file_cap.to_string())
            .env(
                "RIPR_TS_MAX_WORKSPACE_READ_BYTES",
                workspace_budget.to_string(),
            )
            .output()
            .map_err(|error| format!("run ripr: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr check failed with {}:\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        serde_json::from_slice(&output.stdout).map_err(|error| format!("parse json: {error}"))
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
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

fn named_refusal<'a>(
    report: &'a serde_json::Value,
    path: &str,
    reason: &str,
) -> Result<&'a serde_json::Value, String> {
    let limitations = report["analysis_outcome"]["outcome"]["limitations"]
        .as_array()
        .ok_or("limitations is not an array")?;
    limitations
        .iter()
        .find(|limit| {
            limit["path"] == path
                && limit["bounded_detail"]
                    .as_str()
                    .is_some_and(|detail| detail.starts_with(reason))
        })
        .ok_or_else(|| format!("missing {reason} on {path}: {limitations:?}"))
}

fn assert_read_limit(report: &serde_json::Value, knob: &str) -> Result<(), String> {
    let finding = only_finding(report)?;
    assert_eq!(finding["classification"], "static_unknown", "{finding}");
    assert_eq!(
        finding["static_limit_kind"], "typescript_workspace_read_limited",
        "{finding}"
    );
    let next = finding["recommended_next_step"]
        .as_str()
        .ok_or("withheld finding has no next step")?;
    assert!(next.contains(knob), "{next}");
    assert_eq!(finding["related_tests_total"], 0);
    assert_eq!(finding["missing_discriminators"], serde_json::json!([]));
    assert_eq!(
        finding["preview_actionability"]["gap_state"],
        "static_limitation"
    );
    assert_eq!(
        finding["preview_actionability"]["actionability_category"],
        "typescript_workspace_read_limited"
    );
    assert_eq!(
        finding["preview_actionability"]["repair_route"],
        next.trim_end_matches('.')
    );
    assert_eq!(
        finding["preview_actionability"]["missing_actionability_fields"],
        serde_json::json!([])
    );
    assert!(
        finding["preview_actionability"]["raw_evidence_refs"]
            .as_array()
            .is_some_and(|refs| !refs.is_empty())
    );
    assert_eq!(
        finding["preview_actionability"]["repair_packet_ready"],
        false
    );
    assert!(finding.get("typescript_repair_packet").is_none());
    assert!(finding.get("canonical_gap").is_none());
    assert_eq!(report["analysis_outcome"]["analysis_complete"], false);
    Ok(())
}

#[test]
fn a_capped_test_file_withholds_no_static_path() -> Result<(), String> {
    let workspace = Workspace::new()?;
    workspace.write_large("tests/discount.test.ts", TEST)?;
    let report = workspace.check_json(SMALL_CAP, LARGE_CAP)?;
    named_refusal(&report, "tests/discount.test.ts", "file_read_capped:")?;
    assert_read_limit(&report, "RIPR_TS_MAX_FILE_READ_BYTES")
}

#[test]
fn an_exhausted_workspace_budget_withholds_no_static_path() -> Result<(), String> {
    let workspace = Workspace::new()?;
    workspace.write_large("tests/discount.test.ts", TEST)?;
    let report = workspace.check_json(LARGE_CAP, SMALL_CAP)?;
    named_refusal(
        &report,
        "tests/discount.test.ts",
        "workspace_read_budget_exhausted:",
    )?;
    assert_read_limit(&report, "RIPR_TS_MAX_WORKSPACE_READ_BYTES")
}

#[test]
fn enough_budget_reads_the_test_and_retains_its_boundary_witness() -> Result<(), String> {
    let workspace = Workspace::new()?;
    workspace.write_large("tests/discount.test.ts", TEST)?;
    let report = workspace.check_json(LARGE_CAP, LARGE_CAP)?;
    let finding = only_finding(&report)?;
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert_eq!(finding["related_tests_total"], 1);
    assert_eq!(finding["oracle_kind"], "exact_value");
    assert!(finding.get("static_limit_kind").is_none());
    assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
    Ok(())
}

#[test]
fn a_finished_read_without_tests_keeps_no_static_path() -> Result<(), String> {
    let workspace = Workspace::new()?;
    let report = workspace.check_json(SMALL_CAP, SMALL_CAP)?;
    let finding = only_finding(&report)?;
    assert_eq!(finding["classification"], "no_static_path", "{finding}");
    assert_eq!(finding["related_tests_total"], 0);
    assert!(finding.get("static_limit_kind").is_none());
    assert_eq!(report["analysis_outcome"]["analysis_complete"], true);
    Ok(())
}

#[test]
fn an_observed_test_keeps_its_finding_beside_unread_sources() -> Result<(), String> {
    let workspace = Workspace::new()?;
    workspace.write("tests/discount.test.ts", TEST)?;
    workspace.write_large("unrelated/padding.ts", "export const padding = 1;\n")?;
    let complete = workspace.check_json(LARGE_CAP, LARGE_CAP)?;
    let observed = only_finding(&complete)?;
    assert_eq!(observed["classification"], "exposed", "{observed}");
    for (file_cap, workspace_budget, reason) in [
        (SMALL_CAP, LARGE_CAP, "file_read_capped:"),
        (LARGE_CAP, SMALL_CAP, "workspace_read_budget_exhausted:"),
    ] {
        let partial = workspace.check_json(file_cap, workspace_budget)?;
        named_refusal(&partial, "unrelated/padding.ts", reason)?;
        assert_eq!(only_finding(&partial)?, observed);
        assert_eq!(partial["analysis_outcome"]["analysis_complete"], false);
    }
    Ok(())
}

#[test]
fn a_refused_changed_source_does_not_invent_a_finding() -> Result<(), String> {
    let workspace = Workspace::new()?;
    let report = workspace.check_json(1, LARGE_CAP)?;
    named_refusal(&report, "src/discount.ts", "file_read_capped:")?;
    assert_eq!(report["findings"], serde_json::json!([]));
    assert_eq!(report["analysis_outcome"]["analysis_complete"], false);
    Ok(())
}
