//! End-to-end delegation pins for TypeScript family-relevant assertion
//! selection (RIPR-SPEC-0224, #5525): each case runs the production `check`
//! path on an on-disk workspace and feeds the JSON check output to the gap
//! ledger, so the real repair-packet and agent-packet decisions are exercised.

use crate::output::gap_decision_ledger::{
    GapDecisionLedgerInput, GapDecisionLedgerSourceKind, build_gap_decision_ledger_report,
    render_gap_decision_ledger_json,
};
use serde_json::Value;
use std::fs;

const TYPESCRIPT_CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";

const PACKAGE_JSON: &str = "{\n  \"name\": \"discount-lib\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"jest\": \"^29.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"jest\"\n  }\n}\n";

const DISCOUNT_TS: &str = "export function applyDiscount(amount: number, threshold: number): number {\n    if (amount >= threshold) {\n        return amount * 0.9;\n    }\n    return amount;\n}\n";

const DISCOUNT_DIFF: &str = "diff --git a/src/discount.ts b/src/discount.ts\nindex 0000000..1111111 100644\n--- a/src/discount.ts\n+++ b/src/discount.ts\n@@ -1,6 +1,6 @@\n export function applyDiscount(amount: number, threshold: number): number {\n-    if (amount > threshold) {\n+    if (amount >= threshold) {\n         return amount * 0.9;\n     }\n     return amount;\n }\n";

const VALUE_TEST: &str = "import { applyDiscount } from '../src/discount';\n\ntest('applyDiscount applies discount when amount meets threshold', () => {\n    const result = applyDiscount(100, 100);\n    expect(result).toBeGreaterThan(50);\n});\n";

struct TypeScriptCheck {
    finding: Value,
    ledger: Value,
}

impl TypeScriptCheck {
    fn class(&self) -> Option<&str> {
        self.finding.get("classification").and_then(Value::as_str)
    }

    fn repair_packet_ready(&self) -> Option<bool> {
        self.finding
            .get("preview_actionability")
            .and_then(|actionability| actionability.get("repair_packet_ready"))
            .and_then(Value::as_bool)
    }

    fn has_repair_packet(&self) -> bool {
        self.finding
            .get("typescript_repair_packet")
            .is_some_and(|packet| !packet.is_null())
    }

    fn any_agent_packet_eligible(&self) -> bool {
        self.ledger
            .get("records")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|record| {
                record
                    .get("projection_eligibility")
                    .and_then(|projection| projection.get("agent_packet"))
                    .and_then(|packet| packet.get("eligible"))
                    .and_then(Value::as_bool)
                    == Some(true)
            })
    }

    fn selection_disclosure(&self) -> Option<String> {
        self.finding
            .get("evidence")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .find(|line| line.starts_with("typescript_assertion_selection:"))
            .map(str::to_string)
    }
}

