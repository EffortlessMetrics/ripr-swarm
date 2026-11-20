//! Plain builtin `@property` reads (#15490, RIPR-SPEC-0238 Decision 10).
//!
//! A pytest test that compares a direct read of the property with a literal
//! credits the getter's changed `return`, and only that read lifts the
//! owner's `decorator_indirection` limit. Every other shape keeps the limit.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_FILE: &str = "src/fx/core.py";
const HEAD: &str = "class Box:\n    def __init__(self, items):\n        self.items = items\n\n";
const GETTER: &str = "    @property\n    def total(self):\n        return sum(self.items) + 1\n";
const NEW_LINE: &str = "        return sum(self.items) + 1";
const OLD_LINE: &str = "        return sum(self.items)";
const INLINE_TEST: &str =
    "from fx.core import Box\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n";
const BOUND_TEST: &str = "from fx.core import Box\n\n\ndef test_total():\n    b = Box([1, 2, 3])\n    assert b.total == 6\n";
const LIMIT: &str = "static_limit decorator_indirection";
const CREDIT: &str = "strong_oracle_observes_property_read";

fn plain_source() -> String {
    format!("{HEAD}{GETTER}")
}

fn classify(source: &str, test_src: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new(OWNER_FILE), source);
    let tests = extract_tests(Path::new("tests/test_box.py"), test_src);
    if tests.is_empty() {
        return Err(format!("fixture must parse a test: {test_src}"));
    }
    let line = source
        .lines()
        .position(|line| line == NEW_LINE)
        .map(|index| index + 1)
        .ok_or_else(|| format!("the changed line must be in {source}"))?;
    if !owners
        .iter()
        .any(|owner| owner.qualified_name == "Box.total" && owner.start_line < line)
    {
        return Err(format!("the getter must be an owner: {source}"));
    }
    classify_change_with_source(
        Path::new(OWNER_FILE),
        line,
        NEW_LINE,
        Some(OLD_LINE),
        &owners,
        &tests,
        Some(source),
    )
    .ok_or_else(|| "the change must yield a finding".to_string())
}

fn limited(finding: &Finding) -> bool {
    finding.evidence.iter().any(|line| line.contains(LIMIT))
}

fn assert_limited(case: &str, finding: &Finding) -> Result<(), String> {
    if finding.class != ExposureClass::StaticUnknown
        || !limited(finding)
        || finding.alignment_reason.as_deref() == Some(CREDIT)
    {
        return Err(format!(
            "{case}: must stay static_unknown with decorator_indirection: {:?} {:?} {:?}",
            finding.class, finding.alignment_reason, finding.evidence
        ));
    }
    Ok(())
}

#[test]
fn a_literal_comparison_of_a_property_read_is_exposed() -> Result<(), String> {
    for (case, test) in [
        ("inline construction", INLINE_TEST),
        ("bound once", BOUND_TEST),
    ] {
        let finding = classify(&plain_source(), test)?;
        if finding.class != ExposureClass::Exposed
            || limited(&finding)
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
    let aliased_builtins = format!(
        "import builtins as bi\n\n\n{HEAD}    @bi.property\n    def total(self):\n        return sum(self.items) + 1\n"
    );
    let imported_property = format!("from fx.descriptors import property\n\n\n{HEAD}{GETTER}");
    let stacked = format!(
        "def trace(f):\n    return f\n\n\n{HEAD}    @property\n    @trace\n    def total(self):\n        return sum(self.items) + 1\n"
    );
    let attribute_hook = format!(
        "{HEAD}    def __getattribute__(self, name):\n        return object.__getattribute__(self, name)\n\n{GETTER}"
    );
    for (case, source) in [
        ("setter sibling", setter.as_str()),
        ("deleter sibling", deleter.as_str()),
        ("builtins alias", aliased_builtins.as_str()),
        ("imported property", imported_property.as_str()),
        ("stacked decorators", stacked.as_str()),
        ("attribute hook", attribute_hook.as_str()),
    ] {
        assert_limited(case, &classify(source, INLINE_TEST)?)?;
    }
    Ok(())
}

#[test]
fn a_test_that_does_not_read_the_property_directly_keeps_the_limit() -> Result<(), String> {
    let other_class = "from fx.core import Box\nfrom fx.other import Other\n\n\ndef test_total():\n    assert Other([1, 2, 3]).total == 6\n";
    let class_access =
        "from fx.core import Box\n\n\ndef test_total():\n    assert Box.total == 6\n";
    let called =
        "from fx.core import Box\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total() == 6\n";
    let written = "from fx.core import Box\n\n\ndef test_total():\n    b = Box([1, 2, 3])\n    b.total = 6\n    assert b.total == 6\n";
    let property_mock = "from unittest.mock import PropertyMock\nfrom fx.core import Box\n\n\ndef test_total():\n    b = Box([1, 2, 3])\n    type(b).total = PropertyMock(return_value=6)\n    assert b.total == 6\n";
    let helper = "from fx.core import Box\n\n\ndef read_total(box):\n    return box.total\n\n\ndef test_total():\n    assert read_total(Box([1, 2, 3])) == 6\n";
    let bare = "from fx.core import Box\n\n\ndef test_total():\n    b = Box([1, 2, 3])\n    assert b.total\n";
    let rebound = "from fx.core import Box\n\n\ndef test_total():\n    b = Box([1, 2, 3])\n    b = Box([4])\n    assert b.total == 6\n";
    let module_write = "from fx.core import Box\n\nBox.total = 6\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n";
    let module_rebind = "from fx.core import Box\nfrom fx.other import Other as Box\n\n\ndef test_total():\n    assert Box([1, 2, 3]).total == 6\n";
    for (case, test) in [
        ("unrelated class", other_class),
        ("class access", class_access),
        ("call of the read", called),
        ("assignment", written),
        ("PropertyMock", property_mock),
        ("helper read", helper),
        ("bare truthiness read", bare),
        ("rebound local", rebound),
        ("module-level write", module_write),
        ("module-level rebinding", module_rebind),
    ] {
        assert_limited(case, &classify(&plain_source(), test)?)?;
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
        (INLINE_TEST, true),
        (BOUND_TEST, true),
        (
            "from fx.core import Box\n\n\ndef test_total():\n    assert Box.total == 6\n",
            false,
        ),
    ] {
        let tests = extract_tests(Path::new("tests/test_box.py"), test_src);
        let [test] = tests.as_slice() else {
            return Err(format!("one test expected: {test_src}"));
        };
        let relation = related_tests::related_test_relation(test, owner).map(|kind| kind.as_str());
        if (relation == Some("property_read")) != expected {
            return Err(format!(
                "relation {relation:?} != {expected:?} for {test_src}"
            ));
        }
    }
    Ok(())
}
