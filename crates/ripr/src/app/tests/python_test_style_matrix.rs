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
        name: "pytest.raises with a base class",
        sources: &[("src/checked.py", RAISE)],
        edit: RAISE_EDIT,
        test: "import pytest\n\nfrom src.checked import checked\n\n\ndef test_negative():\n    with pytest.raises(Exception):\n        checked(-1)\n",
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
    WeakOnly {
        name: "hypothesis property pinning the formula",
        sources: &[("src/calc.py", CALC)],
        edit: CALC_EDIT,
        test: "from hypothesis import given, strategies as st\n\nfrom src.calc import discount\n\n\n@given(st.integers(min_value=100, max_value=1000))\ndef test_discount(total):\n    assert discount(total) == total - 10\n",
    },
];

#[test]
fn every_python_test_style_keeps_weak_tests_out_of_exposed() -> Result<(), String> {
    let mut failures = Vec::new();
    let weak_rows = STYLES
        .iter()
        .map(|style| (style.name, style.sources, style.edit, style.weak))
        .chain(
            WEAK_ONLY
                .iter()
                .map(|row| (row.name, row.sources, row.edit, row.test)),
        );
    for (name, sources, edit, test) in weak_rows {
        let finding = check_style(name, sources, edit, test)?;
        if finding.class == ExposureClass::Exposed {
            failures.push(format!("{name}: weak test read exposed"));
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
    assert_ne!(finding.class, ExposureClass::Exposed, "{finding:?}");
    assert!(finding.related_tests.is_empty(), "{finding:?}");
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
