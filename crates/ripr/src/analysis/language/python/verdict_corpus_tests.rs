//! Positive and negative pins for the Python verdict rules the verdict
//! corpus found missing (#6599, #6600, #6601; RIPR-SPEC-0233).
//!
//! Each rule gets a test that must credit `exposed` and an alternate that
//! must not, so a rule that stops firing and a rule that over-fires both fail
//! here before they reach the corpus rates.

use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_FILE: &str = "src/shop.py";
const TEST_FILE: &str = "tests/test_shop.py";

/// Classify a one-line change in `owner` against `test_source`.
fn classify(
    owner: &str,
    line: usize,
    old_line: &str,
    test_source: &str,
) -> Result<ExposureClass, String> {
    let owners = extract_owners(Path::new(OWNER_FILE), owner);
    let tests = extract_tests(Path::new(TEST_FILE), test_source);
    let new_line = owner
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("owner has no line {line}"))?;
    classify_change_with_old(
        Path::new(OWNER_FILE),
        line,
        new_line,
        Some(old_line),
        &owners,
        &tests,
    )
    .map(|finding| finding.class)
    .ok_or_else(|| "changed line should classify".to_string())
}

fn test_module(imports: &str, body: &str) -> String {
    format!("{imports}\n\n\n{body}")
}

const FEE: &str = "def fee(amount):\n    return amount * 3\n\n\ndef charge(amount):\n    return amount + fee(amount)\n";
const FEE_OLD: &str = "    return amount * 2";
const FEE_IMPORT: &str = "from src.shop import charge, fee";

fn fee_class(body: &str) -> Result<ExposureClass, String> {
    classify(FEE, 2, FEE_OLD, &test_module(FEE_IMPORT, body))
}

