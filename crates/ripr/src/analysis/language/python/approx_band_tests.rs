//! RIPR-SPEC-0238 rule 21 (#13723): the `approx` band shapes read beyond the
//! keyword band on a one-statement function, and the neighbouring shapes that
//! must keep abstaining.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use crate::domain::{ExposureClass, StaticLimitKind};
use std::path::Path;

type Reading = (ExposureClass, Option<StaticLimitKind>);

const GAP: Reading = (ExposureClass::WeaklyExposed, None);
const CREDIT: Reading = (ExposureClass::Exposed, None);
const UNREAD: Reading = (
    ExposureClass::StaticUnknown,
    Some(StaticLimitKind::UnsupportedSyntax),
);

/// Classify replacing `old` with `new` on line `line` of `bands/core.py`
/// (`owner` is the new-side source) under `tests/test_bands.py`.
fn classify(
    owner: &str,
    line: usize,
    old: &str,
    test_source: &str,
    sole_line_replacement: bool,
) -> Result<Reading, String> {
    let new = owner
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("owner has no line {line}"))?;
    let finding = classify_change_with_context(
        Path::new("bands/core.py"),
        line,
        new,
        Some(old),
        &extract_owners(Path::new("bands/core.py"), owner),
        &extract_tests(Path::new("tests/test_bands.py"), test_source),
        PythonNoBehaviorContext {
            sole_line_replacement,
            ..PythonNoBehaviorContext::default()
        },
        Some(owner),
    )
    .ok_or_else(|| "the edit should classify".to_string())?;
    Ok((finding.class, finding.static_limit_kind))
}

/// A test module importing `names` from `bands.core`, with `module_lines`
/// above one test asserting `assertion`.
fn test_module(names: &str, module_lines: &str, assertion: &str) -> String {
    format!(
        "import pytest\n\nfrom bands.core import {names}\n{module_lines}\n\n\
         def test_band():\n    assert {assertion}\n"
    )
}

const FUNCTION: &str = "def band(x):\n    return x * 3\n";

fn function_edit(new_return: &str, assertion: &str) -> Result<Reading, String> {
    function_edit_with(new_return, "", assertion)
}

fn function_edit_with(
    new_return: &str,
    module_lines: &str,
    assertion: &str,
) -> Result<Reading, String> {
    classify(
        &FUNCTION.replace("x * 3", new_return),
        2,
        "    return x * 3",
        &test_module("band", module_lines, assertion),
        true,
    )
}

#[test]
fn a_positional_band_is_read_in_pytests_parameter_order() -> Result<(), String> {
    // The second argument is `rel`, the third `abs`, the fourth `nan_ok`.
    for (assertion, absorbed) in [
        ("band(10) == pytest.approx(30, 0.1)", GAP),
        ("band(10) == pytest.approx(30, None, 1)", GAP),
        ("band(10) == pytest.approx(30, 0.1, None, False)", GAP),
        ("pytest.approx(30, 0.1) == band(10)", GAP),
    ] {
        assert_eq!(
            function_edit("x * 3 + 0.5", assertion)?,
            absorbed,
            "{assertion}"
        );
        assert_eq!(
            function_edit("x * 3 + 5", assertion)?,
            CREDIT,
            "{assertion}"
        );
    }
    Ok(())
}

#[test]
fn only_pytests_own_approx_band_is_read() -> Result<(), String> {
    let edit = |module: String| {
        classify(
            &FUNCTION.replace("x * 3", "x * 3 + 0.5"),
            2,
            "    return x * 3",
            &module,
            true,
        )
    };
    let test = |head: &str, assertion: &str| {
        format!("{head}\nfrom bands.core import band\n\ndef test_band():\n    assert {assertion}\n")
    };
    // `from pytest import approx` is pytest's own.
    assert_eq!(
        edit(test(
            "from pytest import approx\n",
            "band(10) == approx(30, 0.1)"
        ))?,
        GAP
    );
    // A project helper named `approx` may read its second argument as an
    // absolute tolerance: `approx(30, 0.1)` is then a band of 0.1, not 3.
    for (head, assertion) in [
        (
            "import pytest\n\ndef approx(expected, tol):\n    return pytest.approx(expected, abs=tol)\n",
            "band(10) == approx(30, 0.1)",
        ),
        (
            "import pytest\n\ndef approx(expected, rel):\n    return pytest.approx(expected, abs=rel)\n",
            "band(10) == approx(30, rel=0.1)",
        ),
        (
            "from tests.helpers import approx\n",
            "band(10) == approx(30, 0.1)",
        ),
        (
            "from pytest import approx\n\ndef approx(expected, tol):\n    return expected\n",
            "band(10) == approx(30, 0.1)",
        ),
        ("import pytest as pt\n", "band(10) == pt.approx(30, 0.1)"),
        (
            "import helpers as pytest\n",
            "band(10) == pytest.approx(30, 0.1)",
        ),
        (
            "import pytest\npytest.approx = lambda expected, tol: expected\n",
            "band(10) == pytest.approx(30, 0.1)",
        ),
    ] {
        let (class, _) = edit(test(head, assertion))?;
        assert_eq!(class, ExposureClass::StaticUnknown, "{head}{assertion}");
    }
    Ok(())
}

