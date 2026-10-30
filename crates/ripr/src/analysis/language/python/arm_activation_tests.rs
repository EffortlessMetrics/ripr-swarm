//! Pins for the unreached `match` arm and `.get` default rules (#9429).
//!
//! Each rule has a test that must stop crediting `exposed` and a control that
//! must keep crediting the same shape when an observed input reaches the edit.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_FILE: &str = "src/shop.py";
const TEST_FILE: &str = "tests/test_shop.py";

/// Classify line `line` of `owner` against `test_body`, with the owner's
/// new-side source threaded in as the production diff path does.
fn classify_at(
    owner: &str,
    line: usize,
    old_line: &str,
    imports: &str,
    test_body: &str,
) -> Result<ExposureClass, String> {
    let new_line = owner
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("owner has no line {line}"))?;
    let owners = extract_owners(Path::new(OWNER_FILE), owner);
    let tests = extract_tests(Path::new(TEST_FILE), &format!("{imports}\n\n\n{test_body}"));
    classify_change_with_context(
        Path::new(OWNER_FILE),
        line,
        new_line,
        Some(old_line),
        &owners,
        &tests,
        PythonNoBehaviorContext::default(),
        Some(owner),
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "changed line should classify".to_string())
}

/// Classify the last line of `owner`.
fn classify(
    owner: &str,
    old_line: &str,
    imports: &str,
    test_body: &str,
) -> Result<ExposureClass, String> {
    classify_at(owner, owner.lines().count(), old_line, imports, test_body)
}

const KIND_IMPORT: &str = "from src.shop import kind";
const KIND_OLD: &str = "            return \"other\"";
const KIND: &str = "def kind(value):\n    match value:\n        case 0:\n            return \"zero\"\n        case int():\n            return \"int\"\n        case _:\n            return \"misc\"\n";

