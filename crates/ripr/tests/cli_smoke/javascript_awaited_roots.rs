//! Public CLI controls over pinned Nano ID JavaScript, with authored consumers.

use super::{run_command, unique_temp_workspace, workspace_root};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

struct Consumer {
    root: PathBuf,
}

impl Consumer {
    fn new(registration: &str) -> TestResult<Self> {
        let fixture = workspace_root().join("fixtures/javascript_awaited_registration");
        let root = unique_temp_workspace("javascript-awaited-root");
        fs::create_dir_all(&root)?;
        let consumer = Self { root };
        for file in [
            "package.json",
            "ripr.toml",
            "non-secure/index.js",
            "url-alphabet/index.js",
        ] {
            let destination = consumer.root.join(file);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(fixture.join("input").join(file), destination)?;
        }
        fs::create_dir_all(consumer.root.join("test"))?;
        fs::write(
            consumer.root.join("test/nanoid.test.js"),
            format!(
                "import assert from 'node:assert/strict';\nimport {{ test, describe }} from 'node:test';\nimport {{ nanoid }} from '../non-secure/index.js';\n{registration}\n"
            ),
        )?;
        fs::copy(
            fixture.join("diff.patch"),
            consumer.root.join("change.patch"),
        )?;
        Ok(consumer)
    }

    fn check(&self) -> TestResult<Value> {
        let output = run_command(
            env!("CARGO_BIN_EXE_ripr"),
            Some(&self.root),
            &[
                "check",
                "--root",
                ".",
                "--diff",
                "change.patch",
                "--mode",
                "fast",
                "--json",
            ],
        )?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: Value = serde_json::from_slice(&output.stdout)?;
        let findings = report["findings"].as_array().ok_or("missing findings")?;
        assert_eq!(findings.len(), 1, "nonempty anchored subject: {report}");
        assert_eq!(findings[0]["probe"]["file"], "non-secure/index.js");
        assert_eq!(findings[0]["probe"]["line"], 21);
        assert_eq!(findings[0]["language"], "javascript");
        assert_eq!(findings[0]["language_status"], "preview");
        assert_eq!(findings[0]["source_currentness"], "candidate_current");
        assert_eq!(findings[0]["ripr"]["infect"]["state"], "unknown");
        assert_eq!(findings[0]["ripr"]["propagate"]["state"], "unknown");
        Ok(report)
    }
}