#[test]
fn a_positional_band_outside_the_read_shape_keeps_abstaining() -> Result<(), String> {
    for assertion in [
        // `rel` given twice is a `TypeError`, not a band.
        "band(10) == pytest.approx(30, 0.1, rel=0.2)",
        // A `nan_ok` that is not a literal flag, and too many arguments.
        "band(10) == pytest.approx(30, 0.1, None, ok)",
        "band(10) == pytest.approx(30, 0.1, None, False, 1)",
        // A spread may carry anything.
        "band(10) == pytest.approx(30, *band_args)",
        "band(10) == pytest.approx(30, 0.1, **extra)",
    ] {
        assert_eq!(
            function_edit("x * 3 + 0.5", assertion)?,
            UNREAD,
            "{assertion}"
        );
    }
    Ok(())
}

#[test]
fn a_band_named_by_test_module_constants_is_read() -> Result<(), String> {
    let constants = "EXPECTED = 30\nREL = 0.1\n";
    let assertion = "band(10) == pytest.approx(EXPECTED, rel=REL)";
    assert_eq!(
        function_edit_with("x * 3 + 0.5", constants, assertion)?,
        GAP
    );
    assert_eq!(
        function_edit_with("x * 3 + 5", constants, assertion)?,
        CREDIT
    );
    // A positional named band reads the same.
    assert_eq!(
        function_edit_with(
            "x * 3 + 0.5",
            constants,
            "band(10) == pytest.approx(EXPECTED, REL)"
        )?,
        GAP
    );
    Ok(())
}

#[test]
fn a_named_band_that_may_differ_at_the_assertion_keeps_abstaining() -> Result<(), String> {
    for module_lines in [
        // Bound twice, rewritten through `global`, from a call, or imported.
        "EXPECTED = 30\nREL = 0.1\nREL = 0.5\n",
        "EXPECTED = 30\nREL = 0.1\n\ndef widen():\n    global REL\n    REL = 0.5\n",
        "EXPECTED = 30\nREL = float('0.1')\n",
        "from bands.limits import REL\nEXPECTED = 30\n",
        // A string constant is not a number.
        "EXPECTED = 30\nREL = '0.1'\n",
    ] {
        assert_eq!(
            function_edit_with(
                "x * 3 + 0.5",
                module_lines,
                "band(10) == pytest.approx(EXPECTED, rel=REL)"
            )?,
            UNREAD,
            "{module_lines}"
        );
    }
    Ok(())
}

const MEMOIZED: &str = "from functools import lru_cache\n\n\n@lru_cache(maxsize=None)\ndef band(x):\n    return x * 3\n";

fn memoized_edit(owner: &str, new_return: &str) -> Result<Reading, String> {
    let line = owner
        .lines()
        .position(|text| text == "    return x * 3")
        .ok_or("owner has no return line")?
        + 1;
    classify(
        &owner.replace("    return x * 3", &format!("    return {new_return}")),
        line,
        "    return x * 3",
        &test_module("band", "", "band(10) == pytest.approx(30, rel=0.1)"),
        true,
    )
}

#[test]
fn functools_memoization_returns_the_decorated_result() -> Result<(), String> {
    for owner in [
        MEMOIZED.to_string(),
        MEMOIZED.replace("(maxsize=None)", ""),
        MEMOIZED.replace("(maxsize=None)", "(maxsize=32, typed=True)"),
        "import functools\n\n\n@functools.cache\ndef band(x):\n    return x * 3\n".to_string(),
    ] {
        assert_eq!(memoized_edit(&owner, "x * 3 + 0.5")?, GAP, "{owner}");
        assert_eq!(memoized_edit(&owner, "x * 3 + 5")?, CREDIT, "{owner}");
    }
    Ok(())
}

