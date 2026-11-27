//! #16431: a runner whose default export is its `test` function (`tape`,
//! `ava`, `node:test`) registers its tests under any local name
//! (`const check = require('tape')`, `import check from 'ava'`) like the
//! `test` spelling does. Before, the renamed-alias gate (#10987) accepted
//! only a renamed named member (`{ test: t }`, `import { test as t }`), so
//! a renamed default binding registered nothing.

use super::*;

const OWNER: &str = "export function total(n: number): number {\n  if (n > 50) {\n    return n - 5;\n  }\n  return n;\n}\n";
const CHANGED: (usize, &str) = (2, "  if (n > 50) {");
const IMPORT: &str = "import { total } from '../src/cart';\n";
const TAPE: &str = "check('checks total', (t) => {\n  t.equal(total(50), 50);\n  t.end();\n});\n";
const AVA: &str = "check('checks total', (t) => {\n  t.is(total(50), 50);\n});\n";
const NODE: &str = "import assert = require('node:assert');\ncheck('checks total', () => {\n  assert.strictEqual(total(50), 50);\n});\n";

fn class_for(label: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cart.ts"), OWNER)?;
    ts_write_file(
        &root.join("src/helper.ts"),
        "export default function (name: string, run: (t: any) => void) {}\n",
    )?;
    ts_write_file(&root.join("tests/cart.test.ts"), test_source)?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.ts", &[CHANGED])],
    );
    let _ = std::fs::remove_dir_all(&root);
    result?
        .findings
        .into_iter()
        .next()
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding"))
}

#[test]
fn a_renamed_default_runner_registers_like_the_test_spelling() -> Result<(), String> {
    for (label, prelude, body) in [
        (
            "tape-test-reference",
            "const test = require('tape');\n",
            TAPE.replace("check(", "test("),
        ),
        (
            "tape-require",
            "const check = require('tape');\n",
            TAPE.to_string(),
        ),
        (
            "tape-import-equals",
            "import check = require('tape');\n",
            TAPE.to_string(),
        ),
        (
            "tape-default-import",
            "import check from 'tape';\n",
            TAPE.to_string(),
        ),
        (
            "ava-default-import",
            "import check from 'ava';\n",
            AVA.to_string(),
        ),
        (
            "node-test-default-import",
            "import check from 'node:test';\n",
            NODE.to_string(),
        ),
    ] {
        let class = class_for(label, &format!("{prelude}{IMPORT}{body}"))?;
        assert_eq!(class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn a_renamed_default_binding_that_is_not_a_known_runner_registers_nothing() -> Result<(), String> {
    for (label, prelude, body) in [
        // The binding is written, so it may hold any function.
        (
            "written-binding",
            "let check = require('tape');\ncheck = (name: string, run: any) => {};\n",
            TAPE.to_string(),
        ),
        // mocha's and node-tap's default exports are not `test`.
        (
            "mocha-default",
            "const check = require('mocha');\n",
            TAPE.to_string(),
        ),
        (
            "tap-namespace",
            "const check = require('tap');\n",
            TAPE.to_string(),
        ),
        // A local module's default export is no runner.
        (
            "local-default",
            "import check from '../src/helper';\n",
            TAPE.to_string(),
        ),
        // An inactive modifier registers no running test.
        (
            "skipped",
            "const check = require('tape');\n",
            TAPE.replace("check(", "check.skip("),
        ),
    ] {
        let class = class_for(label, &format!("{prelude}{IMPORT}{body}"))?;
        assert_eq!(class, ExposureClass::NoStaticPath, "{label}");
    }
    Ok(())
}
