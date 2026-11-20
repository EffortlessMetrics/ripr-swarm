//! Plain builtin `@property` reads (#15490, RIPR-SPEC-0238 Decision 10).
//!
//! A pytest test that compares a direct read of the property with a literal
//! credits the getter's one changed `return`, and only that read lifts the
//! owner's `decorator_indirection` limit. Every other shape keeps the limit,
//! with the relation it had: a refused test shape never relates as
//! `property_read`, and a refused owner or assertion keeps the relation but
//! not the credit.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_FILE: &str = "src/fx/core.py";
const HEAD: &str = "class Box:\n    def __init__(self, items):\n        self.items = items\n\n";
const GETTER: &str = "    @property\n    def total(self):\n        return sum(self.items) + 1\n";
const NEW_LINE: &str = "        return sum(self.items) + 1";
const OLD_LINE: &str = "        return sum(self.items)";
const IMPORT: &str = "from fx.core import Box\n\n\n";
const INLINE_TEST: &str =
    "from fx.core import Box\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n";
const BOUND_TEST: &str = "from fx.core import Box\n\n\ndef test_total():\n    b = Box([1, 2, 3])\n    assert b.total == 6\n";
const PROPERTY_LIMIT: &str = "static_limit decorator_indirection: `property`";
const RELATION: &str = "related_test_relation: property_read (test_total)";
const CREDIT: &str = "strong_oracle_observes_property_read";

fn plain_source() -> String {
    format!("{HEAD}{GETTER}")
}

/// A pytest module holding `test_total` with `body` (indented lines).
fn test_module(body: &str) -> String {
    format!("{IMPORT}def test_total():\n{body}")
}

fn classify_at(
    source: &str,
    new_line: &str,
    old_line: &str,
    test_src: &str,
) -> Result<Finding, String> {
    let owners = extract_owners(Path::new(OWNER_FILE), source);
    let tests = extract_tests(Path::new("tests/test_box.py"), test_src);
    if !tests.iter().any(|test| test.name == "test_total") {
        return Err(format!("fixture must parse `test_total`: {test_src}"));
    }
    let mut lines = source
        .lines()
        .enumerate()
        .filter(|(_, line)| *line == new_line);
    let (Some((index, _)), None) = (lines.next(), lines.next()) else {
        return Err(format!("the changed line must occur once in {source}"));
    };
    let line = index + 1;
    if !owners.iter().any(|owner| {
        owner.qualified_name == "Box.total" && owner.start_line < line && line <= owner.end_line
    }) {
        return Err(format!("the changed line must be in the getter: {source}"));
    }
    classify_change_with_source(
        Path::new(OWNER_FILE),
        line,
        new_line,
        Some(old_line),
        &owners,
        &tests,
        Some(source),
    )
    .ok_or_else(|| "the change must yield a finding".to_string())
}

fn classify(source: &str, test_src: &str) -> Result<Finding, String> {
    classify_at(source, NEW_LINE, OLD_LINE, test_src)
}

fn has(finding: &Finding, line: &str) -> bool {
    finding.evidence.iter().any(|evidence| evidence == line)
}

/// The finding keeps `static_unknown` with the named decorator's limit, no
/// property-read credit, and the property-read relation exactly when
/// `relates` holds.
fn assert_kept(case: &str, finding: &Finding, limit: &str, relates: bool) -> Result<(), String> {
    if finding.class != ExposureClass::StaticUnknown
        || !has(finding, limit)
        || finding.alignment_reason.as_deref() == Some(CREDIT)
        || has(finding, RELATION) != relates
    {
        return Err(format!(
            "{case}: expected static_unknown, `{limit}`, property_read relation {relates}: {:?} {:?} {:?}",
            finding.class, finding.alignment_reason, finding.evidence
        ));
    }
    Ok(())
}