#[test]
fn a_decorator_that_may_change_the_result_keeps_abstaining() -> Result<(), String> {
    for owner in [
        // A callable first argument makes `lru_cache` wrap that callable.
        MEMOIZED.replace("(maxsize=None)", "(other)"),
        MEMOIZED.replace("(maxsize=None)", "(maxsize=size)"),
        // The name is rebound after the import.
        MEMOIZED.replace(
            "\n\n\n@lru_cache",
            "\nlru_cache = lambda **kw: (lambda f: f)\n\n\n@lru_cache",
        ),
        "import functools\n\n\n@functools.cache()\ndef band(x):\n    return x * 3\n".to_string(),
    ] {
        assert_eq!(
            memoized_edit(&owner, "x * 3 + 0.5")?.0,
            ExposureClass::StaticUnknown,
            "{owner}"
        );
    }
    // The rebinding the transparency check does not see stays limited by the
    // band reader alone.
    assert_eq!(
        memoized_edit(
            &MEMOIZED.replace(
                "\n\n\n@lru_cache",
                "\nlru_cache = lambda **kw: (lambda f: f)\n\n\n@lru_cache",
            ),
            "x * 3 + 0.5"
        )?,
        UNREAD
    );
    assert_eq!(
        memoized_edit(
            &MEMOIZED.replace("(maxsize=None)", "(other)"),
            "x * 3 + 0.5"
        )?,
        UNREAD
    );
    // The decorator line may have changed in the same diff.
    assert_eq!(
        classify(
            &MEMOIZED.replace("    return x * 3", "    return x * 3 + 0.5"),
            6,
            "    return x * 3",
            &test_module("band", "", "band(10) == pytest.approx(30, rel=0.1)"),
            false,
        )?,
        UNREAD
    );
    Ok(())
}

const TWO_STATEMENTS: &str = "def band(x):\n    y = x * 3\n    return y\n";

fn body_edit(owner: &str, line: usize, old: &str, sole: bool) -> Result<Reading, String> {
    classify(
        owner,
        line,
        old,
        &test_module("band", "", "band(10) == pytest.approx(30, rel=0.1)"),
        sole,
    )
}

#[test]
fn a_straight_line_body_is_rebuilt_when_the_line_is_the_whole_diff() -> Result<(), String> {
    let absorbed = TWO_STATEMENTS.replace("y = x * 3", "y = x * 3 + 0.5");
    assert_eq!(body_edit(&absorbed, 2, "    y = x * 3", true)?, GAP);
    let rejected = TWO_STATEMENTS.replace("y = x * 3", "y = x * 3 + 5");
    assert_eq!(body_edit(&rejected, 2, "    y = x * 3", true)?, CREDIT);
    // The edited line may be the return itself.
    let returned = TWO_STATEMENTS.replace("return y", "return y + 5");
    assert_eq!(body_edit(&returned, 3, "    return y", true)?, CREDIT);
    Ok(())
}

#[test]
fn a_body_that_cannot_be_rebuilt_keeps_abstaining() -> Result<(), String> {
    let absorbed = TWO_STATEMENTS.replace("y = x * 3", "y = x * 3 + 0.5");
    // Another line of the file may have changed too.
    assert_eq!(body_edit(&absorbed, 2, "    y = x * 3", false)?, UNREAD);
    // Two statements spell the changed line, so which one changed is unknown.
    let repeated = "def band(x):\n    y = x * 3 + 0.5\n    y = x * 3 + 0.5\n    return y\n";
    assert_eq!(body_edit(repeated, 2, "    y = x * 3", true)?, UNREAD);
    // A statement other than a plain assignment, and a call the evaluation
    // does not model.
    let guarded = "def band(x):\n    y = x * 3 + 0.5\n    if x:\n        y = 0\n    return y\n";
    assert_eq!(body_edit(guarded, 2, "    y = x * 3", true)?, UNREAD);
    let called = "def band(x):\n    y = abs(x) * 3 + 0.5\n    return y\n";
    assert_eq!(body_edit(called, 2, "    y = abs(x) * 3", true)?, UNREAD);
    Ok(())
}

const METHOD: &str = "class Rates:\n    def band(self, x):\n        return x * 3\n";

