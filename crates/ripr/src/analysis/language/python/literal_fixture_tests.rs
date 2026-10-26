//! A conftest fixture that returns a fresh literal-built instance of the
//! method owner's class resolves the `unresolved_pytest_fixture` limit
//! (#9619, `py-fixtures-deposit`). Every other fixture shape, and any way the
//! instance can differ from the inlined construction, keeps the limit.

use super::fixtures_tests::workspace;
use super::*;
use std::path::Path;

const SHOP: &str = "class Account:\n    def __init__(self, balance):\n        self.balance = balance\n\n    def deposit(self, amount):\n        self.balance += amount\n\n\nclass Ledger:\n    def __init__(self, balance):\n        self.balance = balance\n";

const CONFTEST: &str = "import pytest\n\nfrom shop import Account\n\n\n@pytest.fixture\ndef account():\n    return Account(100)\n";

const TEST: &str = "def test_deposit_through_fixture(account):\n    account.deposit(50)\n    assert account.balance == 150\n";

const INLINED: &str = "from shop import Account\n\n\ndef test_deposit_inline():\n    account = Account(100)\n    account.deposit(50)\n    assert account.balance == 150\n";

fn classify_deposit(files: &[(&str, &str)]) -> Result<Finding, String> {
    let (owners, tests) = workspace(files);
    classify_change(
        Path::new("shop/__init__.py"),
        6,
        "        self.balance = self.balance + amount",
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed deposit line must classify".to_string())
}

fn with_conftest(conftest: &str) -> Result<Finding, String> {
    with_files(conftest, TEST)
}

fn with_files(conftest: &str, test: &str) -> Result<Finding, String> {
    classify_deposit(&[
        ("shop/__init__.py", SHOP),
        ("tests/conftest.py", conftest),
        ("tests/test_shop.py", test),
    ])
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
fn the_inlined_construction_is_exposed() -> Result<(), String> {
    let finding = classify_deposit(&[("shop/__init__.py", SHOP), ("tests/test_shop.py", INLINED)])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_literal_owner_instance_fixture_reads_like_the_inlined_construction() -> Result<(), String> {
    let finding = with_conftest(CONFTEST)?;
    assert_eq!(finding.static_limit_kind, None);
    assert_eq!(finding.class, ExposureClass::Exposed);
    // A docstring, `@pytest.fixture()` and literal keywords keep the shape.
    let variant = CONFTEST
        .replace("@pytest.fixture\n", "@pytest.fixture()\n")
        .replace(
            "    return Account(100)",
            "    \"\"\"A fresh account.\"\"\"\n    return Account(balance=100)",
        );
    assert_eq!(with_conftest(&variant)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_same_module_literal_fixture_is_resolved_too() -> Result<(), String> {
    let test = format!(
        "import pytest\n\nfrom shop import Account\n\n\n@pytest.fixture\ndef account():\n    return Account(100)\n\n\n{TEST}"
    );
    let finding = classify_deposit(&[("shop/__init__.py", SHOP), ("tests/test_shop.py", &test)])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn fixture_shapes_other_than_a_bare_literal_construction_keep_the_limit() -> Result<(), String> {
    let cases = [
        (
            "yield",
            CONFTEST.replace("    return Account(100)", "    yield Account(100)"),
        ),
        (
            "extra statements",
            CONFTEST.replace(
                "    return Account(100)",
                "    account = Account(100)\n    account.deposit(1)\n    return account",
            ),
        ),
        (
            "fixture parameter",
            CONFTEST.replace("def account():", "def account(request):"),
        ),
        (
            "module scope",
            CONFTEST.replace("@pytest.fixture\n", "@pytest.fixture(scope=\"module\")\n"),
        ),
        (
            "params",
            CONFTEST.replace("@pytest.fixture\n", "@pytest.fixture(params=[1, 2])\n"),
        ),
        (
            "second decorator",
            CONFTEST.replace("@pytest.fixture\n", "@pytest.fixture\n@functools.cache\n"),
        ),
        (
            "async",
            CONFTEST.replace("def account():", "async def account():"),
        ),
        (
            "non-literal argument",
            CONFTEST.replace("    return Account(100)", "    return Account(START)"),
        ),
        (
            "starred argument",
            CONFTEST.replace("    return Account(100)", "    return Account(*[100])"),
        ),
        (
            "different class",
            CONFTEST
                .replace("from shop import Account", "from shop import Ledger")
                .replace("Account(100)", "Ledger(100)"),
        ),
        (
            "class from another module",
            CONFTEST.replace("from shop import Account", "from bank import Account"),
        ),
        (
            "renamed import",
            CONFTEST
                .replace(
                    "from shop import Account",
                    "from shop import Account as Acct",
                )
                .replace("Account(100)", "Acct(100)"),
        ),
        (
            "class rebound after the import",
            CONFTEST.replace(
                "from shop import Account\n",
                "from shop import Account\n\n\nclass Account:\n    pass\n",
            ),
        ),
        (
            "fixture name rebound",
            format!("{CONFTEST}\n\naccount = None\n"),
        ),
        (
            "star import",
            CONFTEST.replace("import pytest\n", "import pytest\nfrom helpers import *\n"),
        ),
        (
            "pytest_plugins",
            format!("pytest_plugins = [\"bank_plugin\"]\n{CONFTEST}"),
        ),
    ];
    for (case, conftest) in cases {
        assert_fixture_limited(&with_conftest(&conftest)?, case);
    }
    Ok(())
}

#[test]
fn another_fixture_or_hook_that_names_the_fixture_keeps_the_limit() -> Result<(), String> {
    let cases = [
        (
            "autouse fixture that deposits",
            format!(
                "{CONFTEST}\n\n@pytest.fixture(autouse=True)\ndef primed(account):\n    account.deposit(1)\n"
            ),
        ),
        (
            "renamed fixture that deposits",
            format!(
                "{CONFTEST}\n\n@pytest.fixture(name=\"primed\", autouse=True)\ndef _primed(account):\n    account.deposit(1)\n"
            ),
        ),
        (
            "getfixturevalue by string",
            format!(
                "{CONFTEST}\n\n@pytest.fixture(autouse=True)\ndef primed(request):\n    request.getfixturevalue(\"account\").deposit(1)\n"
            ),
        ),
        (
            "hook that reaches the funcarg",
            format!(
                "{CONFTEST}\n\ndef pytest_runtest_call(item):\n    item.funcargs[\"account\"].deposit(1)\n"
            ),
        ),
    ];
    for (case, conftest) in cases {
        assert_fixture_limited(&with_conftest(&conftest)?, case);
    }
    // The same interference from a conftest further up the chain.
    let upper = "import pytest\n\n\n@pytest.fixture(autouse=True)\ndef primed(account):\n    account.deposit(1)\n";
    let finding = classify_deposit(&[
        ("shop/__init__.py", SHOP),
        ("conftest.py", upper),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_shop.py", TEST),
    ])?;
    assert_fixture_limited(&finding, "upper conftest autouse fixture");
    Ok(())
}

#[test]
fn a_test_that_does_not_read_the_fixture_as_built_keeps_the_limit() -> Result<(), String> {
    let cases = [
        (
            "test rebinds the parameter",
            TEST.replace(
                "    account.deposit(50)",
                "    account = make_account()\n    account.deposit(50)",
            ),
        ),
        (
            "test module binds the name",
            format!("from helpers import account\n\n\n{TEST}"),
        ),
        (
            "class-level test",
            "class TestShop:\n    def test_deposit_through_fixture(self, account):\n        account.deposit(50)\n        assert account.balance == 150\n".to_string(),
        ),
    ];
    for (case, test) in cases {
        assert_fixture_limited(&with_files(CONFTEST, &test)?, case);
    }
    Ok(())
}
