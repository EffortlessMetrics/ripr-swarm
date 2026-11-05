//! End-to-end delegation pins for TypeScript error-variant repair packets
//! (#12953): a weakly-exposed thrown-variant change whose only test uses a
//! bare `toThrow()` must yield a validator-complete packet pinning the new
//! variant, while a test that already pins it offers no packet.

use crate::output::gap_decision_ledger::{
    GapDecisionLedgerInput, GapDecisionLedgerSourceKind, build_gap_decision_ledger_report,
    render_gap_decision_ledger_json,
};
use serde_json::Value;
use std::fs;

const TYPESCRIPT_CONFIG: &str = "[languages]\nenabled = [\"rust\", \"typescript\"]\n";

const PACKAGE_JSON: &str = "{\n  \"name\": \"parse-lib\",\n  \"version\": \"1.0.0\",\n  \"devDependencies\": {\n    \"jest\": \"^29.0.0\"\n  },\n  \"scripts\": {\n    \"test\": \"jest\"\n  }\n}\n";

const PARSE_TS: &str = "export class EmptyError extends Error {}\nexport class TooLongError extends Error {}\nexport function parse(s: string): number {\n  if (s.length === 0) {\n    throw new EmptyError();\n  }\n  if (s.length > 10) {\n    throw new TooLongError();\n  }\n  return s.length;\n}\n";

const PARSE_DIFF: &str = "diff --git a/src/index.ts b/src/index.ts\nindex 0000000..1111111 100644\n--- a/src/index.ts\n+++ b/src/index.ts\n@@ -6,7 +6,7 @@ export function parse(s: string): number {\n   if (s.length > 10) {\n-    throw new EmptyError();\n+    throw new TooLongError();\n   }\n   return s.length;\n }\n";

const BARE_TEST: &str = "import { parse } from '../src/index';\n\ntest('long input is error', () => {\n  expect(() => parse('abcdefghijklmnop')).toThrow();\n});\n";

const PINNED_TEST: &str = "import { parse } from '../src/index';\n\ntest('long input is error', () => {\n  expect(() => parse('abcdefghijklmnop')).toThrow(TooLongError);\n});\n";

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

    fn assertion_shape(&self) -> Option<&str> {
        self.finding
            .get("typescript_repair_packet")
            .and_then(|packet| packet.get("assertion_shape"))
            .and_then(Value::as_str)
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
}

fn check_typescript(label: &str, test: &str) -> Result<TypeScriptCheck, String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-typescript-error-variant-{label}-{}-{stamp}",
        std::process::id()
    ));
    let files = [
        ("package.json", PACKAGE_JSON),
        ("ripr.toml", TYPESCRIPT_CONFIG),
        ("src/index.ts", PARSE_TS),
        ("tests/parse.test.ts", test),
    ];
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
        fs::write(root.join("diff.patch"), PARSE_DIFF)
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
                scope: Default::default(),
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

/// #12953 (a): the bare `toThrow()` finding is weakly exposed, packet-ready
/// through the shared validator, and its edit target pins the new variant.
#[test]
fn bare_tothrow_error_variant_gets_a_complete_packet() -> Result<(), String> {
    let check = check_typescript("bare", BARE_TEST)?;
    if check.class() != Some("weakly_exposed")
        || check.repair_packet_ready() != Some(true)
        || !check.any_agent_packet_eligible()
    {
        return Err(format!("bare toThrow lost its packet: {}", check.finding));
    }
    let shape = check.assertion_shape().unwrap_or_default();
    if shape != "expect(() => parse('abcdefghijklmnop')).toThrow(TooLongError)" {
        return Err(format!("packet must pin the thrown variant: {shape}"));
    }
    Ok(())
}

/// #12953 negative control: a test that already pins the thrown variant
/// reads `exposed` and offers no packet or agent start.
#[test]
fn pinned_tothrow_error_variant_offers_no_packet() -> Result<(), String> {
    let check = check_typescript("pinned", PINNED_TEST)?;
    if check.class() != Some("exposed")
        || check.repair_packet_ready() == Some(true)
        || check.assertion_shape().is_some()
        || check.any_agent_packet_eligible()
    {
        return Err(format!(
            "pinned toThrow must offer no packet: {}",
            check.finding
        ));
    }
    Ok(())
}
