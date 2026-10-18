//! Tests for the weak `fixture_owner_call` relation (#4563): a test that
//! names a pytest fixture whose body calls the owner is related with
//! reach-only evidence and never earns an oracle or `exposed`.

use super::fixtures::attach_fixture_defs;
use super::source_facts::extract_source_facts;
use super::source_utils::is_test_file;
use super::*;
use std::collections::BTreeMap;
use std::path::Path;

const UTILS_PY: &str = "def _short(text, limit):\n    if limit > 3:\n        return text[:limit]\n    return text[:3]\n";

/// Owners from production files and tests (with fixtures attached) from test
/// files, the way the workspace pass builds them.
fn workspace(files: &[(&str, &str)]) -> (Vec<PythonOwner>, Vec<PythonTest>) {
    let mut owners = Vec::new();
    let mut tests = Vec::new();
    let mut defs: BTreeMap<PathBuf, Vec<PythonFixtureDef>> = BTreeMap::new();
    for (path, source) in files {
        let facts = extract_source_facts(Path::new(path), source);
        if !facts.fixtures.is_empty() {
            defs.insert(PathBuf::from(path), facts.fixtures.clone());
        }
        if is_test_file(Path::new(path)) {
            tests.extend(facts.tests);
        } else {
            owners.extend(facts.owners);
        }
    }
    attach_fixture_defs(&mut tests, &defs);
    (owners, tests)
}

fn relations(files: &[(&str, &str)]) -> Result<Vec<(String, &'static str)>, String> {
    let (owners, tests) = workspace(files);
    let owner = owners
        .iter()
        .find(|owner| owner.name == "_short")
        .ok_or_else(|| "fixture must parse owner `_short`".to_string())?;
    Ok(related_tests::related_test_candidates(owner, &tests)
        .iter()
        .map(|candidate| (candidate.test.name.clone(), candidate.relation.as_str()))
        .collect())
}

fn classify(files: &[(&str, &str)]) -> Result<Finding, String> {
    let (owners, tests) = workspace(files);
    classify_change(
        Path::new("src/utils.py"),
        2,
        "    if limit > 3:",
        &owners,
        &tests,
    )
    .ok_or_else(|| "expected a finding".to_string())
}

const SAME_MODULE: &str = "import pytest\nfrom src.utils import _short\n\n\n@pytest.fixture\ndef out():\n    return _short('abcd', 3)\n\n\ndef test_short(out):\n    assert out == 'abc'\n";

#[test]
fn given_a_same_module_fixture_that_calls_the_owner_then_the_test_is_related_weakly()
-> Result<(), String> {
    let files = [
        ("src/utils.py", UTILS_PY),
        ("tests/test_utils.py", SAME_MODULE),
    ];
    assert_eq!(
        relations(&files)?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    let finding = classify(&files)?;
    assert_ne!(finding.class, ExposureClass::NoStaticPath);
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].oracle, None);
    Ok(())
}

