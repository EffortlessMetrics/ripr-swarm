//! Tests for same-file value helpers that return the owner call (#15350): the
//! test is related reach-only (`helper_owner_call`), so the change no longer
//! reads `no_static_path`, and it never earns an oracle or `exposed`.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const CORE_PY: &str = "def total(n):\n    return n + 10\n";
const CORE: &str = "src/pkg/core.py";
const TESTS: &str = "tests/test_core.py";

fn finding_for(tests_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new(CORE), CORE_PY);
    let tests = extract_tests(Path::new(TESTS), tests_source);
    classify_change(Path::new(CORE), 2, "    return n + 10", &owners, &tests)
        .ok_or_else(|| "expected a finding for the changed owner line".to_string())
}

#[test]
fn given_a_helper_returning_the_owner_call_then_the_test_is_related_reach_only()
-> Result<(), String> {
    for tests in [
        // Module-level import.
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // The import inside the helper (iniconfig's `parse`), a docstring and
        // a straight-line local first.
        "def run(n):\n    \"\"\"Run it.\"\"\"\n    from pkg.core import total\n\n    value = n\n    return total(value)\n\n\ndef test_total():\n    assert run(1) == 11\n",
    ] {
        let finding = finding_for(tests)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{tests}");
        assert_eq!(finding.related_tests.len(), 1, "{tests}");
        let row = &finding.related_tests[0];
        assert_eq!(row.name, "test_total");
        assert_eq!(row.oracle, None, "a reach-only row carries no oracle");
        assert!(
            finding
                .evidence
                .iter()
                .any(|line| line == "related_test_relation: helper_owner_call (test_total)"),
            "{:?}",
            finding.evidence
        );
    }
    Ok(())
}

#[test]
fn given_a_helper_shape_that_is_not_established_then_the_test_stays_unrelated() -> Result<(), String>
{
    for tests in [
        // Decorated.
        "import functools\nfrom pkg.core import total\n\n\n@functools.cache\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // Branches before the return.
        "from pkg.core import total\n\n\ndef run(n):\n    if n > 0:\n        return total(n)\n    return 0\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // Async.
        "from pkg.core import total\n\n\nasync def run(n):\n    return total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // Defined twice: which one runs is not settled here.
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef run(n):\n    return n\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // The test binds the helper's name itself.
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    run = lambda n: n\n    assert run(1) == 11\n",
        // A fixture of the helper's name shadows it.
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total(run):\n    assert run(1) == 11\n",
        // The helper does not call the owner.
        "from pkg.core import total\n\n\ndef run(n):\n    return n + 11\n\n\ndef test_total():\n    assert run(1) == 12\n",
        // The test never calls the helper.
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert 11 == 11\n",
        // A generator helper.
        "from pkg.core import total\n\n\ndef run(n):\n    x = yield n\n    return total(x)\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // A `yield` inside the returned expression.
        "from pkg.core import total\n\n\ndef run(n):\n    return (yield) or total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // A lambda nested in a dict never runs the owner.
        "from pkg.core import total\n\n\ndef run(n):\n    return {\"f\": lambda: total(n)}\n\n\ndef test_total():\n    assert run(1)\n",
        // A walrus in the body.
        "from pkg.core import total\n\n\ndef run(n):\n    return (f := total)(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
        // A parameter named like the owner is what the helper calls.
        "from pkg.core import total\n\n\ndef run(total, n):\n    return total(n)\n\n\ndef test_total():\n    assert run(len, 1) == 11\n",
        // A local named like the owner.
        "from pkg.core import total\n\n\ndef run(n):\n    total = abs\n    return total(n)\n\n\ndef test_total():\n    assert run(1) == 1\n",
        // A parameter shadowing the alias the owner is imported as.
        "from pkg.core import total as t\n\n\ndef run(t, n):\n    return t(n)\n\n\ndef test_total():\n    assert run(abs, 1) == 1\n",
        // Called through an attribute, not the module-level helper.
        "from pkg.core import total\nimport helpers\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert helpers.run(1) == 11\n",
        // A longer name that ends in the helper's name.
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert rerun(1) == 11\n",
        // Only spelled in a string.
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert \"run(1)\" == \"run(1)\"\n",
    ] {
        let finding = finding_for(tests)?;
        assert_eq!(finding.class, ExposureClass::NoStaticPath, "{tests}");
        assert!(finding.related_tests.is_empty(), "{tests}");
    }
    Ok(())
}