impl Drop for Consumer {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn awaited_nanoid_consumer_credits_the_same_assertion_as_a_bare_root() -> TestResult {
    for registration in [
        "test('default id length', () => { assert.strictEqual(nanoid().length, 21); });",
        "await test('default id length', () => { assert.strictEqual(nanoid().length, 21); });",
        "await test('default id length', (t) => { t.assert.strictEqual(nanoid().length, 21); });",
        "await test('default id length', { skip: false, expectFailure: false }, () => { assert.strictEqual(nanoid().length, 21); });",
        "await test('default id length', { skip: undefined }, () => { assert.strictEqual(nanoid().length, 21); });",
        "await describe('ids', async () => { await test('default id length', () => { assert.strictEqual(nanoid().length, 21); }); });",
    ] {
        let report = Consumer::new(registration)?.check()?;
        let finding = &report["findings"][0];
        assert_eq!(
            finding["classification"], "exposed",
            "{registration}: {finding}"
        );
        assert_eq!(finding["oracle_kind"], "exact_value");
        assert_eq!(finding["oracle_strength"], "strong");
        assert_eq!(finding["related_tests_total"], 1);
        assert_eq!(finding["related_tests"][0]["oracle_kind"], "exact_value");
    }
    Ok(())
}

#[test]
fn awaited_nanoid_smoke_consumer_does_not_pin_the_changed_length() -> TestResult {
    let report = Consumer::new("await test('default id exists', () => { assert.ok(nanoid()); });")?
        .check()?;
    let finding = &report["findings"][0];
    assert_eq!(finding["classification"], "weakly_exposed", "{finding}");
    assert_eq!(finding["oracle_kind"], "smoke_only");
    assert_eq!(finding["related_tests_total"], 1);
    Ok(())
}

#[test]
fn awaited_roots_preserve_skips_expected_failure_and_assertion_shadows() -> TestResult {
    for registration in [
        "await test.skip('ids', () => { assert.strictEqual(nanoid().length, 21); });",
        "await test('ids', { skip: true }, () => { assert.strictEqual(nanoid().length, 21); });",
        "await test('ids', { todo: true }, () => { assert.strictEqual(nanoid().length, 21); });",
        "await test('ids', { expectFailure: true }, () => { assert.strictEqual(nanoid().length, 21); });",
        "const undefined = true; await test('ids', { skip: undefined }, () => { assert.strictEqual(nanoid().length, 21); });",
        "const undefined = true; await describe('ids', { skip: undefined }, async () => { await test('id', () => { assert.strictEqual(nanoid().length, 21); }); });",
    ] {
        let report = Consumer::new(registration)?.check()?;
        assert_eq!(
            report["findings"][0]["classification"], "no_static_path",
            "{registration}: {report}"
        );
        assert_eq!(report["findings"][0]["related_tests_total"], 0);
    }
    for registration in [
        "await describe('ids', async (assert) => { await test('id', () => { assert.strictEqual(nanoid().length, 21); }); });",
        "await describe('ids', async () => { const assert = { strictEqual() {} }; await test('id', () => { assert.strictEqual(nanoid().length, 21); }); });",
        "await test('ids', (assert) => { assert.strictEqual(nanoid().length, 21); });",
    ] {
        let report = Consumer::new(registration)?.check()?;
        assert_ne!(
            report["findings"][0]["classification"], "exposed",
            "{registration}: {report}"
        );
        assert_eq!(report["findings"][0]["oracle_kind"], "unknown");
    }
    Ok(())
}

#[test]
fn awaited_initializer_and_helper_registrations_stay_outside_root_discovery() -> TestResult {
    for registration in [
        "const registered = await test('ids', () => { assert.strictEqual(nanoid().length, 21); });",
        "async function register() { await test('ids', () => { assert.strictEqual(nanoid().length, 21); }); }",
    ] {
        let report = Consumer::new(registration)?.check()?;
        assert_eq!(
            report["findings"][0]["classification"], "no_static_path",
            "{registration}: {report}"
        );
        assert_eq!(report["findings"][0]["related_tests_total"], 0);
    }
    Ok(())
}

#[test]
fn awaited_local_root_that_discards_its_callback_cannot_supply_credit() -> TestResult {
    let consumer = Consumer::new(
        "const test = async (_name, _callback) => {}; await test('ids', () => { assert.strictEqual(nanoid().length, 21); });",
    )?;
    // This control declares its own root; remove the runner import so the
    // input parses and reaches the actual provenance decision.
    let file = consumer.root.join("test/nanoid.test.js");
    let source = fs::read_to_string(&file)?;
    fs::write(
        file,
        source.replace("import { test, describe } from 'node:test';\n", ""),
    )?;
    let report = consumer.check()?;
    assert_ne!(
        report["findings"][0]["classification"], "exposed",
        "a local async helper never invokes the exact-assertion callback: {report}"
    );
    assert_eq!(report["findings"][0]["related_tests_total"], 0);
    Ok(())
}

#[test]
fn awaited_enclosing_roots_cannot_shadow_the_real_runner_binding() -> TestResult {
    for registration in [
        "await describe('ids', async (test) => { await test('id', () => { assert.strictEqual(nanoid().length, 21); }); });",
        "await describe('ids', async () => { const test = async (_name, _callback) => {}; await test('id', () => { assert.strictEqual(nanoid().length, 21); }); });",
        "for (const test of [async (_name, _callback) => {}]) { await test('id', () => { assert.strictEqual(nanoid().length, 21); }); }",
    ] {
        let report = Consumer::new(registration)?.check()?;
        assert_ne!(
            report["findings"][0]["classification"], "exposed",
            "{registration}: {report}"
        );
        assert_eq!(report["findings"][0]["related_tests_total"], 0);
    }
    Ok(())
}

#[test]
fn awaited_runner_binding_survives_a_sibling_suite_helper() -> TestResult {
    let report = Consumer::new(
        "describe('helper suite', () => { const test = async (_name, _callback) => {}; }); await test('real id', () => { assert.strictEqual(nanoid().length, 21); });",
    )?
    .check()?;
    let finding = &report["findings"][0];
    assert_eq!(finding["classification"], "exposed", "{finding}");
    assert_eq!(finding["oracle_kind"], "exact_value");
    assert_eq!(finding["related_tests_total"], 1);
    Ok(())
}

#[test]
fn awaited_registration_requires_a_local_callable_runner_import() -> TestResult {
    for (binding, credited) in [
        (
            "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url); const { test } = require('node:test');",
            true,
        ),
        ("export { test } from 'node:test';", false),
        ("import * as test from 'node:test';", false),
    ] {
        let consumer =
            Consumer::new("await test('id', () => { assert.strictEqual(nanoid().length, 21); });")?;
        let file = consumer.root.join("test/nanoid.test.js");
        let source = fs::read_to_string(&file)?;
        fs::write(
            file,
            source.replace("import { test, describe } from 'node:test';", binding),
        )?;
        let report = consumer.check()?;
        let finding = &report["findings"][0];
        assert_eq!(
            finding["related_tests_total"],
            usize::from(credited),
            "{binding}: {finding}"
        );
        if credited {
            assert_eq!(finding["classification"], "exposed");
            assert_eq!(finding["oracle_kind"], "exact_value");
        } else {
            assert_ne!(finding["classification"], "exposed");
        }
    }
    Ok(())
}

#[test]
fn awaited_forged_or_mutable_commonjs_bindings_cannot_supply_credit() -> TestResult {
    for binding in [
        "const require = () => ({ test: async (_name, _callback) => {} }); const { test } = require('node:test');",
        "const createRequire = (_url) => () => ({ test: async (_name, _callback) => {} }); const require = createRequire(import.meta.url); const { test } = require('node:test');",
        "import { createRequire } from 'node:module'; const require = createRequire(import.meta.url); let { test } = require('node:test'); test = async (_name, _callback) => {};",
        "import { createRequire } from 'node:module'; let require = createRequire(import.meta.url); require = () => ({ test: async (_name, _callback) => {} }); const { test } = require('node:test');",
    ] {
        let consumer =
            Consumer::new("await test('id', () => { assert.strictEqual(nanoid().length, 21); });")?;
        let file = consumer.root.join("test/nanoid.test.js");
        let source = fs::read_to_string(&file)?;
        fs::write(
            file,
            source.replace("import { test, describe } from 'node:test';", binding),
        )?;
        let report = consumer.check()?;
        let finding = &report["findings"][0];
        assert_ne!(finding["classification"], "exposed", "{binding}: {finding}");
        assert_eq!(finding["related_tests_total"], 0, "{binding}: {finding}");
    }
    Ok(())
}

#[test]
fn awaited_mismatched_runner_aliases_remain_unsupported_and_disclosed() -> TestResult {
    for (binding, registration) in [
        (
            "import { describe as test, it } from 'node:test';",
            "await test('ids', () => { it('id', () => { assert.strictEqual(nanoid().length, 21); }); });",
        ),
        (
            "import { test as describe } from 'node:test';",
            "await describe('id', () => { assert.strictEqual(nanoid().length, 21); });",
        ),
    ] {
        let consumer = Consumer::new(registration)?;
        let file = consumer.root.join("test/nanoid.test.js");
        let source = fs::read_to_string(&file)?;
        fs::write(
            file,
            source.replace("import { test, describe } from 'node:test';", binding),
        )?;
        let report = consumer.check()?;
        let finding = &report["findings"][0];
        assert_eq!(finding["related_tests_total"], 0, "{binding}: {finding}");
        assert_eq!(finding["oracle_kind"], "unknown");
        assert_eq!(
            finding["preview_actionability"]["repair_packet_ready"],
            false
        );
        let outcome = &report["analysis_outcome"]["outcome"];
        assert_eq!(
            outcome["kind"], "partial_with_limitations",
            "{binding}: {outcome}"
        );
        assert!(
            outcome["limitations"]
                .as_array()
                .ok_or("missing limitations")?
                .iter()
                .any(|limit| {
                    limit["kind"] == "language_scope_unsupported"
                        && limit["bounded_detail"].as_str().is_some_and(|detail| {
                            detail.contains("runner registration kind mismatch")
                        })
                }),
            "{binding}: {outcome}"
        );
    }
    Ok(())
}

#[test]
fn nanoid_upstream_bytes_keep_their_independent_provenance() -> TestResult {
    let fixture = workspace_root().join("fixtures/javascript_awaited_registration");
    let provenance: Value = serde_json::from_slice(&fs::read(fixture.join("provenance.json"))?)?;
    let pins = provenance["retained_files"]
        .as_array()
        .ok_or("missing source pins")?;
    assert_eq!(pins.len(), 4);
    for pin in pins {
        let path = pin["path"].as_str().ok_or("missing source path")?;
        let bytes = fs::read(fixture.join("upstream").join(path))?;
        let digest = format!("{:x}", Sha256::digest(bytes));
        assert_eq!(Some(digest.as_str()), pin["sha256"].as_str(), "{path}");
        if path != "non-secure/index.js" {
            assert_eq!(
                fs::read(fixture.join("upstream").join(path))?,
                fs::read(fixture.join("input").join(path))?
            );
        }
    }
    let base = fs::read_to_string(fixture.join("upstream/non-secure/index.js"))?;
    let head = fs::read_to_string(fixture.join("input/non-secure/index.js"))?;
    assert_eq!(head.replace("return id.slice(1)", "return id"), base);
    Ok(())
}
