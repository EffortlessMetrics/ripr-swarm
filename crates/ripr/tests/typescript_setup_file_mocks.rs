//! Owner module mocked from a runner setup file (#15949).
//!
//! A mock a governing setup file registers replaces the owner module for
//! every test under the config, so a test that calls the owner reaches the
//! mock and must not be credited. Every case runs `ripr check`, the
//! production path, on a workspace on disk, and every withheld case has a
//! credited twin that differs only in the property under test.

#![cfg(feature = "lang-typescript")]

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Workspace(PathBuf);

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_file(root: &Path, relative: &str, contents: &str) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("mkdir {}: {error}", parent.display()))?;
    }
    std::fs::write(&path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

/// The classification of one finding.
struct OwnerFinding {
    class: String,
}

/// Writes `files` plus a diff that changes line `line` of `owner_file` from
/// `old` to its current text, runs the check, and returns the finding whose
/// owner ends with `owner_name`.
fn owner_finding(
    label: &str,
    files: &[(&str, &str)],
    owner_file: &str,
    (line, old): (usize, &str),
    owner_name: &str,
) -> Result<OwnerFinding, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "ripr-ts-setup-mock-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).map_err(|error| format!("mkdir {}: {error}", root.display()))?;
    let workspace = Workspace(root);
    let root = &workspace.0;
    write_file(root, "ripr.toml", "[languages]\nenabled = ['typescript']\n")?;
    let mut owner_source = None;
    // `@ROOT@` in a file stands for the workspace's absolute path.
    let root_text = root.to_string_lossy().replace('\\', "/");
    for (path, contents) in files {
        write_file(root, path, &contents.replace("@ROOT@", &root_text))?;
        if *path == owner_file {
            owner_source = Some(*contents);
        }
    }
    let owner_source = owner_source.ok_or_else(|| format!("{label}: no owner file"))?;
    let lines: Vec<&str> = owner_source.lines().collect();
    let new = lines
        .get(line - 1)
        .ok_or_else(|| format!("{label}: owner file has no line {line}"))?;
    let mut body = String::new();
    for (index, text) in lines.iter().enumerate() {
        if index + 1 == line {
            body.push_str(&format!("-{old}\n+{new}\n"));
        } else {
            body.push_str(&format!(" {text}\n"));
        }
    }
    let count = lines.len();
    write_file(
        root,
        "change.diff",
        &format!(
            "diff --git a/{owner_file} b/{owner_file}\n--- a/{owner_file}\n+++ b/{owner_file}\n@@ -1,{count} +1,{count} @@\n{body}"
        ),
    )?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"))
        .current_dir(root)
        .args(["check", "--quiet", "--format", "json", "--root"])
        .arg(root)
        .arg("--diff")
        .arg(root.join("change.diff"))
        .output()
        .map_err(|error| format!("{label}: run ripr: {error}"))?;
    let report: Value = serde_json::from_slice(&output.stdout).map_err(|error| {
        format!(
            "{label}: check output is not JSON ({error}): {}",
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    let findings = report["findings"]
        .as_array()
        .ok_or_else(|| format!("{label}: no findings array"))?;
    let finding = findings
        .iter()
        .find(|finding| {
            finding["probe"]["owner"]
                .as_str()
                .is_some_and(|owner| owner.ends_with(owner_name))
        })
        .ok_or_else(|| format!("{label}: expected a finding for `{owner_name}` in {findings:?}"))?;
    let class = finding["classification"]
        .as_str()
        .ok_or_else(|| format!("{label}: finding has no classification"))?
        .to_string();
    Ok(OwnerFinding { class })
}

const OWNER: &str = "export function applyDiscount(amount: number, threshold: number): number {\n    if (amount >= threshold) {\n        return amount - 10;\n    }\n    return amount;\n}\n";
const VITEST_CONFIG: &str = "export default { test: { setupFiles: ['./test/setup.ts'] } };\n";
const MOCK_OWNER: &str = "import { vi } from 'vitest';\nvi.mock('../src/discount');\n";

fn test_source(import_path: &str, runner_import: &str) -> String {
    format!(
        "{runner_import}import {{ applyDiscount }} from '{import_path}';\n\nit('keeps amount at threshold', () => {{\n  expect(applyDiscount(100, 100)).toBe(90);\n}});\n"
    )
}

const VITEST: &str = "import { it, expect } from 'vitest';\n";

const OLD_LINE: &str = "    if (amount > threshold) {";

/// The class of the `applyDiscount` finding for a workspace of `files` plus
/// the owner at `src/discount.ts`.
fn class_of(label: &str, files: &[(&str, &str)]) -> Result<String, String> {
    class_at(label, "src/discount.ts", files)
}

fn class_at(label: &str, owner_file: &str, files: &[(&str, &str)]) -> Result<String, String> {
    let mut all = vec![(owner_file, OWNER)];
    all.extend_from_slice(files);
    owner_finding(label, &all, owner_file, (2, OLD_LINE), "applyDiscount")
        .map(|finding| finding.class)
}

fn assert_class(label: &str, files: &[(&str, &str)], expected: &str) -> Result<(), String> {
    let class = class_of(label, files)?;
    if class == expected {
        Ok(())
    } else {
        Err(format!("{label}: expected {expected}, got {class}"))
    }
}

#[test]
fn a_setup_file_that_mocks_another_module_keeps_the_relation() -> Result<(), String> {
    // Control: the config and setup file are read, and the fixture relates.
    assert_class(
        "setup-control",
        &[
            ("vitest.config.ts", VITEST_CONFIG),
            ("src/other.ts", "export const other = 1;\n"),
            (
                "test/setup.ts",
                "import { vi } from 'vitest';\nvi.mock('../src/other');\n",
            ),
            ("test/d.test.ts", &test_source("../src/discount", VITEST)),
        ],
        "exposed",
    )
}

#[test]
fn a_vitest_setup_file_mock_of_the_owner_withholds() -> Result<(), String> {
    assert_class(
        "setup-vitest",
        &[
            ("vitest.config.ts", VITEST_CONFIG),
            ("test/setup.ts", MOCK_OWNER),
            ("test/d.test.ts", &test_source("../src/discount", VITEST)),
        ],
        "no_static_path",
    )
}

#[test]
fn a_define_config_call_with_a_resolved_entry_withholds() -> Result<(), String> {
    assert_class(
        "setup-define-config",
        &[
            (
                "vite.config.mts",
                "import { defineConfig } from 'vitest/config';\nimport { resolve } from 'node:path';\nexport default defineConfig({\n  test: { globals: true, setupFiles: resolve(__dirname, 'test/setup') },\n});\n",
            ),
            ("test/setup.ts", MOCK_OWNER),
            ("test/d.test.ts", &test_source("../src/discount", VITEST)),
        ],
        "no_static_path",
    )
}

#[test]
fn jest_setup_keys_withhold_from_every_config_form() -> Result<(), String> {
    let setup = ("test/setup.js", "jest.mock('../src/discount');\n");
    let test = test_source("../src/discount", "");
    let configs: [(&str, &str, &str); 4] = [
        (
            "jest-package-json",
            "package.json",
            "{ \"name\": \"x\", \"jest\": { \"setupFilesAfterEnv\": [\"<rootDir>/test/setup.js\"] } }\n",
        ),
        (
            "jest-config-json",
            "jest.config.json",
            "{ \"setupFiles\": [\"./test/setup.js\"] }\n",
        ),
        (
            "jest-config-cjs",
            "jest.config.js",
            "module.exports = {\n  projects: [{ displayName: 'unit', setupFilesAfterEnv: ['<rootDir>/test/setup.js'] }],\n};\n",
        ),
        (
            "jest-config-legacy",
            "jest.config.ts",
            "const config = { setupTestFrameworkScriptFile: `<rootDir>/test/setup.js` };\nexport default config;\n",
        ),
    ];
    let mut misses = Vec::new();
    for (label, config_path, config) in configs {
        let class = class_of(
            label,
            &[(config_path, config), setup, ("test/d.test.ts", &test)],
        )?;
        if class != "no_static_path" {
            misses.push(format!("{label}: got {class}"));
        }
    }
    if misses.is_empty() {
        Ok(())
    } else {
        Err(misses.join("; "))
    }
}

#[test]
fn a_package_json_setup_key_outside_the_jest_block_is_not_a_config() -> Result<(), String> {
    assert_class(
        "jest-package-json-other-key",
        &[
            (
                "package.json",
                "{ \"name\": \"x\", \"other\": { \"setupFiles\": [\"./test/setup.js\"] } }\n",
            ),
            ("test/setup.js", "jest.mock('../src/discount');\n"),
            ("test/d.test.ts", &test_source("../src/discount", "")),
        ],
        "exposed",
    )
}

#[test]
fn global_setup_registers_no_mock_for_a_test() -> Result<(), String> {
    assert_class(
        "global-setup",
        &[
            (
                "vitest.config.ts",
                "export default { test: { globalSetup: ['./test/setup.ts'] } };\n",
            ),
            ("test/setup.ts", MOCK_OWNER),
            ("test/d.test.ts", &test_source("../src/discount", VITEST)),
        ],
        "exposed",
    )
}

#[test]
fn a_literal_root_dir_bases_the_setup_entry() -> Result<(), String> {
    assert_class(
        "root-dir",
        &[
            (
                "jest.config.js",
                "module.exports = { rootDir: 'test', setupFiles: ['<rootDir>/setup.js'] };\n",
            ),
            ("test/setup.js", "jest.mock('../src/discount');\n"),
            ("test/d.test.ts", &test_source("../src/discount", "")),
        ],
        "no_static_path",
    )
}

#[test]
fn the_mock_specifier_resolves_from_the_setup_file() -> Result<(), String> {
    // `../src/discount` names the owner from `config/setup.ts`; from the
    // test file two levels down the same text would name `tests/src/discount`.
    let test = test_source("../../src/discount", VITEST);
    assert_class(
        "setup-relative",
        &[
            (
                "vitest.config.ts",
                "export default { test: { setupFiles: ['config/setup.ts'] } };\n",
            ),
            ("config/setup.ts", MOCK_OWNER),
            ("tests/unit/d.test.ts", &test),
        ],
        "no_static_path",
    )?;
    // Twin: one directory deeper, the same specifier names `config/src/discount`.
    assert_class(
        "setup-relative-other",
        &[
            (
                "vitest.config.ts",
                "export default { test: { setupFiles: ['config/deep/setup.ts'] } };\n",
            ),
            ("config/deep/setup.ts", MOCK_OWNER),
            ("config/src/discount.ts", "export const unrelated = 1;\n"),
            ("tests/unit/d.test.ts", &test),
        ],
        "exposed",
    )
}

#[test]
fn a_mock_in_a_file_the_setup_file_imports_withholds_within_the_hop_bound() -> Result<(), String> {
    let test = test_source("../src/discount", VITEST);
    let config = ("vitest.config.ts", VITEST_CONFIG);
    // setup -> mocks/index (directory import) -> mocks/discount: two hops.
    assert_class(
        "setup-import-chain",
        &[
            config,
            ("test/setup.ts", "import './mocks';\n"),
            ("test/mocks/index.ts", "export * from './discount';\n"),
            (
                "test/mocks/discount.ts",
                "import { vi } from 'vitest';\nvi.mock('../../src/discount');\n",
            ),
            ("test/d.test.ts", &test),
        ],
        "no_static_path",
    )?;
    // Twin: the mock sits three hops out, past the stated bound.
    assert_class(
        "setup-import-chain-past-bound",
        &[
            config,
            ("test/setup.ts", "import './mocks';\n"),
            ("test/mocks/index.ts", "import './a';\n"),
            ("test/mocks/a.ts", "import './b';\n"),
            (
                "test/mocks/b.ts",
                "import { vi } from 'vitest';\nvi.mock('../../src/discount');\n",
            ),
            ("test/d.test.ts", &test),
        ],
        "exposed",
    )
}

#[test]
fn a_config_governs_only_tests_under_its_directory() -> Result<(), String> {
    let config = (
        "packages/a/vitest.config.ts",
        "export default { test: { setupFiles: ['./setup.ts'] } };\n",
    );
    let setup = (
        "packages/a/setup.ts",
        "import { vi } from 'vitest';\nvi.mock('../b/src/discount');\n",
    );
    let class = class_at(
        "setup-other-package",
        "packages/b/src/discount.ts",
        &[
            config,
            setup,
            (
                "packages/b/test/d.test.ts",
                &test_source("../src/discount", VITEST),
            ),
        ],
    )?;
    if class != "exposed" {
        return Err(format!("sibling package: expected Exposed, got {class}"));
    }
    // Twin: the same test under the config's own directory is governed.
    let class = class_at(
        "setup-same-package",
        "packages/b/src/discount.ts",
        &[
            config,
            setup,
            (
                "packages/a/test/d.test.ts",
                &test_source("../../b/src/discount", VITEST),
            ),
        ],
    )?;
    if class != "no_static_path" {
        return Err(format!(
            "governed package: expected NoStaticPath, got {class}"
        ));
    }
    Ok(())
}

#[test]
fn an_unreadable_setup_specifier_withholds_like_an_in_file_one() -> Result<(), String> {
    assert_class(
        "setup-unresolved",
        &[
            ("vitest.config.ts", VITEST_CONFIG),
            (
                "test/setup.ts",
                "import { vi } from 'vitest';\nconst target = process.env.MOCKED;\nvi.mock(target);\n",
            ),
            ("test/d.test.ts", &test_source("../src/discount", VITEST)),
        ],
        "no_static_path",
    )
}

const JEST_SETUP: (&str, &str) = ("test/setup.js", "jest.mock('../src/discount');\n");

#[test]
fn a_jest_json_config_without_a_manifest_beside_it_governs_its_directory() -> Result<(), String> {
    // `jest --config test/jest.config.json`: no `package.json` under `test/`.
    let test = test_source("../src/discount", "");
    let manifest = ("package.json", "{ \"name\": \"x\" }\n");
    assert_class(
        "jest-json-nested",
        &[
            manifest,
            (
                "test/jest.config.json",
                "{ \"setupFiles\": [\"./setup.js\"] }\n",
            ),
            JEST_SETUP,
            ("test/d.test.ts", &test),
        ],
        "no_static_path",
    )?;
    // Twin: the same config in a sibling directory governs no test here.
    assert_class(
        "jest-json-sibling",
        &[
            manifest,
            (
                "other/jest.config.json",
                "{ \"setupFiles\": [\"../test/setup.js\"] }\n",
            ),
            JEST_SETUP,
            ("test/d.test.ts", &test),
        ],
        "exposed",
    )
}

#[test]
fn a_bare_require_in_a_setup_file_loads_the_helper() -> Result<(), String> {
    let test = test_source("../src/discount", "");
    let config = (
        "jest.config.js",
        "module.exports = { setupFilesAfterEnv: ['<rootDir>/test/setup.cjs'] };\n",
    );
    let helper = ("test/mock-discounts.cjs", "jest.mock('../src/discount');\n");
    assert_class(
        "setup-bare-require",
        &[
            config,
            ("test/setup.cjs", "require('./mock-discounts.cjs');\n"),
            helper,
            ("test/d.test.ts", &test),
        ],
        "no_static_path",
    )?;
    // Twin: `require.resolve` names the helper without running it.
    assert_class(
        "setup-require-resolve",
        &[
            config,
            (
                "test/setup.cjs",
                "require.resolve('./mock-discounts.cjs');\n",
            ),
            helper,
            ("test/d.test.ts", &test),
        ],
        "exposed",
    )
}

#[test]
fn a_type_only_re_export_does_not_load_the_helper() -> Result<(), String> {
    let test = test_source("../src/discount", VITEST);
    let config = ("vitest.config.ts", VITEST_CONFIG);
    let helper = (
        "test/mock-cart.ts",
        "import { vi } from 'vitest';\nvi.mock('../src/discount');\nexport type CartFixture = { total: number };\nexport const fixture = { total: 1 };\n",
    );
    for (label, setup) in [
        (
            "setup-export-type",
            "export type { CartFixture } from './mock-cart';\n",
        ),
        (
            "setup-export-type-star",
            "export type * from './mock-cart';\n",
        ),
    ] {
        assert_class(
            label,
            &[
                config,
                ("test/setup.ts", setup),
                helper,
                ("test/d.test.ts", &test),
            ],
            "exposed",
        )?;
    }
    // Twin: a value re-export of the same helper runs it.
    assert_class(
        "setup-export-value",
        &[
            config,
            ("test/setup.ts", "export { fixture } from './mock-cart';\n"),
            helper,
            ("test/d.test.ts", &test),
        ],
        "no_static_path",
    )
}

#[test]
fn an_absolute_setup_path_is_read_against_the_workspace_root() -> Result<(), String> {
    let test = test_source("../src/discount", "");
    for (label, config) in [
        (
            "jest-absolute-entry",
            "{ \"setupFiles\": [\"@ROOT@/test/setup.js\"] }\n",
        ),
        (
            "jest-absolute-root-dir",
            "{ \"rootDir\": \"@ROOT@/test\", \"setupFiles\": [\"<rootDir>/setup.js\"] }\n",
        ),
    ] {
        assert_class(
            label,
            &[
                ("jest.config.json", config),
                JEST_SETUP,
                ("test/d.test.ts", &test),
            ],
            "no_static_path",
        )?;
    }
    // Twin: an absolute path outside the workspace is not rebased onto the
    // workspace file with the same tail.
    assert_class(
        "jest-absolute-outside",
        &[
            (
                "jest.config.json",
                "{ \"setupFiles\": [\"/test/setup.js\"] }\n",
            ),
            JEST_SETUP,
            ("test/d.test.ts", &test),
        ],
        "exposed",
    )
}