#[test]
fn given_a_direct_owner_call_beside_the_helper_then_the_direct_relation_wins() -> Result<(), String>
{
    let tests = "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert total(1) == 11\n    assert run(1) == 11\n";
    let finding = finding_for(tests)?;

    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn given_a_helper_local_import_of_a_rival_owner_then_the_module_import_does_not_mask_it()
-> Result<(), String> {
    let rival = "def total(n):\n    return n\n";
    let mut owners: Vec<PythonOwner> = extract_owners(Path::new(CORE), CORE_PY)
        .into_iter()
        .chain(extract_owners(Path::new("src/pkg/other.py"), rival))
        .filter(|owner| owner.name == "total")
        .collect();
    super::related_tests::apply_rival_definition_modules(&mut owners);
    let owner = owners
        .iter()
        .find(|owner| owner.file == Path::new(CORE))
        .ok_or_else(|| "expected the core owner".to_string())?;
    let relation = |source: &str| {
        super::related_tests::related_test_candidates(
            owner,
            &extract_tests(Path::new(TESTS), source),
        )
        .first()
        .map(|candidate| candidate.relation)
    };

    // Control: the helper uses the module's import of the owner.
    assert_eq!(
        relation(
            "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_x():\n    assert run(1) == 11\n"
        ),
        Some(PythonRelationKind::HelperOwnerCall)
    );
    // The helper's own import of the rival shadows the module's import.
    assert_eq!(
        relation(
            "from pkg.core import total\n\n\ndef run(n):\n    from pkg.other import total\n    return total(n)\n\n\ndef test_x():\n    assert run(1) == 11\n"
        ),
        None
    );
    // The inverse: the module binds the rival, the helper imports the owner.
    assert_eq!(
        relation(
            "from pkg.other import total\n\n\ndef run(n):\n    from pkg.core import total\n    return total(n)\n\n\ndef test_x():\n    assert run(1) == 11\n"
        ),
        Some(PythonRelationKind::HelperOwnerCall)
    );
    // A test-local import never reaches the helper, which resolves `t` in
    // its own module: here the rival.
    assert_eq!(
        relation(
            "from pkg.other import total as t\n\n\ndef run(n):\n    return t(n)\n\n\ndef test_x():\n    from pkg.core import total as t\n    assert run(1) == 11\n"
        ),
        None
    );
    // And here the owner, whatever the test binds `t` to.
    assert_eq!(
        relation(
            "from pkg.core import total as t\n\n\ndef run(n):\n    return t(n)\n\n\ndef test_x():\n    from pkg.other import total as t\n    assert run(1) == 11\n"
        ),
        Some(PythonRelationKind::HelperOwnerCall)
    );
    Ok(())
}

#[test]
fn given_a_helper_parameter_shadowing_the_module_alias_then_the_test_stays_unrelated()
-> Result<(), String> {
    // Control: the module alias reaches the owner.
    let related = finding_for(
        "import pkg.core as core\n\n\ndef run(n):\n    return core.total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
    )?;
    assert_eq!(
        related.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        related.evidence
    );
    // The parameter `core` is whatever the test passes, not the module.
    let shadowed = finding_for(
        "import pkg.core as core\n\n\ndef run(core, n):\n    return core.total(n)\n\n\ndef test_total():\n    assert run(object(), 1) == 11\n",
    )?;
    assert_eq!(
        shadowed.class,
        ExposureClass::NoStaticPath,
        "{:?}",
        shadowed.evidence
    );
    Ok(())
}

#[test]
fn given_a_helper_parameter_named_like_a_longer_import_then_the_test_is_still_related()
-> Result<(), String> {
    // `n` is the alias of `subtotal`, not of `total`, so the parameter does
    // not rebind the owner.
    let finding = finding_for(
        "from pkg.core import total\nfrom pkg.other import subtotal as n\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    Ok(())
}

#[test]
fn given_a_value_helper_test_beside_a_same_module_caller_test_then_both_rows_stay()
-> Result<(), String> {
    let core_py = "def total(n):\n    return n + 10\n\n\ndef wrapper(n):\n    return total(n)\n";
    let owners = extract_owners(Path::new(CORE), core_py);
    let mut tests = extract_tests(
        Path::new("tests/test_wrapper.py"),
        "from pkg.core import wrapper\n\n\ndef test_wrapper():\n    assert wrapper(1) == 11\n",
    );
    tests.extend(extract_tests(
        Path::new(TESTS),
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
    ));
    let finding = classify_change(Path::new(CORE), 2, "    return n + 10", &owners, &tests)
        .ok_or_else(|| "expected a finding for the changed owner line".to_string())?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    let mut names: Vec<&str> = finding
        .related_tests
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["test_total", "test_wrapper"],
        "{:?}",
        finding.evidence
    );
    assert!(finding.related_tests.iter().all(|row| row.oracle.is_none()));
    Ok(())
}

#[test]
fn given_a_value_helper_test_beside_a_cross_module_caller_test_then_the_reading_stays_weak()
-> Result<(), String> {
    let mut owners = extract_owners(Path::new(CORE), CORE_PY);
    owners.extend(extract_owners(
        Path::new("src/pkg/api.py"),
        "from pkg.core import total\n\n\ndef api(n):\n    return total(n)\n",
    ));
    let mut tests = extract_tests(
        Path::new("tests/test_api.py"),
        "from pkg.api import api\n\n\ndef test_api():\n    assert api(1) == 11\n",
    );
    tests.extend(extract_tests(
        Path::new(TESTS),
        "from pkg.core import total\n\n\ndef run(n):\n    return total(n)\n\n\ndef test_total():\n    assert run(1) == 11\n",
    ));
    let finding = classify_change(Path::new(CORE), 2, "    return n + 10", &owners, &tests)
        .ok_or_else(|| "expected a finding for the changed owner line".to_string())?;
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "{:?}",
        finding.evidence
    );
    let mut names: Vec<&str> = finding
        .related_tests
        .iter()
        .map(|row| row.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["test_api", "test_total"], "{:?}", finding.evidence);
    assert!(finding.related_tests.iter().all(|row| row.oracle.is_none()));
    Ok(())
}