#[test]
fn a_wildcard_arm_no_observed_input_reaches_does_not_credit() -> Result<(), String> {
    // `kind(0)` takes the literal arm and `kind(5)` the `int()` arm.
    let unreached =
        "def test_kind():\n    assert kind(0) == \"zero\"\n    assert kind(5) == \"int\"\n";
    assert_eq!(
        classify(KIND, KIND_OLD, KIND_IMPORT, unreached)?,
        ExposureClass::WeaklyExposed
    );
    // A string input falls through to the wildcard, so the arm is observed.
    let reached =
        "def test_kind():\n    assert kind(0) == \"zero\"\n    assert kind(\"x\") == \"misc\"\n";
    assert_eq!(
        classify(KIND, KIND_OLD, KIND_IMPORT, reached)?,
        ExposureClass::Exposed
    );
    // One reaching input among unreaching ones keeps the credit.
    let mixed =
        "def test_kind():\n    assert kind(5) == \"int\"\n    assert kind(\"x\") == \"misc\"\n";
    assert_eq!(
        classify(KIND, KIND_OLD, KIND_IMPORT, mixed)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_literal_arm_is_unreached_by_a_different_literal_only() -> Result<(), String> {
    let old = "            return \"nil\"";
    let source = "def kind(value):\n    match value:\n        case 0:\n            return \"zero\"\n        case 1:\n            return \"one\"\n";
    // The unchanged source ends on the changed `case 1` body line.
    assert_eq!(
        classify(
            source,
            old,
            KIND_IMPORT,
            "def test_kind():\n    assert kind(0) == \"zero\"\n"
        )?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(
            source,
            old,
            KIND_IMPORT,
            "def test_kind():\n    assert kind(1) == \"one\"\n"
        )?,
        ExposureClass::Exposed
    );
    // `True == 1`: the bool input takes `case 1`.
    assert_eq!(
        classify(
            source,
            old,
            KIND_IMPORT,
            "def test_kind():\n    assert kind(True) == \"one\"\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_unreadable_match_subject_or_pattern_fails_open() -> Result<(), String> {
    // A sequence pattern, a guard on an earlier arm and a rebound subject
    // leave the arm possibly reached.
    for source in [
        "def kind(value):\n    match value:\n        case [x, y]:\n            return \"pair\"\n        case _:\n            return \"misc\"\n",
        "def kind(value):\n    match value:\n        case int() if value > 3:\n            return \"big\"\n        case _:\n            return \"misc\"\n",
        "def kind(value):\n    value = str(value)\n    match value:\n        case int():\n            return \"int\"\n        case _:\n            return \"misc\"\n",
    ] {
        assert_eq!(
            classify(
                source,
                KIND_OLD,
                KIND_IMPORT,
                "def test_kind():\n    assert kind(5) == \"misc\"\n"
            )?,
            ExposureClass::Exposed,
            "{source}"
        );
    }
    Ok(())
}

const RATES_IMPORT: &str = "from src.shop import lookup";
const RATES_OLD: &str = "    return RATE_TABLE.get(tier, 0)";
const RATES: &str = "RATE_TABLE = {\"gold\": 10, \"silver\": 5}\n\n\ndef lookup(tier):\n    return RATE_TABLE.get(tier, 1)\n";

#[test]
fn a_get_default_every_observed_key_misses_does_not_credit() -> Result<(), String> {
    let present = "def test_lookup():\n    assert lookup(\"gold\") == 10\n    assert lookup(\"silver\") == 5\n";
    assert_eq!(
        classify(RATES, RATES_OLD, RATES_IMPORT, present)?,
        ExposureClass::WeaklyExposed
    );
    // An absent key runs the default.
    let absent = "def test_lookup():\n    assert lookup(\"bronze\") == 1\n";
    assert_eq!(
        classify(RATES, RATES_OLD, RATES_IMPORT, absent)?,
        ExposureClass::Exposed
    );
    let mixed = "def test_lookup():\n    assert lookup(\"gold\") == 10\n    assert lookup(\"bronze\") == 1\n";
    assert_eq!(
        classify(RATES, RATES_OLD, RATES_IMPORT, mixed)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_get_default_stays_credited_when_the_table_can_change() -> Result<(), String> {
    let present = "def test_lookup():\n    assert lookup(\"gold\") == 10\n";
    // A table the module stores into after the definition.
    let stored = RATES.replace(
        "\n\ndef lookup",
        "\nRATE_TABLE[\"bronze\"] = 3\n\n\ndef lookup",
    );
    assert_eq!(
        classify(&stored, RATES_OLD, RATES_IMPORT, present)?,
        ExposureClass::Exposed
    );
    // A test that touches the table.
    let patched =
        "def test_lookup():\n    RATE_TABLE.pop(\"gold\")\n    assert lookup(\"gold\") == 1\n";
    assert_eq!(
        classify(RATES, RATES_OLD, RATES_IMPORT, patched)?,
        ExposureClass::Exposed
    );
    // A table that is not one literal dict is not read.
    let computed = RATES.replace("{\"gold\": 10, \"silver\": 5}", "build_table()");
    assert_eq!(
        classify(&computed, RATES_OLD, RATES_IMPORT, present)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_table_the_owner_shadows_is_not_read() -> Result<(), String> {
    // `RATE_TABLE` is a parameter inside `lookup`: the module dict's keys say
    // nothing about the lookup, and `lookup("gold")` runs the default.
    let owner = "RATE_TABLE = {\"gold\": 10}\n\n\ndef lookup(tier, RATE_TABLE={}):\n    return RATE_TABLE.get(tier, 1)\n";
    assert_eq!(
        classify(
            owner,
            RATES_OLD,
            RATES_IMPORT,
            "def test_lookup():\n    assert lookup(\"gold\") == 1\n"
        )?,
        ExposureClass::Exposed
    );
    // A local rebinding before the lookup shadows it the same way.
    let local = "RATE_TABLE = {\"gold\": 10}\n\n\ndef lookup(tier):\n    RATE_TABLE = {}\n    return RATE_TABLE.get(tier, 1)\n";
    assert_eq!(
        classify(
            local,
            RATES_OLD,
            RATES_IMPORT,
            "def test_lookup():\n    assert lookup(\"gold\") == 1\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn only_an_unconditional_dict_literal_is_read_as_the_table() -> Result<(), String> {
    let present = "def test_lookup():\n    assert lookup(\"gold\") == 1\n";
    for definition in [
        "RATE_TABLE = {\"gold\": 10} if False else {}",
        "RATE_TABLE = {\"gold\": 10} | {}",
        "RATE_TABLE = {\"gold\": 10}; RATE_TABLE.pop(\"gold\")",
    ] {
        let owner =
            format!("{definition}\n\n\ndef lookup(tier):\n    return RATE_TABLE.get(tier, 1)\n");
        assert_eq!(
            classify(&owner, RATES_OLD, RATES_IMPORT, present)?,
            ExposureClass::Exposed,
            "{definition}"
        );
    }
    Ok(())
}

#[test]
fn a_rebound_builtin_class_is_not_read_as_the_builtin() -> Result<(), String> {
    // `int = str` makes `case int()` match strings, so `kind("x")` reaches
    // the changed arm.
    let owner = "int = str\n\n\ndef kind(value):\n    match value:\n        case int():\n            return \"string\"\n        case _:\n            return \"other\"\n";
    assert_eq!(
        classify_at(
            owner,
            7,
            "            return \"text\"",
            KIND_IMPORT,
            "def test_kind():\n    assert kind(\"x\") == \"string\"\n"
        )?,
        ExposureClass::Exposed
    );
    // The unrebound builtin excludes the same call: `"x"` is not an int.
    let intact = owner.replace("int = str\n\n\n", "");
    assert_eq!(
        classify_at(
            &intact,
            4,
            "            return \"text\"",
            KIND_IMPORT,
            "def test_kind():\n    assert kind(\"x\") == \"other\"\n"
        )?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn string_escape_spellings_are_not_compared_as_text() -> Result<(), String> {
    // `"\n"` and `"\x0a"` are one string, so the call reaches `case "\n"`.
    let owner = "def kind(value):\n    match value:\n        case \"\\n\":\n            return \"newline\"\n";
    assert_eq!(
        classify(
            owner,
            "            return \"nl\"",
            KIND_IMPORT,
            "def test_kind():\n    assert kind(\"\\x0a\") == \"newline\"\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_non_ascii_table_name_fails_open_instead_of_panicking() -> Result<(), String> {
    // A multi-byte identifier character delimits the table name here; slicing
    // past `position + 1` would split it and panic mid-analysis.
    let owner = "größe = {\"gold\": 10}\n\n\ndef lookup(tier):\n    return größe.get(tier, 1)\n";
    assert_eq!(
        classify(
            owner,
            "    return größe.get(tier, 0)",
            RATES_IMPORT,
            "def test_lookup():\n    assert lookup(\"gold\") == 10\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const INVOICE_IMPORT: &str = "from src.shop import Invoice";
const INVOICE_OLD: &str = "        return cls(0, 0.0)";
const INVOICE_HEAD: &str = "from dataclasses import dataclass\n\n\n@dataclass\nclass Invoice:\n    amount: int\n    tax_rate: float = 0.2\n\n    def total(self):\n        return round(self.amount * (1 + self.tax_rate))\n\n    def rate(self):\n        return self.tax_rate\n\n    @classmethod\n    def free(cls):\n        return cls(0, 0.2)\n";

#[test]
fn a_chained_method_that_masks_the_edited_field_does_not_credit() -> Result<(), String> {
    // `total()` prices an amount of 0 to 0 at any rate.
    let masked = "def test_free():\n    assert Invoice.free().total() == 0\n";
    assert_eq!(
        classify(INVOICE_HEAD, INVOICE_OLD, INVOICE_IMPORT, masked)?,
        ExposureClass::WeaklyExposed
    );
    // Control: a method that returns the edited state directly observes it.
    let getter = "def test_free():\n    assert Invoice.free().rate() == 0.2\n";
    assert_eq!(
        classify(INVOICE_HEAD, INVOICE_OLD, INVOICE_IMPORT, getter)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_chained_method_rule_fails_open_when_the_result_is_read_otherwise() -> Result<(), String> {
    // A direct attribute read observes the edited field.
    let attribute = "def test_free():\n    assert Invoice.free().tax_rate == 0.2\n";
    assert_eq!(
        classify(INVOICE_HEAD, INVOICE_OLD, INVOICE_IMPORT, attribute)?,
        ExposureClass::Exposed
    );
    // One strong test that reads the field directly keeps the credit even
    // when another reads only through the masking method.
    let mixed = "def test_free_total():\n    assert Invoice.free().total() == 0\n\n\ndef test_free_rate():\n    assert Invoice.free().rate() == 0.2\n";
    assert_eq!(
        classify(INVOICE_HEAD, INVOICE_OLD, INVOICE_IMPORT, mixed)?,
        ExposureClass::Exposed
    );
    // A class that is not a plain dataclass is not read.
    let plain = INVOICE_HEAD.replace("@dataclass\n", "");
    let masked = "def test_free():\n    assert Invoice.free().total() == 0\n";
    assert_eq!(
        classify(&plain, INVOICE_OLD, INVOICE_IMPORT, masked)?,
        ExposureClass::Exposed
    );
    Ok(())
}
