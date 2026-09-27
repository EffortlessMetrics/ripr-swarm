//! Owner-module mock forms beyond a string-literal `./`/`../` path on a
//! literal `vi`/`jest` object (#4294). Each form replaces the owner module,
//! so a test that calls the owner through it reaches the mock, not the
//! changed code, and must not be credited.

use super::*;

const CART: &str = "export class Cart {\n  private items: number[] = [];\n\n  add(qty: number): void {\n    this.items.push(qty);\n  }\n\n  total(): number {\n    return this.items.reduce((a, b) => a + b, 1);\n  }\n}\n";
const TOTAL_LINE: (usize, &str) = (9, "    return this.items.reduce((a, b) => a + b, 1);");
const BODY: &str =
    "\nit('totals', () => {\n  const cart = new Cart();\n  expect(cart.total()).toBe(1);\n});\n";

fn total_class(label: &str, header: &str) -> Result<ExposureClass, String> {
    let root = ts_unique_tempdir(label)?;
    ts_write_file(&root.join("src/cart.ts"), CART)?;
    ts_write_file(
        &root.join("tests/cart.test.ts"),
        &format!("{header}import {{ Cart }} from '../src/cart';\n{BODY}"),
    )?;
    let result = TypeScriptAdapter.analyze_diff(
        &ts_analysis_options(root.clone()),
        &OraclePolicy::default(),
        &[changed_with_lines("src/cart.ts", &[TOTAL_LINE])],
    )?;
    let _ = std::fs::remove_dir_all(&root);
    result
        .findings
        .into_iter()
        .find(|finding| {
            finding
                .probe
                .owner
                .as_ref()
                .is_some_and(|owner| owner.0.ends_with("total"))
        })
        .map(|finding| finding.class)
        .ok_or_else(|| format!("{label}: expected a finding for `total`"))
}

fn assert_class(label: &str, header: &str, expected: ExposureClass) -> Result<(), String> {
    assert_all(&[(label, header)], expected)
}

/// Checks every case and reports each one that misses, so a regression in
/// one form does not hide another.
fn assert_all(cases: &[(&str, &str)], expected: ExposureClass) -> Result<(), String> {
    let misses: Vec<String> = cases
        .iter()
        .filter_map(|(label, header)| match total_class(label, header) {
            Ok(class) if class == expected => None,
            Ok(class) => Some(format!("{label}: expected {expected:?}, got {class:?}")),
            Err(error) => Some(error),
        })
        .collect();
    if misses.is_empty() {
        Ok(())
    } else {
        Err(misses.join("; "))
    }
}

const FACTORY: &str = "() => ({ Cart: class { total() { return 1; } } })";

#[test]
fn unmocked_owner_relates() -> Result<(), String> {
    // Control: the fixture relates when nothing mocks the owner, so every
    // `NoStaticPath` below comes from the mock, not from the fixture.
    assert_class(
        "control",
        "import { it, expect } from 'vitest';\n",
        ExposureClass::Exposed,
    )
}

#[test]
fn literal_relative_mock_withholds() -> Result<(), String> {
    assert_class(
        "literal",
        &format!(
            "import {{ it, expect, vi }} from 'vitest';\nvi.mock('../src/cart', {FACTORY});\n"
        ),
        ExposureClass::NoStaticPath,
    )
}

#[test]
fn typed_import_mock_withholds() -> Result<(), String> {
    assert_class(
        "typed-import",
        &format!(
            "import {{ it, expect, vi }} from 'vitest';\nvi.mock(import('../src/cart'), {FACTORY});\n"
        ),
        ExposureClass::NoStaticPath,
    )
}

#[test]
fn root_relative_mock_withholds() -> Result<(), String> {
    // The runner's root is not modelled, so a root-relative specifier that is
    // a path-segment suffix of the owner module (`/cart` under a `src` root)
    // also withholds: the unknown root is resolved toward the safe side.
    let cases: Vec<(&str, String)> = [
        ("root", "/src/cart"),
        ("root-ext", "/src/cart.ts"),
        ("root-subdir", "/cart"),
        ("root-double-slash", "//src/cart"),
        ("root-dot-segment", "/src/./cart"),
        ("root-query", "/src/cart?raw"),
        ("root-backslash", "\\\\src\\\\cart"),
    ]
    .into_iter()
    .map(|(label, path)| {
        (
            label,
            format!("import {{ it, expect, vi }} from 'vitest';\nvi.mock('{path}', {FACTORY});\n"),
        )
    })
    .collect();
    let cases: Vec<(&str, &str)> = cases
        .iter()
        .map(|(label, header)| (*label, header.as_str()))
        .collect();
    assert_all(&cases, ExposureClass::NoStaticPath)
}

#[test]
fn aliased_runner_object_mock_withholds() -> Result<(), String> {
    assert_all(
        &[
            (
                "alias-vi",
                "import { it, expect, vi as v } from 'vitest';\nv.mock('../src/cart');\n",
            ),
            (
                "alias-jest",
                "import { it, expect, jest as j } from '@jest/globals';\nj.mock('../src/cart');\n",
            ),
            (
                "namespace-vi",
                "import { it, expect } from 'vitest';\nimport * as vt from 'vitest';\nvt.vi.mock('../src/cart');\n",
            ),
            (
                "const-alias",
                "import { it, expect, vi } from 'vitest';\nconst mocker = vi;\nmocker.doMock('../src/cart');\n",
            ),
            (
                "computed",
                "import { it, expect, vi } from 'vitest';\nvi['mock']('../src/cart');\n",
            ),
            (
                "computed-template",
                "import { it, expect, vi } from 'vitest';\nvi[`mock`]('../src/cart');\n",
            ),
            (
                "namespace-computed",
                "import { it, expect } from 'vitest';\nimport * as vt from 'vitest';\nvt['vi'].mock('../src/cart');\n",
            ),
            (
                "destructured-method",
                "import { it, expect, vi } from 'vitest';\nconst { mock } = vi;\nmock('../src/cart');\n",
            ),
            (
                "destructured-namespace",
                "import { it, expect } from 'vitest';\nimport * as vt from 'vitest';\nconst { vi: v } = vt;\nv.mock('../src/cart');\n",
            ),
            (
                "exported-alias",
                "import { it, expect, vi } from 'vitest';\nexport const m = vi;\nm.mock('../src/cart');\n",
            ),
            (
                "nested-alias",
                "import { it, expect, vi, describe } from 'vitest';\ndescribe('d', () => {\n  const m = vi;\n  m.mock('../src/cart');\n});\n",
            ),
        ],
        ExposureClass::NoStaticPath,
    )
}

