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