#[test]
fn a_literal_comparison_of_a_property_read_is_exposed() -> Result<(), String> {
    let docstring_and_local = format!(
        "{HEAD}    @property\n    def total(self):\n        \"\"\"The sum.\"\"\"\n        base = 1\n        return sum(self.items) + 1\n"
    );
    for (case, source, test) in [
        ("inline construction", plain_source(), INLINE_TEST),
        ("bound once", plain_source(), BOUND_TEST),
        ("straight-line getter", docstring_and_local, INLINE_TEST),
    ] {
        let finding = classify(&source, test)?;
        if finding.class != ExposureClass::Exposed
            || has(&finding, PROPERTY_LIMIT)
            || !has(&finding, RELATION)
            || finding.alignment_reason.as_deref() != Some(CREDIT)
            || finding.oracle_alignment.as_deref() != Some("direct")
        {
            return Err(format!(
                "{case}: the exact property read must be exposed: {:?} {:?} {:?} {:?}",
                finding.class, finding.oracle_alignment, finding.alignment_reason, finding.evidence
            ));
        }
    }
    Ok(())
}

#[test]
fn an_owner_that_may_not_be_a_plain_builtin_property_keeps_the_limit() -> Result<(), String> {
    let setter = format!(
        "{HEAD}{GETTER}\n    @total.setter\n    def total(self, value):\n        self.items = [value]\n"
    );
    let deleter = format!(
        "{HEAD}{GETTER}\n    @total.deleter\n    def total(self):\n        self.items = []\n"
    );
    let imported_property = format!("from fx.descriptors import property\n\n\n{HEAD}{GETTER}");
    let attribute_hook = format!(
        "{HEAD}    def __getattribute__(self, name):\n        return object.__getattribute__(self, name)\n\n{GETTER}"
    );
    // Writes inside the class that replace or shadow the property.
    let class_write_in_init = format!(
        "class Box:\n    def __init__(self, items):\n        self.items = items\n        Box.total = 6\n\n{GETTER}"
    );
    let class_del_in_method =
        format!("{HEAD}    def reset(self):\n        del Box.total\n\n{GETTER}");
    let type_write = format!(
        "class Box:\n    def __init__(self, items):\n        self.items = items\n        type(self).total = 6\n\n{GETTER}"
    );
    let dunder_class_write = format!(
        "class Box:\n    def __init__(self, items):\n        self.items = items\n        self.__class__.total = 6\n\n{GETTER}"
    );
    let setattr_write = format!(
        "class Box:\n    def __init__(self, items):\n        self.items = items\n        setattr(self, 'total', 6)\n\n{GETTER}"
    );
    let instance_store = format!("{HEAD}    def reset(self):\n        self.total = 0\n\n{GETTER}");
    let instance_del = format!("{HEAD}    def reset(self):\n        del self.total\n\n{GETTER}");
    for (case, source) in [
        ("setter sibling", setter.as_str()),
        ("deleter sibling", deleter.as_str()),
        ("attribute hook", attribute_hook.as_str()),
        ("class write in __init__", class_write_in_init.as_str()),
        ("class del in a method", class_del_in_method.as_str()),
        ("type(self) write", type_write.as_str()),
        ("__class__ write", dunder_class_write.as_str()),
        ("setattr write", setattr_write.as_str()),
        ("instance store of the property", instance_store.as_str()),
        ("instance del of the property", instance_del.as_str()),
    ] {
        assert_kept(case, &classify(source, INLINE_TEST)?, PROPERTY_LIMIT, true)?;
    }
    // A decorator other than the bare builtin name, or a second decorator,
    // is not a property owner: no relation, and the limit stays.
    let aliased_builtins = format!(
        "import builtins as bi\n\n\n{HEAD}    @bi.property\n    def total(self):\n        return sum(self.items) + 1\n"
    );
    let stacked = format!(
        "def trace(f):\n    return f\n\n\n{HEAD}    @property\n    @trace\n    def total(self):\n        return sum(self.items) + 1\n"
    );
    // An imported `property` fails the owner gate: no relation either.
    assert_kept(
        "imported property",
        &classify(&imported_property, INLINE_TEST)?,
        PROPERTY_LIMIT,
        false,
    )?;
    assert_kept(
        "builtins alias",
        &classify(&aliased_builtins, INLINE_TEST)?,
        "static_limit decorator_indirection: `bi.property`",
        false,
    )?;
    assert_kept(
        "stacked decorators",
        &classify(&stacked, INLINE_TEST)?,
        PROPERTY_LIMIT,
        false,
    )
}

