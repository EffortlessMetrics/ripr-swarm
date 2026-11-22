//! Class identity for a method owner (`reexports.rs`, `related_tests.rs`):
//! a `from M import Class` names the owner's class only when `M` is the
//! owner's module or a package whose `__init__.py` re-exports the class.
//! Established end to end through `analyze_diff` on on-disk workspaces shaped
//! like pluggy (`from pluggy import PluginManager` in `testing/conftest.py`,
//! the class in `src/pluggy/_manager.py`).

use super::reexport_tests::analyze_one_line;
use super::*;

const MANAGER_PY: &str = "class Manager:\n    def __init__(self, name):\n        self.name = name\n        self.blocked = []\n\n    def is_blocked(self, n):\n        return len(self.blocked) > n\n";

const MARKER_PY: &str = "class Marker:\n    pass\n";

const INIT_PY: &str = "from .manager import Manager\nfrom .marker import Marker\n\n__all__ = [\"Manager\", \"Marker\"]\n";

/// A different class with the owner class's name and method.
const RIVAL_PY: &str = "class Manager:\n    def __init__(self, name):\n        self.name = name\n\n    def is_blocked(self, n):\n        return False\n";

const FIXTURE_TEST: &str = "def test_is_blocked(pm):\n    assert pm.is_blocked(0) is False\n";

fn body_test(import: &str, class: &str) -> String {
    format!(
        "{import}\n\n\ndef test_is_blocked():\n    pm = {class}(\"example\")\n    assert pm.is_blocked(0) is False\n"
    )
}

fn conftest(imports: &str) -> String {
    format!(
        "import pytest\n\n{imports}\n\n\n@pytest.fixture\ndef pm():\n    return Manager(\"example\")\n"
    )
}

/// The finding for the changed `is_blocked` return, over the package plus
/// `extra` files.
fn is_blocked_finding(label: &str, extra: &[(&str, &str)]) -> Result<Finding, String> {
    let mut files = vec![
        ("src/pkg/manager.py", MANAGER_PY),
        ("src/pkg/marker.py", MARKER_PY),
        ("src/pkg/__init__.py", INIT_PY),
    ];
    for (path, contents) in extra {
        files.retain(|(existing, _)| existing != path);
        files.push((path, contents));
    }
    analyze_one_line(label, &files, "src/pkg/manager.py", 7)
}

fn body_finding(label: &str, import: &str, class: &str) -> Result<Finding, String> {
    is_blocked_finding(
        label,
        &[("tests/test_manager.py", &body_test(import, class))],
    )
}

fn fixture_finding(label: &str, extra: &[(&str, &str)]) -> Result<Finding, String> {
    let mut files = vec![("tests/test_manager.py", FIXTURE_TEST)];
    files.extend_from_slice(extra);
    is_blocked_finding(label, &files)
}

fn assert_fixture_limited(finding: &Finding, case: &str) {
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{case}");
    assert_eq!(
        finding.static_limit_kind,
        Some(StaticLimitKind::UnresolvedPytestFixture),
        "{case}"
    );
}

#[test]
fn a_class_imported_from_its_module_or_reexporting_package_binds_the_receiver() -> Result<(), String>
{
    for (case, import, class) in [
        ("owner module", "from pkg.manager import Manager", "Manager"),
        ("package re-export", "from pkg import Manager", "Manager"),
        ("aliased re-export", "from pkg import Manager as M", "M"),
        ("package receiver", "import pkg", "pkg.Manager"),
        ("aliased package receiver", "import pkg as p", "p.Manager"),
        (
            "module receiver",
            "import pkg.manager",
            "pkg.manager.Manager",
        ),
    ] {
        let finding = body_finding("class-identity-bound", import, class)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{case}");
    }
    Ok(())
}

#[test]
fn a_same_named_class_from_another_module_is_not_the_owner_class() -> Result<(), String> {
    // A module outside the workspace cannot define the workspace's class.
    let foreign = body_finding(
        "class-identity-foreign",
        "from vendor import Manager",
        "Manager",
    )?;
    assert_eq!(
        foreign.class,
        ExposureClass::WeaklyExposed,
        "foreign module"
    );
    // A workspace class of the same name and method is a different receiver:
    // the test never runs the changed line.
    for (case, import, class) in [
        ("rival class", "from rival import Manager", "Manager"),
        ("rival package receiver", "import rival", "rival.Manager"),
    ] {
        let finding = is_blocked_finding(
            "class-identity-rival",
            &[
                ("src/rival/__init__.py", RIVAL_PY),
                ("tests/test_manager.py", &body_test(import, class)),
            ],
        )?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{case}");
    }
    Ok(())
}

#[test]
fn a_package_that_binds_another_class_under_the_name_does_not_identify_the_owner()
-> Result<(), String> {
    // The package sits above the owner's module, but the `Manager` it
    // exports is not the owner's class.
    for (case, init) in [
        (
            "re-exports a sibling's class",
            "from .legacy import Manager\n",
        ),
        (
            "rebinds the name after the import",
            "from .manager import Manager\n\nManager = object\n",
        ),
        ("exports nothing", ""),
        (
            "star import the module's __all__ leaves out",
            "from .hidden import *\n",
        ),
    ] {
        let finding = is_blocked_finding(
            "class-identity-package-rival",
            &[
                ("src/pkg/__init__.py", init),
                ("src/pkg/legacy.py", RIVAL_PY),
                (
                    "src/pkg/hidden.py",
                    "__all__ = []\n\n\nclass Manager:\n    pass\n",
                ),
                (
                    "tests/test_manager.py",
                    &body_test("from pkg import Manager", "Manager"),
                ),
            ],
        )?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{case}");
    }
    Ok(())
}

