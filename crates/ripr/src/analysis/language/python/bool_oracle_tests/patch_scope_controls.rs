//! Rebinding controls for the patch-scope exemptions (#10391).

use super::*;

#[test]
fn constructor_and_call_results_cannot_escape_mock_tracking() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    for body in [
        "    handle = (mock.Mock())\n    handle.side_effect = change_default\n    handle()\n",
        "    handle, = (mock.Mock(),)\n    handle.side_effect = change_default\n    handle()\n",
        "    use(mock.Mock())\n",
        "    handle = mock.Mock()\n    other = handle()\n    other.side_effect = change_default\n    other()\n",
        "    handle = mock.Mock()\n    f\"{(other := handle())}\"\n    other.side_effect = change_default\n    other()\n",
        "    handle = mock.Mock()\n    f\"{(other := ｈａｎｄｌｅ())}\"\n    other.side_effect = change_default\n    other()\n",
        "    handle = mock.Mock()\n    ｈａｎｄｌｅ.side_effect = change_default\n    ｈａｎｄｌｅ()\n",
        "    class Holder:\n        handle = mock.Mock()\n    Holder.handle.side_effect = change_default\n    Holder.handle()\n",
    ] {
        // Use a different helper parameter so the assigned-child control cannot
        // pass because of an unrelated duplicate binding of `handle`.
        let tests = tests_with(
            "from unittest import mock\ndef change_default():\n    is_adult.__defaults__ = (0,)\ndef use(value):\n    value.side_effect = change_default\n    value()\n",
            "",
            body,
        );
        assert_eq!(
            credited_strengths(owner, &tests).last(),
            Some(&OracleStrength::Smoke),
            "{tests}"
        );
    }
}

#[test]
fn mock_calls_and_inspection_do_not_execute_user_equality_or_rebound_methods() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    for (prelude, body) in [
        (
            "from unittest import mock\nclass Value:\n    def __eq__(self, other):\n        is_adult.__defaults__ = (0,)\n        return True\n",
            "    handle = mock.Mock()\n    handle(Value())\n    handle.assert_called_with('x')\n",
        ),
        (
            "from unittest import mock\nclass Local:\n    def assert_called(self):\n        is_adult.__defaults__ = (0,)\n",
            "    handle = mock.Mock()\n    handle = Local()\n    handle.assert_called()\n",
        ),
    ] {
        let tests = tests_with(prelude, "", body);
        assert_eq!(
            credited_strengths(owner, &tests).last(),
            Some(&OracleStrength::Smoke),
            "{tests}"
        );
    }
}

#[test]
fn comprehension_targets_do_not_borrow_mock_receiver_or_import_identity() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    for (prelude, body) in [
        (
            "from unittest import mock\nclass Local:\n    def reset_mock(self):\n        is_adult.__defaults__ = (0,)\n",
            "    handle = mock.Mock()\n    [handle.reset_mock() for handle in [Local()]]\n",
        ),
        (
            "import unittest.mock\nclass Factory:\n    def Mock(self):\n        is_adult.__defaults__ = (0,)\nclass Local:\n    @property\n    def mock(self):\n        return Factory()\n",
            "    [unittest.mock.Mock() for unittest in [Local()]]\n",
        ),
    ] {
        let tests = tests_with(prelude, "", body);
        assert_eq!(
            credited_strengths(owner, &tests).last(),
            Some(&OracleStrength::Smoke),
            "{tests}"
        );
    }
}

#[test]
fn environment_arguments_do_not_execute_user_protocols() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    for (method, body) in [
        ("__fspath__", "    monkeypatch.chdir(Value())\n"),
        ("__str__", "    monkeypatch.setenv('A', Value())\n"),
    ] {
        let prelude = format!(
            "class Value:\n    def {method}(self):\n        is_adult.__defaults__ = (0,)\n        return '/tmp'\n"
        );
        let tests = tests_with(&prelude, "monkeypatch", body);
        assert_eq!(
            credited_strengths(owner, &tests),
            vec![OracleStrength::Smoke],
            "{tests}"
        );
    }
}

