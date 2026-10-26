//! A fixture whose literal `params=` rows reach the test through
//! `request.param` and a leading unpack reads as the parametrized test it is
//! (#9619, `py-fixtures-fee-boundary`). Every other shape, and any way the
//! values can differ from the rows, keeps the `unresolved_pytest_fixture`
//! limit.

use super::fixtures_tests::workspace;
use super::*;
use std::path::Path;

const SHOP: &str = "def handling_fee(amount):\n    \"\"\"Orders of 100 or more pay no handling fee.\"\"\"\n    return 0 if amount >= 100 else 5\n";

const CONFTEST: &str = "import pytest\n\n\n@pytest.fixture(params=[(99, 5), (100, 0)], ids=[\"below\", \"at\"])\ndef fee_case(request):\n    return request.param\n";

const TEST: &str = "from shop import handling_fee\n\n\ndef test_handling_fee_boundary(fee_case):\n    amount, fee = fee_case\n    assert handling_fee(amount) == fee\n";

const DECORATED: &str = "import pytest\n\nfrom shop import handling_fee\n\n\n@pytest.mark.parametrize(\"amount, fee\", [(99, 5), (100, 0)])\ndef test_handling_fee_boundary(amount, fee):\n    assert handling_fee(amount) == fee\n";

fn classify_fee(files: &[(&str, &str)]) -> Result<Finding, String> {
    let (owners, tests) = workspace(files);
    classify_change(
        Path::new("shop/__init__.py"),
        3,
        "    return 0 if amount > 99 else 5",
        &owners,
        &tests,
    )
    .ok_or_else(|| "the changed fee line must classify".to_string())
}

