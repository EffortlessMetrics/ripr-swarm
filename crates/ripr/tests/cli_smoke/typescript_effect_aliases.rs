//! Public consumer controls for the effect side-channel owner guard (#7514).

use super::{run_command, unique_temp_workspace, workspace_root};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Consumer {
    root: PathBuf,
}

impl Consumer {
    fn new(imports: &str, body: &str, owner_suffix: &str) -> TestResult<Self> {
        let fixture = workspace_root().join("fixtures/typescript_effect_owner_alias");
        let root = unique_temp_workspace("typescript-effect-alias");
        fs::create_dir_all(root.join("src"))?;
        fs::create_dir_all(root.join("tests"))?;
        let consumer = Self { root };
        for path in ["ripr.toml", "package.json"] {
            fs::copy(fixture.join("input").join(path), consumer.root.join(path))?;
        }
        let owner = fs::read_to_string(fixture.join("input/src/receipt.ts"))?;
        fs::write(
            consumer.root.join("src/receipt.ts"),
            format!("{owner}{owner_suffix}"),
        )?;
        fs::write(
            consumer.root.join("tests/receipt.test.ts"),
            format!(
                "import assert from 'node:assert/strict';\nimport {{ test }} from 'node:test';\n{imports}\ntest('audit contract', () => {{\n{body}\n}});\n"
            ),
        )?;
        fs::copy(
            fixture.join("diff.patch"),
            consumer.root.join("change.diff"),
        )?;
        Ok(consumer)
    }

    fn check(&self, class: &str, observation: &str) -> TestResult<Value> {
        self.check_subject(class, observation, 2, "function", "side_effect")
    }

    fn check_subject(
        &self,
        class: &str,
        observation: &str,
        line: u64,
        owner_kind: &str,
        family: &str,
    ) -> TestResult<Value> {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&self.root),
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "change.diff",
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
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "nonempty changed subject: {report}");
        let finding = &findings[0];
        assert_eq!(finding["probe"]["file"], "src/receipt.ts");
        assert_eq!(finding["probe"]["line"], line);
        assert_eq!(
            finding["probe"]["owner"],
            "typescript:src/receipt.ts::emitReceipt"
        );
        assert_eq!(finding["owner_kind"], owner_kind);
        assert_eq!(finding["probe"]["family"], family);
        assert_eq!(finding["language"], "typescript");
        assert_eq!(finding["language_status"], "preview");
        assert_eq!(finding["source_currentness"], "candidate_current");
        assert_eq!(finding["classification"], class, "{finding}");
        assert_eq!(finding["ripr"]["reach"]["state"], "yes");
        assert_eq!(finding["ripr"]["infect"]["state"], "unknown");
        assert_eq!(finding["ripr"]["propagate"]["state"], "unknown");
        assert_eq!(finding["ripr"]["observe"]["state"], observation);
        assert_eq!(finding["ripr"]["discriminate"]["state"], observation);
        assert_eq!(finding["oracle_kind"], "exact_value");
        assert_eq!(finding["oracle_strength"], "strong");
        assert_eq!(finding["related_tests_total"], 1);
        let related = finding["related_tests"]
            .as_array()
            .ok_or("missing related tests")?;
        assert_eq!(related.len(), 1);
        assert_eq!(related[0]["file"], "tests/receipt.test.ts");
        assert_eq!(related[0]["name"], "audit contract");
        assert_eq!(related[0]["oracle_kind"], "exact_value");
        assert_eq!(related[0]["oracle_strength"], "strong");
        Ok(report)
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn return_only_aliases_do_not_observe_the_changed_effect() -> TestResult {
    for (imports, body, suffix) in [
        (
            "import { emitReceipt } from '../src/receipt.ts';",
            "assert.strictEqual(emitReceipt(100), 90);",
            "",
        ),
        (
            "import { emitReceipt as discount } from '../src/receipt.ts';",
            "assert.strictEqual(discount(100), 90);",
            "",
        ),
        (
            "import discount from '../src/receipt.ts';",
            "assert.strictEqual(discount(100), 90);",
            "export default emitReceipt;\n",
        ),
        (
            "import { emitReceipt, emitReceipt as discount } from '../src/receipt.ts';",
            "emitReceipt(100); assert.strictEqual(discount(100), 90);",
            "",
        ),
        (
            "import discount, { emitReceipt as renamed } from '../src/receipt.ts';",
            "renamed(100); assert.strictEqual(discount(100), 90);",
            "export default emitReceipt;\n",
        ),
    ] {
        let report = Consumer::new(imports, body, suffix)?.check("weakly_exposed", "weak")?;
        let finding = &report["findings"][0];
        for surface in ["preview_actionability", "typescript_preview_card"] {
            assert_eq!(finding[surface]["repair_packet_ready"], false, "{finding}");
            let reason = finding[surface]["why_not_actionable"]
                .as_str()
                .ok_or("missing packet refusal reason")?;
            assert!(reason.contains("effect-observing assertion"), "{reason}");
            let route = finding[surface]["repair_route"]
                .as_str()
                .ok_or("missing repair route")?;
            assert!(!route.contains("expect("), "return-only repair: {route}");
        }
        assert!(finding["typescript_repair_packet"].is_null(), "{finding}");
    }
    Ok(())
}

