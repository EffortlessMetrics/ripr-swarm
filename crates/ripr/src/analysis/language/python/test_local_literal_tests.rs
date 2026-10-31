//! #10707: a boundary input held in a test-local name places the input like
//! the literal would, and every way that name can differ at the call keeps the
//! finding `weakly_exposed`.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_NEW: &str =
    "def tier(score):\n    if score >= 50:\n        return \"gold\"\n    return \"basic\"\n";
const OLD_LINE: &str = "    if score > 50:";
const NEW_LINE: &str = "    if score >= 50:";

fn class_with_test(test_source: &str) -> Result<ExposureClass, String> {
    let owners = extract_owners(Path::new("src/pricing.py"), OWNER_NEW);
    let tests = extract_tests(Path::new("tests/test_pricing.py"), test_source);
    classify_change_with_old(
        Path::new("src/pricing.py"),
        2,
        NEW_LINE,
        Some(OLD_LINE),
        &owners,
        &tests,
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "expected a finding".to_string())
}

fn body(statements: &str) -> String {
    format!(
        "from src.pricing import tier\n\n\ndef test_gold():\n    assert tier(80) == \"gold\"\n\n\ndef test_edge():\n{statements}\n"
    )
}

#[test]
fn a_single_assignment_local_places_the_boundary_input() -> Result<(), String> {
    assert_eq!(
        class_with_test(&body("    s = 50\n    assert tier(s) == \"basic\""))?,
        ExposureClass::Exposed
    );
    assert_eq!(
        class_with_test(&body("    s = 50\n    assert tier(score=s) == \"basic\""))?,
        ExposureClass::Exposed
    );
    // The literal itself is the control this matches.
    assert_eq!(
        class_with_test(&body("    assert tier(50) == \"basic\""))?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_local_that_is_not_the_boundary_value_does_not_credit() -> Result<(), String> {
    assert_eq!(
        class_with_test(&body("    s = 49\n    assert tier(s) == \"basic\""))?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn every_other_way_the_name_can_differ_keeps_the_weak_verdict() -> Result<(), String> {
    for statements in [
        // A second binding, in any form.
        "    s = 50\n    s = 60\n    assert tier(s) == \"basic\"",
        "    s = 50\n    s += 1\n    assert tier(s) == \"basic\"",
        "    s = 50\n    for s in (1, 2):\n        pass\n    assert tier(s) == \"basic\"",
        "    s = 50\n    del s\n    assert tier(s) == \"basic\"",
        "    s = 50\n    (s := 60)\n    assert tier(s) == \"basic\"",
        "    lvl = 50\n    with open('f') as lvl:\n        pass\n    assert tier(lvl) == \"basic\"",
        "    s = 50\n    import os as s\n    assert tier(s) == \"basic\"",
        "    lvl = 50\n    try:\n        pass\n    except ValueError as lvl:\n        pass\n    assert tier(lvl) == \"basic\"",
        // Chained, unpacked or conditional bindings are not one plain binding.
        "    s = t = 50\n    assert tier(s) == \"basic\"",
        "    s, t = 50, 1\n    assert tier(s) == \"basic\"",
        "    if True:\n        s = 50\n    assert tier(s) == \"basic\"",
        // The call runs before the binding.
        "    assert tier(s) == \"basic\"\n    s = 50",
        // The call's `s` is another scope's name.
        "    s = 50\n    assert (lambda s: tier(s))(60) == \"gold\"",
        // A nested definition or declaration can rebind it.
        "    s = 50\n    def s():\n        pass\n    assert tier(s) == \"basic\"",
        "    lvl = 50\n    match 1:\n        case lvl:\n            pass\n    assert tier(lvl) == \"basic\"",
        // A non-literal value is not known.
        "    s = 25 * 2\n    assert tier(s) == \"basic\"",
        "    s = int('50')\n    assert tier(s) == \"basic\"",
    ] {
        assert_eq!(
            class_with_test(&body(statements))?,
            ExposureClass::WeaklyExposed,
            "{statements}"
        );
    }
    Ok(())
}

#[test]
fn a_parameter_of_the_test_is_not_a_local_literal() -> Result<(), String> {
    let source = "from src.pricing import tier\n\n\ndef test_gold():\n    assert tier(80) == \"gold\"\n\n\ndef test_edge(s):\n    s = 50\n    assert tier(s) == \"basic\"\n";
    assert_eq!(class_with_test(source)?, ExposureClass::WeaklyExposed);
    Ok(())
}
