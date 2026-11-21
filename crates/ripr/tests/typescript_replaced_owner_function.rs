//! RIPR-SPEC-0243 / #15939: a function owner is not related through a test
//! file that replaces it on its module object. Every case runs the built
//! binary over a real workspace; each replaced shape has a call-through or
//! other-target control that must keep `exposed`.

#![cfg(feature = "lang-typescript")]

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const NAMESPACE_IMPORT: &str = "import * as pricing from '../src/discount';";
const NAMESPACE_CALL: &str = "expect(pricing.applyDiscount(100, 100)).toBe(90);";
const LIMITATION: &str = "typescript_limitation: typescript_spy_fabricated_observer";

struct Workspace(PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write_file(root: &Path, relative: &str, contents: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    }
    fs::write(&path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

impl Workspace {
    /// A workspace whose `applyDiscount` owner changes `>` to `>=`, with one
    /// test file made of `prelude` (imports, hooks, top-level statements) and
    /// a single test whose body is `body`. `export_keyword` is `export` or
    /// `export default`.
    fn new(export_keyword: &str, prelude: &str, body: &str) -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = std::env::temp_dir();
        fs::create_dir_all(&parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
        let root = parent.join(format!(
            "ripr-ts-replaced-owner-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|error| format!("mkdir {}: {error}", root.display()))?;
        let workspace = Self(root);
        let root = &workspace.0;
        write_file(root, "ripr.toml", "[languages]\nenabled = ['typescript']\n")?;
        write_file(
            root,
            "package.json",
            r#"{"devDependencies":{"vitest":"*","sinon":"*"}}"#,
        )?;
        write_file(
            root,
            "src/discount.ts",
            &format!(
                "{export_keyword} function applyDiscount(amount: number, threshold: number): number {{\n  if (amount >= threshold) {{\n    return amount - 10;\n  }}\n  return amount;\n}}\n"
            ),
        )?;
        write_file(
            root,
            "test/pricing.test.ts",
            &format!(
                "import {{ describe, it, expect, vi, beforeEach, beforeAll }} from 'vitest';\n{prelude}\n\ndescribe('discount', () => {{\n  it('discounts at the threshold', () => {{\n    {body}\n  }});\n}});\n"
            ),
        )?;
        write_file(
            root,
            "change.diff",
            &format!(
                "diff --git a/src/discount.ts b/src/discount.ts\n--- a/src/discount.ts\n+++ b/src/discount.ts\n@@ -1,3 +1,3 @@\n {export_keyword} function applyDiscount(amount: number, threshold: number): number {{\n-  if (amount > threshold) {{\n+  if (amount >= threshold) {{\n     return amount - 10;\n"
            ),
        )?;
        Ok(workspace)
    }

    fn finding(&self) -> Result<Value, String> {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
            .current_dir(&self.0)
            .args(["check", "--quiet", "--format", "json", "--root"])
            .arg(&self.0)
            .arg("--diff")
            .arg(self.0.join("change.diff"))
            .output()
            .map_err(|error| format!("spawn ripr check: {error}"))?;
        if !output.status.success() {
            return Err(format!(
                "ripr check exited {}:\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let report: Value = serde_json::from_slice(&output.stdout)
            .map_err(|error| format!("ripr check output is not JSON: {error}"))?;
        let findings = report["findings"]
            .as_array()
            .ok_or_else(|| format!("missing findings: {report}"))?;
        assert_eq!(
            findings.len(),
            1,
            "one changed owner must be analyzed: {report}"
        );
        let finding = findings.first().ok_or("missing changed-owner finding")?;
        assert_eq!(finding["probe"]["file"], "src/discount.ts", "{finding}");
        assert_eq!(finding["probe"]["line"], 2, "{finding}");
        Ok(finding.clone())
    }
}

fn names_limitation(finding: &Value) -> bool {
    finding["evidence"]
        .as_array()
        .is_some_and(|evidence| evidence.iter().any(|line| line == LIMITATION))
}

/// The owner is replaced: no related test carries a relation, the finding is
/// not `exposed`, and it names the limitation.
fn assert_withheld(
    export_keyword: &str,
    prelude: &str,
    body: &str,
    label: &str,
) -> Result<(), String> {
    let finding = Workspace::new(export_keyword, prelude, body)?.finding()?;
    assert_ne!(finding["classification"], "exposed", "{label}: {finding}");
    let related = finding["related_tests"]
        .as_array()
        .ok_or_else(|| format!("{label}: missing related_tests: {finding}"))?;
    assert!(
        related
            .iter()
            .all(|test| test.get("relation_reason").is_none()),
        "{label}: a replaced owner must not be related: {finding}"
    );
    assert!(names_limitation(&finding), "{label}: {finding}");
    Ok(())
}

/// The owner still runs: the test is related, the exact assertion at the
/// boundary reads `exposed`, and the limitation is absent.
fn assert_exposed(
    export_keyword: &str,
    prelude: &str,
    body: &str,
    label: &str,
) -> Result<(), String> {
    let finding = Workspace::new(export_keyword, prelude, body)?.finding()?;
    assert_eq!(finding["classification"], "exposed", "{label}: {finding}");
    assert!(
        finding["related_tests"][0]["relation_reason"].is_string(),
        "{label}: {finding}"
    );
    assert!(!names_limitation(&finding), "{label}: {finding}");
    Ok(())
}

#[test]
fn plain_namespace_call_is_exposed() -> Result<(), String> {
    assert_exposed("export", NAMESPACE_IMPORT, NAMESPACE_CALL, "baseline")
}

#[test]
fn replacement_in_the_test_body_withholds_the_relation() -> Result<(), String> {
    for (label, replacement) in [
        (
            "sinon.stub",
            "sinon.stub(pricing, 'applyDiscount').returns(90);",
        ),
        (
            "sandbox.stub",
            "sandbox.stub(pricing, \"applyDiscount\").callsFake(() => 90);",
        ),
        ("bare stub", "stub(pricing, 'applyDiscount').returns(90);"),
        (
            "sinon.replace",
            "sinon.replace(pricing, 'applyDiscount', () => 90);",
        ),
        (
            "mock.method with an implementation",
            "mock.method(pricing, 'applyDiscount', () => 90);",
        ),
        (
            "jest.replaceProperty",
            "jest.replaceProperty(pricing, 'applyDiscount', () => 90);",
        ),
        (
            "Object.defineProperty",
            "Object.defineProperty(pricing, 'applyDiscount', { value: () => 90 });",
        ),
        ("member assignment", "pricing.applyDiscount = () => 90;"),
        (
            "member assignment through a cast",
            "(pricing as any).applyDiscount = () => 90;",
        ),
        (
            "mockReturnValueOnce",
            "vi.spyOn(pricing, 'applyDiscount').mockReturnValueOnce(90);",
        ),
        (
            "mockImplementationOnce",
            "vi.spyOn(pricing, 'applyDiscount').mockImplementationOnce(() => 90);",
        ),
        (
            "jasmine and.returnValue",
            "spyOn(pricing, 'applyDiscount').and.returnValue(90);",
        ),
        ("whole-object stub", "sinon.stub(pricing);"),
        (
            "stub with a computed member",
            "sinon.stub(pricing, memberName).returns(90);",
        ),
        (
            "stub on a required module expression",
            "sinon.stub(require('../src/discount'), 'applyDiscount').returns(90);",
        ),
    ] {
        assert_withheld(
            "export",
            NAMESPACE_IMPORT,
            &format!("{replacement}\n    {NAMESPACE_CALL}"),
            label,
        )?;
    }
    Ok(())
}

#[test]
fn replacement_outside_the_test_body_withholds_the_relation() -> Result<(), String> {
    for (label, statement) in [
        (
            "stub in beforeEach",
            "beforeEach(() => { sinon.stub(pricing, 'applyDiscount').returns(90); });",
        ),
        (
            "fabricated spy in beforeEach",
            "beforeEach(() => { vi.spyOn(pricing, 'applyDiscount').mockReturnValue(90); });",
        ),
        (
            "assignment in beforeAll",
            "beforeAll(() => { (pricing as any).applyDiscount = () => 90; });",
        ),
        (
            "top-level stub",
            "sinon.stub(pricing, 'applyDiscount').returns(90);",
        ),
    ] {
        assert_withheld(
            "export",
            &format!("{NAMESPACE_IMPORT}\n{statement}"),
            NAMESPACE_CALL,
            label,
        )?;
    }
    Ok(())
}

#[test]
fn spy_bound_in_a_hook_and_fabricated_in_the_test_withholds() -> Result<(), String> {
    assert_withheld(
        "export",
        &format!(
            "{NAMESPACE_IMPORT}\nlet spy: any;\nbeforeEach(() => {{ spy = vi.spyOn(pricing, 'applyDiscount'); }});"
        ),
        &format!("spy.mockReturnValue(90);\n    {NAMESPACE_CALL}"),
        "spy bound in a hook",
    )
}

#[test]
fn stubbed_namespace_withholds_a_bare_named_import_call() -> Result<(), String> {
    // Under a CommonJS transpile the named import reads the module object at
    // call time, so the stub is what runs.
    let prelude = format!("{NAMESPACE_IMPORT}\nimport {{ applyDiscount }} from '../src/discount';");
    let call = "expect(applyDiscount(100, 100)).toBe(90);";
    assert_exposed("export", &prelude, call, "named import, no stub")?;
    assert_withheld(
        "export",
        &prelude,
        &format!("sinon.stub(pricing, 'applyDiscount').returns(90);\n    {call}"),
        "named import beside a stubbed namespace",
    )
}

#[test]
fn replaced_require_binding_withholds_the_relation() -> Result<(), String> {
    let prelude = "const pricing = require('../src/discount');";
    assert_exposed(
        "export",
        prelude,
        NAMESPACE_CALL,
        "require binding, no stub",
    )?;
    assert_withheld(
        "export",
        prelude,
        &format!("pricing.applyDiscount = () => 90;\n    {NAMESPACE_CALL}"),
        "assignment on a require binding",
    )
}

#[test]
fn default_export_owner_stubbed_as_default_is_withheld() -> Result<(), String> {
    let prelude = format!("{NAMESPACE_IMPORT}\nimport applyDiscount from '../src/discount';");
    let call = "expect(applyDiscount(100, 100)).toBe(90);";
    // The control only discriminates when the unstubbed default call relates.
    let baseline = Workspace::new("export default", &prelude, call)?.finding()?;
    assert!(
        baseline["related_tests"][0]["relation_reason"].is_string(),
        "default import, no stub: {baseline}"
    );
    assert!(!names_limitation(&baseline), "{baseline}");
    assert_withheld(
        "export default",
        &prelude,
        &format!("sinon.stub(pricing, 'default').returns(90);\n    {call}"),
        "stubbed default member",
    )
}

#[test]
fn call_through_spies_and_other_targets_stay_exposed() -> Result<(), String> {
    for (label, statement) in [
        (
            "call-through vi.spyOn",
            "const spy = vi.spyOn(pricing, 'applyDiscount');",
        ),
        (
            "call-through sinon.spy",
            "sinon.spy(pricing, 'applyDiscount');",
        ),
        (
            "two-argument mock.method",
            "mock.method(pricing, 'applyDiscount');",
        ),
        (
            "stub of another member",
            "sinon.stub(pricing, 'roundPrice').returns(1);",
        ),
        (
            "fabricated spy of another member",
            "vi.spyOn(pricing, 'roundPrice').mockReturnValue(1);",
        ),
        (
            "stub of the same member name on a global",
            "sinon.stub(console, 'applyDiscount').returns(1);",
        ),
        (
            "assignment of the same member name on a body local",
            "const form = { applyDiscount: null as unknown }; form.applyDiscount = 1;",
        ),
        (
            "whole-object stub of a body local",
            "const logger = { warn() {} }; sinon.stub(logger);",
        ),
        (
            "String.prototype.replace",
            "const label = 'applyDiscount'.replace('apply', 'applyDiscount');",
        ),
        ("anonymous stub", "const callback = sinon.stub();"),
        (
            "comparison of the member",
            "if (pricing.applyDiscount === undefined) { throw new Error('missing'); }",
        ),
    ] {
        assert_exposed(
            "export",
            NAMESPACE_IMPORT,
            &format!("{statement}\n    {NAMESPACE_CALL}"),
            label,
        )?;
    }
    Ok(())
}
