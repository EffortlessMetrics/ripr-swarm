//! Workspace discovery for the TypeScript preview adapter.

use super::*;

const TYPESCRIPT_JAVASCRIPT_EXTENSIONS: &[&str] =
    &["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"];
const TEST_FILE_STEM_SUFFIXES: &[&str] = &[".test", "-test", "_test", ".spec", ".cy"];
const TEST_DIRECTORY_NAMES: &[&str] = &["test", "tests", "__tests__", "spec"];

/// Whether a path is a test file by convention.
///
/// The adapter recognizes three bounded convention families:
///
/// 1. Jest/Vitest-style `*.test.*` and `*.spec.*` files across every
///    TypeScript/JavaScript module extension the adapter accepts.
/// 2. Node test-runner names: `test.*`, `test-*`, `*-test.*`, and `*_test.*`.
/// 3. Cypress `*.cy.*` files and source files under exact `test`, `tests`,
///    `__tests__`, or Jasmine-style `spec` directory components.
///
/// Directory matching is component-based, not substring-based, so
/// `src/latest/foo.ts`, `test-utils/foo.ts`, and `src/contest.ts` remain
/// production paths. The source extension is checked before any naming or
/// directory convention. Test extraction remains fail-closed: a recognized
/// path contributes test evidence only when parsing finds supported
/// `test()` / `it()` / `describe()` call shapes.
pub(crate) fn is_test_file(path: &Path) -> bool {
    has_typescript_or_javascript_extension(path)
        && (has_test_file_stem(path) || has_test_directory_component(path))
}

fn has_typescript_or_javascript_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| TYPESCRIPT_JAVASCRIPT_EXTENSIONS.contains(&extension))
}

fn has_test_file_stem(path: &Path) -> bool {
    let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
        return false;
    };

    stem == "test"
        || stem
            .strip_prefix("test-")
            .is_some_and(|remainder| !remainder.is_empty())
        || TEST_FILE_STEM_SUFFIXES
            .iter()
            .any(|suffix| stem.ends_with(suffix))
}

fn has_test_directory_component(path: &Path) -> bool {
    path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(|name| TEST_DIRECTORY_NAMES.contains(&name))
    })
}

pub(crate) fn collect_workspace_typescript_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    visit_workspace(root, root, &mut out);
    out.sort();
    out
}

pub(crate) fn visit_workspace(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if name == ".git"
            || name == "target"
            || name == "node_modules"
            || name == ".ripr"
            || name == ".direnv"
        {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            visit_workspace(root, &path, out);
        } else if file_type.is_file() {
            let adapter = TypeScriptAdapter;
            if adapter.accepts_path(&path) {
                let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                out.push(relative);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_names_cover_supported_runner_conventions() {
        for path in [
            "src/cart.test.mts",
            "src/cart.spec.cts",
            "src/cart-test.mjs",
            "src/cart_test.cjs",
            "src/test-cart.ts",
            "src/test.tsx",
            "cypress/e2e/checkout.cy.ts",
            "src/Button.cy.tsx",
        ] {
            assert!(is_test_file(Path::new(path)), "expected test path: {path}");
        }
    }

    #[test]
    fn test_directories_cover_feature_named_test_files() {
        for path in [
            "test/body-size.ts",
            "tests/utils.ts",
            "src/__tests__/Header.tsx",
            "packages/core/test/index.mjs",
            "spec/request_contract.js",
        ] {
            assert!(is_test_file(Path::new(path)), "expected test path: {path}");
        }
    }

    #[test]
    fn test_layout_matching_stays_component_and_extension_bounded() {
        for path in [
            "src/latest/feature.ts",
            "test-utils/helper.ts",
            "src/contest.ts",
            "src/cart_test.txt",
            "spec/request_contract.md",
            "src/cypress.ts",
            "src/specification.ts",
        ] {
            assert!(
                !is_test_file(Path::new(path)),
                "unexpected test path: {path}"
            );
        }
    }

    #[test]
    fn newly_recognized_layouts_reach_supported_test_call_shapes() {
        let cases = [
            (
                "src/cart_test.mts",
                r#"test("node layout", () => { expect(cart()).toBe(1); });"#,
            ),
            (
                "cypress/e2e/cart.cy.ts",
                r#"describe("cart", () => { it("checks out", () => { expect(cart()).toBe(1); }); });"#,
            ),
            (
                "spec/cart_contract.js",
                r#"describe("cart", () => { it("keeps its contract", () => { expect(cart()).toBe(1); }); });"#,
            ),
        ];

        for (path, source) in cases {
            let path = Path::new(path);
            assert!(is_test_file(path), "expected test path: {}", path.display());
            assert_eq!(
                extract_tests(path, source).len(),
                1,
                "expected one extracted test for {}",
                path.display()
            );
        }
    }
}
