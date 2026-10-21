use super::assertion_selection::PythonAssertionFocus;
use super::mock_patch::mock_sites_are_disjoint_from_owner;
use super::owners_tests::{extract_owners, extract_tests};
use super::probe_shape::classify_probe_shape;
use super::related_tests::related_test_candidates;
use super::static_limits::static_limit_for_focused_change;
use crate::domain::StaticLimitKind;
use std::path::Path;

const OWNER_SOURCE: &str = "import json\nfrom .store import load\n\ndef total(items):\n    data = load(items)\n    return json.dumps(data)\n";

/// The changed line calls no imported symbol, so no other static limit masks
/// the mocked-module decision. The unused `json` import still counts as a name
/// the owner's file uses.
const END_TO_END_OWNER_SOURCE: &str = "import json\n\ndef total(items):\n    return len(items)\n";

const HEAD: &str = "from unittest.mock import patch\nfrom src.service import total\n\n";

fn owner() -> Result<super::PythonOwner, String> {
    extract_owners(Path::new("src/service.py"), OWNER_SOURCE)
        .into_iter()
        .find(|owner| owner.name == "total")
        .ok_or_else(|| "owner `total` was not extracted".to_string())
}

fn first_test(test_source: &str) -> Result<super::PythonTest, String> {
    let mut tests = extract_tests(Path::new("tests/test_service.py"), test_source);
    if tests.is_empty() {
        return Err(format!("no test extracted from:\n{test_source}"));
    }
    Ok(tests.remove(0))
}

fn disjoint(test_source: &str, owner_source: Option<&str>) -> Result<bool, String> {
    let test = first_test(test_source)?;
    assert!(
        super::test_has_mocked_module(&test),
        "fixture must be a mock-syntax test:\n{test_source}"
    );
    Ok(mock_sites_are_disjoint_from_owner(
        &test,
        &owner()?,
        owner_source,
    ))
}

#[test]
fn given_decorator_patching_an_unrelated_stdlib_name_then_it_is_disjoint() -> Result<(), String> {
    let source = format!(
        "{HEAD}@patch(\"os.getcwd\")\ndef test_total(mock_cwd):\n    assert total([1]) == \"[1]\"\n"
    );
    assert!(disjoint(&source, Some(OWNER_SOURCE))?);
    Ok(())
}

#[test]
fn given_body_patch_and_monkeypatch_string_targets_then_all_must_be_disjoint() -> Result<(), String>
{
    let both = format!(
        "{HEAD}def test_total(monkeypatch):\n    monkeypatch.setattr(\"os.getcwd\", lambda: \"/\")\n    with patch(target=\"time.time\"):\n        assert total([1]) == \"[1]\"\n"
    );
    assert!(disjoint(&both, Some(OWNER_SOURCE))?);
    let one_hits_owner = format!(
        "{HEAD}def test_total(monkeypatch):\n    monkeypatch.setattr(\"os.getcwd\", lambda: \"/\")\n    with patch(\"pkg.store.load\"):\n        assert total([1]) == \"[1]\"\n"
    );
    assert!(!disjoint(&one_hits_owner, Some(OWNER_SOURCE))?);
    Ok(())
}

#[test]
fn given_target_naming_owner_module_name_import_or_callee_then_limit_stays() -> Result<(), String> {
    for target in [
        "src.service.remote_total", // owner module (file stem)
        "src.service.total",        // owner name
        "json.dumps",               // module the owner imports
        "pkg.store.load",           // callee / relative import the owner uses
        "builtins.items",           // identifier the owner body uses
    ] {
        let source = format!(
            "{HEAD}@patch(\"{target}\")\ndef test_total(m):\n    assert total([1]) == \"[1]\"\n"
        );
        assert!(
            !disjoint(&source, Some(OWNER_SOURCE))?,
            "target {target} must keep the limit"
        );
    }
    Ok(())
}