fn with_files(conftest: &str, test: &str) -> Result<Finding, String> {
    classify_fee(&[
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
fn the_decorated_parametrize_control_is_exposed() -> Result<(), String> {
    let finding = classify_fee(&[
        ("shop/__init__.py", SHOP),
        ("tests/test_shop.py", DECORATED),
    ])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn literal_params_unpacked_by_the_test_read_like_the_decorated_control() -> Result<(), String> {
    let finding = with_files(CONFTEST, TEST)?;
    assert_eq!(finding.static_limit_kind, None);
    assert_eq!(finding.class, ExposureClass::Exposed);
    // Tuple params, no ids, a fixture docstring and a test docstring keep it.
    let conftest = CONFTEST
        .replace(
            "params=[(99, 5), (100, 0)], ids=[\"below\", \"at\"]",
            "params=((99, 5), (100, 0))",
        )
        .replace(
            "    return request.param",
            "    \"\"\"One boundary case.\"\"\"\n    return request.param",
        );
    let test = TEST.replace(
        "    amount, fee = fee_case\n",
        "    \"\"\"Both sides of the boundary.\"\"\"\n    amount, fee = fee_case\n",
    );
    assert_eq!(with_files(&conftest, &test)?.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_same_module_params_fixture_is_read_too() -> Result<(), String> {
    let test = format!(
        "{}\n\n{}",
        CONFTEST,
        TEST.replace("from shop", "\nfrom shop")
    );
    let finding = classify_fee(&[("shop/__init__.py", SHOP), ("tests/test_shop.py", &test)])?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn rows_off_the_boundary_are_weakly_exposed_like_the_decorated_control() -> Result<(), String> {
    // The rows, not the lifted limit, decide the verdict: 50 and 200 never
    // tell `>= 100` from `> 99`'s neighbours, exactly as the decorator reads.
    let off = |text: &str| text.replace("(99, 5), (100, 0)", "(50, 5), (200, 0)");
    let decorated = classify_fee(&[
        ("shop/__init__.py", SHOP),
        ("tests/test_shop.py", &off(DECORATED)),
    ])?;
    let fixture = with_files(&off(CONFTEST), TEST)?;
    assert_eq!(decorated.class, ExposureClass::WeaklyExposed);
    assert_eq!(fixture.class, decorated.class);
    assert_eq!(fixture.static_limit_kind, None);
    Ok(())
}

#[test]
fn a_fixture_shape_that_is_not_literal_rows_keeps_the_limit() -> Result<(), String> {
    let rows = "params=[(99, 5), (100, 0)], ids=[\"below\", \"at\"]";
    for (case, from, to) in [
        ("named params", rows, "params=CASES"),
        ("list rows", rows, "params=[[99, 5], [100, 0]]"),
        ("named element", rows, "params=[(LIMIT, 5), (100, 0)]"),
        (
            "pytest.param row",
            rows,
            "params=[pytest.param((99, 5)), (100, 0)]",
        ),
        ("starred row", rows, "params=[(*LOW,), (100, 0)]"),
        ("empty params", rows, "params=[]"),
        ("callable ids", rows, "params=[(99, 5), (100, 0)], ids=str"),
        (
            "scope",
            rows,
            "params=[(99, 5), (100, 0)], scope=\"module\"",
        ),
        ("autouse", rows, "params=[(99, 5), (100, 0)], autouse=True"),
        (
            "positional scope",
            rows,
            "\"function\", params=[(99, 5), (100, 0)]",
        ),
        (
            "transformed param",
            "    return request.param",
            "    return (request.param[0] + 1, request.param[1])",
        ),
        (
            "extra statement",
            "    return request.param",
            "    request.addfinalizer(print)\n    return request.param",
        ),
        ("other parameter", "(request)", "(request, tmp_path)"),
        (
            "renamed request",
            "(request):\n    return request.param",
            "(req):\n    return req.param",
        ),
        ("not the param", "request.param", "request.params"),
    ] {
        let conftest = CONFTEST.replace(from, to);
        assert_ne!(conftest, CONFTEST, "{case}: fixture must change");
        assert_fixture_limited(&with_files(&conftest, TEST)?, case);
    }
    let twice = format!(
        "{CONFTEST}\n\n@pytest.fixture(params=[(1, 5)])\ndef fee_case(request):\n    return request.param\n"
    );
    assert_fixture_limited(&with_files(&twice, TEST)?, "redefined fixture");
    let stacked = CONFTEST.replace("@pytest.fixture(", "@other\n@pytest.fixture(");
    assert_fixture_limited(&with_files(&stacked, TEST)?, "second decorator");
    Ok(())
}

#[test]
fn a_test_that_does_not_unpack_the_rows_cleanly_keeps_the_limit() -> Result<(), String> {
    let unpack = "    amount, fee = fee_case\n";
    for (case, from, to) in [
        ("arity", unpack, "    amount, fee, extra = fee_case\n"),
        ("starred target", unpack, "    amount, *fee = fee_case\n"),
        ("nested target", unpack, "    (amount, fee), = fee_case\n"),
        (
            "attribute target",
            unpack,
            "    amount, self.fee = fee_case\n",
        ),
        (
            "not first",
            unpack,
            "    seen = 1\n    amount, fee = fee_case\n",
        ),
        ("chained", unpack, "    amount, fee = row = fee_case\n"),
        (
            "subscripted",
            unpack,
            "    amount, fee = fee_case[0], fee_case[1]\n",
        ),
        (
            "rebinds a name",
            unpack,
            "    amount, fee = fee_case\n    amount = 150\n",
        ),
        (
            "rebinds in a loop",
            unpack,
            "    amount, fee = fee_case\n    for amount in (150,):\n        pass\n",
        ),
        (
            "reads the fixture again",
            unpack,
            "    amount, fee = fee_case\n    assert fee_case\n",
        ),
        (
            "nested scope",
            unpack,
            "    amount, fee = fee_case\n    def helper():\n        return amount\n",
        ),
        ("duplicate name", unpack, "    amount, amount = fee_case\n"),
        ("defaulted parameter", "(fee_case):", "(fee_case=(99, 5)):"),
        (
            "name shadows a parameter",
            "(fee_case):\n    amount, fee = fee_case",
            "(fee_case, amount):\n    amount, fee = fee_case",
        ),
        (
            "foreign decorator",
            "\n\ndef test_",
            "\n\n@retry\ndef test_",
        ),
    ] {
        let test = TEST.replace(from, to);
        assert_ne!(test, TEST, "{case}: test must change");
        let module = format!("import pytest\n{test}");
        // The test stays as extracted: no cases, the unpack still in its body.
        let (_, tests) = workspace(&[
            ("shop/__init__.py", SHOP),
            ("tests/conftest.py", CONFTEST),
            ("tests/test_shop.py", &module),
        ]);
        assert!(
            tests.iter().all(|test| test.parametrize.is_none()
                && test.body_text.contains("= fee_case")
                || test.body_text.contains("fee_case=")),
            "{case}: rewritten"
        );
        let finding = with_files(CONFTEST, &module)?;
        assert_ne!(finding.class, ExposureClass::Exposed, "{case}");
        assert!(
            finding.static_limit_kind.is_some(),
            "{case}: {:?}",
            finding.class
        );
    }
    Ok(())
}

#[test]
fn anything_else_that_can_reach_the_fixture_keeps_the_limit() -> Result<(), String> {
    let other_fixture = format!(
        "{CONFTEST}\n\n@pytest.fixture(autouse=True)\ndef shift(fee_case):\n    return fee_case\n"
    );
    let walk = format!(
        "{CONFTEST}\n\n@pytest.fixture(autouse=True)\ndef walk(request):\n    return request.getfixturevalue('x')\n"
    );
    let hook = format!(
        "{CONFTEST}\n\ndef pytest_generate_tests(metafunc):\n    metafunc.parametrize('fee_case', [(1, 5)], indirect=True)\n"
    );
    let star = format!("from helpers import *\n{CONFTEST}");
    for (case, conftest) in [
        ("dependent fixture", other_fixture.as_str()),
        ("request walk", walk.as_str()),
        ("generate hook", hook.as_str()),
        ("star import", star.as_str()),
    ] {
        assert_fixture_limited(&with_files(conftest, TEST)?, case);
    }
    // The test module binds the name over the conftest fixture.
    let overridden = format!("from helpers import fee_case\n{TEST}");
    assert_fixture_limited(&with_files(CONFTEST, &overridden)?, "module import");
    // A nearer conftest rebinds it.
    let nearer = classify_fee(&[
        ("shop/__init__.py", SHOP),
        ("conftest.py", CONFTEST),
        ("tests/conftest.py", "fee_case = (1, 5)\n"),
        ("tests/test_shop.py", TEST),
    ])?;
    assert_fixture_limited(&nearer, "nearer binding");
    // Control: the same fixture one conftest up, nothing in between.
    let upper = classify_fee(&[
        ("shop/__init__.py", SHOP),
        ("conftest.py", CONFTEST),
        ("tests/test_shop.py", TEST),
    ])?;
    assert_eq!(upper.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_single_name_target_takes_each_scalar_row() -> Result<(), String> {
    let conftest = CONFTEST.replace(
        "params=[(99, 5), (100, 0)], ids=[\"below\", \"at\"]",
        "params=[100, 150]",
    );
    let test = TEST
        .replace("    amount, fee = fee_case\n", "    amount = fee_case\n")
        .replace("== fee", "== 0");
    let decorated = "import pytest\n\nfrom shop import handling_fee\n\n\n@pytest.mark.parametrize(\"amount\", [100, 150])\ndef test_handling_fee_boundary(amount):\n    assert handling_fee(amount) == 0\n";
    let control = classify_fee(&[
        ("shop/__init__.py", SHOP),
        ("tests/test_shop.py", decorated),
    ])?;
    let finding = with_files(&conftest, &test)?;
    assert_eq!(finding.static_limit_kind, None);
    assert_eq!(finding.class, control.class);
    Ok(())
}

#[test]
fn a_test_with_its_own_parametrize_mark_keeps_only_the_marks_cases() {
    // pytest would combine both; the read stays with the decorator's cases
    // and leaves the unpack in the body.
    let test = TEST.replace(
        "\n\ndef test_",
        "\n\n@pytest.mark.parametrize(\"unit\", [1])\ndef test_",
    );
    let (_, tests) = workspace(&[
        ("shop/__init__.py", SHOP),
        ("tests/conftest.py", CONFTEST),
        ("tests/test_shop.py", &format!("import pytest\n{test}")),
    ]);
    let found = tests
        .iter()
        .find(|test| test.name == "test_handling_fee_boundary")
        .and_then(|test| test.parametrize.as_ref())
        .map(|cases| cases.argnames().into_iter().collect::<Vec<_>>());
    assert_eq!(found, Some(vec!["unit".to_string()]));
    assert!(
        tests
            .iter()
            .all(|test| test.body_text.contains("amount, fee = fee_case"))
    );
}

#[test]
fn a_parametrize_mark_does_not_resolve_the_tests_other_fixtures() -> Result<(), String> {
    // Off-boundary rows behind a fixture the mark leaves unread: the mark
    // binds only `unit`, so `fee_case` stays an unresolved fixture rather
    // than reading as an exposed discriminator.
    let conftest = CONFTEST.replace("(99, 5), (100, 0)", "(50, 5), (200, 0)");
    let test = TEST.replace(
        "\n\ndef test_",
        "\n\n@pytest.mark.parametrize(\"unit\", [1])\ndef test_",
    );
    let finding = with_files(&conftest, &format!("import pytest\n{test}"))?;
    assert_fixture_limited(&finding, "parametrized test with a fixture");
    // Control: the mark's own argnames are values, not fixtures.
    let decorated = classify_fee(&[
        ("shop/__init__.py", SHOP),
        ("tests/test_shop.py", DECORATED),
    ])?;
    assert_eq!(decorated.static_limit_kind, None);
    Ok(())
}
