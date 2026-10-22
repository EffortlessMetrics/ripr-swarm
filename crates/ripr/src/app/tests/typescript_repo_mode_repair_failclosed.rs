//! Repo-mode TypeScript findings never carry a repair-ready signal (#5531).
//!
//! The capability table has no `repair_routing` cell for TypeScript in repo
//! mode (absent cell = `not_proven`), so a repo run (bounded whole-file working
//! set, capped/partial denominators) must not flip `repair_packet_ready` even
//! when the same finding in diff mode is a complete, delegatable packet.
use serde_json::Value;
use std::fs;

const CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";
const PACKAGE_JSON: &str = "{\n  \"name\": \"discount-lib\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"jest\": \"^29.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"jest\"\n  }\n}\n";
const DISCOUNT_TS: &str = "export function applyDiscount(amount: number, threshold: number): number {\n    if (amount >= threshold) {\n        return amount * 0.9;\n    }\n    return amount;\n}\n";
const DISCOUNT_DIFF: &str = "diff --git a/src/discount.ts b/src/discount.ts\nindex 0000000..1111111 100644\n--- a/src/discount.ts\n+++ b/src/discount.ts\n@@ -1,6 +1,6 @@\n export function applyDiscount(amount: number, threshold: number): number {\n-    if (amount > threshold) {\n+    if (amount >= threshold) {\n         return amount * 0.9;\n     }\n     return amount;\n }\n";
const VALUE_TEST: &str = "import { applyDiscount } from '../src/discount';\n\ntest('applyDiscount applies discount when amount meets threshold', () => {\n    const result = applyDiscount(100, 100);\n    expect(result).toBeGreaterThan(50);\n});\n";

fn run(label: &str, repo: bool) -> Result<Vec<Value>, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-repo-failclosed-{label}-{}-{stamp}",
        std::process::id()
    ));
    let output = (|| -> Result<String, String> {
        for (path, contents) in [
            ("package.json", PACKAGE_JSON),
            ("ripr.toml", CONFIG),
            ("src/discount.ts", DISCOUNT_TS),
            ("tests/discount.test.ts", VALUE_TEST),
            ("diff.patch", DISCOUNT_DIFF),
        ] {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        let config = crate::config::tests_only_parse(CONFIG)?;
        let input = crate::CheckInput {
            root: root.clone(),
            base: None,
            diff_file: (!repo).then(|| root.join("diff.patch")),
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        };
        let output = if repo {
            crate::app::check_workspace_repo_with_config(input, &config)?
        } else {
            crate::app::check_workspace_with_config(input, &config)?
        };
        crate::render_check(&output, &crate::OutputFormat::Json)
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    let rendered = output?;
    cleanup?;
    let value: Value =
        serde_json::from_str(&rendered).map_err(|error| format!("parse check output: {error}"))?;
    Ok(value
        .get("findings")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn ts_findings(findings: &[Value]) -> Vec<&Value> {
    findings
        .iter()
        .filter(|finding| finding.to_string().contains("discount.ts"))
        .collect()
}

fn ready(finding: &Value) -> Option<bool> {
    finding
        .get("preview_actionability")
        .and_then(|a| a.get("repair_packet_ready"))
        .and_then(Value::as_bool)
}

#[test]
fn diff_mode_control_is_repair_ready() -> Result<(), String> {
    let findings = run("diff", false)?;
    let ts = ts_findings(&findings);
    if ts.len() != 1 || ready(ts[0]) != Some(true) {
        return Err(format!("diff control lost its packet: {findings:?}"));
    }
    Ok(())
}

#[test]
fn repo_mode_typescript_finding_is_not_repair_ready() -> Result<(), String> {
    let findings = run("repo", true)?;
    let ts = ts_findings(&findings);
    if ts.is_empty() {
        return Err(format!("repo run produced no TS finding: {findings:?}"));
    }
    for finding in ts {
        eprintln!("REPO_FINDING {finding}");
        if ready(finding) == Some(true)
            || finding
                .get("typescript_repair_packet")
                .is_some_and(|packet| !packet.is_null())
        {
            return Err(format!(
                "repo-mode finding reached repair signal: {finding}"
            ));
        }
    }
    Ok(())
}