#[test]
fn an_owner_compared_with_itself_does_not_credit() -> Result<(), String> {
    // Rule 1: both operands change together, so the assertion cannot fail.
    assert_ne!(
        fee_class("def test_fee():\n    assert fee(10) == fee(10)\n")?,
        ExposureClass::Exposed
    );
    assert_eq!(
        fee_class("def test_fee():\n    assert fee(10) == 30\n")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_and_chain_asserts_each_conjunct_but_an_or_chain_does_not() -> Result<(), String> {
    assert_eq!(
        fee_class("def test_fee():\n    assert fee(10) == 30 and fee(0) == 0\n")?,
        ExposureClass::Exposed
    );
    assert_ne!(
        fee_class("def test_fee():\n    assert fee(10) == 30 or fee(0) == 0\n")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn the_crediting_assertion_must_itself_observe_the_owner() -> Result<(), String> {
    // Rule 4: credit does not depend on assertion order, and an exact
    // assertion on something else does not lend its strength to a weak one
    // on the owner.
    assert_eq!(
        fee_class("def test_fee():\n    assert fee(10) > 0\n    assert fee(10) == 30\n")?,
        ExposureClass::Exposed
    );
    assert_ne!(
        fee_class("def test_fee():\n    assert str(1) == \"1\"\n    assert fee(10) > 0\n")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_self_computed_expected_value_does_not_credit() -> Result<(), String> {
    // RIPR-SPEC-0035: the expected side recomputes the changed owner, so a
    // caller's result agrees with it before and after the change.
    assert_ne!(
        fee_class("def test_charge():\n    assert charge(10) == 10 + fee(10)\n")?,
        ExposureClass::Exposed
    );
    assert_eq!(
        fee_class("def test_fee():\n    assert fee(10) + 1 == 31\n")?,
        ExposureClass::Exposed
    );
    Ok(())
}

const PARSE: &str = "def parse(text):\n    if not text:\n        raise ValueError(\"empty input\")\n    return int(text)\n";
const PARSE_OLD: &str = "        raise KeyError(\"empty input\")";

fn parse_class(body: &str) -> Result<ExposureClass, String> {
    classify(
        PARSE,
        3,
        PARSE_OLD,
        &test_module("import pytest\n\nfrom src.shop import parse", body),
    )
}

#[test]
fn a_raises_match_that_admits_any_message_is_broad() -> Result<(), String> {
    // Rules 2 and 6: `match=".*"` checks no message and the error-path gate
    // takes only an exact error variant.
    assert_eq!(
        parse_class(
            "def test_parse():\n    with pytest.raises(ValueError, match=\"empty\"):\n        parse(\"\")\n"
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(
        parse_class(
            "def test_parse():\n    with pytest.raises(Exception, match=\".*\"):\n        parse(\"\")\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const CART: &str =
    "class Cart:\n    LABEL = \"cart\"\n\n    def total(self, n):\n        return n * 3\n";
const CART_OLD: &str = "        return n * 2";

fn cart_class(body: &str) -> Result<ExposureClass, String> {
    classify(
        CART,
        5,
        CART_OLD,
        &test_module("from src.shop import Cart", body),
    )
}

#[test]
fn a_method_owner_is_credited_through_a_bound_receiver_not_its_class_name() -> Result<(), String> {
    // Rule 9: `Cart` alone does not name `Cart.total`.
    assert_eq!(
        cart_class("def test_total():\n    cart = Cart()\n    assert cart.total(3) == 9\n")?,
        ExposureClass::Exposed
    );
    assert_ne!(
        cart_class(
            "def test_total():\n    cart = Cart()\n    cart.total(3)\n    assert Cart.LABEL == \"cart\"\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const LIMIT: &str = "LIMIT = 5000\n\n\ndef shipping_fee(subtotal):\n    if subtotal > LIMIT - 1:\n        return 0\n    return 499\n";
const LIMIT_OLD: &str = "    if subtotal >= LIMIT:";

fn limit_class(body: &str) -> Result<ExposureClass, String> {
    classify(
        LIMIT,
        5,
        LIMIT_OLD,
        &test_module("from src.shop import shipping_fee", body),
    )
}

#[test]
fn an_offset_constant_boundary_is_pinned_only_at_its_offset_value() -> Result<(), String> {
    assert_eq!(
        limit_class("def test_fee():\n    assert shipping_fee(4999) == 499\n")?,
        ExposureClass::Exposed
    );
    assert_ne!(
        limit_class("def test_fee():\n    assert shipping_fee(6000) == 0\n")?,
        ExposureClass::Exposed
    );
    Ok(())
}

const GREET: &str = "def greet(name):\n    print(f\"hello {name}\")\n";
const GREET_OLD: &str = "    print(f\"hi {name}\")";

#[test]
fn captured_stdout_observes_a_changed_print() -> Result<(), String> {
    let imports = "from src.shop import greet";
    assert_eq!(
        classify(
            GREET,
            2,
            GREET_OLD,
            &test_module(
                imports,
                "def test_greet(capsys):\n    greet(\"a\")\n    assert capsys.readouterr().out == \"hello a\\n\"\n"
            )
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(
        classify(
            GREET,
            2,
            GREET_OLD,
            &test_module(
                imports,
                "def test_greet():\n    assert greet(\"a\") is None\n"
            )
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const SAVE: &str = "def save(path, text):\n    path.write_text(text.lower())\n";
const SAVE_OLD: &str = "    path.write_text(text.upper())";

#[test]
fn reading_the_written_file_observes_a_changed_write() -> Result<(), String> {
    let imports = "from src.shop import save";
    assert_eq!(
        classify(
            SAVE,
            2,
            SAVE_OLD,
            &test_module(
                imports,
                "def test_save(tmp_path):\n    target = tmp_path / \"f\"\n    save(target, \"A\")\n    assert target.read_text() == \"a\"\n"
            )
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(
        classify(
            SAVE,
            2,
            SAVE_OLD,
            &test_module(
                imports,
                "def test_save(tmp_path):\n    target = tmp_path / \"f\"\n    save(target, \"A\")\n    assert target.exists()\n"
            )
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const NOTIFY: &str = "def notify(client, user):\n    client.send(user, \"welcome\")\n";
const NOTIFY_OLD: &str = "    client.send(user, \"hi\")";

#[test]
fn a_mock_call_assertion_with_arguments_pins_the_changed_call() -> Result<(), String> {
    let imports = "from unittest.mock import Mock\n\nfrom src.shop import notify";
    assert_eq!(
        classify(
            NOTIFY,
            2,
            NOTIFY_OLD,
            &test_module(
                imports,
                "def test_notify():\n    client = Mock()\n    notify(client, \"u\")\n    client.send.assert_called_once_with(\"u\", \"welcome\")\n"
            )
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(
        classify(
            NOTIFY,
            2,
            NOTIFY_OLD,
            &test_module(
                imports,
                "def test_notify():\n    client = Mock()\n    notify(client, \"u\")\n    client.send.assert_called_once()\n"
            )
        )?,
        ExposureClass::Exposed
    );
    let imports = "from unittest.mock import ANY, Mock\n\nfrom src.shop import notify";
    let notify_test = |check: &str| {
        classify(
            NOTIFY,
            2,
            NOTIFY_OLD,
            &test_module(
                imports,
                &format!(
                    "def test_notify():\n    client = Mock()\n    notify(client, \"u\")\n    client.send.{check}\n"
                ),
            ),
        )
    };
    assert_eq!(
        notify_test("assert_any_call(\"u\", \"welcome\")")?,
        ExposureClass::Exposed
    );
    // `ANY` and a starred argument accept whatever the changed call passes.
    assert_ne!(
        notify_test("assert_called_once_with(\"u\", ANY)")?,
        ExposureClass::Exposed
    );
    assert_ne!(
        notify_test("assert_called_with(*expected)")?,
        ExposureClass::Exposed
    );
    Ok(())
}

const COUNTER: &str = "class Counter:\n    def __init__(self):\n        self.count = 0\n\n    def bump(self):\n        self.count = self.count + 2\n";
const COUNTER_OLD: &str = "        self.count = self.count + 1";

#[test]
fn a_bound_local_reads_a_changed_self_field() -> Result<(), String> {
    let imports = "from src.shop import Counter";
    assert_eq!(
        classify(
            COUNTER,
            6,
            COUNTER_OLD,
            &test_module(
                imports,
                "def test_bump():\n    counter = Counter()\n    counter.bump()\n    assert counter.count == 2\n"
            )
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(
        classify(
            COUNTER,
            6,
            COUNTER_OLD,
            &test_module(
                imports,
                "def test_bump():\n    counter = Counter()\n    counter.bump()\n    assert counter.count > 0\n"
            )
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const PASSED: &str = "def passed(score):\n    return score >= 50\n";
const PASSED_OLD: &str = "    return score > 50";
const HYPOTHESIS_IMPORTS: &str = "from hypothesis import example, given\nfrom hypothesis import strategies as st\n\nfrom src.shop import passed";

fn passed_class(body: &str) -> Result<ExposureClass, String> {
    classify(
        PASSED,
        2,
        PASSED_OLD,
        &test_module(HYPOTHESIS_IMPORTS, body),
    )
}

#[test]
fn given_parameters_are_generated_inputs_not_fixtures() {
    let tests = extract_tests(
        Path::new(TEST_FILE),
        &test_module(
            HYPOTHESIS_IMPORTS,
            "@given(st.integers(0, 100))\ndef test_passed(tmp_path, score):\n    assert passed(score) in (True, False)\n",
        ),
    );
    let test = tests.first();
    assert_eq!(
        test.map(|test| test.fixtures.clone()),
        Some(vec!["tmp_path".to_string()])
    );
    assert_eq!(
        test.map(|test| test.generated_inputs.clone()),
        Some(vec!["score".to_string()])
    );
}

#[test]
fn example_rows_pin_a_boundary_that_generated_inputs_cannot() -> Result<(), String> {
    assert_eq!(
        passed_class(
            "@given(st.integers(0, 100))\n@example(49)\n@example(50)\ndef test_passed(score):\n    assert passed(score) == (score >= 50)\n"
        )?,
        ExposureClass::Exposed
    );
    assert_eq!(
        passed_class(
            "@given(st.integers(0, 100))\ndef test_passed(score):\n    assert passed(score) == (score >= 50)\n"
        )?,
        ExposureClass::StaticUnknown
    );
    Ok(())
}

#[test]
fn a_given_test_without_assertions_reads_as_a_gap_not_a_limit() -> Result<(), String> {
    // `py-hypothesis-label-no-crash`: no oracle for the generated input to
    // hide, so the finding keeps its actionable gap verdict.
    assert_eq!(
        passed_class("@given(st.integers(0, 100))\ndef test_passed(score):\n    passed(score)\n")?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}