#[test]
fn a_changed_line_that_may_not_reach_the_read_keeps_the_limit() -> Result<(), String> {
    let guarded = format!(
        "{HEAD}    @property\n    def total(self):\n        if not self.items:\n            return 1\n        return sum(self.items) + 1\n"
    );
    // Two returns: the top-level one changes, but the body branches.
    assert_kept(
        "multi-return getter",
        &classify(&guarded, INLINE_TEST)?,
        PROPERTY_LIMIT,
        true,
    )?;
    // The changed return sits under the guard the test never enters.
    assert_kept(
        "changed return under if",
        &classify_at(
            &guarded,
            "            return 1",
            "            return 0",
            INLINE_TEST,
        )?,
        PROPERTY_LIMIT,
        true,
    )?;
    // The changed line is not the return.
    let local = format!(
        "{HEAD}    @property\n    def total(self):\n        n = sum(self.items) + 1\n        return n\n"
    );
    assert_kept(
        "changed non-return line",
        &classify_at(
            &local,
            "        n = sum(self.items) + 1",
            "        n = sum(self.items)",
            INLINE_TEST,
        )?,
        PROPERTY_LIMIT,
        true,
    )
}

#[test]
fn a_getter_whose_return_may_branch_keeps_the_limit() -> Result<(), String> {
    let getter = |body: &str| format!("{HEAD}    @property\n    def total(self):\n{body}");
    for (case, body, new_line, old_line) in [
        (
            "conditional expression",
            "        return self.items[0] if self.items else 1\n",
            "        return self.items[0] if self.items else 1",
            "        return self.items[0] if self.items else 0",
        ),
        (
            "or chain",
            "        return self.items or 1\n",
            "        return self.items or 1",
            "        return self.items or 0",
        ),
        (
            "and chain",
            "        return self.items and 1\n",
            "        return self.items and 1",
            "        return self.items and 0",
        ),
        (
            "conditional comprehension",
            "        return sum([i for i in self.items if i > 1])\n",
            "        return sum([i for i in self.items if i > 1])",
            "        return sum([i for i in self.items if i > 0])",
        ),
        (
            "branch in an assignment before the return",
            "        n = 1 if self.items else 0\n        return sum(self.items) + n\n",
            "        return sum(self.items) + n",
            "        return sum(self.items) - n",
        ),
    ] {
        assert_kept(
            case,
            &classify_at(&getter(body), new_line, old_line, INLINE_TEST)?,
            PROPERTY_LIMIT,
            true,
        )?;
    }
    // Regression pins: slots naming the property, and a dynamic read in the
    // getter, keep their limits.
    let slots = format!(
        "class Box:\n    __slots__ = ('total',)\n\n    def __init__(self, items):\n        self.items = items\n\n{GETTER}"
    );
    // A dunder class attribute makes the class non-plain: no relation.
    assert_kept(
        "slots",
        &classify(&slots, INLINE_TEST)?,
        PROPERTY_LIMIT,
        false,
    )?;
    let dynamic = getter("        return getattr(self, 'total')\n");
    let finding = classify_at(
        &dynamic,
        "        return getattr(self, 'total')",
        "        return getattr(self, 'items')",
        INLINE_TEST,
    )?;
    if finding.class != ExposureClass::StaticUnknown
        || finding.alignment_reason.as_deref() == Some(CREDIT)
        || !finding
            .evidence
            .iter()
            .any(|line| line.starts_with("static_limit dynamic_dispatch"))
    {
        return Err(format!(
            "getattr getter: must stay dynamic_dispatch: {:?} {:?}",
            finding.class, finding.evidence
        ));
    }
    Ok(())
}

#[test]
fn an_assertion_that_is_not_an_exact_literal_read_keeps_the_limit() -> Result<(), String> {
    for (case, body) in [
        ("not-equal", "    assert Box([1, 2, 3]).total != 5\n"),
        ("identity", "    assert Box([1, 2, 3]).total is 6\n"),
        ("membership", "    assert Box([1, 2, 3]).total in (6, 7)\n"),
        (
            "chained compare",
            "    assert 5 < Box([1, 2, 3]).total == 6\n",
        ),
        ("or chain", "    assert Box([1, 2, 3]).total == 6 or True\n"),
        ("message", "    assert Box([1, 2, 3]).total == 6, 'total'\n"),
        (
            "bare truthiness",
            "    b = Box([1, 2, 3])\n    assert b.total\n",
        ),
    ] {
        assert_kept(
            case,
            &classify(&plain_source(), &test_module(body))?,
            PROPERTY_LIMIT,
            true,
        )?;
    }
    Ok(())
}