#[test]
fn wrapped_mock_calls_withhold() -> Result<(), String> {
    assert_all(
        &[
            (
                "optional-call",
                "import { it, expect, vi } from 'vitest';\nvi.mock?.('../src/cart');\n",
            ),
            (
                "optional-member",
                "import { it, expect, vi } from 'vitest';\nvi?.mock('../src/cart');\n",
            ),
            (
                "parenthesized",
                "import { it, expect, vi } from 'vitest';\n(vi).mock('../src/cart');\n",
            ),
            (
                "sequence-callee",
                "import { it, expect, vi } from 'vitest';\n(0, vi.mock)('../src/cart');\n",
            ),
            (
                "awaited",
                "import { it, expect, vi } from 'vitest';\nawait vi.doMock('../src/cart');\n",
            ),
            (
                "arrow-initializer",
                "import { it, expect, vi, beforeAll } from 'vitest';\nconst setup = () => { vi.doMock('../src/cart'); };\nbeforeAll(setup);\n",
            ),
            (
                "asserted-specifier",
                "import { it, expect, vi } from 'vitest';\nvi.mock('../src/cart' as string);\n",
            ),
            (
                "require-resolve",
                "import { it, expect } from '@jest/globals';\njest.mock(require.resolve('../src/cart'));\n",
            ),
        ],
        ExposureClass::NoStaticPath,
    )
}

#[test]
fn other_runner_module_mock_apis_withhold() -> Result<(), String> {
    assert_all(
        &[
            (
                "jest-esm",
                "import { it, expect, jest } from '@jest/globals';\njest.unstable_mockModule('../src/cart', () => ({}));\n",
            ),
            (
                "jest-set-mock",
                "import { it, expect, jest } from '@jest/globals';\njest.setMock('../src/cart', {});\n",
            ),
            (
                "bun-mock-module",
                "import { it, expect, mock } from 'bun:test';\nmock.module('../src/cart', () => ({}));\n",
            ),
            (
                "node-mock-module",
                "import { it, mock } from 'node:test';\nimport { expect } from 'vitest';\nmock.module('../src/cart', {});\n",
            ),
        ],
        ExposureClass::NoStaticPath,
    )
}

#[test]
fn opaque_mock_specifier_fails_closed() -> Result<(), String> {
    assert_all(
        &[
            (
                "variable",
                "import { it, expect, vi } from 'vitest';\nconst target = '../src/cart';\nvi.mock(target);\n",
            ),
            (
                "template",
                "import { it, expect, vi } from 'vitest';\nconst dir = 'src';\nvi.mock(`../${dir}/cart`);\n",
            ),
            (
                "spread",
                "import { it, expect, vi } from 'vitest';\nconst args = ['../src/cart'] as const;\nvi.mock(...args);\n",
            ),
            (
                "dynamic-import",
                "import { it, expect, vi } from 'vitest';\nconst target = '../src/cart';\nvi.mock(import(target));\n",
            ),
        ],
        ExposureClass::NoStaticPath,
    )
}

#[test]
fn mocks_of_other_modules_still_relate() -> Result<(), String> {
    // Alternate proof: each widened form keeps resolving its specifier, so a
    // mock of a different module, or a `.mock` on an object that is not the
    // test runner, leaves the owner relation in place.
    assert_all(
        &[
            (
                "other-typed-import",
                "import { it, expect, vi } from 'vitest';\nvi.mock(import('../src/other'));\n",
            ),
            (
                "other-root",
                "import { it, expect, vi } from 'vitest';\nvi.mock('/src/other');\n",
            ),
            (
                "other-root-partial-segment",
                "import { it, expect, vi } from 'vitest';\nvi.mock('/art');\n",
            ),
            (
                "other-require-resolve",
                "import { it, expect } from '@jest/globals';\njest.mock(require.resolve('../src/other'));\n",
            ),
            (
                "other-root-parent-free",
                "import { it, expect, vi } from 'vitest';\nvi.mock('/src/./other?raw');\n",
            ),
            (
                "non-runner-mock-module",
                "import { it, expect } from 'vitest';\nimport { mock } from './helpers';\nmock.module('../src/cart');\n",
            ),
            (
                "non-runner-destructure",
                "import { it, expect } from 'vitest';\nimport api from './api';\nconst { mock } = api;\nmock('../src/cart');\n",
            ),
            (
                "other-alias",
                "import { it, expect, vi as v } from 'vitest';\nv.mock('../src/other');\n",
            ),
            (
                "non-runner-object",
                "import { it, expect } from 'vitest';\nimport fetchMock from 'fetch-mock';\nfetchMock.mock('/src/cart', 200);\n",
            ),
            (
                "non-runner-vi-member",
                "import { it, expect } from 'vitest';\nimport * as helpers from './helpers';\nhelpers.vi.mock('../src/cart');\n",
            ),
        ],
        ExposureClass::Exposed,
    )
}
