//! Behavior suite for the Python preview adapter.
//!
//! Moved verbatim from the former inline `mod tests` in `python.rs`.
//! Covers: test-framework detection, `parse_source` acceptance, owner and
//! test harvesting, probe-shape and constructor/return-field
//! discriminators, class/method/free-function identity semantics,
//! changed-sink delta rules, annotation-only and docstring/comment
//! no-behavior suppression, dict/list element and fstring/length
//! invariants, changed-default policy, `weakly_exposed` boundaries,
//! repair-class discriminators, related-test matching, static-limit
//! fail-closed behavior, and end-to-end `analyze_diff` /
//! `analyze_repo` calls.
//!
//! The sibling `python_tests` module holds the separate coverage-pinning
//! suite for previously uncovered adapter branches; the two suites are
//! intentionally not merged.

fn unique_test_root(label: &str) -> std::path::PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("ripr-py-fw-{label}-{}-{stamp}", std::process::id()))
}

#[test]
fn detect_python_test_framework_reads_setup_cfg_and_tox_sections() -> Result<(), String> {
    let root = unique_test_root("setup-cfg");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(root.join("setup.cfg"), "[tool:pytest]\naddopts = -q\n")
        .map_err(|err| format!("write setup.cfg: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), Some("pytest"));
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

    let root = unique_test_root("tox");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(root.join("tox.ini"), "[pytest]\naddopts = -q\n")
        .map_err(|err| format!("write tox.ini: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), Some("pytest"));
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
    Ok(())
}

#[test]
fn detect_python_test_framework_reads_conftest_py() -> Result<(), String> {
    let root = unique_test_root("conftest");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(root.join("conftest.py"), "import pytest\n")
        .map_err(|err| format!("write conftest.py: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), Some("pytest"));
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
    Ok(())
}

#[test]
fn detect_python_test_framework_reads_pytest_ini_and_pyproject() -> Result<(), String> {
    let root = unique_test_root("pytest-ini");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(root.join("pytest.ini"), "[pytest]\n")
        .map_err(|err| format!("write pytest.ini: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), Some("pytest"));
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

    let root = unique_test_root("pyproject");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(root.join("pyproject.toml"), "[tool.pytest.ini_options]\n")
        .map_err(|err| format!("write pyproject.toml: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), Some("pytest"));
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
    Ok(())
}

#[test]
fn detect_python_test_framework_recognizes_from_unittest_import() -> Result<(), String> {
    let root = unique_test_root("from-unittest");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(
        root.join("test_pricing.py"),
        "from unittest import TestCase\n\nclass TestPricing(TestCase):\n    pass\n",
    )
    .map_err(|err| format!("write test file: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), Some("unittest"));
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
    Ok(())
}

#[test]
fn detect_python_test_framework_rejects_lookalike_and_commented_imports() -> Result<(), String> {
    // Negative fixtures (#2106 review): a lookalike identifier and a
    // commented-out import must NOT report unittest.
    let root = unique_test_root("lookalike");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(root.join("test_lookalike.py"), "import unittesting\n")
        .map_err(|err| format!("write test file: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), None);
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;

    let root = unique_test_root("commented");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    std::fs::write(root.join("test_commented.py"), "# import unittest\n")
        .map_err(|err| format!("write test file: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), None);
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
    Ok(())
}

#[test]
fn detect_python_test_framework_detects_unittest_from_code_evidence() -> Result<(), String> {
    let root = unique_test_root("unittest");
    let tests_dir = root.join("tests");
    std::fs::create_dir_all(&tests_dir).map_err(|err| format!("create tests dir: {err}"))?;
    std::fs::write(
        tests_dir.join("test_pricing.py"),
        "import unittest\n\nclass TestPricing(unittest.TestCase):\n    pass\n",
    )
    .map_err(|err| format!("write test file: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), Some("unittest"));
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
    Ok(())
}

#[test]
fn detect_python_test_framework_is_fail_closed_for_empty_root() -> Result<(), String> {
    let root = unique_test_root("empty");
    std::fs::create_dir_all(&root).map_err(|err| format!("create root: {err}"))?;
    assert_eq!(super::detect_python_test_framework(&root), None);
    std::fs::remove_dir_all(&root).map_err(|err| format!("remove root: {err}"))?;
    Ok(())
}
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::{Path, PathBuf};

pub(super) fn changed(path: &str) -> ChangedFile {
    ChangedFile {
        path: PathBuf::from(path),
        added_lines: Vec::new(),
        removed_lines: Vec::new(),
    }
}

pub(super) fn missing_discriminator_values(finding: &Finding) -> Vec<&str> {
    finding
        .activation
        .missing_discriminators
        .iter()
        .map(|missing| missing.value.as_str())
        .collect()
}

pub(super) fn evidence_value<'a>(finding: &'a Finding, prefix: &str) -> Option<&'a str> {
    finding
        .evidence
        .iter()
        .find_map(|entry| entry.strip_prefix(prefix))
}

#[test]
fn accepts_py_paths() {
    let adapter = PythonAdapter;
    assert!(adapter.accepts_path(Path::new("scripts/run.py")));
    assert!(adapter.accepts_path(Path::new("src/lib/util.py")));
    assert!(!adapter.accepts_path(Path::new("src/lib.rs")));
    assert!(!adapter.accepts_path(Path::new("src/index.ts")));
    assert!(!adapter.accepts_path(Path::new("src/index.tsx")));
    assert!(!adapter.accepts_path(Path::new("README.md")));
    assert!(!adapter.accepts_path(Path::new("no-extension")));
}

#[test]
fn parse_source_accepts_simple_python() {
    let ok = parse_module(
        Path::new("src/discount.py"),
        "def discount(amount: int) -> int:\n    return amount\n",
    )
    .is_some();
    assert!(ok, "valid Python should parse without errors");
}

#[test]
fn parse_source_accepts_class_and_decorator() {
    let ok = parse_module(
        Path::new("src/repo.py"),
        "class Repo:\n    @staticmethod\n    def make() -> 'Repo':\n        return Repo()\n",
    )
    .is_some();
    assert!(ok, "decorated class methods should parse");
}

#[test]
fn parse_source_accepts_async_def_and_fstring() {
    let ok = parse_module(
        Path::new("src/http.py"),
        "async def load(url: str) -> str:\n    return f\"{url}!\"\n",
    )
    .is_some();
    assert!(ok, "async def + f-string should parse");
}

#[test]
fn parse_source_rejects_garbage() {
    let ok = parse_module(
        Path::new("src/oops.py"),
        "this is not :: valid +++ python at all",
    )
    .is_some();
    assert!(!ok, "garbage source should produce parse errors");
}

#[test]
fn extract_owners_recognizes_functions_and_methods() {
    let owners = extract_owners(
        Path::new("src/pricing.py"),
        r#"
def apply_discount(amount):
    return amount

async def load_total(client):
    return await client.total()

class Policy:
    def apply(self, amount):
        return amount

    @staticmethod
    def normalize(amount):
        return amount

    @classmethod
    def from_config(cls, config):
        return cls()
"#,
    );

    assert_eq!(
        owners
            .iter()
            .map(|owner| owner.qualified_name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "apply_discount",
            "load_total",
            "Policy.apply",
            "Policy.normalize",
            "Policy.from_config",
            "Policy",
            "<module>"
        ]
    );
    assert_eq!(owners[0].owner_kind, Some(OwnerKind::Function));
    assert_eq!(owners[1].decorators, vec!["async_def"]);
    assert_eq!(owners[2].owner_kind, Some(OwnerKind::Method));
    assert_eq!(owners[3].owner_kind, Some(OwnerKind::ClassMethod));
    assert_eq!(owners[4].owner_kind, Some(OwnerKind::ClassMethod));
}

#[test]
fn extract_tests_recognizes_pytest_parametrize_and_unittest() {
    let tests = extract_tests(
        Path::new("tests/test_pricing.py"),
        r#"
import unittest
import pytest

@pytest.mark.parametrize("amount", [1, 2])
def test_apply_discount(amount):
    apply_discount(amount)

class TestPytestStyle:
    def test_class_style(self, client):
        assert client.get("/discount").status_code == 200

class Helper:
    def test_not_a_pytest_class(self):
        apply_discount(10)

class PriceTests(unittest.TestCase):
    def test_apply_method(self):
        Policy().apply(10)
"#,
    );

    assert_eq!(
        tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        vec![
            "test_apply_discount",
            "test_class_style",
            "test_apply_method"
        ]
    );
    assert!(tests[0].parametrized);
    assert_eq!(tests[0].fixtures, vec!["amount".to_string()]);
    assert_eq!(tests[0].qualified_name, "test_apply_discount");
    assert_eq!(tests[0].framework, "pytest");
    assert_eq!(tests[1].fixtures, vec!["client".to_string()]);
    assert_eq!(tests[1].qualified_name, "TestPytestStyle.test_class_style");
    assert_eq!(tests[1].framework, "pytest");
    assert_eq!(tests[2].qualified_name, "PriceTests.test_apply_method");
    assert_eq!(tests[2].framework, "unittest");
    assert!(
        tests
            .iter()
            .all(|test| test.name != "test_not_a_pytest_class")
    );
}

#[test]
fn extract_tests_records_module_import_aliases() {
    let tests = extract_tests(
        Path::new("tests/test_imports.py"),
        r#"
import src.catalog as catalog
from src.tax import apply_fee, apply_tax as taxed

def test_imports():
    assert catalog.calculate_total(10) == 17
    assert taxed(10) == 12
"#,
    );

    assert_eq!(
        tests[0]
            .imports
            .iter()
            .map(|import| (import.imported.as_str(), import.alias.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("src.catalog", "catalog"),
            ("apply_fee", "apply_fee"),
            ("apply_tax", "taxed")
        ]
    );
}

#[test]
fn extract_tests_collects_pytest_assertion_oracles() {
    let tests = extract_tests(
        Path::new("tests/test_pricing.py"),
        r#"
def test_apply_discount_exact():
    assert apply_discount(100, 50) == 90

def test_apply_discount_negative():
    assert apply_discount(10, 50) != 90

def test_apply_discount_smoke():
    assert apply_discount(10, 50)

def test_apply_discount_type():
    assert isinstance(apply_discount(10, 50), int)
"#,
    );

    assert_eq!(tests.len(), 4);
    assert_eq!(tests[0].assertions[0].oracle_kind, OracleKind::ExactValue);
    assert_eq!(
        tests[0].assertions[0].oracle_strength,
        OracleStrength::Strong
    );
    assert_eq!(
        tests[1].assertions[0].oracle_kind,
        OracleKind::RelationalCheck
    );
    assert_eq!(tests[1].assertions[0].oracle_strength, OracleStrength::Weak);
    assert_eq!(tests[2].assertions[0].oracle_kind, OracleKind::SmokeOnly);
    assert_eq!(
        tests[2].assertions[0].oracle_strength,
        OracleStrength::Smoke
    );
    assert_eq!(
        tests[3].assertions[0].oracle_kind,
        OracleKind::RelationalCheck
    );
    assert_eq!(tests[3].assertions[0].oracle_strength, OracleStrength::Weak);
}

#[test]
fn extract_tests_collects_pytest_raises_oracle() {
    let tests = extract_tests(
        Path::new("tests/test_validation.py"),
        r#"
import pytest

def test_apply_discount_rejects_negative():
    with pytest.raises(ValueError):
        apply_discount(-1, 50)
"#,
    );

    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].assertions[0].oracle_kind, OracleKind::BroadError);
    assert_eq!(tests[0].assertions[0].oracle_strength, OracleStrength::Weak);
}

#[test]
fn extract_tests_collects_unittest_assertion_oracles() {
    let tests = extract_tests(
        Path::new("tests/test_pricing.py"),
        r#"
import unittest

class PriceTests(unittest.TestCase):
    def test_apply_discount_exact(self):
        self.assertEqual(apply_discount(100, 50), 90)

    def test_apply_discount_raises(self):
        with self.assertRaises(ValueError):
            apply_discount(-1, 50)

    def test_apply_discount_boolean(self):
        self.assertTrue(apply_discount(10, 50) >= 0)
"#,
    );

    assert_eq!(tests.len(), 3);
    assert_eq!(tests[0].assertions[0].oracle_kind, OracleKind::ExactValue);
    assert_eq!(
        tests[0].assertions[0].oracle_strength,
        OracleStrength::Strong
    );
    assert_eq!(tests[1].assertions[0].oracle_kind, OracleKind::BroadError);
    assert_eq!(tests[1].assertions[0].oracle_strength, OracleStrength::Weak);
    assert_eq!(tests[2].assertions[0].oracle_kind, OracleKind::SmokeOnly);
    assert_eq!(
        tests[2].assertions[0].oracle_strength,
        OracleStrength::Smoke
    );
}

#[test]
fn extract_tests_collects_mock_call_oracle() {
    let tests = extract_tests(
        Path::new("tests/test_notifier.py"),
        r#"
def test_notifies_callback():
    callback = Mock()
    send_alert(callback)
    callback.assert_called_once_with("sent")
"#,
    );

    assert_eq!(tests.len(), 1);
    assert_eq!(
        tests[0].assertions[0].oracle_kind,
        OracleKind::MockExpectation
    );
    assert_eq!(
        tests[0].assertions[0].oracle_strength,
        OracleStrength::Medium
    );
}

#[test]
fn classify_probe_shape_recognizes_python_predicate_shapes() {
    let (family, delta) = classify_probe_shape("    if amount >= threshold:");
    assert_eq!(family, ProbeFamily::Predicate);
    assert_eq!(delta, DeltaKind::Control);

    let (family, delta) =
        classify_probe_shape("    label = \"high\" if amount >= threshold else \"normal\"");
    assert_eq!(family, ProbeFamily::Predicate);
    assert_eq!(delta, DeltaKind::Control);
}

#[test]
fn classify_probe_shape_recognizes_python_return_and_error_shapes() {
    let (family, delta) = classify_probe_shape("    return amount - 10");
    assert_eq!(family, ProbeFamily::ReturnValue);
    assert_eq!(delta, DeltaKind::Value);

    let (family, delta) = classify_probe_shape("    raise ValueError(\"bad\")");
    assert_eq!(family, ProbeFamily::ErrorPath);
    assert_eq!(delta, DeltaKind::Control);

    let (family, delta) = classify_probe_shape("    except ValueError:");
    assert_eq!(family, ProbeFamily::ErrorPath);
    assert_eq!(delta, DeltaKind::Control);
}

#[test]
fn classify_probe_shape_recognizes_python_field_and_call_shapes() {
    let (family, delta) = classify_probe_shape("    self.status = \"paid\"");
    assert_eq!(family, ProbeFamily::FieldConstruction);
    assert_eq!(delta, DeltaKind::Value);

    let (family, delta) = classify_probe_shape("    return User(active=True)");
    assert_eq!(family, ProbeFamily::FieldConstruction);
    assert_eq!(delta, DeltaKind::Value);

    let (family, delta) = classify_probe_shape("    notifier(\"receipt.sent\", order_id)");
    assert_eq!(family, ProbeFamily::SideEffect);
    assert_eq!(delta, DeltaKind::Effect);

    let (family, delta) = classify_probe_shape("    callback = MagicMock(name=\"receipt\")");
    assert_eq!(family, ProbeFamily::SideEffect);
    assert_eq!(delta, DeltaKind::Effect);
}

#[test]
fn return_dict_field_parts_prefer_literal_changed_value_candidates() {
    assert_eq!(
        python_return_dict_field_parts("return {\"name\": name, \"status\": \"active\"}"),
        Some(("status".to_string(), "\"active\"".to_string()))
    );
    assert_eq!(
        python_return_dict_field_discriminator("return {\"name\": name, \"status\": \"active\"}")
            .as_deref(),
        Some("status == \"active\"")
    );
    assert_eq!(
        python_return_dict_field_parts("return {\"label\": \"ready, set\", \"status\": status}"),
        Some(("label".to_string(), "\"ready, set\"".to_string()))
    );
    assert_eq!(
        python_return_dict_field_parts("return {\"status\": status}"),
        Some(("status".to_string(), "status".to_string()))
    );
}

#[test]
fn return_dict_field_parts_handle_nested_segments_and_literal_kinds() {
    assert_eq!(
        top_level_python_segments(
            "\"payload\": {\"status\": \"active, pending\"}, \"note\": \"a,b\""
        ),
        vec![
            "\"payload\": {\"status\": \"active, pending\"}",
            "\"note\": \"a,b\""
        ]
    );
    assert_eq!(
        top_level_python_segments("\"label\": \"ready\\\"set\", \"status\": status"),
        vec!["\"label\": \"ready\\\"set\"", "\"status\": status"]
    );
    assert_eq!(
        python_dict_field_segment_parts("\"url\": \"https://example.test/a:b\""),
        Some(("url", "\"https://example.test/a:b\""))
    );
    assert_eq!(python_dict_field_segment_parts("\"status\""), None);
    assert!(is_literal_python_model_field_value("True"));
    assert!(is_literal_python_model_field_value("-1.5"));
    assert!(!is_literal_python_model_field_value("status"));
    assert_eq!(
        python_return_dict_field_parts(
            "return {\"status\": status, invalid_segment, \"count\": total}"
        ),
        Some(("status".to_string(), "status".to_string()))
    );
    assert_eq!(
        python_return_dict_field_parts("return {\"payload\": make_payload(a, b)}"),
        Some(("payload".to_string(), "make_payload(a, b)".to_string()))
    );
    assert_eq!(python_return_dict_field_parts("return {}"), None);
}

#[test]
fn constructor_keyword_field_parts_accept_simple_model_field_values() {
    assert_eq!(
        python_return_constructor_field_parts("return User(active=True)"),
        Some(("User".to_string(), "active".to_string(), "True".to_string()))
    );
    assert_eq!(
        python_return_constructor_field_parts("return models.User(name=\"Ada\")"),
        Some((
            "models.User".to_string(),
            "name".to_string(),
            "\"Ada\"".to_string()
        ))
    );
    assert_eq!(
        python_return_constructor_field_parts("return _User(score=-1.5)"),
        Some(("_User".to_string(), "score".to_string(), "-1.5".to_string()))
    );
    assert_eq!(
        python_return_constructor_field_parts("return User(plan=default_plan)"),
        Some((
            "User".to_string(),
            "plan".to_string(),
            "default_plan".to_string()
        ))
    );
    assert_eq!(
        python_return_constructor_field_parts("return User(label=\"a=b\")"),
        Some((
            "User".to_string(),
            "label".to_string(),
            "\"a=b\"".to_string()
        ))
    );
}

#[test]
fn constructor_keyword_field_parts_fail_closed_for_ambiguous_shapes() {
    assert_eq!(
        python_return_constructor_field_parts("return build_user(active=True)"),
        None
    );
    assert_eq!(
        python_return_constructor_field_parts("return User(\"Ada\")"),
        None
    );
    assert_eq!(
        python_return_constructor_field_parts("return User(profile.active=True)"),
        None
    );
    assert_eq!(
        python_return_constructor_field_parts("return User(active=build_active())"),
        None
    );
    assert_eq!(
        python_return_constructor_field_parts(
            "return User(config={\"active\": True}, active=True)"
        ),
        None
    );
    assert_eq!(
        python_return_constructor_field_parts("value = User(active=True)"),
        None
    );
}

#[test]
fn first_python_keyword_argument_skips_positional_and_nested_arguments() {
    assert_eq!(
        first_python_keyword_argument("factory(a=b), active=True"),
        Some(("active", "True"))
    );
    assert_eq!(
        first_python_keyword_argument("name=\"Ada, Lovelace\", active=True"),
        Some(("name", "\"Ada, Lovelace\""))
    );
    assert_eq!(
        first_python_keyword_argument("metadata={\"a\": \"b,c\"}, active=True"),
        Some(("metadata", "{\"a\": \"b,c\"}"))
    );
    assert_eq!(first_python_keyword_argument("factory(a=b), user"), None);
}

#[test]
fn classify_change_uses_constructor_keyword_field_discriminator() -> Result<(), String> {
    let source = r#"
from dataclasses import dataclass

@dataclass
class User:
    active: bool

def build_user():
    return User(active=True)
"#;
    let owners = extract_owners(Path::new("src/users.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_users.py"),
        r#"
from src.users import build_user

def test_build_user_smoke():
    user = build_user()
    assert user
"#,
    );

    let Some(finding) = classify_change(
        Path::new("src/users.py"),
        9,
        "    return User(active=True)",
        &owners,
        &tests,
    ) else {
        return Err("changed constructor return inside owner should classify".to_string());
    };

    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.probe.family, ProbeFamily::FieldConstruction);
    assert_eq!(
        finding
            .activation
            .missing_discriminators
            .first()
            .map(|missing| missing.value.as_str()),
        Some("result.active == True")
    );
    assert!(
        finding
            .evidence
            .iter()
            .any(|entry| entry == "missing_discriminator: result.active == True")
    );
    Ok(())
}

#[test]
fn constructor_keyword_field_helpers_stay_bounded_and_fail_closed() {
    assert_eq!(
        python_return_constructor_field_discriminator("return User(active=True)").as_deref(),
        Some("result.active == True")
    );
    assert_eq!(
        python_return_constructor_field_discriminator("return models.User(score=-1.5)").as_deref(),
        Some("result.score == -1.5")
    );
    assert_eq!(
        split_python_constructor_call("User(active=True)"),
        Some(("User", "active=True"))
    );
    assert_eq!(split_python_constructor_call("User()"), None);
    assert_eq!(split_python_constructor_call("(User(active=True))"), None);
    assert!(is_python_constructor_callee("models.User"));
    assert!(is_python_constructor_callee("_PrivateUser"));
    assert!(!is_python_constructor_callee("make_user"));
    assert_eq!(
        first_python_keyword_argument("ignored, active=True"),
        Some(("active", "True"))
    );
    assert_eq!(
        first_python_keyword_argument("label=\"a,b=c\", active=False"),
        Some(("label", "\"a,b=c\""))
    );
    assert_eq!(
        first_python_keyword_argument("meta={\"threshold\": \"a=b,c\"}"),
        Some(("meta", "{\"threshold\": \"a=b,c\"}"))
    );
    assert_eq!(python_keyword_argument_parts("not keyword"), None);
    assert_eq!(top_level_equals("metadata={\"a\": \"b=c\"}"), Some(8));
    assert_eq!(top_level_equals("metadata"), None);
    assert!(is_simple_python_model_field_value("\"active\""));
    assert!(is_simple_python_model_field_value("True"));
    assert!(is_simple_python_model_field_value("None"));
    assert!(is_simple_python_model_field_value("-1.25"));
    assert!(is_simple_python_model_field_value(".5"));
    assert!(!is_simple_python_model_field_value("."));
    assert!(!is_simple_python_model_field_value("-"));
    assert!(!is_simple_python_model_field_value("1.2.3"));
    assert!(!is_simple_python_model_field_value("make_value()"));
    assert_eq!(
        python_return_constructor_field_discriminator("return make_user(active=True)"),
        None
    );
    assert_eq!(
        python_return_constructor_field_discriminator("return User(active=make_value())"),
        None
    );
    assert_eq!(
        python_assignment_constructor_field_parts(
            "response = Response(status_code=422, detail=\"coupon expired\")"
        ),
        Some((
            "response".to_string(),
            "Response".to_string(),
            "status_code".to_string(),
            "422".to_string()
        ))
    );
    assert_eq!(
        python_assignment_constructor_field_parts("response.body = Response(status_code=422)"),
        None
    );
    assert_eq!(
        python_assignment_constructor_field_parts("response = make_response(status_code=422)"),
        None
    );
    assert_eq!(
        python_assignment_constructor_field_parts("response = Response(detail=message())"),
        None
    );
    assert_eq!(
        python_route_response_field_discriminator("status_code", "422").as_deref(),
        Some("response.status_code == 422")
    );
    assert_eq!(
        python_route_response_field_discriminator("detail", "\"coupon expired\"").as_deref(),
        Some("response.json()[\"detail\"] == \"coupon expired\"")
    );
    assert_eq!(
        python_route_response_field_discriminator("headers", "expected_headers").as_deref(),
        Some("response.headers == expected_headers")
    );
}

#[test]
fn construct_result_is_called_distinguishes_inline_from_bound() {
    // open-paren index of the first `(` in each fixture string (always present).
    let at = |s: &str| s.find('(').unwrap_or(0);
    // Inline construct-call `C(...)(...)`: the constructed instance is called.
    let inline = "Renderer()(None, event)";
    assert!(construct_result_is_called(inline, at(inline)));
    let inline_args = "Renderer(sort=True)(event)";
    assert!(construct_result_is_called(inline_args, at(inline_args)));
    // Bound local `x = C(...)` then a separate `x(...)`: NOT an inline call —
    // the constructor's `)` is followed by a newline, not `(`. Keeps the
    // local-callable case uncertain (consistent with #1221).
    let bound = "stop = stop_after_attempt(3)\n    stop(3)";
    assert!(!construct_result_is_called(bound, at(bound)));
    // Plain construction with no following call.
    let plain = "r = Renderer()";
    assert!(!construct_result_is_called(plain, at(plain)));
}

fn call_owner(owners: &[PythonOwner]) -> Result<&PythonOwner, String> {
    owners
        .iter()
        .find(|owner| owner.name == "__call__")
        .ok_or_else(|| "fixture defines a __call__ owner".to_string())
}

const STOP_SOURCE: &str = "class stop_after_attempt:\n    def __init__(self, max_attempt_number):\n        self.max_attempt_number = max_attempt_number\n\n    def __call__(self, attempt_number):\n        return attempt_number >= self.max_attempt_number\n";

#[test]
fn local_binding_relation_links_direct_and_surfaces_smoke_oracle() -> Result<(), String> {
    // The tenacity false-actionable shape: `stop = stop_after_attempt(3)`
    // bound once and called via `stop(3)` under a broad-boolean smoke oracle.
    let owners = extract_owners(Path::new("src/stop.py"), STOP_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_stop.py"),
        "import unittest\n\nfrom src.stop import stop_after_attempt\n\n\nclass StopTest(unittest.TestCase):\n    def test_stop_after_attempt(self):\n        stop = stop_after_attempt(3)\n        self.assertTrue(stop(3))\n",
    );
    let owner = call_owner(&owners)?;
    assert_eq!(
        related_test_relation(&tests[0], owner),
        Some(PythonRelationKind::LocalBinding),
        "single bound local called as `stop(3)` should link directly via local_binding"
    );

    let Some(finding) = classify_change(
        Path::new("src/stop.py"),
        6,
        "        return attempt_number >= self.max_attempt_number",
        &owners,
        &tests,
    ) else {
        return Err("changed return inside __call__ should classify".to_string());
    };
    // Must STAY weakly_exposed — a smoke oracle never credits `exposed`.
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Smoke,
        "the assertTrue(stop(3)) smoke oracle must be surfaced, not dropped to unknown"
    );
    Ok(())
}

#[test]
fn local_binding_does_not_fire_for_inline_construct_call() -> Result<(), String> {
    // Inline `C()(...)` is ConstructCall territory, not a bound local.
    let owners = extract_owners(Path::new("src/stop.py"), STOP_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_stop.py"),
        "from src.stop import stop_after_attempt\n\n\ndef test_inline():\n    assert stop_after_attempt(3)(3)\n",
    );
    let owner = call_owner(&owners)?;
    assert_eq!(
        related_test_relation(&tests[0], owner),
        Some(PythonRelationKind::ConstructCall),
        "inline construct-call must stay ConstructCall, not LocalBinding"
    );
    Ok(())
}

#[test]
fn local_binding_does_not_fire_for_reassigned_binding() -> Result<(), String> {
    // A rebound local is ambiguous: which construction is called is unclear.
    let owners = extract_owners(Path::new("src/stop.py"), STOP_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_stop.py"),
        "from src.stop import stop_after_attempt\n\n\ndef test_reassigned():\n    stop = stop_after_attempt(3)\n    stop = stop_after_attempt(4)\n    assert stop(3)\n",
    );
    let owner = call_owner(&owners)?;
    assert!(
        !local_binding_calls_owner(&tests[0], owner),
        "two constructions / reassigned binding must not link via local_binding"
    );
    Ok(())
}

#[test]
fn local_binding_does_not_fire_for_wrapper_keyword_argument() -> Result<(), String> {
    // `Retrying(stop=stop_after_attempt(3))` binds a wrapper, not the class —
    // the assignment target is `retrying`, not `stop_after_attempt(...)`.
    let owners = extract_owners(Path::new("src/stop.py"), STOP_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_stop.py"),
        "from src.stop import stop_after_attempt\nimport tenacity\n\n\ndef test_wrapper():\n    retrying = tenacity.Retrying(stop=stop_after_attempt(3))\n    assert retrying(lambda: None)\n",
    );
    let owner = call_owner(&owners)?;
    assert!(
        !local_binding_calls_owner(&tests[0], owner),
        "a keyword-argument construction inside a wrapper must not link via local_binding"
    );
    Ok(())
}

#[test]
fn local_binding_requires_importing_the_owner_class() -> Result<(), String> {
    // Guard B: a same-named local without importing the class must not link.
    let owners = extract_owners(Path::new("src/stop.py"), STOP_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_stop.py"),
        "def test_no_import():\n    stop = stop_after_attempt(3)\n    assert stop(3)\n",
    );
    let owner = call_owner(&owners)?;
    assert!(
        !local_binding_calls_owner(&tests[0], owner),
        "without importing the owner class, local_binding must not fire (Guard B)"
    );
    Ok(())
}

#[test]
fn binding_target_extracts_only_direct_identifier_assignment() {
    // Direct `local = Class(` extracts the bare identifier.
    let direct = "    stop = stop_after_attempt(3)\n";
    let idx = direct.find("stop_after_attempt(").unwrap_or(0);
    assert_eq!(
        binding_target_for_construction(direct, idx).as_deref(),
        Some("stop")
    );
    // Keyword argument is not an assignment target.
    let kwarg = "    retrying = Retrying(stop=stop_after_attempt(3))\n";
    let kidx = kwarg.find("stop_after_attempt(").unwrap_or(0);
    assert_eq!(binding_target_for_construction(kwarg, kidx), None);
    // Attribute target `self.x = Class(` is not a bare local.
    let attr = "    self.stop = stop_after_attempt(3)\n";
    let aidx = attr.find("stop_after_attempt(").unwrap_or(0);
    assert_eq!(binding_target_for_construction(attr, aidx), None);
}

#[test]
fn oracle_text_observes_token_requires_identifier_boundary() {
    // Whole-word matches still observe (preserves genuine sink alignment).
    assert!(oracle_text_observes_token(
        "assert raises(ValueError, match='Invalid key: x')",
        "key"
    ));
    assert!(oracle_text_observes_token("assert stop(3)", "stop"));
    assert!(oracle_text_observes_token(
        "assert x.max_buffer_size == 2",
        "max_buffer_size"
    ));
    // Substring co-occurrence must NOT observe: the confirmed false-exposed
    // vector — `buffer` (a changed-sink token) inside an unrelated
    // `buffered_stream` oracle from a different class.
    assert!(!oracle_text_observes_token(
        "assert buffered_stream.receive_exactly(10) == b\"x\"",
        "buffer"
    ));
    assert!(!oracle_text_observes_token("assert client.send()", "len"));
    assert!(!oracle_text_observes_token("assert keys() == []", "key"));
    assert!(!oracle_text_observes_token("anything", ""));
}

#[test]
fn classify_change_returns_exposed_when_related_test_has_strong_oracle() -> Result<(), String> {
    let owners = extract_owners(
        Path::new("src/pricing.py"),
        "def apply_discount(amount):\n    if amount >= 100:\n        return amount - 10\n    return amount\n",
    );
    let tests = extract_tests(
        Path::new("tests/test_pricing.py"),
        "from src.pricing import apply_discount\n\n\ndef test_apply_discount():\n    assert apply_discount(100) == 90\n",
    );

    let Some(finding) = classify_change(
        Path::new("src/pricing.py"),
        2,
        "    if amount >= 100:",
        &owners,
        &tests,
    ) else {
        return Err("changed line inside owner should classify".to_string());
    };

    assert_eq!(finding.class, ExposureClass::Exposed);
    assert!(
        (finding.confidence - 0.6).abs() < 0.0001,
        "exposed Python preview confidence should be 0.6"
    );
    assert_eq!(finding.related_tests.len(), 1);
    assert_eq!(finding.related_tests[0].oracle_kind, OracleKind::ExactValue);
    assert_eq!(
        finding.related_tests[0].oracle_strength,
        OracleStrength::Strong
    );
    assert!(finding.activation.missing_discriminators.is_empty());
    assert!(
        finding
            .evidence
            .iter()
            .all(|entry| !entry.starts_with("missing_discriminator:"))
    );
    Ok(())
}

const AUTH_SOURCE: &str = "class TokenValidator:\n    def __init__(self, valid):\n        self._valid = valid\n\n    def validate(self, token):\n        return token.strip() in self._valid\n";
const AUTH_CHANGED_LINE: &str = "        return token.strip() in self._valid";

#[test]
fn method_name_without_class_identity_stays_orthogonal() -> Result<(), String> {
    // False-`exposed` guard: the only related test exercises a DIFFERENT
    // class's same-named method (`PaymentProcessor.validate`) and never
    // imports the owner's class. The bare method-name token `validate` must
    // not credit `direct` alignment without owner-class identity.
    let owners = extract_owners(Path::new("src/auth.py"), AUTH_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_billing.py"),
        "from src.billing import PaymentProcessor\n\n\ndef test_billing_validate():\n    proc = PaymentProcessor()\n    assert proc.validate(\"card1234 \") == True\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/auth.py"),
        6,
        AUTH_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside validate should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "a same-named method on an unrelated class must not credit exposed"
    );
    assert_eq!(finding.oracle_alignment.as_deref(), Some("orthogonal"));
    Ok(())
}

#[test]
fn method_name_with_class_import_identity_credits_exposed() -> Result<(), String> {
    // Identity preserved: the test imports and constructs the owner's class,
    // then observes its method under an exact-value oracle. The bare
    // method-name match is legitimate here, so `direct`/`exposed` stands.
    let owners = extract_owners(Path::new("src/auth.py"), AUTH_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_auth.py"),
        "from src.auth import TokenValidator\n\n\ndef test_auth_validate():\n    validator = TokenValidator([\"card1234\"])\n    assert validator.validate(\"card1234\") == True\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/auth.py"),
        6,
        AUTH_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside validate should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a test that imports and exercises the owner class keeps exposed credit"
    );
    assert_eq!(finding.oracle_alignment.as_deref(), Some("direct"));
    Ok(())
}

#[test]
fn method_name_dead_import_does_not_credit_exposed() -> Result<(), String> {
    // Bypass guard: a DEAD import of the owner class (never used in the test
    // body) is not identity evidence. The test exercises a different class's
    // same-named method, so it must stay conservative.
    let owners = extract_owners(Path::new("src/auth.py"), AUTH_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_billing.py"),
        "from src.billing import PaymentProcessor\nfrom src.auth import TokenValidator\n\n\ndef test_billing_validate():\n    proc = PaymentProcessor()\n    assert proc.validate(\"card1234 \") == True\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/auth.py"),
        6,
        AUTH_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside validate should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "a dead import of the owner class must not credit exposed"
    );
    assert_ne!(finding.oracle_alignment.as_deref(), Some("direct"));
    Ok(())
}

#[test]
fn method_owner_free_function_alias_does_not_credit_exposed() -> Result<(), String> {
    // Bypass guard: a same-named FREE function aliased from an unrelated
    // module must not credit `exposed`/`alias` for a method owner. The test
    // never imports or exercises the owner's class.
    let owners = extract_owners(Path::new("src/auth.py"), AUTH_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_helpers.py"),
        "from src.helpers import validate as run_check\n\n\ndef test_helpers_run_check():\n    assert run_check(\"data\") == True\n",
    );
    if let Some(finding) = classify_change(
        Path::new("src/auth.py"),
        6,
        AUTH_CHANGED_LINE,
        &owners,
        &tests,
    ) {
        assert_ne!(
            finding.class,
            ExposureClass::Exposed,
            "a same-named free-function alias must not credit exposed for a method owner"
        );
        assert_ne!(finding.oracle_alignment.as_deref(), Some("alias"));
    }
    Ok(())
}

#[test]
fn method_name_class_constructed_but_method_on_other_receiver_does_not_credit_exposed()
-> Result<(), String> {
    // Receiver-identity guard (the residual leak the #1253 class-import gate
    // left open): the owner class is imported AND constructed in real code, but
    // the strong oracle's `.validate(` runs on an UNRELATED receiver. Class
    // identity is present; receiver identity is not, so it must stay
    // conservative.
    let owners = extract_owners(Path::new("src/auth.py"), AUTH_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_billing.py"),
        "from src.auth import TokenValidator\nfrom src.billing import PaymentProcessor\n\n\ndef test_billing_validate():\n    reference = TokenValidator([\"card1234\"])\n    proc = PaymentProcessor()\n    assert proc.validate(\"card1234 \") == True\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/auth.py"),
        6,
        AUTH_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside validate should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "constructing the owner class is not enough; the asserted method ran on a different receiver"
    );
    assert_ne!(finding.oracle_alignment.as_deref(), Some("direct"));
    Ok(())
}

#[test]
fn method_name_inline_construct_call_credits_exposed() -> Result<(), String> {
    // Positive control: an inline `OwnerClass(...).method(...)` binds the
    // receiver to the owner class, and a strong exact-value oracle observes it.
    let owners = extract_owners(Path::new("src/auth.py"), AUTH_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_auth.py"),
        "from src.auth import TokenValidator\n\n\ndef test_auth_inline():\n    assert TokenValidator([\"card1234\"]).validate(\"card1234\") == True\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/auth.py"),
        6,
        AUTH_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside validate should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "inline construct-call binds the receiver to the owner class"
    );
    assert_eq!(finding.oracle_alignment.as_deref(), Some("direct"));
    Ok(())
}

#[test]
fn method_name_classmethod_direct_call_credits_exposed() -> Result<(), String> {
    // Positive control: a classmethod called directly on the owner class
    // (`OwnerClass.method(...)`) is receiver-bound by construction.
    let source = "class TokenRegistry:\n    @classmethod\n    def lookup(cls, token):\n        return token.strip()\n";
    let owners = extract_owners(Path::new("src/registry.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_registry.py"),
        "from src.registry import TokenRegistry\n\n\ndef test_registry_lookup():\n    assert TokenRegistry.lookup(\"abc \") == \"abc\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/registry.py"),
        4,
        "        return token.strip()",
        &owners,
        &tests,
    ) else {
        return Err("changed return inside lookup should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a classmethod called on the owner class is receiver-bound"
    );
    assert_eq!(finding.oracle_alignment.as_deref(), Some("direct"));
    Ok(())
}

const SESSION_SOURCE: &str =
    "class Session:\n    def refresh(self):\n        self.status = \"active\"\n";
const SESSION_CHANGED_LINE: &str = "        self.status = \"active\"";

#[test]
fn attribute_sink_different_receiver_and_value_does_not_credit_exposed() -> Result<(), String> {
    // Cluster A guard: a changed attribute write `self.status = "active"` must
    // not credit `changed_sink_token` when the strong oracle observes a
    // DIFFERENT receiver's same-named attribute with a different value
    // (`conn.status == "closed"`) — pure attribute-name token coincidence.
    let owners = extract_owners(Path::new("src/session.py"), SESSION_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_session.py"),
        "from src.session import Session\n\n\ndef test_refresh(conn):\n    Session().refresh()\n    assert conn.status == \"closed\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/session.py"),
        3,
        SESSION_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed attribute write should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a different receiver AND different value must not credit exposed"
    );
    assert_ne!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

#[test]
fn attribute_sink_value_and_attr_observed_credits_exposed() -> Result<(), String> {
    // Positive control: observing the assigned VALUE together with the
    // attribute name (`assert s.status == "active"`) is change-specific
    // evidence, so the changed-sink token credit stands.
    let owners = extract_owners(Path::new("src/session.py"), SESSION_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_session.py"),
        "from src.session import Session\n\n\ndef test_refresh_status():\n    s = Session()\n    s.refresh()\n    assert s.status == \"active\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/session.py"),
        3,
        SESSION_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed attribute write should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "observing the assigned value and the attribute name keeps exposed"
    );
    assert_eq!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

#[test]
fn attribute_sink_common_value_without_attr_does_not_credit_exposed() -> Result<(), String> {
    // Common-literal guard: the assigned value "active" is ubiquitous;
    // observing it on a DIFFERENT attribute (`widget.state == "active"`) must
    // not credit, because the changed attribute name `status` is not
    // co-observed.
    let owners = extract_owners(Path::new("src/session.py"), SESSION_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_session.py"),
        "from src.session import Session\n\n\ndef test_refresh_widget(widget):\n    Session().refresh()\n    assert widget.state == \"active\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/session.py"),
        3,
        SESSION_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed attribute write should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a common assigned value on a different attribute must not credit exposed"
    );
    assert_ne!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

const HANDLER_SOURCE: &str = "def normalize(payload):\n    return payload.strip()\n";
const HANDLER_CHANGED_LINE: &str = "    return payload.strip()";

#[test]
fn free_function_imported_from_other_module_does_not_credit_exposed() -> Result<(), String> {
    // Cluster B guard: the changed free function is src.handler.normalize, but
    // the only related test imports a same-named normalize from src.checker.
    // The bare function-name token is not identity-bearing across modules.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_checker.py"),
        "from src.checker import normalize\n\n\ndef test_checker_normalize():\n    assert normalize(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a same-named free function imported from a different module is not identity-bearing"
    );
    Ok(())
}

#[test]
fn free_function_imported_from_owner_module_credits_exposed() -> Result<(), String> {
    // Positive control: same-module import + a strong exact-value oracle.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_handler.py"),
        "from src.handler import normalize\n\n\ndef test_handler_normalize():\n    assert normalize(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "importing the function from the owner's module is identity-bearing"
    );
    assert_eq!(finding.oracle_alignment.as_deref(), Some("direct"));
    Ok(())
}

#[test]
fn free_function_relative_import_from_owner_module_credits_exposed() -> Result<(), String> {
    // Common package-local Python tests often use explicit relative imports.
    // Resolve the importing file's package before checking free-function
    // module identity so `from .handler import normalize` is not treated as
    // unrelated bare-name token coincidence.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("src/test_handler.py"),
        "from .handler import normalize\n\n\ndef test_handler_normalize_relative():\n    assert normalize(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a resolved relative import from the owner's module is identity-bearing"
    );
    assert_eq!(finding.oracle_alignment.as_deref(), Some("direct"));
    Ok(())
}

#[test]
fn free_function_relative_import_from_sibling_module_stays_fail_closed() -> Result<(), String> {
    // Boundary: `from .other import normalize` resolves to `src.other`, not
    // the owner's `src.handler` module — identity must NOT be credited from
    // the bare-name token coincidence.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("src/test_handler.py"),
        "from .other import normalize\n\n\ndef test_handler_normalize_sibling():\n    assert normalize(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    if finding.class == ExposureClass::Exposed {
        return Err(format!(
            "a relative import from a different module was wrongly credited: {:?}",
            finding.class
        ));
    }
    Ok(())
}

#[test]
fn relative_import_escaping_the_package_fails_closed() -> Result<(), String> {
    // `from ...handler import normalize` from a shallow file traverses above
    // the package root: the resolver must fail closed to an empty module
    // rather than fabricate an identity.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("src/test_handler.py"),
        "from ...handler import normalize\n\n\ndef test_handler_normalize_overtraverse():\n    assert normalize(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    if finding.class == ExposureClass::Exposed {
        return Err(format!(
            "an over-traversing relative import was wrongly credited: {:?}",
            finding.class
        ));
    }
    Ok(())
}

#[test]
fn relative_import_of_same_stem_helper_in_sibling_package_stays_fail_closed() -> Result<(), String>
{
    // The P1 review case: `src/tests/test_handler.py` importing
    // `from .handler import normalize` resolves to `src.tests.handler` — a
    // DIFFERENT module with the same stem as the owner's `src.handler`.
    // Stem-only matching would wrongly credit identity here.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("src/tests/test_handler.py"),
        "from .handler import normalize\n\n\ndef test_handler_same_stem_helper():\n    assert normalize(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    if finding.class == ExposureClass::Exposed {
        return Err(format!(
            "a same-stem helper in a sibling package was wrongly credited: {:?}",
            finding.class
        ));
    }
    Ok(())
}

#[test]
fn vendored_same_stem_module_does_not_match_owner_identity() -> Result<(), String> {
    // `from src.vendor.handler import normalize` names a same-stem module in
    // a different package — full-path identity must reject it.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("src/tests/test_handler.py"),
        "from src.vendor.handler import normalize\n\n\ndef test_handler_vendored():\n    assert normalize(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    if finding.class == ExposureClass::Exposed {
        return Err(format!(
            "a vendored same-stem module was wrongly credited: {:?}",
            finding.class
        ));
    }
    Ok(())
}

#[test]
fn free_function_aliased_import_from_owner_module_credits_exposed() -> Result<(), String> {
    // Positive control: aliased same-module import (`as norm`) keeps identity.
    let owners = extract_owners(Path::new("src/handler.py"), HANDLER_SOURCE);
    let tests = extract_tests(
        Path::new("tests/test_handler.py"),
        "from src.handler import normalize as norm\n\n\ndef test_handler_normalize_alias():\n    assert norm(\" ok \") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        HANDLER_CHANGED_LINE,
        &owners,
        &tests,
    ) else {
        return Err("changed return inside normalize should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "an aliased same-module import is identity-bearing"
    );
    assert_eq!(finding.oracle_alignment.as_deref(), Some("alias"));
    Ok(())
}

#[test]
fn free_function_changed_value_token_from_other_module_does_not_credit_exposed()
-> Result<(), String> {
    // Sibling-branch guard (#1249 lesson): even when a wrong-module test's
    // strong oracle observes the changed VALUE token ("ok"), the
    // changed_sink_token path must require free-function module identity too —
    // not just the direct/alias paths.
    let source = "def classify(payload):\n    return payload.strip() == \"ok\"\n";
    let owners = extract_owners(Path::new("src/handler.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_checker.py"),
        "from src.checker import classify\n\n\ndef test_checker_classify():\n    assert classify(\"ok\") == \"ok\"\n",
    );
    let Some(finding) = classify_change(
        Path::new("src/handler.py"),
        2,
        "    return payload.strip() == \"ok\"",
        &owners,
        &tests,
    ) else {
        return Err("changed return inside classify should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a changed-value token observed by a different-module test is not identity-bearing"
    );
    assert_ne!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

#[test]
fn changed_sink_token_requires_delta_not_unchanged_operand() -> Result<(), String> {
    // #1276: the delta is `max` (the wrap); the oracle observes the UNCHANGED
    // operand `_balance` and never invokes the changed `balance` property, so it
    // does not discriminate the change. Must not credit changed_sink_token.
    let source = "class Account:\n    def __init__(self, balance):\n        self._balance = balance\n\n    @property\n    def balance(self):\n        return max(0, self._balance)\n";
    let owners = extract_owners(Path::new("src/account.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_account.py"),
        "from src.account import Account\n\n\ndef test_account_init():\n    account = Account(100)\n    assert account._balance == 100\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/account.py"),
        7,
        "        return max(0, self._balance)",
        Some("        return self._balance"),
        &owners,
        &tests,
    ) else {
        return Err("changed property body should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "an unchanged operand observed by the test is not the behavior delta"
    );
    assert_ne!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

#[test]
fn changed_sink_token_credits_when_oracle_observes_the_delta_value() -> Result<(), String> {
    // Positive control: the changed VALUE "paid" IS the delta, and the oracle
    // observes it on the same owner instance — the credit stands.
    let source = "class Invoice:\n    def __init__(self):\n        self.status = \"open\"\n\n    def settle(self):\n        self.status = \"paid\"\n";
    let owners = extract_owners(Path::new("src/invoice.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_invoice.py"),
        "from src.invoice import Invoice\n\n\ndef test_settle():\n    inv = Invoice()\n    inv.settle()\n    assert inv.status == \"paid\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/invoice.py"),
        6,
        "        self.status = \"paid\"",
        Some("        self.status = \"settled\""),
        &owners,
        &tests,
    ) else {
        return Err("changed field write should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "observing the changed value (the delta) credits exposed"
    );
    assert_eq!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

#[test]
fn empty_delta_operator_change_does_not_credit_unchanged_input_operand() -> Result<(), String> {
    // #1278: `+` -> `-` is an operator change with an EMPTY token delta. The only
    // strong oracle observes the UNCHANGED input parameter `count`, not the
    // changed return value, so the test does not discriminate the change. The
    // #1277 empty-delta fallback must NOT credit a value-family change on an
    // input operand.
    let source = "def next_value(count):\n    return count - 1\n";
    let owners = extract_owners(Path::new("src/counter.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_counter.py"),
        "from src.counter import next_value\n\n\ndef test_next():\n    count = 5\n    result = next_value(count)\n    assert count == 5\n    assert result > 0\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/counter.py"),
        2,
        "    return count - 1",
        Some("    return count + 1"),
        &owners,
        &tests,
    ) else {
        return Err("changed return body should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "an operator change observed only via an unchanged input operand is not exposed"
    );
    assert_ne!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

#[test]
fn empty_delta_operator_change_stays_exposed_when_oracle_observes_owner_output()
-> Result<(), String> {
    // #1278 inverse control: the same operator change IS exposed when a strong
    // oracle observes the owner's OUTPUT by calling it (the `direct` path), not
    // via an input operand. This proves operator-change discrimination is
    // preserved when the test actually exercises the changed value.
    let source = "def next_value(count):\n    return count - 1\n";
    let owners = extract_owners(Path::new("src/counter.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_counter.py"),
        "from src.counter import next_value\n\n\ndef test_next():\n    assert next_value(5) == 4\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/counter.py"),
        2,
        "    return count - 1",
        Some("    return count + 1"),
        &owners,
        &tests,
    ) else {
        return Err("changed return body should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "observing the owner's output (the call result) discriminates the operator change"
    );
    Ok(())
}

#[test]
fn empty_delta_predicate_change_still_credits_outcome_oracle() -> Result<(), String> {
    // #1278 preserve: a CONTROL-flow operator change (`<=` -> `<`) keeps the
    // empty-delta fallback — an outcome oracle (`pytest.raises`) discriminates the
    // changed branch, mirroring `python_cross_file_construct_call`.
    let source = "class Formatter:\n    def __call__(self, event):\n        for key in event:\n            if any(c < \" \" for c in key):\n                raise ValueError(f'Invalid key: \"{key}\"')\n        return \",\".join(event)\n";
    let owners = extract_owners(Path::new("src/formatter.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_render.py"),
        "import pytest\n\nfrom src.formatter import Formatter\n\n\ndef test_rejects_space_in_key():\n    with pytest.raises(ValueError, match='Invalid key'):\n        Formatter()({\"bad key\": \"value\"})\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/formatter.py"),
        4,
        "            if any(c < \" \" for c in key):",
        Some("            if any(c <= \" \" for c in key):"),
        &owners,
        &tests,
    ) else {
        return Err("changed predicate should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a control-flow operator change observed by an outcome oracle stays exposed"
    );
    Ok(())
}

#[test]
fn local_assignment_operator_change_does_not_credit_input_operand() -> Result<(), String> {
    // #1288: `total = base + bonus` -> `base - bonus` is a plain LOCAL ASSIGNMENT
    // with an empty token delta. `classify_probe_shape` defaults it to Control, so
    // the #1278 gate (keyed on delta_kind == Control) wrongly kept the operand
    // fallback and credited the UNCHANGED input `base`. The precise control-flow
    // line check must withhold the fallback for a non-control assignment.
    let source = "def compute(base, bonus):\n    total = base - bonus\n    return total\n";
    let owners = extract_owners(Path::new("src/calc.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_calc.py"),
        "from src.calc import compute\n\n\ndef test_base_unchanged():\n    base = 10\n    compute(base, 3)\n    assert base == 10\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/calc.py"),
        2,
        "    total = base - bonus",
        Some("    total = base + bonus"),
        &owners,
        &tests,
    ) else {
        return Err("changed local assignment should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a local-assignment operator change observed only via an unchanged input operand is not exposed"
    );
    assert_ne!(
        finding.oracle_alignment.as_deref(),
        Some("changed_sink_token")
    );
    Ok(())
}

#[test]
fn augmented_assignment_operator_change_does_not_credit_input_operand() -> Result<(), String> {
    // #1288: augmented assignment `acc += step` -> `acc -= step` likewise defaults
    // to Control in classify_probe_shape; the precise control-flow check must
    // withhold the operand fallback for the unchanged input `step`.
    let source = "def accumulate(values, step):\n    acc = 0\n    for value in values:\n        acc -= step\n    return acc\n";
    let owners = extract_owners(Path::new("src/agg.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_agg.py"),
        "from src.agg import accumulate\n\n\ndef test_step_unchanged():\n    step = 2\n    accumulate([1, 2, 3], step)\n    assert step == 2\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/agg.py"),
        4,
        "        acc -= step",
        Some("        acc += step"),
        &owners,
        &tests,
    ) else {
        return Err("changed augmented assignment should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "an augmented-assignment operator change observed only via an unchanged input is not exposed"
    );
    Ok(())
}

#[test]
fn annotation_only_def_change_emits_no_probe() -> Result<(), String> {
    // #1289: changing only a parameter annotation (`int` -> `str`) has no runtime
    // behavior; Python does not enforce annotations. No probe must be emitted.
    let source = "def discount(amount: str) -> int:\n    return amount\n";
    let owners = extract_owners(Path::new("src/pricing.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_pricing.py"),
        "from src.pricing import discount\n\n\ndef test_discount_passthrough():\n    assert discount(100) == 100\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/pricing.py"),
        1,
        "def discount(amount: str) -> int:",
        Some("def discount(amount: int) -> int:"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_none(),
        "an annotation-only def change carries no behavior delta and must emit no probe"
    );
    Ok(())
}

#[test]
fn return_annotation_only_change_emits_no_probe() -> Result<(), String> {
    // #1289: a return-annotation-only change is likewise a no-op.
    let source = "def parse(text):\n    return int(text)\n";
    let owners = extract_owners(Path::new("src/p.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_p.py"),
        "from src.p import parse\n\n\ndef test_parse():\n    assert parse(\"4\") == 4\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/p.py"),
        1,
        "def parse(text) -> str:",
        Some("def parse(text) -> int:"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_none(),
        "a return-annotation-only change must emit no probe"
    );
    Ok(())
}

#[test]
fn default_value_change_in_def_still_classifies() -> Result<(), String> {
    // #1289 safety: a DEFAULT-VALUE change on a def header is behavioral and must
    // NOT be mistaken for an annotation-only change. The skeleton captures default
    // value source text, so this differs and is still analyzed.
    let source = "def page(size=20):\n    return size\n";
    let owners = extract_owners(Path::new("src/page.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_page.py"),
        "from src.page import page\n\n\ndef test_page_default():\n    assert page() == 20\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/page.py"),
        1,
        "def page(size=20):",
        Some("def page(size=10):"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_some(),
        "a default-value change is behavioral and must still classify (not suppressed as annotation-only)"
    );
    Ok(())
}

#[test]
fn annotation_only_detection_is_conservative() {
    // Annotation-only (suppress):
    assert!(is_annotation_only_def_change(
        "def f(x: int):",
        "def f(x: str):"
    ));
    assert!(is_annotation_only_def_change(
        "def f(x) -> int:",
        "def f(x) -> str:"
    ));
    assert!(is_annotation_only_def_change(
        "    def m(self, a: int, b: Dict[str, int]) -> None:",
        "    def m(self, a: str, b: Dict[str, str]) -> None:"
    ));
    // NOT annotation-only (must still analyze):
    assert!(!is_annotation_only_def_change("def f(x=1):", "def f(x=2):")); // default value
    assert!(!is_annotation_only_def_change("def f(a):", "def f(b):")); // param rename
    assert!(!is_annotation_only_def_change("def f(a):", "def f(a, b):")); // added param
    assert!(!is_annotation_only_def_change(
        "def f(x):",
        "async def f(x):"
    )); // async-ness
    assert!(!is_annotation_only_def_change("def f(x):", "def f(x):")); // identical
    assert!(!is_annotation_only_def_change(
        "    return x + 1",
        "    return x - 1"
    )); // not a def
}

#[test]
fn bare_var_annotation_only_change_at_module_scope_emits_no_probe() -> Result<(), String> {
    // #1289: a module-scope annotated variable whose ONLY change is the
    // annotation (`int` -> `str`, value unchanged) has no runtime behavior —
    // Python does not enforce annotations at module scope. No probe.
    let source = "CACHE_TTL: str = 30\n\n\ndef get_ttl():\n    return CACHE_TTL\n";
    let owners = extract_owners(Path::new("src/config.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_config.py"),
        "from src.config import get_ttl\n\n\ndef test_ttl():\n    assert get_ttl() == 30\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/config.py"),
        1,
        "CACHE_TTL: str = 30",
        Some("CACHE_TTL: int = 30"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_none(),
        "a module-scope annotation-only var change carries no behavior delta and must emit no probe"
    );
    Ok(())
}

#[test]
fn bare_var_annotation_only_change_no_value_emits_no_probe() -> Result<(), String> {
    // #1289: a pure annotation with no value (`x: int` -> `x: str`) is also a
    // no-op at module scope when only the annotation differs.
    let source = "LABEL: str\n\n\ndef get_label():\n    return LABEL\n";
    let owners = extract_owners(Path::new("src/config.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_config.py"),
        "from src.config import get_label\n\n\ndef test_label():\n    assert get_label() == \"x\"\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/config.py"),
        1,
        "LABEL: str",
        Some("LABEL: int"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_none(),
        "a pure annotation change (no value) at module scope must emit no probe"
    );
    Ok(())
}

#[test]
fn bare_var_value_change_still_classifies() -> Result<(), String> {
    // #1289 safety: a VALUE change on an annotated variable is behavioral
    // and must NOT be suppressed. The skeleton captures the value source, so
    // `= 5` vs `= 6` differs and the line is still analyzed.
    let source = "LIMIT: int = 6\n\n\ndef get_limit():\n    return LIMIT\n";
    let owners = extract_owners(Path::new("src/config.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_config.py"),
        "from src.config import get_limit\n\n\ndef test_limit():\n    assert get_limit() == 6\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/config.py"),
        1,
        "LIMIT: int = 6",
        Some("LIMIT: int = 5"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_some(),
        "a value change is behavioral and must still classify (not suppressed as annotation-only)"
    );
    Ok(())
}

#[test]
fn bare_var_annotation_change_in_class_body_still_classifies() -> Result<(), String> {
    // #1289 safety: an annotation-only change INSIDE a class body is NOT
    // suppressed — `@dataclass`/Pydantic make class-body annotations
    // runtime-meaningful, and base-class tracking does not exist yet. The
    // guard is module-scope only; fail closed for class bodies.
    let source =
        "class Config:\n    ttl: str = 30\n\n    def get(self):\n        return self.ttl\n";
    let owners = extract_owners(Path::new("src/config.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_config.py"),
        "from src.config import Config\n\n\ndef test_ttl():\n    assert Config().get() == 30\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/config.py"),
        2,
        "    ttl: str = 30",
        Some("    ttl: int = 30"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_some(),
        "a class-body annotation-only change must still classify (fail closed for class bodies)"
    );
    Ok(())
}

#[test]
fn non_annotation_assignment_still_classifies() -> Result<(), String> {
    // #1289 safety: a plain assignment (`x = 5`, no annotation) is not an
    // annotated variable and must classify normally.
    let source = "COUNT = 6\n\n\ndef get_count():\n    return COUNT\n";
    let owners = extract_owners(Path::new("src/config.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_config.py"),
        "from src.config import get_count\n\n\ndef test_count():\n    assert get_count() == 6\n",
    );
    let finding = classify_change_with_old(
        Path::new("src/config.py"),
        1,
        "COUNT = 6",
        Some("COUNT = 5"),
        &owners,
        &tests,
    );
    assert!(
        finding.is_some(),
        "a plain assignment (no annotation) must still classify"
    );
    Ok(())
}

#[test]
fn is_annotation_only_var_change_is_conservative() {
    // Annotation-only (suppress):
    assert!(is_annotation_only_var_change("x: int = 5", "x: str = 5")); // value identical
    assert!(is_annotation_only_var_change("x: int", "x: str")); // no value either side
    assert!(is_annotation_only_var_change(
        "MAX: List[int] = []",
        "MAX: List[str] = []"
    )); // subscripted annotation, value identical
    // NOT annotation-only (must still analyze):
    assert!(!is_annotation_only_var_change("x: int = 5", "x: int = 6")); // value changed
    assert!(!is_annotation_only_var_change("x: int", "x: int = 5")); // value added
    assert!(!is_annotation_only_var_change("x: int = 5", "x: int")); // value removed
    assert!(!is_annotation_only_var_change("a: int = 5", "b: int = 5")); // target rename
    assert!(!is_annotation_only_var_change("x: int = 5", "x: int = 5")); // identical
    assert!(!is_annotation_only_var_change("x = 5", "x = 6")); // not an annotation
    assert!(!is_annotation_only_var_change(
        "    return x + 1",
        "    return x - 1"
    )); // not an assignment at all
}

#[test]
fn dict_changed_element_sibling_key_oracle_not_exposed() -> Result<(), String> {
    // #1290: the changed key is `port`, but the only strong oracle observes the
    // unchanged SIBLING key `host`, so it does not discriminate the change.
    let source = "def build_config():\n    return {\"host\": \"localhost\", \"port\": 9090}\n";
    let owners = extract_owners(Path::new("src/conf.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_conf.py"),
        "from src.conf import build_config\n\n\ndef test_host():\n    assert build_config()[\"host\"] == \"localhost\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/conf.py"),
        2,
        "    return {\"host\": \"localhost\", \"port\": 9090}",
        Some("    return {\"host\": \"localhost\", \"port\": 8080}"),
        &owners,
        &tests,
    ) else {
        return Err("changed dict literal should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "an oracle observing a sibling dict key does not discriminate the changed key"
    );
    Ok(())
}

#[test]
fn dict_changed_element_observed_value_stays_exposed() -> Result<(), String> {
    // #1290 preserve: the oracle observes the CHANGED key's new value, so it
    // genuinely discriminates the change.
    let source = "def build_config():\n    return {\"host\": \"localhost\", \"port\": 9090}\n";
    let owners = extract_owners(Path::new("src/conf.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_conf.py"),
        "from src.conf import build_config\n\n\ndef test_port():\n    assert build_config()[\"port\"] == 9090\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/conf.py"),
        2,
        "    return {\"host\": \"localhost\", \"port\": 9090}",
        Some("    return {\"host\": \"localhost\", \"port\": 8080}"),
        &owners,
        &tests,
    ) else {
        return Err("changed dict literal should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "observing the changed key's value discriminates the change"
    );
    Ok(())
}

#[test]
fn list_changed_element_sibling_index_oracle_not_exposed() -> Result<(), String> {
    // #1290: index 1 changed (`search` -> `browse`), but the only strong oracle
    // observes the unchanged SIBLING index 0, so it does not discriminate.
    let source = "def route_order():\n    return [\"index\", \"browse\", \"detail\"]\n";
    let owners = extract_owners(Path::new("src/routes.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_routes.py"),
        "from src.routes import route_order\n\n\ndef test_first():\n    assert route_order()[0] == \"index\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/routes.py"),
        2,
        "    return [\"index\", \"browse\", \"detail\"]",
        Some("    return [\"index\", \"search\", \"detail\"]"),
        &owners,
        &tests,
    ) else {
        return Err("changed list literal should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "an oracle observing a sibling list index does not discriminate the changed index"
    );
    Ok(())
}

#[test]
fn list_changed_element_observed_index_stays_exposed() -> Result<(), String> {
    // #1290 preserve: observing the changed index credits.
    let source = "def route_order():\n    return [\"index\", \"browse\", \"detail\"]\n";
    let owners = extract_owners(Path::new("src/routes.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_routes.py"),
        "from src.routes import route_order\n\n\ndef test_second():\n    assert route_order()[1] == \"browse\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/routes.py"),
        2,
        "    return [\"index\", \"browse\", \"detail\"]",
        Some("    return [\"index\", \"search\", \"detail\"]"),
        &owners,
        &tests,
    ) else {
        return Err("changed list literal should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "observing the changed index discriminates the change"
    );
    Ok(())
}

#[test]
fn fstring_return_is_not_treated_as_dict_literal() -> Result<(), String> {
    // #1290 follow-up regression guard: an f-string `f"{value:.3f}"` contains `{`
    // and `}` but is NOT a dict literal — it must not be gated by the dict-element
    // check. A genuine f-string discriminator stays exposed.
    assert!(parse_dict_literal_fields("    return f\"{value:.3f}\"").is_none());
    assert!(
        dict_changed_keys_and_values(
            Some("    return f\"{value:.2f}\""),
            "    return f\"{value:.3f}\""
        )
        .is_none()
    );
    let source = "def render_price(value):\n    return f\"{value:.3f}\"\n";
    let owners = extract_owners(Path::new("src/price.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_price.py"),
        "from src.price import render_price\n\n\ndef test_render_price_uses_three_decimals():\n    assert render_price(3.14159) == \"3.142\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/price.py"),
        2,
        "    return f\"{value:.3f}\"",
        Some("    return f\"{value:.2f}\""),
        &owners,
        &tests,
    ) else {
        return Err("changed f-string should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "a genuine f-string discriminator must not be downgraded by the dict-element gate"
    );
    Ok(())
}

#[test]
fn fstring_length_invariant_change_via_len_aggregate_not_exposed() -> Result<(), String> {
    // #1290 1b: `f"OK:{code}"` -> `f"NO:{code}"` changes only equal-length literal
    // text (interpolation unchanged), so output length is invariant. The only
    // strong oracle is `len(...)`, which cannot discriminate it.
    let source = "def status_label(code):\n    return f\"NO:{code}\"\n";
    let owners = extract_owners(Path::new("src/status.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_status.py"),
        "from src.status import status_label\n\n\ndef test_len():\n    assert len(status_label(7)) == 4\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/status.py"),
        2,
        "    return f\"NO:{code}\"",
        Some("    return f\"OK:{code}\""),
        &owners,
        &tests,
    ) else {
        return Err("changed f-string should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a length-invariant f-string change observed only via len() is not discriminated"
    );
    Ok(())
}

#[test]
fn fstring_length_invariant_change_with_string_oracle_stays_exposed() -> Result<(), String> {
    // #1290 1b preserve: the same length-invariant change observed by an exact
    // string comparison IS discriminated.
    let source = "def status_label(code):\n    return f\"NO:{code}\"\n";
    let owners = extract_owners(Path::new("src/status.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_status.py"),
        "from src.status import status_label\n\n\ndef test_exact():\n    assert status_label(7) == \"NO:7\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/status.py"),
        2,
        "    return f\"NO:{code}\"",
        Some("    return f\"OK:{code}\""),
        &owners,
        &tests,
    ) else {
        return Err("changed f-string should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "an exact string oracle observes the changed f-string output"
    );
    Ok(())
}

#[test]
fn error_path_change_with_value_oracle_not_exposed() -> Result<(), String> {
    // #1290 Class C: a `raise` type change on an untaken branch, observed only by a
    // normal-path value oracle (the test never triggers the raise), is not
    // discriminated.
    let source = "def parse(text):\n    if not text:\n        raise KeyError(\"empty\")\n    return int(text)\n";
    let owners = extract_owners(Path::new("src/parseint.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_parseint.py"),
        "from src.parseint import parse\n\n\ndef test_parse_ok():\n    assert parse(\"42\") == 42\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/parseint.py"),
        3,
        "        raise KeyError(\"empty\")",
        Some("        raise ValueError(\"empty\")"),
        &owners,
        &tests,
    ) else {
        return Err("changed raise should classify".to_string());
    };
    assert_ne!(
        finding.class,
        ExposureClass::Exposed,
        "a raise change observed only by a normal-path value oracle is not discriminated"
    );
    Ok(())
}

#[test]
fn error_path_change_with_exception_oracle_stays_exposed() -> Result<(), String> {
    // #1290 Class C preserve: the same raise change IS exposed when the test
    // observes the raised exception via pytest.raises.
    let source = "def parse(text):\n    if not text:\n        raise KeyError(\"empty\")\n    return int(text)\n";
    let owners = extract_owners(Path::new("src/parseint.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_parseint.py"),
        "import pytest\n\nfrom src.parseint import parse\n\n\ndef test_parse_empty():\n    with pytest.raises(KeyError, match=\"empty\"):\n        parse(\"\")\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/parseint.py"),
        3,
        "        raise KeyError(\"empty\")",
        Some("        raise ValueError(\"empty\")"),
        &owners,
        &tests,
    ) else {
        return Err("changed raise should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "an exception oracle observes the changed raise"
    );
    Ok(())
}

#[test]
fn changed_default_explicit_kwarg_override_not_exposed() -> Result<(), String> {
    // #1289 trap 45: the `verbose` default changes (False -> True), but the only
    // strong oracle calls `render("Sam", verbose=False)`, explicitly overriding
    // the parameter. The changed default is never exercised, so the test passes
    // identically before and after — not discriminated.
    let source =
        "def render(name, verbose=True):\n    return f\"[debug] {name}\" if verbose else name\n";
    let owners = extract_owners(Path::new("src/render.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_render.py"),
        "from src.render import render\n\n\ndef test_render_explicit_verbose_false():\n    assert render(\"Sam\", verbose=False) == \"Sam\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/render.py"),
        1,
        "def render(name, verbose=True):",
        Some("def render(name, verbose=False):"),
        &owners,
        &tests,
    ) else {
        return Err("a default-value change should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "an explicit kwarg override does not exercise the changed default"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|entry| entry.contains("without `verbose`")),
        "the downgrade must name the parameter to test by omission"
    );
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `render` without `verbose`"),
        "the structured missing discriminator must carry the omission guidance"
    );
    Ok(())
}

#[test]
fn changed_default_explicit_positional_override_not_exposed() -> Result<(), String> {
    // #1289 trap 45: a positional argument at `verbose`'s index (1) overrides the
    // changed default just as a kwarg would.
    let source =
        "def render(name, verbose=True):\n    return f\"[debug] {name}\" if verbose else name\n";
    let owners = extract_owners(Path::new("src/render.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_render.py"),
        "from src.render import render\n\n\ndef test_render_positional_false():\n    assert render(\"Sam\", False) == \"Sam\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/render.py"),
        1,
        "def render(name, verbose=True):",
        Some("def render(name, verbose=False):"),
        &owners,
        &tests,
    ) else {
        return Err("a default-value change should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::WeaklyExposed,
        "an explicit positional override does not exercise the changed default"
    );
    assert!(
        finding
            .missing
            .iter()
            .any(|entry| entry.contains("without `verbose`")),
        "the downgrade must name the parameter to test by omission"
    );
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `render` without `verbose`"),
        "the structured missing discriminator must carry the omission guidance"
    );
    Ok(())
}

#[test]
fn changed_default_used_by_omission_stays_exposed() -> Result<(), String> {
    // #1289 trap 45 preserve: when the call OMITS the parameter, the changed
    // default IS exercised, and a strong oracle observing the output discriminates
    // it. Must stay exposed.
    let source =
        "def render(name, verbose=True):\n    return f\"[debug] {name}\" if verbose else name\n";
    let owners = extract_owners(Path::new("src/render.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_render.py"),
        "from src.render import render\n\n\ndef test_render_default_verbose():\n    assert render(\"Sam\") == \"[debug] Sam\"\n",
    );
    let Some(finding) = classify_change_with_old(
        Path::new("src/render.py"),
        1,
        "def render(name, verbose=True):",
        Some("def render(name, verbose=False):"),
        &owners,
        &tests,
    ) else {
        return Err("a default-value change should classify".to_string());
    };
    assert_eq!(
        finding.class,
        ExposureClass::Exposed,
        "omitting the parameter exercises the changed default under a strong oracle"
    );
    Ok(())
}

#[test]
fn changed_default_value_params_detects_pure_value_change_only() -> Result<(), String> {
    // Pure default-value change -> the changed parameter is reported.
    let Some(changed) = changed_default_value_params(
        "def render(name, verbose=False):",
        "def render(name, verbose=True):",
    ) else {
        return Err("a value-to-value default change is a pure default-value change".to_string());
    };
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].name, "verbose");
    assert_eq!(changed[0].index, 1);
    assert!(changed[0].positionally_bindable);
    // No default change at all -> None.
    assert!(changed_default_value_params("def f(x=1):", "def f(x=1):").is_none());
    // Added default (requiredness change) -> None, fail open.
    assert!(changed_default_value_params("def f(x):", "def f(x=1):").is_none());
    // Removed default -> None, fail open.
    assert!(changed_default_value_params("def f(x=1):", "def f(x):").is_none());
    // Param rename alongside a default change -> None (not a pure value change).
    assert!(changed_default_value_params("def f(a=1):", "def f(b=2):").is_none());
    // Added parameter -> None.
    assert!(changed_default_value_params("def f(x=1):", "def f(x=1, y=2):").is_none());
    // Not a def header -> None.
    assert!(changed_default_value_params("    return x + 1", "    return x - 1").is_none());
    Ok(())
}

#[test]
fn analyze_call_args_classifies_positional_and_keyword() -> Result<(), String> {
    let Some(shape) = analyze_call_args("\"Sam\", verbose=False") else {
        return Err("positional + kwarg call is tractable".to_string());
    };
    assert_eq!(shape.positional_count, 1);
    assert_eq!(shape.keywords, vec!["verbose".to_string()]);
    // Comparison operators are not keyword bindings.
    let Some(cmp) = analyze_call_args("x == 1, y") else {
        return Err("comparison-operand call is tractable".to_string());
    };
    assert_eq!(cmp.positional_count, 2);
    assert!(cmp.keywords.is_empty());
    // Nested calls and brackets stay one positional argument each.
    let Some(nested) = analyze_call_args("g(a, b), [1, 2], k=3") else {
        return Err("nested-argument call is tractable".to_string());
    };
    assert_eq!(nested.positional_count, 2);
    assert_eq!(nested.keywords, vec!["k".to_string()]);
    // *args / **kwargs unpacking is undecidable -> None (fail open).
    assert!(analyze_call_args("*args").is_none());
    assert!(analyze_call_args("a, **kwargs").is_none());
    // An inline `# comment` in the arglist makes binding ambiguous -> None
    // (fail open, never a false-clean from a comment-parsed `)` or text).
    assert!(analyze_call_args("a  # note with ) paren").is_none());
    Ok(())
}