fn check_typescript(label: &str, tests: &[(&str, &str)]) -> Result<TypeScriptCheck, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-typescript-family-{label}-{}-{stamp}",
        std::process::id()
    ));
    let mut files = vec![
        ("package.json", PACKAGE_JSON),
        ("ripr.toml", TYPESCRIPT_CONFIG),
        ("src/discount.ts", DISCOUNT_TS),
    ];
    files.extend_from_slice(tests);
    let output = (|| -> Result<String, String> {
        for (path, contents) in &files {
            let path = root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("create {}: {error}", parent.display()))?;
            }
            fs::write(&path, contents)
                .map_err(|error| format!("write {}: {error}", path.display()))?;
        }
        fs::write(root.join("diff.patch"), DISCOUNT_DIFF)
            .map_err(|error| format!("write diff fixture: {error}"))?;
        let config = crate::config::tests_only_parse(TYPESCRIPT_CONFIG)?;
        let output = crate::app::check_workspace_with_config(
            crate::CheckInput {
                root: root.clone(),
                base: None,
                diff_file: Some(root.join("diff.patch")),
                mode: crate::Mode::Draft,
                format: crate::OutputFormat::Json,
                include_unchanged_tests: true,
                perl_facts_path: None,
                suppression_policy: None,
                git_timeout: None,
                git_candidate: None,
            },
            &config,
        )?;
        crate::render_check(&output, &crate::OutputFormat::Json)
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("remove fixture {}: {error}", root.display()));
    let check_output = output?;
    cleanup?;
    let value: Value = serde_json::from_str(&check_output)
        .map_err(|error| format!("parse check output: {error}"))?;
    let finding = match value.get("findings").and_then(Value::as_array) {
        Some(findings) if findings.len() == 1 => findings[0].clone(),
        other => return Err(format!("expected one finding, got {other:?}")),
    };
    let report = build_gap_decision_ledger_report(GapDecisionLedgerInput {
        root: format!("source-fixture/{label}"),
        generated_at: "test".to_string(),
        source_kind: GapDecisionLedgerSourceKind::CheckOutput,
        records_path: format!("{label}.json"),
        records_json: Ok(check_output),
    });
    let ledger = render_gap_decision_ledger_json(&report)?;
    let ledger: Value =
        serde_json::from_str(&ledger).map_err(|error| format!("parse ledger: {error}"))?;
    Ok(TypeScriptCheck { finding, ledger })
}

/// Control: the complete repair-packet shape stays delegatable, so the cases
/// below are discriminating.
#[test]
fn family_relevant_value_assertion_keeps_its_packet() -> Result<(), String> {
    let check = check_typescript("control", &[("tests/discount.test.ts", VALUE_TEST)])?;
    if check.class() != Some("weakly_exposed")
        || check.repair_packet_ready() != Some(true)
        || !check.has_repair_packet()
        || !check.any_agent_packet_eligible()
        || check.selection_disclosure().is_some()
    {
        return Err(format!("control lost its packet: {}", check.finding));
    }
    Ok(())
}

/// A stronger wrong-family `toThrow(TypeError)` in the same test used to be
/// the strength-only row, so the packet was built on it. The row now shows
/// the weaker predicate observer, the move is disclosed, and no packet is
/// emitted; the class is unchanged.
#[test]
fn passed_over_wrong_family_assertion_suppresses_the_packet() -> Result<(), String> {
    let test = "import { applyDiscount } from '../src/discount';\n\ntest('applyDiscount applies discount when amount meets threshold', () => {\n    const result = applyDiscount(100, 100);\n    expect(result).toBeGreaterThan(50);\n    expect(() => applyDiscount(Number.NaN, 1)).toThrow(TypeError);\n});\n";
    let check = check_typescript("passed-over", &[("tests/discount.test.ts", test)])?;
    let disclosure = check.selection_disclosure().unwrap_or_default();
    if check.class() != Some("weakly_exposed")
        || !disclosure.contains("other_behavior_assertion_passed_over")
        || check.repair_packet_ready() != Some(false)
        || check.has_repair_packet()
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "passed-over selection was delegated: {}",
            check.finding
        ));
    }
    Ok(())
}

/// Two tests: the wrong-family one is stronger, so the strength-only target
/// row and inferred verify command pointed at a test that cannot observe the
/// predicate. The guard withholds the packet instead of retargeting it.
#[test]
fn wrong_family_only_test_cannot_become_the_packet_target() -> Result<(), String> {
    let throw_test = "import { applyDiscount } from '../src/discount';\n\ntest('applyDiscount rejects NaN', () => {\n    expect(() => applyDiscount(Number.NaN, 1)).toThrow(TypeError);\n});\n";
    let check = check_typescript(
        "wrong-family-target",
        &[
            ("tests/a.test.ts", throw_test),
            ("tests/b.test.ts", VALUE_TEST),
        ],
    )?;
    let disclosure = check.selection_disclosure().unwrap_or_default();
    if !disclosure.contains("no_predicate_relevant_assertion")
        || check.repair_packet_ready() != Some(false)
        || check.has_repair_packet()
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "wrong-family test became the packet target: {}",
            check.finding
        ));
    }
    Ok(())
}
