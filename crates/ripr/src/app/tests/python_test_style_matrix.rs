//! Python test-style matrix (#13869): one edit per common test style, each
//! read through the real `check` pipeline against a weak and a strong test.
//!
//! A weak test reaches the changed owner but cannot notice the edit; it must
//! never read `exposed`. A strong test pins the changed value; where the style
//! is supported it must read `exposed`. Each strong test appears twice: as
//! written before the diff (it pins the old value) and as updated with the diff
//! (it pins the new value). Styles whose strong reading is still missing are
//! listed in `WEAK_ONLY` so the no-false-exposed half is pinned for them too;
//! their missing credit is tracked on #13869.

use crate::domain::{ExposureClass, Finding};
use std::fs;
use std::path::Path;

const CALC: &str =
    "def discount(total):\n    if total >= 100:\n        return total - 10\n    return total\n";
const CALC_EDIT: Edit = Edit {
    owner: "src/calc.py",
    old: "        return total - 10",
    new: "        return total - 5",
};

const CART: &str = "class Cart:\n    def __init__(self, total):\n        self.total = total\n\n    def discounted(self):\n        if self.total >= 100:\n            return self.total - 10\n        return self.total\n";
const CART_EDIT: Edit = Edit {
    owner: "src/cart.py",
    old: "            return self.total - 10",
    new: "            return self.total - 5",
};

const RATE: &str = "def scaled(total):\n    return total * 0.9\n";
const RATE_EDIT: Edit = Edit {
    owner: "src/rate.py",
    old: "    return total * 0.9",
    new: "    return total * 0.85",
};

const RAISE: &str = "def checked(total):\n    if total < 0:\n        raise ValueError(\"negative total\")\n    return total\n";
const RAISE_EDIT: Edit = Edit {
    owner: "src/checked.py",
    old: "        raise ValueError(\"negative total\")",
    new: "        raise TypeError(\"negative total\")",
};

const WARN: &str = "import warnings\n\n\ndef legacy(total):\n    warnings.warn(\"legacy is deprecated\", DeprecationWarning)\n    return total\n";
const WARN_EDIT: Edit = Edit {
    owner: "src/legacy.py",
    old: "    warnings.warn(\"legacy is deprecated\", DeprecationWarning)",
    new: "    warnings.warn(\"legacy is deprecated\", UserWarning)",
};

const PRICE_APP: &str = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
const PRICE_EDIT: Edit = Edit {
    owner: "src/app.py",
    old: "    return {\"total\": 200 - 10, \"count\": 3}",
    new: "    return {\"total\": 200 - 5, \"count\": 3}",
};
const CLIENT_IMPORTS: &str =
    "from fastapi.testclient import TestClient\n\nfrom src.app import app\n\n\n";

const CONFTEST_CART: &str = "import pytest\n\nfrom src.cart import Cart\n\n\n@pytest.fixture\ndef cart():\n    return Cart(200)\n";

#[derive(Clone, Copy)]
struct Edit {
    owner: &'static str,
    old: &'static str,
    new: &'static str,
}

struct Style {
    name: &'static str,
    sources: &'static [(&'static str, &'static str)],
    edit: Edit,
    weak: &'static str,
    /// Pins the value the owner returned before the diff.
    strong_before: &'static str,
    /// The same test updated with the diff: it pins the new value.
    strong_after: &'static str,
}

