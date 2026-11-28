//! #16370: a TypeScript test file that loads its runner with
//! `import test = require('tape')` or `import runner = require('node:test')`
//! registers its tests like the `const test = require('tape')` twin it
//! compiles to. Before, the admission allowlist read only ES imports and
//! `require()` declarations, so each import-equals twin registered no test.

use super::*;

const OWNER: &str = "export function total(n: number): number {\n  if (n > 50) {\n    return n - 5;\n  }\n  return n;\n}\n";
const CHANGED: (usize, &str) = (2, "  if (n > 50) {");
const IMPORT: &str = "import { total } from '../src/cart';\n";
const TAPE: &str = "test('checks total', (t) => {\n  t.equal(total(50), 50);\n  t.end();\n});\n";
const NODE: &str = "import assert = require('node:assert');\ntest('checks total', () => {\n  assert.strictEqual(total(50), 50);\n});\n";

fn class_for(label: &str, test_source: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cart.ts"), OWNER)?;
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
fn an_import_equals_runner_registers_like_its_require_twin() -> Result<(), String> {
    for (label, prelude, body) in [
        (
            "tape-require-reference",
            "const test = require('tape');\n",
            TAPE,
        ),
        (
            "tape-import-equals",
            "import test = require('tape');\n",
            TAPE,
        ),
        (
            "node-require-pick-reference",
            "const runner = require('node:test');\nconst { test } = runner;\n",
            NODE,
        ),
        (
            "node-import-equals-pick",
            "import runner = require('node:test');\nconst { test } = runner;\n",
            NODE,
        ),
        (
            "node-import-equals-member",
            "import runner = require('node:test');\nconst test = runner.test;\n",
            NODE,
        ),
    ] {
        let class = class_for(label, &format!("{prelude}{IMPORT}{body}"))?;
        assert_eq!(class, ExposureClass::Exposed, "{label}");
    }
    Ok(())
}

#[test]
fn an_import_equals_binding_that_is_not_a_known_runner_registers_nothing() -> Result<(), String> {
    for (label, prelude, body) in [
        // A file that declares its own `require` is not calling Node's loader.
        (
            "shadowed-require",
            "function require(id: string): any {\n  return () => {};\n}\nimport test = require('tape');\n",
            TAPE,
        ),
        // A written namespace may hold any `test`.
        (
            "written-namespace",
            "import runner = require('node:test');\nrunner.test = (() => {}) as any;\nconst { test } = runner;\n",
            NODE,
        ),
        // `skip` is no active registration root.
        (
            "inactive-member",
            "import runner = require('node:test');\nconst { skip: test } = runner;\n",
            NODE,
        ),
    ] {
        let class = class_for(label, &format!("{prelude}{IMPORT}{body}"))?;
        assert_eq!(class, ExposureClass::NoStaticPath, "{label}");
    }
    Ok(())
}

/// The pick is renamed to `check`, a local no ambient runner global spells,
/// so only the import can register it.
#[test]
fn a_type_only_import_equals_is_no_runner_import() -> Result<(), String> {
    let body = NODE.replace("test(", "check(");
    let pick = "runner = require('node:test');\nconst { test: check } = runner;\n";
    let value = class_for("renamed-value", &format!("import {pick}{IMPORT}{body}"))?;
    assert_eq!(value, ExposureClass::Exposed);
    let type_only = class_for(
        "renamed-type-only",
        &format!("import type {pick}{IMPORT}{body}"),
    )?;
    assert_eq!(type_only, ExposureClass::NoStaticPath);
    Ok(())
}