#[test]
fn a_class_name_the_module_defines_twice_is_not_followed_through_a_reexport() -> Result<(), String>
{
    let twice = format!("{RIVAL_PY}\n\n{MANAGER_PY}");
    let line = twice
        .lines()
        .position(|line| line.contains("len(self.blocked) > n"))
        .map(|index| index + 1)
        .ok_or("the changed line is in the second definition")?;
    let finding = analyze_one_line(
        "class-identity-twice",
        &[
            ("src/pkg/manager.py", &twice),
            ("src/pkg/__init__.py", "from .manager import Manager\n"),
            (
                "tests/test_manager.py",
                &body_test("from pkg import Manager", "Manager"),
            ),
        ],
        "src/pkg/manager.py",
        line,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_literal_instance_fixture_resolves_through_a_reexported_class() -> Result<(), String> {
    for (case, imports) in [
        ("owner module", "from pkg.manager import Manager"),
        ("package re-export", "from pkg import Manager"),
        (
            "beside another name of the package",
            "from pkg import Manager\nfrom pkg import Marker",
        ),
        (
            "beside a fixture-free workspace module",
            "from pkg import Manager\nfrom pkg.marker import Marker",
        ),
        (
            "beside standard-library names",
            "from __future__ import annotations\n\nfrom pathlib import Path\nfrom typing import Any\nfrom unittest.mock import patch\n\nfrom pkg import Manager",
        ),
    ] {
        let finding = fixture_finding(
            "class-identity-fixture",
            &[("tests/conftest.py", &conftest(imports))],
        )?;
        assert_eq!(finding.static_limit_kind, None, "{case}");
        assert_eq!(finding.class, ExposureClass::Exposed, "{case}");
    }
    Ok(())
}

#[test]
fn a_fixture_built_from_another_class_keeps_the_limit() -> Result<(), String> {
    let finding = fixture_finding(
        "class-identity-fixture-rival",
        &[
            ("src/rival/__init__.py", RIVAL_PY),
            ("tests/conftest.py", &conftest("from rival import Manager")),
        ],
    )?;
    assert_fixture_limited(&finding, "rival class");
    let finding = fixture_finding(
        "class-identity-fixture-foreign",
        &[("tests/conftest.py", &conftest("from vendor import Manager"))],
    )?;
    assert_fixture_limited(&finding, "foreign class");
    Ok(())
}

#[test]
fn a_scope_import_that_may_bring_a_fixture_keeps_the_limit() -> Result<(), String> {
    const AUTOUSE: &str = "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef primed(request):\n    request.getfixturevalue(\"pm\").blocked.append(1)\n";
    // Control: the same layout with a plain object behind the import resolves.
    let control = fixture_finding(
        "class-identity-fixture-scope-control",
        &[
            (
                "tests/conftest.py",
                &conftest("from pkg import Manager\nfrom support.autouse import primed"),
            ),
            ("support/__init__.py", "from .autouse import primed\n"),
            ("support/autouse.py", "primed = object()\n"),
        ],
    )?;
    assert_eq!(
        control.class,
        ExposureClass::Exposed,
        "plain object control"
    );
    for (case, imports, support) in [
        (
            "module outside the workspace",
            "from pkg import Manager\nfrom vendor.plugin import primed",
            "",
        ),
        (
            "workspace module that defines a fixture",
            "from pkg import Manager\nfrom support.autouse import primed",
            AUTOUSE,
        ),
        (
            "workspace module that imports pytest",
            "from pkg import Manager\nfrom support.autouse import primed",
            "import pytest\n\nprimed = pytest.fixture(autouse=True)(lambda: None)\n",
        ),
        (
            "workspace module that re-exports a fixture module",
            "from pkg import Manager\nfrom support import primed",
            AUTOUSE,
        ),
        (
            "workspace module that star-imports",
            "from pkg import Manager\nfrom support.autouse import primed",
            "from vendor.plugin import *\n",
        ),
        (
            "workspace module that imports a module outside the workspace",
            "from pkg import Manager\nfrom support.autouse import primed",
            "from vendor.plugin import primed\n",
        ),
        (
            "relative import",
            "from pkg import Manager\nfrom .autouse import primed",
            "",
        ),
        (
            "workspace module that shadows a standard-library name",
            "from pkg import Manager\nfrom typing import primed",
            "",
        ),
    ] {
        let finding = fixture_finding(
            "class-identity-fixture-scope",
            &[
                ("tests/conftest.py", &conftest(imports)),
                ("support/__init__.py", "from .autouse import primed\n"),
                ("support/autouse.py", support),
                ("typing.py", AUTOUSE),
            ],
        )?;
        assert_fixture_limited(&finding, case);
    }
    Ok(())
}

#[test]
fn only_standard_library_names_are_standard_library_modules() {
    for name in ["__future__", "typing", "pathlib", "unittest", "cProfile"] {
        assert!(super::fixtures::is_stdlib_module(name), "{name}");
    }
    for name in ["pytest", "_pytest", "test", "conftest", "vendor", "", "typ"] {
        assert!(!super::fixtures::is_stdlib_module(name), "{name}");
    }
}