#[test]
fn a_test_that_does_not_read_the_property_directly_keeps_the_limit() -> Result<(), String> {
    let other_class = "from fx.core import Box\nfrom fx.other import Other\n\n\ndef test_total():\n    assert Other([1, 2, 3]).total == 6\n";
    let property_mock = "from unittest.mock import PropertyMock\nfrom fx.core import Box\n\n\ndef test_total():\n    b = Box([1, 2, 3])\n    type(b).total = PropertyMock(return_value=6)\n    assert b.total == 6\n";
    let helper = "from fx.core import Box\n\n\ndef read_total(box):\n    return box.total\n\n\ndef test_total():\n    assert read_total(Box([1, 2, 3])) == 6\n";
    let module_write = "from fx.core import Box\n\nBox.total = 6\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n";
    let module_rebind = "from fx.core import Box\nfrom fx.other import Other as Box\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n";
    // A namespace writer in a sibling test of the same module.
    let sibling_setattr = "from fx.core import Box\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n\n\ndef test_other():\n    setattr(Box, 'total', 6)\n";
    let sibling_monkeypatch = "from fx.core import Box\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n\n\ndef test_other(monkeypatch):\n    monkeypatch.setattr('fx.core.Box.total', 6)\n";
    let autouse = "import pytest\nfrom fx.core import Box\n\n\n@pytest.fixture(autouse=True)\ndef fake_total():\n    yield\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n";
    let mut cases = vec![
        ("unrelated class", other_class.to_string()),
        ("class access", test_module("    assert Box.total == 6\n")),
        (
            "call of the read",
            test_module("    assert Box([1, 2, 3]).total() == 6\n"),
        ),
        (
            "assignment",
            test_module("    b = Box([1, 2, 3])\n    b.total = 6\n    assert b.total == 6\n"),
        ),
        ("PropertyMock", property_mock.to_string()),
        ("helper read", helper.to_string()),
        (
            "rebound local",
            test_module("    b = Box([1, 2, 3])\n    b = Box([4])\n    assert b.total == 6\n"),
        ),
        (
            "assert under if",
            test_module(
                "    b = Box([1, 2, 3])\n    if b is not None:\n        assert b.total == 6\n",
            ),
        ),
        (
            "assert under with",
            test_module("    with open('x'):\n        assert Box([1, 2, 3]).total == 6\n"),
        ),
        ("module-level write", module_write.to_string()),
        ("module-level rebinding", module_rebind.to_string()),
        ("sibling setattr", sibling_setattr.to_string()),
        (
            "sibling monkeypatch target string",
            sibling_monkeypatch.to_string(),
        ),
        ("autouse fixture", autouse.to_string()),
    ];
    for (case, test) in cases.drain(..) {
        assert_kept(
            case,
            &classify(&plain_source(), &test)?,
            PROPERTY_LIMIT,
            false,
        )?;
    }
    Ok(())
}

#[test]
fn the_property_read_relation_names_its_own_kind() -> Result<(), String> {
    let source = plain_source();
    let owners = extract_owners(Path::new(OWNER_FILE), &source);
    let owner = owners
        .iter()
        .find(|owner| owner.qualified_name == "Box.total")
        .ok_or("the getter must be an owner")?;
    for (test_src, expected) in [
        (INLINE_TEST.to_string(), true),
        (BOUND_TEST.to_string(), true),
        (test_module("    assert Box.total == 6\n"), false),
    ] {
        let tests = extract_tests(Path::new("tests/test_box.py"), &test_src);
        let [test] = tests.as_slice() else {
            return Err(format!("one test expected: {test_src}"));
        };
        let relation = related_tests::related_test_relation(test, owner).map(|kind| kind.as_str());
        if (relation == Some("property_read")) != expected {
            return Err(format!(
                "relation {relation:?}, property_read expected {expected} for {test_src}"
            ));
        }
    }
    Ok(())
}