#[test]
fn given_ambiguous_patch_shapes_then_limit_stays() -> Result<(), String> {
    let ambiguous = [
        // patch.object / patch.multiple / setitem / delattr never have a readable target
        "@patch.object(Client, \"fetch\")\ndef test_total(m):\n    assert total([1]) == 1\n",
        "@patch.multiple(\"os\", getcwd=None)\ndef test_total():\n    assert total([1]) == 1\n",
        "def test_total(monkeypatch):\n    monkeypatch.setitem(CONFIG, \"k\", 1)\n    assert total([1]) == 1\n",
        "def test_total(monkeypatch):\n    monkeypatch.delattr(\"os.getcwd\")\n    assert total([1]) == 1\n",
        // non-literal, computed, prefixed, concatenated or single-segment targets
        "TARGET = \"os.getcwd\"\n@patch(TARGET)\ndef test_total(m):\n    assert total([1]) == 1\n",
        "@patch(\"os.\" + NAME)\ndef test_total(m):\n    assert total([1]) == 1\n",
        "@patch(f\"os.{NAME}\")\ndef test_total(m):\n    assert total([1]) == 1\n",
        "@patch(\"os\")\ndef test_total(m):\n    assert total([1]) == 1\n",
        "@patch(new=1, target=\"os.getcwd\")\ndef test_total(m):\n    assert total([1]) == 1\n",
        // object-form monkeypatch.setattr
        "def test_total(monkeypatch):\n    monkeypatch.setattr(os, \"getcwd\", lambda: \"/\")\n    assert total([1]) == 1\n",
        // bare decorator whose arguments are not readable
        "@patch\ndef test_total(m):\n    assert total([1]) == 1\n",
        // a readable disjoint patch does not hide an unreadable sibling
        "@patch(\"os.getcwd\")\n@patch(OTHER)\ndef test_total(a, b):\n    assert total([1]) == 1\n",
    ];
    for case in ambiguous {
        let source = format!("{HEAD}{case}");
        assert!(
            !disjoint(&source, Some(OWNER_SOURCE))?,
            "must keep the limit:\n{case}"
        );
    }
    Ok(())
}

#[test]
fn given_no_owner_source_then_limit_stays() -> Result<(), String> {
    let source =
        format!("{HEAD}@patch(\"os.getcwd\")\ndef test_total(m):\n    assert total([1]) == 1\n");
    assert!(!disjoint(&source, None)?);
    Ok(())
}

#[test]
fn given_package_init_owner_then_package_name_in_target_keeps_limit() -> Result<(), String> {
    let init_source = "def total():\n    return 1\n";
    let owner = extract_owners(Path::new("src/billing/__init__.py"), init_source)
        .into_iter()
        .find(|owner| owner.name == "total")
        .ok_or_else(|| "owner missing".to_string())?;
    let test = first_test(&format!(
        "{HEAD}@patch(\"billing.rate\")\ndef test_total(m):\n    assert total() == 1\n"
    ))?;
    assert!(!mock_sites_are_disjoint_from_owner(
        &test,
        &owner,
        Some(init_source)
    ));
    Ok(())
}

fn limit_kind(
    test_source: &str,
    owner_source: Option<&str>,
) -> Result<Option<StaticLimitKind>, String> {
    let owner = extract_owners(Path::new("src/service.py"), END_TO_END_OWNER_SOURCE)
        .into_iter()
        .find(|owner| owner.name == "total")
        .ok_or_else(|| "owner `total` was not extracted".to_string())?;
    let tests = extract_tests(Path::new("tests/test_service.py"), test_source);
    let candidates = related_test_candidates(&owner, &tests);
    if !candidates
        .iter()
        .any(|candidate| candidate.relation.uses_oracle())
    {
        return Err("fixture must relate to the owner with an oracle".to_string());
    }
    let line = "    return len(items)";
    let focus = PythonAssertionFocus::for_change(classify_probe_shape(line).0, line, None);
    Ok(
        static_limit_for_focused_change(line, &focus, &owner, &candidates, owner_source)
            .map(|limit| limit.kind),
    )
}

#[test]
fn given_related_test_with_disjoint_patch_then_mocked_module_limit_is_dropped() -> Result<(), String>
{
    let unrelated =
        format!("{HEAD}@patch(\"os.getcwd\")\ndef test_total(m):\n    assert total([1]) == 1\n");
    assert_eq!(limit_kind(&unrelated, Some(END_TO_END_OWNER_SOURCE))?, None);
    // The same test without owner source keeps the pre-existing limit.
    assert_eq!(
        limit_kind(&unrelated, None)?,
        Some(StaticLimitKind::MockedModule)
    );
    // Patching something the owner uses keeps it even with the source.
    let related =
        format!("{HEAD}@patch(\"json.dumps\")\ndef test_total(m):\n    assert total([1]) == 1\n");
    assert_eq!(
        limit_kind(&related, Some(END_TO_END_OWNER_SOURCE))?,
        Some(StaticLimitKind::MockedModule)
    );
    Ok(())
}