#[test]
fn mock_constructor_overrides_do_not_authenticate_inspection() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    let tests = tests_with(
        "from unittest import mock\ndef change_default():\n    is_adult.__defaults__ = (0,)\n",
        "",
        "    mock_handle = mock.Mock(assert_not_called=change_default)\n    mock_handle.assert_not_called()\n",
    );
    assert_eq!(
        credited_strengths(owner, &tests).last(),
        Some(&OracleStrength::Smoke),
        "{tests}"
    );
}

#[test]
fn helper_parameters_are_not_pytest_fixture_bindings() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    let prelude = "class Local:\n    def setenv(self, *args):\n        is_adult.__defaults__ = (0,)\ndef apply(monkeypatch):\n    monkeypatch.setenv('A', 'B')\n";
    let tests = tests_with(prelude, "", "    apply(Local())\n");
    assert_eq!(
        credited_strengths(owner, &tests),
        vec![OracleStrength::Smoke],
        "{tests}"
    );
}

#[test]
fn default_parameters_are_not_pytest_fixture_bindings() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    let prelude =
        "class Local:\n    def setenv(self, *args):\n        is_adult.__defaults__ = (0,)\n";
    let tests = tests_with(
        prelude,
        "monkeypatch=Local()",
        "    monkeypatch.setenv('A', 'B')\n",
    );
    assert_eq!(
        credited_strengths(owner, &tests),
        vec![OracleStrength::Smoke],
        "{tests}"
    );
}

#[test]
fn explicitly_supplied_test_parameters_are_not_fixture_bindings() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    let prelude = "class Local:\n    def setenv(self, *args):\n        is_adult.__defaults__ = (0,)\ndef test_setup(monkeypatch):\n    monkeypatch.setenv('A', 'B')\n";
    for body in [
        "    test_setup(Local())\n",
        "    f\"{test_setup(Local())}\"\n",
    ] {
        let tests = tests_with(prelude, "", body);
        assert_eq!(
            credited_strengths(owner, &tests).last(),
            Some(&OracleStrength::Smoke),
            "{tests}"
        );
    }
}

#[test]
fn executable_strings_do_not_hide_a_patch_factory() {
    for statement in [
        "f\"{mock.patch('tests.test_rules.is_adult', return_value=True).start()}\"",
        "value = f\"{mock.patch('tests.test_rules.is_adult', return_value=True).start()}\"",
        "f\"{monkeypatch.setattr('tests.test_rules.is_adult', lambda age: True)}\"",
    ] {
        let tests = tests_with(
            "from unittest import mock\n",
            "monkeypatch",
            &format!("    {statement}\n"),
        );
        assert_eq!(
            credited_strengths(BOOL_OWNER, &tests),
            vec![OracleStrength::Smoke],
            "{tests}"
        );
    }
}

#[test]
fn parameter_expressions_do_not_hide_a_patch_factory() {
    for (prelude, params, body) in [
        (
            "from unittest import mock\n",
            "swap=lambda: mock.patch",
            "    swap()('tests.test_rules.is_adult', return_value=True).start()\n",
        ),
        (
            "from unittest import mock\n",
            "swap: mock.patch = None",
            "    test_adult.__annotations__['swap']('tests.test_rules.is_adult', return_value=True).start()\n",
        ),
        (
            "from unittest.mock import patch\n",
            "swap=(0, patch)[1]",
            "    swap('tests.test_rules.is_adult', return_value=True).start()\n",
        ),
    ] {
        let tests = tests_with(prelude, params, body);
        assert_eq!(
            credited_strengths(BOOL_OWNER, &tests),
            vec![OracleStrength::Smoke],
            "{tests}"
        );
    }
}

