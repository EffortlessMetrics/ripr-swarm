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

/// Mark every test whose directory chain holds a conftest declaring any
/// fixture or hook, or one RIPR could not read, as implicitly reachable
/// (#5491 `missing_input` fails closed by existence).
pub(super) fn mark_implicit_reach_by_conftest(
    tests: &mut [PythonTest],
    reach_conftests: &BTreeSet<PathBuf>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        if conftest_chain(&test.file).any(|conftest| {
            reach_conftests.contains(&conftest) || is_unreadable_conftest(&conftest)
        }) {
            test.constant_rebinding.implicit_reach = true;
        }
    }
}

/// A built-in fixture (`tmp_path`, `monkeypatch`, ...) is trusted not to
/// assert, but a conftest may redefine it (`def tmp_path(): ...` or
/// `@pytest.fixture(name="tmp_path")`) and assert in its teardown. Withhold
/// `extraction_complete_no_assertion` from a test that requests a built-in
/// fixture while a chain conftest declares any fixture or hook, or could not
/// be read (#5491). Fails closed by existence, like the implicit-reach pass.
pub(super) fn taint_builtin_fixture_overrides_by_conftest(
    tests: &mut [PythonTest],
    reach_conftests: &BTreeSet<PathBuf>,
    mut is_unreadable_conftest: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        if test.assertion_admission != PythonAssertionAdmission::NoAssertionLike
            || !test
                .fixtures
                .iter()
                .any(|name| super::admission::PYTEST_BUILTIN_FIXTURES.contains(&name.as_str()))
        {
            continue;
        }
        if conftest_chain(&test.file).any(|conftest| {
            reach_conftests.contains(&conftest) || is_unreadable_conftest(&conftest)
        }) {
            test.assertion_admission = PythonAssertionAdmission::Unresolved;
        }
    }
}

/// Record, per test, the modules it imports by a bare name that exist beside
/// the test file (#5491). `is_present` answers for a workspace-relative path.
/// A single-file module (`<dir>/<name>.py`) is recorded by that file so the
/// row pass can exempt the module under test; a package or directory is
/// recorded by its path and never exempted.
pub(super) fn record_bare_sibling_imports(
    tests: &mut [PythonTest],
    mut is_present: impl FnMut(&Path) -> bool,
) {
    for test in tests {
        if test.assertion_admission != PythonAssertionAdmission::NoAssertionLike {
            continue;
        }
        let dir = test.file.parent().unwrap_or(Path::new(""));
        let mut found: BTreeSet<PathBuf> = BTreeSet::new();
        for import in &test.imports {
            let module = if import.source_module.is_empty() {
                import.imported.as_str()
            } else {
                import.source_module.as_str()
            };
            let Some(root) = module.split('.').next().filter(|root| !root.is_empty()) else {
                continue;
            };
            let file = dir.join(format!("{root}.py"));
            if is_present(&file) {
                found.insert(file);
            } else {
                let package = dir.join(root);
                if is_present(&package) {
                    found.insert(package);
                }
            }
        }
        test.bare_sibling_modules = found.into_iter().collect();
    }
}