const STYLES: &[Style] = &[
    Style {
        name: "pytest function",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        weak: "from src.calc import discount\n\n\ndef test_discount():\n    assert discount(200) < 200\n",
        strong_before: "from src.calc import discount\n\n\ndef test_discount():\n    assert discount(200) == 190\n",
        strong_after: "from src.calc import discount\n\n\ndef test_discount():\n    assert discount(200) == 195\n",
    },
    Style {
        name: "pytest class",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        weak: "from src.calc import discount\n\n\nclass TestDiscount:\n    def test_big(self):\n        assert discount(200) < 200\n",
        strong_before: "from src.calc import discount\n\n\nclass TestDiscount:\n    def test_big(self):\n        assert discount(200) == 190\n",
        strong_after: "from src.calc import discount\n\n\nclass TestDiscount:\n    def test_big(self):\n        assert discount(200) == 195\n",
    },
    Style {
        name: "pytest parametrize",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        weak: "import pytest\n\nfrom src.calc import discount\n\n\n@pytest.mark.parametrize(\"total\", [150, 200])\ndef test_discount(total):\n    assert discount(total) < total\n",
        strong_before: "import pytest\n\nfrom src.calc import discount\n\n\n@pytest.mark.parametrize(\"total, expected\", [(150, 140), (200, 190)])\ndef test_discount(total, expected):\n    assert discount(total) == expected\n",
        strong_after: "import pytest\n\nfrom src.calc import discount\n\n\n@pytest.mark.parametrize(\"total, expected\", [(150, 145), (200, 195)])\ndef test_discount(total, expected):\n    assert discount(total) == expected\n",
    },
    Style {
        name: "pytest fixture building the owner object",
        sources: &[("src/cart.py", CART)],
        edit: CART_EDIT,
        weak: "import pytest\n\nfrom src.cart import Cart\n\n\n@pytest.fixture\ndef cart():\n    return Cart(200)\n\n\ndef test_discounted(cart):\n    assert cart.discounted() < 200\n",
        strong_before: "import pytest\n\nfrom src.cart import Cart\n\n\n@pytest.fixture\ndef cart():\n    return Cart(200)\n\n\ndef test_discounted(cart):\n    assert cart.discounted() == 190\n",
        strong_after: "import pytest\n\nfrom src.cart import Cart\n\n\n@pytest.fixture\ndef cart():\n    return Cart(200)\n\n\ndef test_discounted(cart):\n    assert cart.discounted() == 195\n",
    },
    Style {
        name: "conftest fixture",
        sources: &[("src/cart.py", CART), ("tests/conftest.py", CONFTEST_CART)],
        edit: CART_EDIT,
        weak: "def test_discounted(cart):\n    assert cart.discounted() < 200\n",
        strong_before: "def test_discounted(cart):\n    assert cart.discounted() == 190\n",
        strong_after: "def test_discounted(cart):\n    assert cart.discounted() == 195\n",
    },
    Style {
        name: "unittest assertEqual",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        weak: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_big(self):\n        self.assertLess(discount(200), 200)\n",
        strong_before: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_big(self):\n        self.assertEqual(discount(200), 190)\n",
        strong_after: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_big(self):\n        self.assertEqual(discount(200), 195)\n",
    },
    Style {
        name: "unittest assertTrue against expected-first assertEqual",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        weak: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_big(self):\n        self.assertTrue(discount(200))\n",
        strong_before: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_big(self):\n        self.assertEqual(190, discount(200))\n",
        strong_after: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_big(self):\n        self.assertEqual(195, discount(200))\n",
    },
    Style {
        name: "unittest setUp receiver",
        sources: &[("src/cart.py", CART)],
        edit: CART_EDIT,
        weak: "import unittest\n\nfrom src.cart import Cart\n\n\nclass CartTests(unittest.TestCase):\n    def setUp(self):\n        self.cart = Cart(200)\n\n    def test_discounted(self):\n        self.assertLess(self.cart.discounted(), 200)\n",
        strong_before: "import unittest\n\nfrom src.cart import Cart\n\n\nclass CartTests(unittest.TestCase):\n    def setUp(self):\n        self.cart = Cart(200)\n\n    def test_discounted(self):\n        self.assertEqual(self.cart.discounted(), 190)\n",
        strong_after: "import unittest\n\nfrom src.cart import Cart\n\n\nclass CartTests(unittest.TestCase):\n    def setUp(self):\n        self.cart = Cart(200)\n\n    def test_discounted(self):\n        self.assertEqual(self.cart.discounted(), 195)\n",
    },
    Style {
        name: "unittest setUpClass receiver",
        sources: &[("src/cart.py", CART)],
        edit: CART_EDIT,
        weak: "import unittest\n\nfrom src.cart import Cart\n\n\nclass CartTests(unittest.TestCase):\n    @classmethod\n    def setUpClass(cls):\n        cls.cart = Cart(200)\n\n    def test_discounted(self):\n        self.assertLess(self.cart.discounted(), 200)\n",
        strong_before: "import unittest\n\nfrom src.cart import Cart\n\n\nclass CartTests(unittest.TestCase):\n    @classmethod\n    def setUpClass(cls):\n        cls.cart = Cart(200)\n\n    def test_discounted(self):\n        self.assertEqual(self.cart.discounted(), 190)\n",
        strong_after: "import unittest\n\nfrom src.cart import Cart\n\n\nclass CartTests(unittest.TestCase):\n    @classmethod\n    def setUpClass(cls):\n        cls.cart = Cart(200)\n\n    def test_discounted(self):\n        self.assertEqual(self.cart.discounted(), 195)\n",
    },
    Style {
        name: "pytest setup_method receiver",
        sources: &[("src/cart.py", CART)],
        edit: CART_EDIT,
        weak: "from src.cart import Cart\n\n\nclass TestCart:\n    def setup_method(self):\n        self.cart = Cart(200)\n\n    def test_discounted(self):\n        assert self.cart.discounted() < 200\n",
        strong_before: "from src.cart import Cart\n\n\nclass TestCart:\n    def setup_method(self):\n        self.cart = Cart(200)\n\n    def test_discounted(self):\n        assert self.cart.discounted() == 190\n",
        strong_after: "from src.cart import Cart\n\n\nclass TestCart:\n    def setup_method(self):\n        self.cart = Cart(200)\n\n    def test_discounted(self):\n        assert self.cart.discounted() == 195\n",
    },
    Style {
        name: "unittest subTest rows",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        weak: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_rows(self):\n        for total in (150, 200):\n            with self.subTest(total=total):\n                self.assertLess(discount(total), total)\n",
        strong_before: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_rows(self):\n        for total, expected in ((150, 140), (200, 190)):\n            with self.subTest(total=total):\n                self.assertEqual(discount(total), expected)\n",
        strong_after: "import unittest\n\nfrom src.calc import discount\n\n\nclass DiscountTests(unittest.TestCase):\n    def test_rows(self):\n        for total, expected in ((150, 145), (200, 195)):\n            with self.subTest(total=total):\n                self.assertEqual(discount(total), expected)\n",
    },
    Style {
        name: "pytest.approx tolerance",
        sources: &[("src/rate.py", RATE)],
        edit: RATE_EDIT,
        weak: "import pytest\n\nfrom src.rate import scaled\n\n\ndef test_scaled():\n    assert scaled(200) == pytest.approx(180, abs=50)\n",
        strong_before: "import pytest\n\nfrom src.rate import scaled\n\n\ndef test_scaled():\n    assert scaled(200) == pytest.approx(180)\n",
        strong_after: "import pytest\n\nfrom src.rate import scaled\n\n\ndef test_scaled():\n    assert scaled(200) == pytest.approx(170)\n",
    },
    Style {
        name: "unittest assertAlmostEqual tolerance",
        sources: &[("src/rate.py", RATE)],
        edit: RATE_EDIT,
        weak: "import unittest\n\nfrom src.rate import scaled\n\n\nclass ScaledTests(unittest.TestCase):\n    def test_scaled(self):\n        self.assertAlmostEqual(scaled(200), 180, delta=50)\n",
        strong_before: "import unittest\n\nfrom src.rate import scaled\n\n\nclass ScaledTests(unittest.TestCase):\n    def test_scaled(self):\n        self.assertAlmostEqual(scaled(200), 180)\n",
        strong_after: "import unittest\n\nfrom src.rate import scaled\n\n\nclass ScaledTests(unittest.TestCase):\n    def test_scaled(self):\n        self.assertAlmostEqual(scaled(200), 170)\n",
    },
    Style {
        name: "pytest.raises with an exact builtin class",
        sources: &[("src/checked.py", RAISE)],
        edit: RAISE_EDIT,
        weak: "import pytest\n\nfrom src.checked import checked\n\n\ndef test_negative():\n    with pytest.raises(Exception):\n        checked(-1)\n",
        strong_before: "import pytest\n\nfrom src.checked import checked\n\n\ndef test_negative():\n    with pytest.raises(ValueError):\n        checked(-1)\n",
        strong_after: "import pytest\n\nfrom src.checked import checked\n\n\ndef test_negative():\n    with pytest.raises(TypeError):\n        checked(-1)\n",
    },
    Style {
        name: "FastAPI TestClient response JSON",
        sources: &[("src/app.py", PRICE_APP)],
        edit: PRICE_EDIT,
        weak: "from fastapi.testclient import TestClient\n\nfrom src.app import app\n\n\ndef test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.status_code == 200\n",
        strong_before: "from fastapi.testclient import TestClient\n\nfrom src.app import app\n\n\ndef test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"total\"] == 190\n",
        strong_after: "from fastapi.testclient import TestClient\n\nfrom src.app import app\n\n\ndef test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"total\"] == 195\n",
    },
    Style {
        name: "unittest assertEqual on a TestClient response",
        sources: &[("src/app.py", PRICE_APP)],
        edit: PRICE_EDIT,
        weak: "import unittest\n\nfrom fastapi.testclient import TestClient\n\nfrom src.app import app\n\n\nclass PriceTests(unittest.TestCase):\n    def test_price(self):\n        client = TestClient(app)\n        response = client.get(\"/price\")\n        self.assertEqual(response.status_code, 200)\n",
        strong_before: "import unittest\n\nfrom fastapi.testclient import TestClient\n\nfrom src.app import app\n\n\nclass PriceTests(unittest.TestCase):\n    def test_price(self):\n        client = TestClient(app)\n        response = client.get(\"/price\")\n        self.assertEqual(response.json()[\"total\"], 190)\n",
        strong_after: "import unittest\n\nfrom fastapi.testclient import TestClient\n\nfrom src.app import app\n\n\nclass PriceTests(unittest.TestCase):\n    def test_price(self):\n        client = TestClient(app)\n        response = client.get(\"/price\")\n        self.assertEqual(response.json()[\"total\"], 195)\n",
    },
];