#[test]
fn constructor_and_inspection_spellings_do_not_authenticate_the_receiver() {
    let owner = "def is_adult(age, threshold=18) -> bool:\n    return age >= threshold\n";
    for (receiver, method) in [
        ("patch", "Mock"),
        ("patch", "assertion"),
        ("monkeypatch", "setenv"),
    ] {
        let prelude = format!(
            "class {receiver}:\n    @staticmethod\n    def {method}():\n        is_adult.__defaults__ = (0,)\n"
        );
        let tests = tests_with(&prelude, "", &format!("    {receiver}.{method}()\n"));
        assert_eq!(
            credited_strengths(owner, &tests),
            vec![OracleStrength::Smoke],
            "{tests}"
        );
    }
    let tests = tests_with(
        "from unittest import mock\nclass mock:\n    @staticmethod\n    def MagicMock():\n        is_adult.__defaults__ = (0,)\n",
        "",
        "    mock.MagicMock()\n",
    );
    assert_eq!(
        credited_strengths(owner, &tests),
        vec![OracleStrength::Smoke],
        "{tests}"
    );
}

#[test]
fn canonical_mock_constructors_and_fixture_types_keep_both_exact_pins() {
    for (prelude, params, setup) in [
        (
            "from unittest import mock\n",
            "monkeypatch",
            "    monkeypatch.setenv('A', '1')\n    monkeypatch.delenv('A')\n    monkeypatch.chdir('/tmp')\n    handle = mock.MagicMock()\n    handle.assert_not_called()\n    handle.reset_mock()\n",
        ),
        (
            "from unittest.mock import Mock\n",
            "",
            "    handle = Mock()\n    handle.assert_not_called()\n",
        ),
        (
            "import unittest.mock\n",
            "",
            "    handle = unittest.mock.Mock()\n    handle.assert_not_called()\n",
        ),
        (
            "from unittest import mock\n",
            "",
            "    mock_handle = mock.Mock()\n    mock_handle.assert_not_called()\n    mock_handle.reset_mock()\n",
        ),
        (
            "import pytest\n",
            "monkeypatch: pytest.MonkeyPatch",
            "    monkeypatch.setenv('A', '1')\n",
        ),
    ] {
        let tests = format!(
            "from src.rules import is_adult\n{prelude}\ndef test_adult({params}):\n{setup}    assert is_adult(18)\n    assert not is_adult(17)\n"
        );
        let strengths = credited_strengths(BOOL_OWNER, &tests);
        assert!(
            strengths.len() >= 2,
            "both intended assertions must be extracted"
        );
        assert_eq!(
            &strengths[strengths.len() - 2..],
            &[OracleStrength::Strong, OracleStrength::Strong],
            "{tests}"
        );
    }
}

#[test]
fn fstring_messages_and_plain_context_names_without_a_patcher_stay_credited() {
    let owner = "def is_adult(age):\n    return age >= 18\n";
    for tests in [
        "from src.rules import is_adult\n\n\ndef test_adult():\n    age = 18\n    assert is_adult(age), f\"age {age} should be adult\"\n    assert not is_adult(17)\n",
        "from src.rules import is_adult\n\n\ndef test_adult():\n    context = 18\n    assert is_adult(context)\n    assert not is_adult(17)\n",
    ] {
        assert_eq!(
            credited_strengths(owner, tests).last(),
            Some(&OracleStrength::Strong),
            "{tests}"
        );
    }
}

#[test]
fn fstring_and_context_names_with_a_patcher_still_disqualify() {
    let owner = "def is_adult(age):\n    return age >= 18\n";
    for tests in [
        "from src.rules import is_adult\n\n\ndef test_adult(monkeypatch):\n    with monkeypatch.context() as ctx:\n        ctx.setattr('src.rules.is_adult', lambda a: True)\n    assert is_adult(18)\n",
        "from src.rules import is_adult\n\n\ndef test_adult(monkeypatch):\n    monkeypatch.setattr('src.rules.is_adult', lambda a: True)\n    assert is_adult(18), f\"{18}\"\n",
    ] {
        assert_ne!(
            credited_strengths(owner, tests).last(),
            Some(&OracleStrength::Strong),
            "{tests}"
        );
    }
}