#[test]
fn an_actual_effect_observer_survives_owner_aliases() -> TestResult {
    for (imports, suffix) in [
        (
            "import { emitReceipt as discount } from '../src/receipt.ts';",
            "",
        ),
        (
            "import discount from '../src/receipt.ts';",
            "export default emitReceipt;\n",
        ),
    ] {
        for return_assertion in ["", "assert.strictEqual(discount(100), 90);"] {
            let body = format!(
                "let captured = 0;\nconst previous = console.log;\nconsole.log = (_tag, value) => {{ captured = value; }};\ntry {{ discount(100); {return_assertion} }} finally {{ console.log = previous; }}\nassert.strictEqual(captured, 900);"
            );
            Consumer::new(imports, &body, suffix)?.check("exposed", "yes")?;
        }
    }
    Ok(())
}

#[test]
fn alias_substrings_and_comments_keep_the_conservative_effect_rule() -> TestResult {
    for (local, observed) in [
        ("emitReceiptTotal", "emitReceiptTotal"),
        ("discountTotal", "discountTotal"),
        ("captured", "captured + /* emitReceipt */ 0"),
        ("captured", "captured + /* discount */ 0"),
    ] {
        let body = format!(
            "let {local} = 0;\nconst previous = console.log;\nconsole.log = (_tag, value) => {{ {local} = value; }};\ntry {{ discount(100); }} finally {{ console.log = previous; }}\nassert.strictEqual({observed}, 900);"
        );
        Consumer::new(
            "import { emitReceipt as discount } from '../src/receipt.ts';",
            &body,
            "",
        )?
        .check("weakly_exposed", "weak")?;
    }
    Ok(())
}

#[test]
fn unrelated_or_shadowed_imports_remain_real_effect_observers() -> TestResult {
    let capture = "const previous = console.log; console.log = captureLog;\ntry { emitReceipt(100); assert.strictEqual(discount(), 900); } finally { console.log = previous; }";
    let support = "let lastAudit = 0;\nexport function captureLog(_tag: unknown, value: number): void { lastAudit = value; }\nexport function emitReceipt(): number { return lastAudit; }\n";
    let other_module = Consumer::new(
        "import { emitReceipt } from '../src/receipt.ts';\nimport { emitReceipt as discount, captureLog } from '../src/support.ts';",
        capture,
        "",
    )?;
    fs::write(other_module.root.join("src/support.ts"), support)?;
    other_module.check("exposed", "yes")?;

    Consumer::new(
        "import discount, { emitReceipt, captureLog } from '../src/receipt.ts';",
        capture,
        "let lastAudit = 0;\nexport function captureLog(_tag: unknown, value: number): void { lastAudit = value; }\nexport default function readAudit(): number { return lastAudit; }\n",
    )?
    .check("exposed", "yes")?;

    Consumer::new(
        "import { emitReceipt, emitReceipt as discount } from '../src/receipt.ts';",
        "let captured = 0;\nconst previous = console.log;\nconsole.log = (_tag, value) => { captured = value; };\ntry {\n  const discount = () => captured;\n  emitReceipt(100);\n  assert.strictEqual(discount(), 900);\n} finally { console.log = previous; }",
        "",
    )?
    .check("exposed", "yes")?;
    Ok(())
}

#[test]
fn a_free_export_does_not_supply_a_same_named_methods_alias() -> TestResult {
    let consumer = Consumer::new(
        "import { Ledger, emitReceipt as discount, captureLog } from '../src/receipt.ts';",
        "const previous = console.log; console.log = captureLog;\ntry {\n  const ledger = new Ledger();\n  ledger.emitReceipt(100);\n  assert.strictEqual(discount(), 900);\n} finally { console.log = previous; }",
        "",
    )?;
    fs::write(
        consumer.root.join("src/receipt.ts"),
        "export class Ledger {\n  emitReceipt(amount: number): number {\n    console.log(\"audit\", amount * 9);\n    return amount - 10;\n  }\n}\nlet lastAudit = 0;\nexport function captureLog(_tag: unknown, value: number): void { lastAudit = value; }\nexport function emitReceipt(): number { return lastAudit; }\n",
    )?;
    fs::write(
        consumer.root.join("change.diff"),
        "diff --git a/src/receipt.ts b/src/receipt.ts\n--- a/src/receipt.ts\n+++ b/src/receipt.ts\n@@ -1,6 +1,6 @@\n export class Ledger {\n   emitReceipt(amount: number): number {\n-    console.log(\"audit\", amount * 7);\n+    console.log(\"audit\", amount * 9);\n     return amount - 10;\n   }\n }\n",
    )?;
    consumer.check_subject("exposed", "yes", 3, "method", "side_effect")?;
    Ok(())
}

#[test]
fn a_value_change_keeps_its_alias_discriminator() -> TestResult {
    let consumer = Consumer::new(
        "import { emitReceipt as discount } from '../src/receipt.ts';",
        "assert.strictEqual(discount(100), 88);",
        "",
    )?;
    fs::write(
        consumer.root.join("src/receipt.ts"),
        "export function emitReceipt(amount: number): number {\n  console.log(\"audit\", amount * 9);\n  return amount - 12;\n}\n",
    )?;
    fs::write(
        consumer.root.join("change.diff"),
        "diff --git a/src/receipt.ts b/src/receipt.ts\n--- a/src/receipt.ts\n+++ b/src/receipt.ts\n@@ -1,4 +1,4 @@\n export function emitReceipt(amount: number): number {\n   console.log(\"audit\", amount * 9);\n-  return amount - 10;\n+  return amount - 12;\n }\n",
    )?;
    consumer.check_subject("exposed", "yes", 3, "function", "return_value")?;
    Ok(())
}