struct WeakOnly {
    name: &'static str,
    sources: &'static [(&'static str, &'static str)],
    edit: Edit,
    test: &'static str,
}

/// Styles whose strong test does not read `exposed` yet (#13869). Their weak
/// test must still never read `exposed`.
const WEAK_ONLY: &[WeakOnly] = &[
    WeakOnly {
        name: "pytest fixture supplying a literal input",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        test: "import pytest\n\nfrom src.calc import discount\n\n\n@pytest.fixture\ndef big_total():\n    return 200\n\n\ndef test_discount(big_total):\n    assert discount(big_total) < big_total\n",
    },
    WeakOnly {
        name: "unittest assertRaises with a base class",
        sources: &[("src/checked.py", RAISE)],
        edit: RAISE_EDIT,
        test: "import unittest\n\nfrom src.checked import checked\n\n\nclass CheckedTests(unittest.TestCase):\n    def test_negative(self):\n        with self.assertRaises(Exception):\n            checked(-1)\n",
    },
    WeakOnly {
        name: "pytest.warns with the Warning base class",
        sources: &[("src/legacy.py", WARN)],
        edit: WARN_EDIT,
        test: "import pytest\n\nfrom src.legacy import legacy\n\n\ndef test_legacy_warns():\n    with pytest.warns(Warning):\n        legacy(1)\n",
    },
    WeakOnly {
        name: "hypothesis property with a loose bound",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        test: "from hypothesis import given, strategies as st\n\nfrom src.calc import discount\n\n\n@given(st.integers(min_value=100, max_value=1000))\ndef test_discount(total):\n    assert discount(total) < total\n",
    },
];