fn method_edit(owner: &str, new_return: &str, test_source: &str) -> Result<Reading, String> {
    let line = owner
        .lines()
        .position(|text| text == "        return x * 3")
        .ok_or("owner has no return line")?
        + 1;
    classify(
        &owner.replace("return x * 3", &format!("return {new_return}")),
        line,
        "        return x * 3",
        test_source,
        true,
    )
}

fn method_test(assertion: &str) -> String {
    test_module("Rates", "", assertion)
}

#[test]
fn a_method_called_on_a_fresh_plain_instance_is_read() -> Result<(), String> {
    let test = method_test("Rates().band(10) == pytest.approx(30, rel=0.1)");
    assert_eq!(method_edit(METHOD, "x * 3 + 0.5", &test)?, GAP);
    assert_eq!(method_edit(METHOD, "x * 3 + 5", &test)?, CREDIT);
    let documented = METHOD.replace("class Rates:\n", "class Rates:\n    \"\"\"Rates.\"\"\"\n\n");
    assert_eq!(method_edit(&documented, "x * 3 + 0.5", &test)?, GAP);
    Ok(())
}

#[test]
fn a_method_whose_instance_may_differ_keeps_abstaining() -> Result<(), String> {
    let test = method_test("Rates().band(10) == pytest.approx(30, rel=0.1)");
    for owner in [
        // An initializer, a base, a class decorator or a dunder can replace
        // the method or the instance.
        METHOD.replace(
            "class Rates:\n",
            "class Rates:\n    def __init__(self):\n        self.band = lambda x: 0\n\n",
        ),
        METHOD.replace("class Rates:", "class Rates(Base):"),
        METHOD.replace("class Rates:", "@register\nclass Rates:"),
        METHOD.replace("class Rates:\n", "class Rates:\n    band = None\n"),
        // The method itself is decorated.
        METHOD.replace("    def band", "    @staticmethod\n    def band"),
    ] {
        assert_eq!(
            method_edit(&owner, "x * 3 + 0.5", &test)?.0,
            ExposureClass::StaticUnknown,
            "{owner}"
        );
    }
    // A dunder can answer the call with another method without writing any
    // attribute.
    for dunder in [
        "    def __getattribute__(self, name):\n        return lambda x: 0\n\n",
        "    def __new__(cls):\n        return Other()\n\n",
    ] {
        assert_eq!(
            method_edit(
                &METHOD.replace("class Rates:\n", &format!("class Rates:\n{dunder}")),
                "x * 3 + 0.5",
                &test
            )?,
            UNREAD,
            "{dunder}"
        );
    }
    for test in [
        // Constructor arguments, a receiver read from `self`, a held
        // instance, and a class the test may rebind.
        method_test("Rates(1).band(10) == pytest.approx(30, rel=0.1)"),
        test_module(
            "Rates",
            "\nRates = type('Rates', (), {'band': lambda self, x: 0})\n",
            "Rates().band(10) == pytest.approx(30, rel=0.1)",
        ),
        test_module(
            "Rates",
            "\n\ndef setup_module():\n    Rates.band = lambda self, x: 0\n",
            "Rates().band(10) == pytest.approx(30, rel=0.1)",
        ),
    ] {
        assert_eq!(
            method_edit(METHOD, "x * 3 + 0.5", &test)?.0,
            ExposureClass::StaticUnknown,
            "{test}"
        );
    }
    assert_eq!(
        method_edit(
            METHOD,
            "x * 3 + 0.5",
            &method_test("Rates(1).band(10) == pytest.approx(30, rel=0.1)")
        )?,
        UNREAD
    );
    let reads_self = "class Rates:\n    def band(self, x):\n        return x * 3 + self.k\n";
    assert_eq!(
        classify(
            &reads_self.replace("+ self.k", "+ self.k + 0.5"),
            3,
            "        return x * 3 + self.k",
            &test,
            true
        )?
        .0,
        ExposureClass::StaticUnknown
    );
    Ok(())
}

#[test]
fn an_async_owner_keeps_abstaining() -> Result<(), String> {
    // A coroutine never equals the band, before or after the edit.
    assert_eq!(
        classify(
            "async def band():\n    return 30.5\n",
            2,
            "    return 30",
            &test_module("band", "", "band() == pytest.approx(30, rel=0.1)"),
            true,
        )?,
        UNREAD
    );
    Ok(())
}
