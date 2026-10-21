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

/// An owner that imports nothing, so no callee can live in another file.
const PURE_OWNER_SOURCE: &str = "def total(items):\n    return len(items)\n";

const HEAD: &str = "from unittest.mock import patch\nfrom src.service import total\n\n";

fn owner_from(source: &str) -> Result<super::PythonOwner, String> {
    extract_owners(Path::new("src/service.py"), source)
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
        &owner_from(owner_source.unwrap_or(OWNER_SOURCE))?,
        owner_source,
    ))
}

#[test]
fn given_decorator_patching_an_unrelated_stdlib_name_then_it_is_disjoint() -> Result<(), String> {
    let source = format!(
        "{HEAD}@patch(\"os.getcwd\")\ndef test_total(mock_cwd):\n    assert total([1]) == \"[1]\"\n"
    );
    assert!(disjoint(&source, Some(PURE_OWNER_SOURCE))?);
    Ok(())
}

#[test]
fn given_several_decorator_patches_then_all_must_be_disjoint() -> Result<(), String> {
    let both = format!(
        "{HEAD}@patch(\"os.getcwd\")\n@patch(target=\"time.time\")\ndef test_total(a, b):\n    assert total([1]) == \"[1]\"\n"
    );
    assert!(disjoint(&both, Some(PURE_OWNER_SOURCE))?);
    // The same disjoint pair stays limited when the owner reaches another file.
    assert!(!disjoint(&both, Some(OWNER_SOURCE))?);
    let json_owner = "import json\n\ndef total(items):\n    return json.dumps(items)\n";
    let one_hits_owner = format!(
        "{HEAD}@patch(\"os.getcwd\")\n@patch(\"json.dumps\")\ndef test_total(a, b):\n    assert total([1]) == \"[1]\"\n"
    );
    assert!(!disjoint(&one_hits_owner, Some(json_owner))?);
    Ok(())
}

#[test]
fn given_body_sites_hidden_spellings_dunders_or_local_imports_then_limit_stays()
-> Result<(), String> {
    let cases = [
        // a body-level patch or monkeypatch is never judged
        "@patch(\"os.getcwd\")\ndef test_total(m):\n    with patch(\"time.time\"):\n        assert total([1]) == 1\n",
        "def test_total(monkeypatch):\n    monkeypatch.setattr(\"os.getcwd\", lambda: \"/\")\n    assert total([1]) == 1\n",
        // a disjoint decorator beside a spelling the scan cannot read
        "@patch(\"os.getcwd\")\ndef test_total(mocker):\n    mocker.spy(service, \"dep\")\n    assert total([1]) == 1\n",
        "@patch(\"os.getcwd\")\ndef test_total(m):\n    sys.modules[\"x\"] = fake\n    assert total([1]) == 1\n",
        // a dunder target rewrites an operator the owner may execute implicitly
        "@patch(\"shop.Money.__add__\")\ndef test_total(m):\n    assert total([1]) == 1\n",
    ];
    for case in cases {
        let source = format!("{HEAD}{case}");
        assert!(
            !disjoint(&source, Some(PURE_OWNER_SOURCE))?,
            "must keep the limit:\n{case}"
        );
    }
    // A function-local import can reach another file.
    let local_import = "def total(items):\n    import helpers\n    return helpers.run(items)\n";
    let plain =
        format!("{HEAD}@patch(\"os.getcwd\")\ndef test_total(m):\n    assert total([1]) == 1\n");
    assert!(!disjoint(&plain, Some(local_import))?);
    // An aliased patch import hides the patch spelling in the body.
    let aliased = "from unittest.mock import patch as p\nfrom src.service import total\n\n@patch(\"os.getcwd\")\ndef test_total(m):\n    with p(\"shop.Money\"):\n        assert total([1]) == 1\n";
    assert!(!disjoint(aliased, Some(PURE_OWNER_SOURCE))?);
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
        // patch.dict beside a readable disjoint patch is an unreadable substitution site
        "@patch(\"os.getcwd\")\n@patch.dict(\"sys.modules\", {\"x\": None})\ndef test_total(m):\n    assert total([1]) == 1\n",
        "@patch(\"os.getcwd\")\n@mock.patch.dict(\"sys.modules\", {\"x\": None})\ndef test_total(m):\n    assert total([1]) == 1\n",
        "@patch(\"os.getcwd\")\ndef test_total(m):\n    with patch.dict(\"sys.modules\", {\"x\": None}):\n        assert total([1]) == 1\n",
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

#[test]
fn given_owner_reaching_another_file_or_a_test_method_then_limit_stays() -> Result<(), String> {
    let disjoint_patch =
        format!("{HEAD}@patch(\"os.getcwd\")\ndef test_total(m):\n    assert total([1]) == 1\n");
    let helper_owner = "from .helpers import run\n\ndef total(items):\n    return run(items)\n";
    assert!(!disjoint(&disjoint_patch, Some(helper_owner))?);
    let super_owner = "class Base:\n    pass\n\nclass Child(Base):\n    def total(self):\n        return super().total()\n";
    assert!(!disjoint(&disjoint_patch, Some(super_owner))?);
    // A class-level `@patch` is not retained on the method, so methods keep the limit.
    let method = format!(
        "{HEAD}@patch(\"time.time\")\nclass TestTotal:\n    @patch(\"os.getcwd\")\n    def test_total(self, m, t):\n        assert total([1]) == 1\n"
    );
    assert!(!disjoint(&method, Some(PURE_OWNER_SOURCE))?);
    Ok(())
}