#[test]
fn python_test_styles_without_strong_credit_keep_weak_tests_out_of_exposed() -> Result<(), String> {
    // The supported styles' weak rows are held to `weakly_exposed` below.
    let mut failures = Vec::new();
    for row in WEAK_ONLY {
        let finding = check_style(row.name, row.sources, row.edit, row.test)?;
        // The row must reach its owner, or a lost test link (`no_static_path`)
        // would pass without exercising the test at all.
        if finding.class == ExposureClass::Exposed || finding.related_tests.is_empty() {
            failures.push(format!(
                "{}: weak test read {:?} with {} related test(s)",
                row.name,
                finding.class,
                finding.related_tests.len()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

#[test]
fn every_supported_python_test_style_reads_its_weak_test_as_weakly_exposed() -> Result<(), String> {
    let mut failures = Vec::new();
    for style in STYLES {
        let finding = check_style(style.name, style.sources, style.edit, style.weak)?;
        if finding.class != ExposureClass::WeaklyExposed || finding.related_tests.is_empty() {
            failures.push(format!(
                "{}: weak test read {:?} with {} related test(s)",
                style.name,
                finding.class,
                finding.related_tests.len()
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

#[test]
fn every_supported_python_test_style_credits_its_strong_test() -> Result<(), String> {
    let mut failures = Vec::new();
    for style in STYLES {
        for (when, test) in [
            ("before the diff", style.strong_before),
            ("updated with the diff", style.strong_after),
        ] {
            let finding = check_style(style.name, style.sources, style.edit, test)?;
            if finding.class != ExposureClass::Exposed {
                failures.push(format!(
                    "{} ({when}): strong test read {:?}",
                    style.name, finding.class
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

#[test]
fn a_doctest_no_runner_collects_is_not_credited() -> Result<(), String> {
    // Without `--doctest-modules` (or a test that runs it) nothing executes the
    // example, so it must not stand in for a test.
    let documented = "def discount(total):\n    \"\"\"Discount.\n\n    >>> discount(200)\n    190\n    \"\"\"\n    if total >= 100:\n        return total - 10\n    return total\n";
    let finding = check_style(
        "uncollected doctest",
        &[("src/calc.py", documented)],
        CALC_EDIT,
        "",
    )?;
    assert_eq!(finding.class, ExposureClass::NoStaticPath, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");
    Ok(())
}

#[test]
fn a_collected_doctest_the_evaluator_cannot_run_abstains() -> Result<(), String> {
    // Under `--doctest-modules` the example runs and pins 190, but the owner
    // has an `if`, which the doctest evaluator does not run. Reading nothing
    // would claim no test path; the honest reading is an abstention.
    let documented = "def discount(total):\n    \"\"\"Discount.\n\n    >>> discount(200)\n    190\n    \"\"\"\n    if total >= 100:\n        return total - 10\n    return total\n";
    let finding = check_style(
        "collected doctest",
        &[
            ("src/calc.py", documented),
            (
                "pyproject.toml",
                "[project]\nname = \"fx\"\nversion = \"0.1.0\"\n\n[tool.pytest.ini_options]\naddopts = \"--doctest-modules\"\n",
            ),
        ],
        CALC_EDIT,
        "",
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    assert!(!finding.related_tests.is_empty(), "{finding:?}");
    Ok(())
}

#[test]
fn a_route_response_read_credits_only_the_changed_key_of_the_owners_route() -> Result<(), String> {
    // Each test calls the edited handler but does not pin the changed
    // `total` value of the owner's own route through one response, so none
    // may read `exposed` (#13869).
    let tests = [
        (
            "another key",
            "def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"count\"] == 3\n",
        ),
        (
            "an arithmetic side",
            "def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    other = {\"x\": 0}\n    assert response.json()[\"total\"] + other[\"x\"] == 190\n",
        ),
        (
            "a rebound response",
            "def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    response = client.get(\"/other\")\n    assert response.json()[\"total\"] == 190\n",
        ),
        (
            "another route",
            "def test_price():\n    client = TestClient(app)\n    response = client.get(\"/other\")\n    assert response.json()[\"total\"] == 190\n",
        ),
        (
            "a method the route does not accept",
            "def test_price():\n    client = TestClient(app)\n    response = client.post(\"/price\")\n    assert response.json()[\"total\"] == 190\n",
        ),
        (
            "an inequality",
            "def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"total\"] != 0\n",
        ),
        (
            "an unconditional disjunction",
            "def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"total\"] == 190 or True\n",
        ),
    ];
    let mut failures = Vec::new();
    for (name, body) in tests {
        let test = format!("{CLIENT_IMPORTS}{body}");
        let finding = check_style(name, &[("src/app.py", PRICE_APP)], PRICE_EDIT, &test)?;
        if finding.class == ExposureClass::Exposed {
            failures.push(format!("{name}: read exposed"));
        }
    }
    // A docstring that spells the binding is not the binding.
    let docstring = format!(
        "{CLIENT_IMPORTS}def test_price():\n    \"\"\"\n    response = client.get(\"/price\")\n    \"\"\"\n    client = TestClient(app)\n    response = client.get(\"/other\")\n    assert response.json()[\"total\"] == 190\n"
    );
    let finding = check_style(
        "docstring binding",
        &[("src/app.py", PRICE_APP)],
        PRICE_EDIT,
        &docstring,
    )?;
    if finding.class == ExposureClass::Exposed {
        failures.push("docstring binding: read exposed".to_string());
    }
    // A response model can drop or reshape the returned dict, and another
    // `return` may be the one the request takes, so the body is not known.
    let strong = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"total\"] == 190\n"
    );
    let nested = Edit {
        owner: "src/app.py",
        old: "        return {\"total\": 200 - 10, \"count\": 3}",
        new: "        return {\"total\": 200 - 5, \"count\": 3}",
    };
    for (name, app, edit) in [
        (
            "response model",
            "from fastapi import FastAPI\nfrom pydantic import BaseModel\n\napp = FastAPI()\n\n\nclass Price(BaseModel):\n    total: int\n\n\n@app.get(\"/price\", response_model=Price)\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n",
            PRICE_EDIT,
        ),
        (
            "two returns",
            "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price(vip: bool = False):\n    if vip:\n        return {\"total\": 200 - 10, \"count\": 3}\n    return {\"total\": 190, \"count\": 3}\n",
            nested,
        ),
    ] {
        let finding = check_style(name, &[("src/app.py", app)], edit, &strong)?;
        if finding.class == ExposureClass::Exposed {
            failures.push(format!("{name}: read exposed"));
        }
    }
    // A whole-body comparison can match a body the changed `return` never
    // produced (here a validation error for a missing parameter).
    let error_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price(qty: int):\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let whole = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json() == {{\"detail\": \"missing\"}}\n"
    );
    let finding = check_style(
        "whole error body",
        &[("src/app.py", error_app)],
        PRICE_EDIT,
        &whole,
    )?;
    if finding.class == ExposureClass::Exposed {
        failures.push("whole error body: read exposed".to_string());
    }
    // A return annotation is an implicit response model, and a decorator
    // under the route may rewrite what the handler returns.
    for (name, app) in [
        (
            "return annotation",
            "from fastapi import FastAPI\nfrom pydantic import BaseModel\n\napp = FastAPI()\n\n\nclass Price(BaseModel):\n    total: int\n\n\n@app.get(\"/price\")\ndef price() -> Price:\n    return {\"total\": 200 - 10, \"count\": 3}\n",
        ),
        (
            "wrapping decorator",
            "from fastapi import FastAPI\n\nfrom src.wrap import redact\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\n@redact\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n",
        ),
    ] {
        let finding = check_style(name, &[("src/app.py", app)], PRICE_EDIT, &strong)?;
        if finding.class == ExposureClass::Exposed {
            failures.push(format!("{name}: read exposed"));
        }
    }
    // A handler that raises can answer with an error body instead of the
    // returned dict, and `detail` is the key of FastAPI's error bodies, so a
    // keyed read of either may observe a response the changed `return` never
    // produced.
    let raising = "from fastapi import FastAPI, HTTPException\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price(vip: bool = True):\n    if not vip:\n        raise HTTPException(404, detail=\"nope\")\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let finding = check_style(
        "raising handler",
        &[("src/app.py", raising)],
        PRICE_EDIT,
        &strong,
    )?;
    if finding.class == ExposureClass::Exposed {
        failures.push("raising handler: read exposed".to_string());
    }
    let detail_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price(qty: int):\n    return {\"detail\": 200 - 10, \"count\": 3}\n";
    let detail_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"detail\": 200 - 10, \"count\": 3}",
        new: "    return {\"detail\": 200 - 5, \"count\": 3}",
    };
    let detail_read = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"detail\"] == 190\n"
    );
    let finding = check_style(
        "detail key",
        &[("src/app.py", detail_app)],
        detail_edit,
        &detail_read,
    )?;
    if finding.class == ExposureClass::Exposed {
        failures.push("detail key: read exposed".to_string());
    }
    // Reads that do not pin the changed entry of the body the request gets:
    // an unchanged sibling field, a changed key only on the expected side, an
    // entry a later `**` unpack or repeated key overrides, a response a
    // `match` capture or walrus rebinds, and a request another route of the
    // workspace also accepts (dispatch order is not read).
    let read = |assertion: &str| {
        format!(
            "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    {assertion}\n"
        )
    };
    let sibling_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"meta\": {\"count\": 3}}\n";
    let sibling_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"total\": 200 - 10, \"meta\": {\"count\": 3}}",
        new: "    return {\"total\": 200 - 5, \"meta\": {\"count\": 3}}",
    };
    let unpack_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, **{\"total\": 190}}\n";
    let unpack_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"total\": 200 - 10, **{\"total\": 190}}",
        new: "    return {\"total\": 200 - 5, **{\"total\": 190}}",
    };
    let repeated_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"total\": 190}\n";
    let repeated_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"total\": 200 - 10, \"total\": 190}",
        new: "    return {\"total\": 200 - 5, \"total\": 190}",
    };
    let shadow_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef cached_price():\n    return {\"total\": 190, \"count\": 3}\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let overlap_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/items/latest\")\ndef latest():\n    return {\"total\": 190}\n\n\n@app.get(\"/items/{item_id}\")\ndef item(item_id: str):\n    return {\"total\": 200 - 10}\n";
    let overlap_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"total\": 200 - 10}",
        new: "    return {\"total\": 200 - 5}",
    };
    let overlap_test = format!(
        "{CLIENT_IMPORTS}def test_latest():\n    client = TestClient(app)\n    response = client.get(\"/items/latest\")\n    assert response.json()[\"total\"] == 190\n"
    );
    let match_test = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    match client.get(\"/other\"):\n        case response:\n            assert response.json()[\"total\"] == 190\n"
    );
    let walrus_test = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    if (response := client.get(\"/other\")):\n        assert response.json()[\"total\"] == 190\n"
    );
    let handler_app = "from fastapi import FastAPI, Request\nfrom fastapi.exceptions import RequestValidationError\nfrom fastapi.responses import JSONResponse\n\napp = FastAPI()\n\n\n@app.exception_handler(RequestValidationError)\nasync def invalid(request: Request, exc):\n    return JSONResponse({\"total\": 190}, status_code=422)\n\n\n@app.get(\"/price\")\ndef price(qty: int):\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let middleware_app = "from fastapi import FastAPI\nfrom fastapi.responses import JSONResponse\n\napp = FastAPI()\n\n\n@app.middleware(\"http\")\nasync def gate(request, call_next):\n    return JSONResponse({\"total\": 190})\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let imperative_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\ndef cached():\n    return {\"total\": 190, \"count\": 3}\n\n\napp.add_api_route(\"/price\", cached)\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let tuple_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": (1, 2), \"count\": 3}\n";
    let tuple_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"total\": (1, 2), \"count\": 3}",
        new: "    return {\"total\": [1, 2], \"count\": 3}",
    };
    let float_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 5, \"count\": 3}\n";
    let float_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"total\": 5, \"count\": 3}",
        new: "    return {\"total\": 5.0, \"count\": 3}",
    };
    let catch_all_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/{rest:path}\")\ndef fallback(rest: str):\n    return {\"total\": 190, \"count\": 3}\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let dynamic_app = "from fastapi import FastAPI\n\napp = FastAPI()\nPRICE = \"/price\"\n\n\n@app.get(PRICE)\ndef cached():\n    return {\"total\": 190, \"count\": 3}\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let mixed_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": (1, 2), \"count\": 3}\n";
    let mixed_edit = Edit {
        owner: "src/app.py",
        old: "    return {\"total\": (1, 2), \"count\": 3}",
        new: "    return {\"total\": [1, 2], \"count\": 4}",
    };
    let other_receiver_app = "from fastapi import APIRouter, FastAPI\n\napp = FastAPI()\nitems_router = APIRouter()\n\n\n@items_router.get(\"/price\")\ndef cached():\n    return {\"total\": 190, \"count\": 3}\n\n\n@app.get(\"/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let prefix_app = "from fastapi import APIRouter, FastAPI\n\napp = FastAPI()\nrouter = APIRouter()\n\n\n@router.get(\"/{item}\")\ndef cached(item: str):\n    return {\"total\": 190, \"count\": 3}\n\n\napp.include_router(router, prefix=\"/items\")\n\n\n@app.get(\"/items/price\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let prefix_test = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/items/price\")\n    assert response.json()[\"total\"] == 190\n"
    );
    let partial_app = "from fastapi import FastAPI\n\napp = FastAPI()\n\n\n@app.get(\"/items/{name}.json\")\ndef named(name: str):\n    return {\"total\": 190, \"count\": 3}\n\n\n@app.get(\"/items/latest.json\")\ndef price():\n    return {\"total\": 200 - 10, \"count\": 3}\n";
    let partial_test = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    response = client.get(\"/items/latest.json\")\n    assert response.json()[\"total\"] == 190\n"
    );
    let two_responses = format!(
        "{CLIENT_IMPORTS}def test_price():\n    client = TestClient(app)\n    first = client.get(\"/price\")\n    second = client.get(\"/price\")\n    assert first.json()[\"total\"] == second.json()[\"total\"]\n"
    );
    let cases: Vec<(&str, &str, Edit, String)> = vec![
        (
            "two responses compared",
            PRICE_APP,
            PRICE_EDIT,
            two_responses,
        ),
        (
            "inline request on the expected side",
            PRICE_APP,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == client.get(\"/price\").json()[\"total\"]"),
        ),
        (
            "the same read in other quotes",
            PRICE_APP,
            PRICE_EDIT,
            read("assert response.json()['total'] == response.json()[\"total\"]"),
        ),
        (
            "exception handler",
            handler_app,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == 190"),
        ),
        (
            "middleware",
            middleware_app,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == 190"),
        ),
        (
            "imperative route",
            imperative_app,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == 190"),
        ),
        (
            "tuple to list",
            tuple_app,
            tuple_edit,
            read("assert response.json()[\"total\"] == [1, 2]"),
        ),
        (
            "int to float",
            float_app,
            float_edit,
            read("assert response.json()[\"total\"] == 5"),
        ),
        (
            "mixed tuple and value change",
            mixed_app,
            mixed_edit,
            read("assert response.json()[\"total\"] == [1, 2]"),
        ),
        (
            "router on another receiver",
            other_receiver_app,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == 190"),
        ),
        ("router prefix", prefix_app, PRICE_EDIT, prefix_test),
        (
            "partial-segment parameter",
            partial_app,
            PRICE_EDIT,
            partial_test,
        ),
        (
            "expected side bound to a request",
            PRICE_APP,
            PRICE_EDIT,
            read(
                "baseline = client.get(\"/price\").json()[\"total\"]\n    assert response.json()[\"total\"] == baseline",
            ),
        ),
        (
            "replaced json method",
            PRICE_APP,
            PRICE_EDIT,
            read(
                "response.json = lambda: {\"total\": 190}\n    assert response.json()[\"total\"] == 190",
            ),
        ),
        (
            "replaced json method with spaced assignment",
            PRICE_APP,
            PRICE_EDIT,
            read(
                "response.json  = lambda: {\"total\": 190}\n    assert response.json()[\"total\"] == 190",
            ),
        ),
        (
            "json method replaced through setattr",
            PRICE_APP,
            PRICE_EDIT,
            read(
                "setattr(response, \"json\", lambda: {\"total\": 190})\n    assert response.json()[\"total\"] == 190",
            ),
        ),
        (
            "catch-all route",
            catch_all_app,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == 190"),
        ),
        (
            "dynamic route path",
            dynamic_app,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == 190"),
        ),
        (
            "unchanged sibling",
            sibling_app,
            sibling_edit,
            read("assert response.json()[\"meta\"] == {\"count\": 3}"),
        ),
        (
            "changed key on the expected side",
            PRICE_APP,
            PRICE_EDIT,
            read(
                "expected = {\"total\": 3}\n    assert response.json()[\"count\"] == expected[\"total\"]",
            ),
        ),
        (
            "overriding unpack",
            unpack_app,
            unpack_edit,
            read("assert response.json()[\"total\"] == 190"),
        ),
        (
            "repeated key",
            repeated_app,
            repeated_edit,
            read("assert response.json()[\"total\"] == 190"),
        ),
        ("match capture", PRICE_APP, PRICE_EDIT, match_test),
        ("walrus rebinding", PRICE_APP, PRICE_EDIT, walrus_test),
        (
            "duplicate route",
            shadow_app,
            PRICE_EDIT,
            read("assert response.json()[\"total\"] == 190"),
        ),
        (
            "static route before a parameter route",
            overlap_app,
            overlap_edit,
            overlap_test,
        ),
    ];
    for (name, app, edit, test) in cases {
        let finding = check_style(name, &[("src/app.py", app)], edit, &test)?;
        if finding.class == ExposureClass::Exposed {
            failures.push(format!("{name}: read exposed"));
        }
    }
    // A test file that registers its own route on the app.
    let test_route = format!(
        "{CLIENT_IMPORTS}@app.get(\"/price\")\ndef cached():\n    return {{\"total\": 190, \"count\": 3}}\n\n\ndef test_price():\n    client = TestClient(app)\n    response = client.get(\"/price\")\n    assert response.json()[\"total\"] == 190\n"
    );
    let finding = check_style(
        "test-file route",
        &[("src/app.py", PRICE_APP)],
        PRICE_EDIT,
        &test_route,
    )?;
    if finding.class == ExposureClass::Exposed {
        failures.push("test-file route: read exposed".to_string());
    }
    // A client over another application does not reach the owner's handler.
    let rival = "from fastapi import FastAPI\nfrom fastapi.testclient import TestClient\n\nfrom src.app import app\n\nother_app = FastAPI()\n\n\ndef test_price():\n    client = TestClient(other_app)\n    response = client.get(\"/price\")\n    assert response.json()[\"total\"] == 190\n";
    let finding = check_style("rival app", &[("src/app.py", PRICE_APP)], PRICE_EDIT, rival)?;
    if finding.class == ExposureClass::Exposed {
        failures.push("rival app: read exposed".to_string());
    }
    assert!(failures.is_empty(), "{failures:#?}");
    Ok(())
}

#[test]
fn a_hypothesis_property_is_a_static_limit_not_a_verdict() -> Result<(), String> {
    // This property pins the formula, so it does notice the edit. Generated
    // inputs are outside what ripr reads statically (property-based limit);
    // the honest reading is an abstention, and it must never be a gap. If
    // ripr learns to credit it, move this row into `STYLES`.
    let finding = check_style(
        "hypothesis property pinning the formula",
        &[("src/calc.py", CALC)],
        CALC_EDIT,
        "from hypothesis import given, strategies as st\n\nfrom src.calc import discount\n\n\n@given(st.integers(min_value=100, max_value=1000))\ndef test_discount(total):\n    assert discount(total) == total - 10\n",
    )?;
    assert_eq!(finding.class, ExposureClass::StaticUnknown, "{finding:?}");
    Ok(())
}

/// Run `check` over a scratch workspace holding `sources` (as they stand after
/// the diff) and, when `test` is not empty, `tests/test_style.py`. Returns the
/// finding for the edited line.
fn check_style(
    name: &str,
    sources: &[(&str, &str)],
    edit: Edit,
    test: &str,
) -> Result<Finding, String> {
    let before = sources
        .iter()
        .find(|(path, _)| *path == edit.owner)
        .map(|(_, source)| *source)
        .ok_or_else(|| format!("{name}: no source for {}", edit.owner))?;
    if before.matches(edit.old).count() != 1 {
        return Err(format!(
            "{name}: edit must match one line of {}",
            edit.owner
        ));
    }
    let after = before.replacen(edit.old, edit.new, 1);
    let diff = single_line_diff(edit, before)?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| format!("system time: {error}"))?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-python-test-style-{}-{stamp}",
        std::process::id()
    ));
    let result = (|| -> Result<Finding, String> {
        let mut files: Vec<(&str, &str)> = vec![
            ("src/__init__.py", ""),
            (
                "pyproject.toml",
                "[project]\nname = \"fx\"\nversion = \"0.1.0\"\n",
            ),
            ("diff.patch", diff.as_str()),
        ];
        for (path, source) in sources {
            files.push((
                path,
                if *path == edit.owner {
                    after.as_str()
                } else {
                    source
                },
            ));
        }
        if !test.is_empty() {
            files.push(("tests/test_style.py", test));
        }
        for (path, text) in files {
            let target = root.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("{name}: create {}: {error}", parent.display()))?;
            }
            fs::write(&target, text).map_err(|error| format!("{name}: write {path}: {error}"))?;
        }
        let output = run_check(&root).map_err(|error| format!("{name}: check failed: {error}"))?;
        let mut findings = output.findings.into_iter();
        match (findings.next(), findings.next()) {
            (Some(finding), None) => Ok(finding),
            _ => Err(format!("{name}: expected exactly one finding")),
        }
    })();
    let cleanup = fs::remove_dir_all(&root)
        .map_err(|error| format!("{name}: remove {}: {error}", root.display()));
    let finding = result?;
    cleanup?;
    Ok(finding)
}

fn run_check(root: &Path) -> Result<crate::CheckOutput, String> {
    let config = crate::config::tests_only_parse("[languages]\nenabled = [\"python\"]\n")?;
    crate::app::check_workspace_with_config(
        crate::CheckInput {
            root: root.to_path_buf(),
            base: None,
            diff_file: Some(root.join("diff.patch")),
            mode: crate::Mode::Draft,
            format: crate::OutputFormat::Json,
            include_unchanged_tests: true,
            perl_facts_path: None,
            suppression_policy: None,
            git_timeout: None,
            git_candidate: None,
            scope: Default::default(),
        },
        &config,
    )
}

/// A unified diff replacing the one line `edit.old` of `before` with
/// `edit.new`, with up to three lines of context.
fn single_line_diff(edit: Edit, before: &str) -> Result<String, String> {
    let lines: Vec<&str> = before.lines().collect();
    let index = lines
        .iter()
        .position(|line| *line == edit.old)
        .ok_or_else(|| format!("edit line not found in {}", edit.owner))?;
    let start = index.saturating_sub(3);
    let end = (index + 4).min(lines.len());
    let count = end - start;
    let mut diff = format!(
        "diff --git a/{0} b/{0}\n--- a/{0}\n+++ b/{0}\n@@ -{1},{2} +{1},{2} @@\n",
        edit.owner,
        start + 1,
        count
    );
    for (offset, line) in lines[start..end].iter().enumerate() {
        if start + offset == index {
            diff.push_str(&format!("-{}\n+{}\n", edit.old, edit.new));
        } else {
            diff.push_str(&format!(" {line}\n"));
        }
    }
    Ok(diff)
}