#[test]
fn given_a_module_attribute_call_in_the_fixture_then_the_test_is_related_weakly()
-> Result<(), String> {
    let test = "import pytest\nfrom src import utils\n\n\n@pytest.fixture\ndef out():\n    return utils._short('abcd', 3)\n\n\ndef test_short(out):\n    assert out == 'abc'\n";
    assert_eq!(
        relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", test)])?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    Ok(())
}

#[test]
fn given_a_conftest_fixture_that_calls_the_owner_then_the_test_is_related_weakly()
-> Result<(), String> {
    let conftest = "import pytest\nfrom src.utils import _short\n\n\n@pytest.fixture\ndef out():\n    return _short('abcd', 3)\n";
    let test = "def test_short(out):\n    assert out == 'abc'\n";
    assert_eq!(
        relations(&[
            ("src/utils.py", UTILS_PY),
            ("tests/conftest.py", conftest),
            ("tests/test_utils.py", test),
        ])?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    Ok(())
}

#[test]
fn given_a_fixture_that_does_not_call_the_owner_then_the_test_stays_unrelated() -> Result<(), String>
{
    let test = "import pytest\n\n\n@pytest.fixture\ndef out():\n    return 'abc'\n\n\ndef test_short(out):\n    assert out == 'abc'\n";
    let files = [("src/utils.py", UTILS_PY), ("tests/test_utils.py", test)];
    assert!(relations(&files)?.is_empty());
    assert_eq!(classify(&files)?.class, ExposureClass::NoStaticPath);
    Ok(())
}

#[test]
fn given_a_test_that_rebinds_the_fixture_name_then_the_test_stays_unrelated() -> Result<(), String>
{
    let test = SAME_MODULE.replace(
        "    assert out == 'abc'",
        "    out = 'abc'\n    assert out == 'abc'",
    );
    assert!(relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?.is_empty());
    Ok(())
}

#[test]
fn given_a_module_fixture_that_overrides_the_conftest_fixture_then_the_nearest_wins()
-> Result<(), String> {
    let conftest = "import pytest\nfrom src.utils import _short\n\n\n@pytest.fixture\ndef out():\n    return _short('abcd', 3)\n";
    let test = "import pytest\n\n\n@pytest.fixture\ndef out():\n    return 'abc'\n\n\ndef test_short(out):\n    assert out == 'abc'\n";
    assert!(
        relations(&[
            ("src/utils.py", UTILS_PY),
            ("tests/conftest.py", conftest),
            ("tests/test_utils.py", test),
        ])?
        .is_empty()
    );
    Ok(())
}

#[test]
fn given_a_renamed_fixture_then_no_fixture_definition_is_recorded() -> Result<(), String> {
    let test = SAME_MODULE.replace("@pytest.fixture", "@pytest.fixture(name='out')");
    assert!(relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?.is_empty());
    Ok(())
}

#[test]
fn given_a_test_that_calls_the_owner_directly_then_the_direct_relation_wins() -> Result<(), String>
{
    let test = SAME_MODULE.replace(
        "    assert out == 'abc'",
        "    assert _short('abcd', 3) == 'abc'",
    );
    assert_eq!(
        relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?,
        vec![("test_short".to_string(), "syntactic_call")]
    );
    Ok(())
}

#[test]
fn given_a_direct_parametrize_argname_matching_a_fixture_then_the_test_stays_unrelated()
-> Result<(), String> {
    let test = SAME_MODULE.replace(
        "def test_short(out):",
        "@pytest.mark.parametrize('out', ['abc'])\ndef test_short(out):",
    );
    assert!(relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?.is_empty());
    Ok(())
}

#[test]
fn given_an_indirect_parametrize_argname_then_the_fixture_is_still_resolved() -> Result<(), String>
{
    let test = SAME_MODULE.replace(
        "def test_short(out):",
        "@pytest.mark.parametrize('out', ['abc'], indirect=True)\ndef test_short(out):",
    );
    assert_eq!(
        relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    Ok(())
}

#[test]
fn given_duplicate_same_name_fixtures_then_the_last_definition_wins() -> Result<(), String> {
    let calls = "@pytest.fixture\ndef out():\n    return _short('abcd', 3)\n\n\n";
    let plain = "@pytest.fixture\ndef out():\n    return 'abc'\n\n\n";
    let head = "import pytest\nfrom src.utils import _short\n\n\n";
    let tail = "def test_short(out):\n    assert out == 'abc'\n";
    let last_plain = format!("{head}{calls}{plain}{tail}");
    assert!(
        relations(&[
            ("src/utils.py", UTILS_PY),
            ("tests/test_utils.py", &last_plain)
        ])?
        .is_empty()
    );
    let last_calls = format!("{head}{plain}{calls}{tail}");
    assert_eq!(
        relations(&[
            ("src/utils.py", UTILS_PY),
            ("tests/test_utils.py", &last_calls)
        ])?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    Ok(())
}

#[test]
fn given_more_fixtures_than_the_cap_then_only_the_first_eight_are_attached() -> Result<(), String> {
    let mut source = String::from("import pytest\n\n\n");
    let mut params = Vec::new();
    for index in 0..10 {
        source.push_str(&format!(
            "@pytest.fixture\ndef f{index}():\n    return {index}\n\n\n"
        ));
        params.push(format!("f{index}"));
    }
    source.push_str(&format!(
        "def test_many({}):\n    assert True\n",
        params.join(", ")
    ));
    let (_, tests) = workspace(&[("tests/test_many.py", &source)]);
    let test = tests.first().ok_or_else(|| "test must parse".to_string())?;
    let attached: Vec<&str> = test
        .fixture_defs
        .iter()
        .map(|def| def.name.as_str())
        .collect();
    assert_eq!(attached, ["f0", "f1", "f2", "f3", "f4", "f5", "f6", "f7"]);
    Ok(())
}

#[test]
fn given_a_skipped_test_with_a_fixture_relation_then_the_finding_is_not_exposed()
-> Result<(), String> {
    let test = SAME_MODULE.replace(
        "def test_short(out):",
        "@pytest.mark.skip\ndef test_short(out):",
    );
    let finding = classify(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?;
    assert_ne!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].oracle, None);
    Ok(())
}

#[test]
fn given_a_conftest_fixture_shadowed_by_a_parametrize_argname_then_the_test_stays_unrelated()
-> Result<(), String> {
    let conftest = "import pytest\nfrom src.utils import _short\n\n\n@pytest.fixture\ndef out():\n    return _short('abcd', 3)\n";
    let test = "import pytest\n\n\n@pytest.mark.parametrize('out', ['abc'])\ndef test_short(out):\n    assert out == 'abc'\n";
    assert!(
        relations(&[
            ("src/utils.py", UTILS_PY),
            ("tests/conftest.py", conftest),
            ("tests/test_utils.py", test),
        ])?
        .is_empty()
    );
    Ok(())
}

/// One test per parametrize shape; `decorator` replaces the plain test header.
fn with_decorator(decorator: &str) -> String {
    SAME_MODULE.replace(
        "def test_short(out):",
        &format!("{decorator}\ndef test_short(out):"),
    )
}

#[test]
fn given_a_parametrize_the_literal_parser_gives_up_on_then_the_argname_is_still_not_a_fixture()
-> Result<(), String> {
    for decorator in [
        "@pytest.mark.parametrize('out', VALUES)",
        "@pytest.mark.parametrize('out', [pytest.param('abc', marks=pytest.mark.skip)])",
        "@pytest.mark.parametrize('out', ['abc'], indirect=False)",
        "@pytest.mark.parametrize('out', ['abc'], indirect=['other'])",
        "@pytest.mark.parametrize('out, other', [('a', 'b')])",
        "@pytest.mark.parametrize(('out', 'other'), [('a', 'b')])",
        "@pytest.mark.parametrize(['other', 'out'], [('a', 'b')])",
        "@pytest.mark.parametrize(argnames='out', argvalues=VALUES)",
        "@pytest.mark.parametrize('other', [1])\n@pytest.mark.parametrize('out', VALUES)",
    ] {
        let test = with_decorator(decorator);
        assert!(
            relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?.is_empty(),
            "{decorator}"
        );
    }
    Ok(())
}

#[test]
fn given_indirect_routing_the_argname_to_its_fixture_then_the_fixture_is_resolved()
-> Result<(), String> {
    for decorator in [
        "@pytest.mark.parametrize('out', ['abc'], indirect=True)",
        "@pytest.mark.parametrize('out', VALUES, indirect=['out'])",
        "@pytest.mark.parametrize('out', VALUES, indirect=('other', 'out'))",
    ] {
        let test = with_decorator(decorator);
        assert_eq!(
            relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?,
            vec![("test_short".to_string(), "fixture_owner_call")],
            "{decorator}"
        );
    }
    Ok(())
}

#[test]
fn given_a_parametrize_naming_a_different_argname_then_the_fixture_is_still_resolved()
-> Result<(), String> {
    let test = SAME_MODULE.replace(
        "def test_short(out):",
        "@pytest.mark.parametrize('other', [1])\ndef test_short(out, other):",
    );
    assert_eq!(
        relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    Ok(())
}

#[test]
fn given_a_class_level_parametrize_naming_a_fixture_then_the_method_stays_unrelated()
-> Result<(), String> {
    let test = SAME_MODULE.replace(
        "def test_short(out):\n    assert out == 'abc'\n",
        "@pytest.mark.parametrize('out', VALUES)\nclass TestShort:\n    def test_short(self, out):\n        assert out == 'abc'\n",
    );
    assert!(relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &test)])?.is_empty());
    let routed = test.replace("VALUES)", "VALUES, indirect=True)");
    assert_eq!(
        relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &routed)])?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    let plain = SAME_MODULE.replace(
        "def test_short(out):\n    assert out == 'abc'\n",
        "class TestShort:\n    def test_short(self, out):\n        assert out == 'abc'\n",
    );
    assert_eq!(
        relations(&[("src/utils.py", UTILS_PY), ("tests/test_utils.py", &plain)])?,
        vec![("test_short".to_string(), "fixture_owner_call")]
    );
    Ok(())
}
