//! pytest fixture definitions and test-to-fixture resolution (#4563).
//!
//! A test that receives the owner's result through a fixture parameter does
//! not call the owner in its own body. This module records module-level
//! `@pytest.fixture` functions per file and attaches, to each test, the
//! definitions its parameters name: the test's own module first, then
//! `conftest.py` files up the directory chain (nearest wins, as pytest
//! resolves them). Facts are advisory: a fixture that cannot be resolved
//! simply leaves the test unrelated, and a renamed fixture
//! (`@pytest.fixture(name=...)`) is not recorded.

use super::owners_tests::{decorator_names, test_imports};
use super::source_utils::{SourceText, text_for_range};
use super::{PythonFixtureDef, PythonImport, PythonTest};
use rustpython_parser::ast::{Expr, Ranged, Stmt};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Most resolved fixture definitions attached to one test.
const MAX_FIXTURE_DEFS_PER_TEST: usize = 8;

/// Module-level `@pytest.fixture` functions of one file.
pub(super) fn collect_fixture_defs(
    file: &Path,
    source: &SourceText<'_>,
    statements: &[Stmt],
    module_imports: &[PythonImport],
) -> Vec<PythonFixtureDef> {
    let mut defs = Vec::new();
    for stmt in statements {
        let (name, decorators, body, range) = match stmt {
            Stmt::FunctionDef(function) => (
                function.name.as_str(),
                &function.decorator_list,
                &function.body,
                function.range,
            ),
            Stmt::AsyncFunctionDef(function) => (
                function.name.as_str(),
                &function.decorator_list,
                &function.body,
                function.range,
            ),
            _ => continue,
        };
        if !is_plain_fixture(source, decorators) {
            continue;
        }
        defs.push(PythonFixtureDef {
            name: name.to_string(),
            file: file.to_path_buf(),
            body_text: text_for_range(source, range),
            imports: test_imports(file, module_imports, body),
        });
    }
    defs
}

/// A `fixture` / `pytest.fixture` / `pytest_asyncio.fixture` decorator that
/// does not rename the fixture.
fn is_plain_fixture(source: &str, decorators: &[Expr]) -> bool {
    decorators.iter().any(|decorator| {
        let names = decorator_names(std::slice::from_ref(decorator));
        names
            .iter()
            .any(|name| name == "fixture" || name.ends_with(".fixture"))
            && !text_for_range(source, decorator.range())
                .replace(' ', "")
                .contains("name=")
    })
}

/// Attach to every test the fixture definitions its parameters name.
pub(super) fn attach_fixture_defs(
    tests: &mut [PythonTest],
    defs_by_file: &BTreeMap<PathBuf, Vec<PythonFixtureDef>>,
) {
    if defs_by_file.is_empty() {
        return;
    }
    for test in tests {
        let mut attached: Vec<PythonFixtureDef> = Vec::new();
        // A direct (non-indirect) parametrize argname is supplied by pytest,
        // not resolved as a fixture.
        let parametrized = test
            .parametrize
            .as_ref()
            .map(|found| found.argnames())
            .unwrap_or_default();
        for parameter in &test.fixtures {
            if parametrized.contains(parameter) {
                continue;
            }
            if attached.len() >= MAX_FIXTURE_DEFS_PER_TEST {
                break;
            }
            if let Some(def) = resolve(&test.file, parameter, defs_by_file) {
                attached.push(def.clone());
            }
        }
        test.fixture_defs = attached;
    }
}

fn resolve<'a>(
    test_file: &Path,
    name: &str,
    defs_by_file: &'a BTreeMap<PathBuf, Vec<PythonFixtureDef>>,
) -> Option<&'a PythonFixtureDef> {
    let find = |file: &Path| {
        defs_by_file
            .get(file)
            // The last same-name definition in a file is the one pytest binds.
            .and_then(|defs| defs.iter().rev().find(|def| def.name == name))
    };
    if let Some(def) = find(test_file) {
        return Some(def);
    }
    let mut dir = test_file.parent();
    while let Some(current) = dir {
        if let Some(def) = find(&current.join("conftest.py")) {
            return Some(def);
        }
        dir = current.parent();
    }
    None
}
