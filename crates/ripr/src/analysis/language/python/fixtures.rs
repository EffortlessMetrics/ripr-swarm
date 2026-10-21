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

use super::admission::PythonAssertionAdmission;
use super::owners_tests::{decorator_names, test_imports};
use super::source_utils::{SourceText, text_for_range};
use super::{PythonFixtureDef, PythonImport, PythonTest};
use rustpython_parser::ast::{Expr, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet};
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
            // A later def of the same name replaces the fixture as the
            // module attribute pytest collects, including a renamed
            // `@pytest.fixture(name=...)` def that is not itself recorded.
            defs.retain(|def: &PythonFixtureDef| def.name != name);
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
        for parameter in &test.fixtures {
            if test.direct_parametrized.contains(parameter) {
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
    conftest_chain(test_file).find_map(|conftest| find(&conftest))
}

/// The `conftest.py` paths pytest consults for a test file, nearest first:
/// one per directory from the file's own up to the workspace root. Shared by
/// fixture resolution and conftest-driven assertion admission (#6657).
pub(super) fn conftest_chain(test_file: &Path) -> impl Iterator<Item = PathBuf> {
    std::iter::successors(test_file.parent(), |dir| dir.parent()).map(|dir| dir.join("conftest.py"))
}

/// Withhold `extraction_complete_no_assertion` from every test whose
/// directory chain holds a conftest that can run assertion-like code around
/// it: an autouse fixture, a `pytest_runtest_*` hook, `pytest_plugins`, or a
/// conftest RIPR could not read (#6657). A recognized assertion stays
/// recognized; only a clean no-assertion state is downgraded, so a conftest
/// can never grant `NoAssertionLike`.
///
/// `implicit_conftests` are the workspace-relative conftest paths known to
/// install implicit setup. `is_unreadable_conftest` answers for a chain
/// conftest that is not among the implicit set: true when it exists but was
/// not read (capped, excluded or unreadable), which counts the same way.
pub(super) fn taint_admission_by_conftest(
    tests: &mut [PythonTest],
    implicit_conftests: &BTreeSet<PathBuf>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        if test.assertion_admission != PythonAssertionAdmission::NoAssertionLike {
            continue;
        }
        if conftest_chain(&test.file).any(|conftest| {
            implicit_conftests.contains(&conftest) || is_unreadable_conftest(&conftest)
        }) {
            test.assertion_admission = PythonAssertionAdmission::Unresolved;
        }
    }
}
