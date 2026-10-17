//! Positive and negative pins for the Python verdict rules the verdict
//! corpus found missing (#6599, #6600, #6601; RIPR-SPEC-0238).
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
    assert_ne!(
        fee_class("def test_fee():\n    assert fee(10) == fee(10) == fee(10)\n")?,
        ExposureClass::Exposed
    );
    // A chain with one repeated link still pins the result through the other.
    assert_eq!(
        fee_class("def test_fee():\n    assert fee(10) == fee(10) == 30\n")?,
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
    // `assert_any_call` and `assert_called_with` can be met by another call the
    // owner makes on the same method.
    assert_ne!(
        notify_test("assert_any_call(\"u\", \"welcome\")")?,
        ExposureClass::Exposed
    );
    assert_ne!(
        notify_test("assert_called_with(\"u\", \"welcome\")")?,
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

#[test]
fn a_read_before_the_owner_call_does_not_observe_its_effect() -> Result<(), String> {
    // The field and the captured output are read before the changed method
    // or print runs, so the assertion sees the earlier state on both sides
    // of the change.
    assert_ne!(
        classify(
            COUNTER,
            6,
            COUNTER_OLD,
            &test_module(
                "from src.shop import Counter",
                "def test_bump():\n    counter = Counter()\n    assert counter.count == 0\n    counter.bump()\n"
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
                "from src.shop import greet",
                "def test_greet(capsys):\n    assert capsys.readouterr().out == \"\"\n    greet(\"a\")\n"
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

#[test]
fn a_weak_conjunct_on_the_owner_does_not_borrow_a_strong_conjunct() -> Result<(), String> {
    // Rule 15: only the strongest conjuncts are read, and `other == 5` does
    // not name the owner.
    assert_ne!(
        fee_class("def test_fee():\n    other = 5\n    assert fee(10) > 0 and other == 5\n")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn non_ascii_identifiers_in_an_assertion_classify() -> Result<(), String> {
    // A byte-wise split must not slice inside a multi-byte character.
    assert_eq!(
        fee_class("def test_fee():\n    größe = 30\n    assert fee(10) == größe\n")?,
        ExposureClass::Exposed
    );
    Ok(())
}

const AUDIT: &str =
    "def notify(client, audit, user):\n    audit.send(user)\n    client.send(user, \"welcome\")\n";
const AUDIT_OLD: &str = "    client.send(user, \"hi\")";

#[test]
fn a_mock_assertion_must_read_the_collaborator_the_changed_line_calls() -> Result<(), String> {
    let imports = "from unittest.mock import Mock\n\nfrom src.shop import notify";
    let class = |assertion: &str| {
        classify(
            AUDIT,
            3,
            AUDIT_OLD,
            &test_module(
                imports,
                &format!(
                    "def test_notify():\n    client = Mock()\n    audit = Mock()\n    notify(client, audit, \"u\")\n    {assertion}\n"
                ),
            ),
        )
    };
    assert_eq!(
        class("client.send.assert_called_once_with(\"u\", \"welcome\")")?,
        ExposureClass::Exposed
    );
    assert_ne!(
        class("audit.send.assert_called_once_with(\"u\")")?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_capture_taken_before_the_owner_runs_does_not_observe_it() -> Result<(), String> {
    let imports = "from src.shop import greet";
    assert_ne!(
        classify(
            GREET,
            2,
            GREET_OLD,
            &test_module(
                imports,
                "def test_greet(capsys):\n    captured = capsys.readouterr()\n    greet(\"a\")\n    assert captured.out == \"\"\n"
            )
        )?,
        ExposureClass::Exposed
    );
    assert_eq!(
        classify(
            GREET,
            2,
            GREET_OLD,
            &test_module(
                imports,
                "def test_greet(capsys):\n    greet(\"a\")\n    captured = capsys.readouterr()\n    assert captured.out == \"hello a\\n\"\n"
            )
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const PRINTER: &str = "class Printer:\n    def show(self, n):\n        print(n * 2)\n";
const PRINTER_OLD: &str = "        print(n * 3)";

#[test]
fn an_effect_of_a_method_owner_needs_a_receiver_bound_to_its_class() -> Result<(), String> {
    let imports = "from src.shop import Printer";
    let class = |body: &str| classify(PRINTER, 3, PRINTER_OLD, &test_module(imports, body));
    assert_eq!(
        class(
            "def test_show(capsys):\n    printer = Printer()\n    printer.show(2)\n    assert capsys.readouterr().out == \"4\\n\"\n"
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(
        class(
            "class Fake:\n    def show(self, n):\n        print(n)\n\n\ndef test_show(capsys):\n    Fake().show(4)\n    assert capsys.readouterr().out == \"4\\n\"\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_match_pattern_bound_to_a_name_or_keyword_is_read() -> Result<(), String> {
    assert_ne!(
        parse_class(
            "def test_parse():\n    pattern = \".*\"\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n"
        )?,
        ExposureClass::Exposed
    );
    assert_eq!(
        parse_class(
            "def test_parse():\n    pattern = \"empty\"\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n"
        )?,
        ExposureClass::Exposed
    );
    let unittest = |pattern: &str| {
        classify(
            PARSE,
            3,
            PARSE_OLD,
            &test_module(
                "import unittest\n\nfrom src.shop import parse",
                &format!(
                    "class ParseTest(unittest.TestCase):\n    def test_parse(self):\n        with self.assertRaisesRegex(ValueError, expected_regex={pattern}):\n            parse(\"\")\n"
                ),
            ),
        )
    };
    assert_ne!(unittest("\".*\"")?, ExposureClass::Exposed);
    assert_eq!(unittest("\"empty\"")?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_literal_none_match_checks_no_message_in_either_framework() -> Result<(), String> {
    // #7849: `match=None` and `assertRaisesRegex(E, None)` disable message
    // matching, so only the exception class is pinned. Exact patterns and an
    // omitted `match=` keep their existing verdicts.
    assert_ne!(
        parse_class(
            "def test_parse():\n    with pytest.raises(ValueError, match=None):\n        parse(\"\")\n"
        )?,
        ExposureClass::Exposed
    );
    let unittest = |pattern: &str| {
        classify(
            PARSE,
            3,
            PARSE_OLD,
            &test_module(
                "import unittest\n\nfrom src.shop import parse",
                &format!(
                    "class ParseTest(unittest.TestCase):\n    def test_parse(self):\n        with self.assertRaisesRegex(ValueError, {pattern}):\n            parse(\"\")\n"
                ),
            ),
        )
    };
    // A name bound exactly once to `None` reads as `None`.
    assert_ne!(
        parse_class(
            "def test_parse():\n    pattern = None\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n"
        )?,
        ExposureClass::Exposed
    );
    // A second binding is not a single `None` assignment: the message
    // stays checked.
    assert_eq!(
        parse_class(
            "def test_parse():\n    pattern = None\n    pattern = \"empty\"\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n"
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(unittest("None")?, ExposureClass::Exposed);
    assert_ne!(unittest("expected_regex=None")?, ExposureClass::Exposed);
    assert_eq!(unittest("\"empty\"")?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_match_name_is_read_from_its_own_function_not_the_whole_file() -> Result<(), String> {
    // The pattern name binds in the asserting function (plus module level),
    // so a same-named assignment in another function neither hides it nor
    // supplies it.
    assert_ne!(
        parse_class(
            "def test_parse():\n    pattern = None\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n\n\ndef test_other():\n    pattern = \"empty\"\n"
        )?,
        ExposureClass::Exposed
    );
    let parametrized = |extra: &str| {
        parse_class(&format!(
            "@pytest.mark.parametrize(\"pattern\", [\"empty\"])\ndef test_parse(pattern):\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n{extra}"
        ))
    };
    assert_eq!(
        parametrized("")?,
        parametrized("\n\ndef test_other():\n    pattern = \".*\"\n")?
    );
    Ok(())
}

#[test]
fn a_scoped_match_name_survives_signatures_line_endings_and_nesting() -> Result<(), String> {
    let broad = |body: &str| -> Result<(), String> {
        assert_ne!(parse_class(body)?, ExposureClass::Exposed, "{body}");
        Ok(())
    };
    // A `#` inside a default string and a colon inside a default dict do not
    // end the signature early and hide the body's binding.
    broad(
        "def test_parse(tag=\"#\", opts={\"k\": 1}):\n    pattern = \".*\"\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n",
    )?;
    // CRLF line endings keep the byte offsets aligned.
    broad(
        &"def test_other():\n    pass\n\n\ndef test_parse():\n    pattern = \".*\"\n    with pytest.raises(ValueError, match=pattern):\n        parse(\"\")\n"
            .replace('\n', "\r\n"),
    )?;
    // A nested def keeps reading the enclosing function's binding.
    broad(
        "def test_parse():\n    pattern = \".*\"\n\n    def check():\n        with pytest.raises(ValueError, match=pattern):\n            parse(\"\")\n\n    check()\n",
    )?;
    Ok(())
}

#[test]
fn a_unittest_method_reads_its_own_match_name_inside_its_class() -> Result<(), String> {
    // A class body does not make a method nested, so the method's own
    // binding is read and a sibling method's binding is not.
    let class = classify(
        PARSE,
        3,
        PARSE_OLD,
        &test_module(
            "import unittest\n\nfrom src.shop import parse",
            "class ParseTest(unittest.TestCase):\n    def test_parse(self):\n        pattern = None\n        with self.assertRaisesRegex(ValueError, pattern):\n            parse(\"\")\n\n    def test_other(self):\n        pattern = \"empty\"\n",
        ),
    )?;
    assert_ne!(class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_parametrize_mark_does_not_stand_in_for_example_rows() -> Result<(), String> {
    assert_eq!(
        passed_class(
            "import pytest\n\n\n@pytest.mark.parametrize(\"bonus\", [0])\n@given(st.integers(0, 100))\ndef test_passed(bonus, score):\n    assert passed(score + bonus) == (score >= 50)\n"
        )?,
        ExposureClass::StaticUnknown
    );
    Ok(())
}

#[test]
fn an_unreadable_example_row_does_not_lift_the_property_limit() -> Result<(), String> {
    // `@example(*[49])` binds nothing ripr reads, so the boundary is still
    // fed only by the strategy and the property-based limit stays.
    assert_eq!(
        passed_class(
            "@given(st.integers(0, 100))\n@example(*[49])\ndef test_passed(score):\n    assert passed(score) == (score >= 50)\n"
        )?,
        ExposureClass::StaticUnknown
    );
    Ok(())
}

#[test]
fn an_effect_observation_reports_its_own_alignment_reason() -> Result<(), String> {
    // Acceptance example 36: the expected `'4\n'` holds no changed-line
    // token, so only the effect branch credits.
    let owner = "def show(n):\n    print(n * 2)\n";
    let owners = extract_owners(Path::new(OWNER_FILE), owner);
    let tests = extract_tests(
        Path::new(TEST_FILE),
        &test_module(
            "from src.shop import show",
            "def test_show(capsys):\n    show(2)\n    assert capsys.readouterr().out == \"4\\n\"\n",
        ),
    );
    let finding = classify_change_with_old(
        Path::new(OWNER_FILE),
        2,
        "    print(n * 2)",
        Some("    print(n * 3)"),
        &owners,
        &tests,
    )
    .ok_or_else(|| "changed print should classify".to_string())?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(
        finding.alignment_reason.as_deref(),
        Some("strong_oracle_observes_changed_effect_channel")
    );
    Ok(())
}

#[test]
fn an_effect_needs_the_owner_from_its_own_module() -> Result<(), String> {
    // A same-named `greet` imported from another module prints its own text.
    assert_ne!(
        classify(
            GREET,
            2,
            GREET_OLD,
            &test_module(
                "from src.other import greet",
                "def test_greet(capsys):\n    greet(\"a\")\n    assert capsys.readouterr().out == \"hello a\\n\"\n"
            )
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const DOUBLE: &str = "def double(x):\n    if x < 0:\n        raise ValueError(\"neg\")\n    return x * 3\n\n\ndef triple(x):\n    return x * 3\n";
const DOUBLE_OLD: &str = "    return x * 2";

#[test]
fn an_assertion_of_another_behavior_family_does_not_credit() -> Result<(), String> {
    // RIPR-SPEC-0224 still filters rule 4's units: the `pytest.raises` call
    // names the owner, but it runs only the raise, never the changed return.
    let imports = "import pytest\n\nfrom src.shop import double, triple";
    assert_eq!(
        classify(
            DOUBLE,
            4,
            DOUBLE_OLD,
            &test_module(
                imports,
                "def test_calc():\n    assert triple(1) == 3\n    with pytest.raises(ValueError, match=\"neg\"):\n        double(-1)\n"
            )
        )?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        classify(
            DOUBLE,
            4,
            DOUBLE_OLD,
            &test_module(
                imports,
                "def test_calc():\n    assert double(1) == 3\n    with pytest.raises(ValueError, match=\"neg\"):\n        double(-1)\n"
            )
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn credit_comes_from_one_owner_observing_assertion_in_any_position() -> Result<(), String> {
    // Rule 4: an unrelated strong assertion before or after the crediting one
    // changes nothing.
    for body in [
        "def test_fee():\n    other = 5\n    assert fee(10) == 30\n    assert other == 5\n",
        "def test_fee():\n    other = 5\n    assert other == 5\n    assert fee(10) == 30\n",
    ] {
        assert_eq!(fee_class(body)?, ExposureClass::Exposed);
    }
    Ok(())
}

#[test]
fn an_assert_message_and_a_mixed_chain_do_not_credit() -> Result<(), String> {
    // Rule 4 gate text drops the failure message; rule 1 weakens a chain
    // that mixes `==` with another operator.
    for body in [
        "def test_fee():\n    other = 5\n    assert other == 5, fee(10)\n",
        "def test_fee():\n    assert int('1') == 1 < fee(10)\n",
        "import unittest\n\n\nclass FeeTest(unittest.TestCase):\n    def test_fee(self):\n        self.assertEqual(fee(10), fee(10))\n",
    ] {
        assert_ne!(fee_class(body)?, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

const SHOW: &str = "def show(n):\n    print(n * 2)\n";
const SHOW_OLD: &str = "    print(n * 3)";

#[test]
fn a_capture_bound_after_the_owner_call_observes_its_output() -> Result<(), String> {
    // The expected `"4\n"` holds no changed-line token, so only the effect
    // channel can credit it.
    let imports = "from src.shop import show";
    assert_eq!(
        classify(
            SHOW,
            2,
            SHOW_OLD,
            &test_module(
                imports,
                "def test_show(capsys):\n    show(2)\n    captured = capsys.readouterr()\n    assert captured.out == \"4\\n\"\n"
            )
        )?,
        ExposureClass::Exposed
    );
    assert_ne!(
        classify(
            SHOW,
            2,
            SHOW_OLD,
            &test_module(
                imports,
                "def test_show(capsys):\n    captured = capsys.readouterr()\n    show(2)\n    assert captured.out == \"4\\n\"\n"
            )
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

fn unittest_fee_class(body: &str) -> Result<ExposureClass, String> {
    fee_class(&format!(
        "import unittest\n\n\nclass FeeTest(unittest.TestCase):\n    def test_fee(self):\n        {body}\n"
    ))
}

#[test]
fn assert_almost_equal_at_its_default_tolerance_pins_the_value() -> Result<(), String> {
    // Rule 12 (#6603): at seven places the assertion fails for any change of
    // at least 1e-7; an explicit tolerance band may admit the old value.
    for body in [
        "self.assertAlmostEqual(fee(10), 30.0)",
        "self.assertAlmostEqual(fee(10), 30.0, places=None)",
        "self.assertAlmostEqual(fee(10), 30.0, None, \"msg\", None)",
    ] {
        assert_eq!(unittest_fee_class(body)?, ExposureClass::Exposed, "{body}");
    }
    for body in [
        "self.assertAlmostEqual(fee(10), 30.0, places=0)",
        "self.assertAlmostEqual(fee(10), 30.0, delta=100)",
        "self.assertAlmostEqual(fee(10), 30.0, 0)",
        "self.assertAlmostEqual(fee(10), 30.0, None, \"msg\", 100)",
        "checks.assertAlmostEqual(fee(10), 30.0)",
        "self.assertNotAlmostEqual(fee(10), 20.0)",
        "self.assertAlmostEqual(fee(10), fee(10))",
    ] {
        assert_ne!(unittest_fee_class(body)?, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_fluent_helper_chain_is_an_opaque_custom_helper() -> Result<(), String> {
    // Rule 3 (#6603): the helper body is not read, so the verdict abstains
    // rather than reporting a gap the helper may already close.
    let imports = "from src.shop import charge, fee\nfrom tests.fluent import assert_that";
    let class = |body: &str| classify(FEE, 2, FEE_OLD, &test_module(imports, body));
    assert_eq!(
        class("def test_fee():\n    assert_that(fee(10)).is_equal_to(30)\n")?,
        ExposureClass::StaticUnknown
    );
    // A chain with no assertion-helper call is still no assertion.
    assert_eq!(
        class("def test_fee():\n    print_that(fee(10)).is_equal_to(30)\n")?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

const MESSAGE: &str = "def parse(text):\n    if not text:\n        raise KeyError(\"blank\")\n    return int(text)\n\n\ndef other(text):\n    if not text:\n        raise KeyError(\"blank\")\n    return int(text)\n";
const MESSAGE_OLD: &str = "        raise KeyError(\"empty\")";

fn message_class(body: &str) -> Result<ExposureClass, String> {
    classify(
        MESSAGE,
        3,
        MESSAGE_OLD,
        &test_module(
            "import unittest\n\nimport pytest\n\nfrom src.shop import other, parse",
            body,
        ),
    )
}

fn unittest_message_class(body: &str) -> Result<ExposureClass, String> {
    message_class(&format!(
        "class ParseTest(unittest.TestCase):\n    def test_parse(self):\n{body}\n"
    ))
}

#[test]
fn the_callable_of_an_exception_assertion_is_called() -> Result<(), String> {
    // Rule 13 (#6603): the assertion calls the callable it is passed.
    for body in [
        "        self.assertRaisesRegex(KeyError, \"empty\", parse, \"\")",
        "        self.assertRaisesRegex(KeyError, \"blank\", parse, \"\")",
    ] {
        assert_eq!(
            unittest_message_class(body)?,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    for body in [
        // Only the class is pinned.
        "        self.assertRaises(KeyError, parse, \"\")",
        "        self.assertRaisesRegex(KeyError, \".*\", parse, \"\")",
        // Another function is called, so the owner is not reached.
        "        self.assertRaisesRegex(KeyError, \"empty\", other, \"\")",
    ] {
        assert_ne!(
            unittest_message_class(body)?,
            ExposureClass::Exposed,
            "{body}"
        );
    }
    // pytest passes `match=` to the callable in this form, whatever the
    // callable is.
    for body in [
        "def test_parse():\n    pytest.raises(KeyError, parse, \"\", match=\"empty\")\n",
        "def test_parse():\n    pytest.raises(KeyError, lambda: parse(\"\"), match=\"empty\")\n",
    ] {
        assert_ne!(message_class(body)?, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

#[test]
fn a_bound_exception_compared_by_value_pins_the_error() -> Result<(), String> {
    // Rule 14 (#6603): the comparison after the `with` reads the raised error.
    for body in [
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    assert str(exc.value) == \"'empty'\"\n",
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    assert exc.value.args[0] == \"blank\"\n",
        "class ParseTest(unittest.TestCase):\n    def test_parse(self):\n        with self.assertRaises(KeyError) as cm:\n            parse(\"\")\n        self.assertEqual(str(cm.exception), \"'empty'\")\n",
        "class ParseTest(unittest.TestCase):\n    def test_parse(self):\n        with self.assertRaisesRegex(KeyError, \"e\") as cm:\n            parse(\"\")\n        self.assertEqual(repr(cm.exception), \"KeyError('empty')\")\n",
    ] {
        assert_eq!(message_class(body)?, ExposureClass::Exposed, "{body}");
    }
    for body in [
        // A coarser read does not pin the message.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    assert len(str(exc.value)) == 7\n",
        // A comparison inside the body runs only when nothing is raised.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n        assert str(exc.value) == \"'empty'\"\n",
        // The exception is rebound before the comparison.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    exc = None\n    assert str(exc.value) == \"'empty'\"\n",
        // An attribute reached through the exception is assigned.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    exc.value.args = (\"empty\",)\n    assert str(exc.value) == \"'empty'\"\n",
        // A tuple target, a walrus or a method call rebinds or mutates it.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    exc, expected = other_error, \"'empty'\"\n    assert str(exc.value) == expected\n",
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    print(exc := other_error)\n    assert str(exc.value) == \"'empty'\"\n",
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    setattr(exc.value, \"args\", (\"empty\",))\n    assert str(exc.value) == \"'empty'\"\n",
        // A method call in an assigned value, a call passed the error inside
        // an assert, or an alias of the error.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    _ = exc.value.__init__(\"empty\")\n    assert str(exc.value) == \"'empty'\"\n",
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    assert setattr(exc.value, \"args\", (\"empty\",)) is None\n    assert str(exc.value) == \"'empty'\"\n",
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    msg = str(exc.value)\n    assert str(exc.value) == msg\n",
        // The other operand reads the error too, so the comparison can hold
        // for any message.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    assert str(exc.value) == repr(exc.value)\n",
        // unittest's field is `exception`, and the receiver must be `self`.
        "class ParseTest(unittest.TestCase):\n    def test_parse(self):\n        with self.assertRaises(KeyError) as cm:\n            parse(\"\")\n        self.assertEqual(str(cm.value), \"'empty'\")\n",
        // `is not None` is not a value comparison.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    assert exc.value is not None\n",
        // A tautology does not promote.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    assert str(exc.value) == str(exc.value)\n",
        // Another bound name.
        "def test_parse():\n    with pytest.raises(KeyError) as exc:\n        parse(\"\")\n    err = KeyError(\"x\")\n    assert str(err) == \"'empty'\"\n",
    ] {
        assert_ne!(message_class(body)?, ExposureClass::Exposed, "{body}");
    }
    Ok(())
}

const QUOTE: &str = "FREE_SHIPPING_CENTS = 5000\n\n\ndef shipping_fee(subtotal_cents):\n    \"\"\"Orders at or above the threshold ship free.\"\"\"\n    if subtotal_cents >= FREE_SHIPPING_CENTS:\n        return 0\n    return 499\n\n\ndef handling_fee(subtotal_cents):\n    return subtotal_cents // 100\n\n\ndef quote(subtotal_cents):\n    return {\"total_cents\": subtotal_cents}\n";
const QUOTE_OLD_SHIPPING: &str =
    "    return {\"total_cents\": subtotal_cents + shipping_fee(subtotal_cents)}";
const QUOTE_OLD_HANDLING: &str =
    "    return {\"total_cents\": subtotal_cents + handling_fee(subtotal_cents)}";

fn quote_class(old: &str, body: &str) -> Result<ExposureClass, String> {
    classify(
        QUOTE,
        16,
        old,
        &test_module("from src.shop import quote", body),
    )
}

#[test]
fn a_removed_term_that_every_asserted_input_makes_zero_is_not_discriminated() -> Result<(), String>
{
    // #6733: `shipping_fee(6000)` is 0, so the test passes with or without it.
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    assert quote(6000)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::WeaklyExposed
    );
    // One input below the threshold activates the term.
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    assert quote(6000)[\"total_cents\"] == 6000\n    assert quote(1000)[\"total_cents\"] == 1499\n"
        )?,
        ExposureClass::Exposed
    );
    // An input the rule cannot resolve leaves the verdict to the oracle.
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    amount = 6000\n    assert quote(amount)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::Exposed
    );
    // A callee that is not one guarded return of literals is not evaluated.
    assert_eq!(
        quote_class(
            QUOTE_OLD_HANDLING,
            "def test_quote():\n    assert quote(50)[\"total_cents\"] == 50\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn an_unasserted_owner_call_does_not_block_an_inactive_term() -> Result<(), String> {
    // #7562: `quote(1000)` is a discarded expression statement (fee 499), but
    // the only strong assertion observes `quote(6000)` where the fee is 0.
    // Removing the term still passes the test, so the finding should
    // downgrade. An asserted nonzero fee must still block. Assigned, nested,
    // or passed-on calls are unclear and still block.
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    quote(1000)\n    assert quote(6000)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    quote(1000)\n    result = quote(6000)\n    assert result[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    assert quote(6000)[\"total_cents\"] == 6000\n    assert quote(1000)[\"total_cents\"] == 1499\n"
        )?,
        ExposureClass::Exposed
    );
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    unused = quote(1000)\n    assert quote(6000)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::Exposed
    );
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    print(quote(1000))\n    assert quote(6000)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::Exposed
    );
    let assigned =
        quote_module("    total = subtotal_cents\n    return {\"total_cents\": total}\n");
    assert_eq!(
        quote_module_class(
            &assigned,
            16,
            "    total = subtotal_cents + shipping_fee(subtotal_cents)",
            "def test_quote():\n    quote(1000)\n    assert quote(6000)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::Exposed
    );
    // Two owner calls on one line: the covering statement cannot pick a
    // single observation, so the nonzero call still blocks.
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    quote(6000); collected.append(quote(1000))\n    assert quote(6000)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::Exposed
    );
    // An attribute that shares the bound local's name is not a local read.
    assert_eq!(
        quote_class(
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    quote(1000)\n    out = quote(6000)\n    assert other.out == 0\n"
        )?,
        ExposureClass::Exposed
    );
    Ok(())
}

const BIG: &str = "def check(n):\n    if n > 10:\n        raise KeyError(\"big\")\n    return n\n";
const BIG_OLD: &str = "    if n >= 10:";

#[test]
fn the_callable_form_binds_its_arguments_for_the_boundary_rule() -> Result<(), String> {
    // Rule 13 calls `check(20)`: both predicates raise there, so the changed
    // boundary is not activated, as in the `with` form.
    let class = |call: &str| {
        classify(
            BIG,
            2,
            BIG_OLD,
            &test_module(
                "import unittest\n\nfrom src.shop import check",
                &format!(
                    "class CheckTest(unittest.TestCase):\n    def test_check(self):\n        {call}\n"
                ),
            ),
        )
    };
    assert_eq!(
        class("self.assertRaisesRegex(KeyError, \"big\", check, 20)")?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        class("self.assertRaisesRegex(KeyError, \"big\", check, 10)")?,
        ExposureClass::Exposed
    );
    Ok(())
}

/// The `QUOTE` module with `quote`'s body replaced.
fn quote_module(quote_body: &str) -> String {
    let head = QUOTE.split("def quote").next().unwrap_or_default();
    format!("{head}def quote(subtotal_cents):\n{quote_body}")
}

fn quote_module_class(
    module: &str,
    line: usize,
    old: &str,
    body: &str,
) -> Result<ExposureClass, String> {
    classify(
        module,
        line,
        old,
        &test_module("import pytest\n\nfrom src.shop import quote", body),
    )
}

#[test]
fn a_removed_term_reads_only_stable_inputs_and_the_one_callee() -> Result<(), String> {
    let free = "def test_quote():\n    assert quote(6000)[\"total_cents\"] == 2000\n";
    // The owner rebinds its parameter before the term, so the call-site
    // value is not the term's argument.
    let rebound = quote_module(
        "    subtotal_cents = subtotal_cents - 4000\n    return {\"total_cents\": subtotal_cents}\n",
    );
    assert_eq!(
        quote_module_class(&rebound, 17, QUOTE_OLD_SHIPPING, free)?,
        ExposureClass::Exposed
    );
    // A strong call the reader cannot bind leaves the inputs unresolved.
    let plain = quote_module("    return {\"total_cents\": subtotal_cents}\n");
    assert_eq!(
        quote_module_class(
            &plain,
            16,
            QUOTE_OLD_SHIPPING,
            "def test_free():\n    assert quote(6000)[\"total_cents\"] == 6000\n\n\ndef test_paid():\n    assert quote(\n        100,  # below the threshold\n    )[\"total_cents\"] == 599\n"
        )?,
        ExposureClass::Exposed
    );
    // A test that patches the callee may give it any value.
    assert_eq!(
        quote_module_class(
            &plain,
            16,
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    shop.shipping_fee = fixed_fee\n    assert quote(6000)[\"total_cents\"] == 6499\n"
        )?,
        ExposureClass::Exposed
    );
    // A second definition of the callee is the one Python binds.
    let redefined = format!(
        "{}\n\ndef shipping_fee(subtotal_cents):\n    return 499\n",
        plain.trim_end()
    );
    assert_eq!(
        quote_module_class(
            &redefined,
            16,
            QUOTE_OLD_SHIPPING,
            "def test_quote():\n    assert quote(6000)[\"total_cents\"] == 6000\n"
        )?,
        ExposureClass::Exposed
    );
    // A subtracted term whose zero comes from the fallback branch.
    let inverted = "FREE_SHIPPING_CENTS = 5000\n\n\ndef rebate(subtotal_cents):\n    if subtotal_cents >= FREE_SHIPPING_CENTS:\n        return 250\n    return 0\n\n\ndef quote(subtotal_cents):\n    return {\"total_cents\": subtotal_cents}\n";
    assert_eq!(
        quote_module_class(
            inverted,
            11,
            "    return {\"total_cents\": subtotal_cents - rebate(subtotal_cents)}",
            "def test_quote():\n    assert quote(100)[\"total_cents\"] == 100\n"
        )?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn an_inactive_term_names_the_missing_discriminator_and_its_inputs() -> Result<(), String> {
    let owners = extract_owners(Path::new(OWNER_FILE), QUOTE);
    let tests = extract_tests(
        Path::new(TEST_FILE),
        &test_module(
            "from src.shop import quote",
            "def test_quote():\n    assert quote(6000)[\"total_cents\"] == 6000\n",
        ),
    );
    let finding = classify_change_with_old(
        Path::new(OWNER_FILE),
        16,
        "    return {\"total_cents\": subtotal_cents}",
        Some(QUOTE_OLD_SHIPPING),
        &owners,
        &tests,
    )
    .ok_or("changed line should classify")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    let missing = &finding.activation.missing_discriminators;
    assert_eq!(
        missing
            .iter()
            .map(|fact| fact.value.as_str())
            .collect::<Vec<_>>(),
        ["shipping_fee(subtotal_cents) != 0"]
    );
    assert_eq!(
        finding
            .activation
            .observed_values
            .iter()
            .map(|fact| fact.value.as_str())
            .collect::<Vec<_>>(),
        ["subtotal_cents = 6000"]
    );
    Ok(())
}

const BONUS: &str = "def bonus(points):\n    if points < 100:\n        return 0\n    return 5\n\n\ndef eligible(points):\n    return points > 10\n";

#[test]
fn an_inactive_term_on_a_predicate_line_shows_the_term_inputs() -> Result<(), String> {
    // `points = 10` sits on the `points > 10` boundary, so the boundary rule
    // observes `points == 10` too. The finding names `bonus(points) != 0`,
    // so it shows only the inputs that make the term 0.
    let owners = extract_owners(Path::new(OWNER_FILE), BONUS);
    let tests = extract_tests(
        Path::new(TEST_FILE),
        &test_module(
            "from src.shop import eligible",
            "def test_eligible():\n    assert eligible(10) == False\n",
        ),
    );
    let finding = classify_change_with_old(
        Path::new(OWNER_FILE),
        8,
        "    return points > 10",
        Some("    return points > 10 + bonus(points)"),
        &owners,
        &tests,
    )
    .ok_or("changed line should classify")?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .map(|fact| fact.value.as_str())
            .collect::<Vec<_>>(),
        ["bonus(points) != 0"]
    );
    assert_eq!(
        finding
            .activation
            .observed_values
            .iter()
            .map(|fact| fact.value.as_str())
            .collect::<Vec<_>>(),
        ["points = 10"]
    );
    Ok(())
}

const BIG_DEFAULT: &str =
    "def check(code, n=15):\n    if n > 15:\n        raise KeyError(\"big\")\n    return code\n";
const BIG_DEFAULT_OLD: &str = "    if n >= 15:";

#[test]
fn the_callable_form_passes_its_keyword_arguments_on() -> Result<(), String> {
    // `check("b", n=20)` is the call: binding the default `n=15` instead
    // would place the input on the changed boundary.
    assert_eq!(
        classify(
            BIG_DEFAULT,
            2,
            BIG_DEFAULT_OLD,
            &test_module(
                "import unittest\n\nfrom src.shop import check",
                "class CheckTest(unittest.TestCase):\n    def test_check(self):\n        self.assertRaisesRegex(KeyError, \"big\", check, \"b\", n=20)\n",
            ),
        )?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

const ON_PARSE: &str =
    "def on_parse(text, handler):\n    handler(int(text) + 1)\n    return handler\n";
const ON_PARSE_OLD: &str = "    handler(int(text))";

fn on_parse_class(
    owner: &str,
    line: usize,
    old: &str,
    body: &str,
) -> Result<ExposureClass, String> {
    classify(
        owner,
        line,
        old,
        &test_module(
            "from unittest.mock import Mock\n\nfrom src.shop import on_parse",
            body,
        ),
    )
}

#[test]
fn a_mock_passed_for_a_called_parameter_pins_its_arguments() -> Result<(), String> {
    // #7586, example 9: the owner hands its handler back, so `m` is the mock
    // the test passed, and the pinned expectation fails on the handed 2.
    let returned =
        "def test_on_parse():\n    m = on_parse('1', Mock())\n    m.assert_called_once_with(1)\n";
    assert_eq!(
        on_parse_class(ON_PARSE, 2, ON_PARSE_OLD, returned)?,
        ExposureClass::Exposed
    );
    // The same mock reached through the local the test passed.
    assert_eq!(
        on_parse_class(
            ON_PARSE,
            2,
            ON_PARSE_OLD,
            "def test_on_parse():\n    h = Mock()\n    on_parse('1', h)\n    h.assert_called_once_with(1)\n"
        )?,
        ExposureClass::Exposed
    );
    // A wrapping call can return its argument unchanged at the test's input
    // (`abs(1) == 1`), so the expectation may still hold.
    let wrapped = "def on_parse(text, handler):\n    handler(abs(int(text)))\n    return handler\n";
    assert_eq!(
        on_parse_class(wrapped, 2, ON_PARSE_OLD, returned)?,
        ExposureClass::WeaklyExposed
    );
    // An owner that returns something else, or rebinds the parameter first,
    // does not hand the mock back.
    for owner in [
        "def on_parse(text, handler):\n    handler(int(text) + 1)\n    return text\n",
        "def on_parse(text, handler):\n    handler(int(text) + 1)\n    handler = Mock()\n    return handler\n",
        "def on_parse(text, handler):\n    handler(int(text) + 1)\n    if text:\n        return None\n    return handler\n",
    ] {
        assert_eq!(
            on_parse_class(owner, 2, ON_PARSE_OLD, returned)?,
            ExposureClass::WeaklyExposed,
            "{owner}"
        );
    }
    // Rule 17's argument rules still apply: an unpinned or last-call-only
    // expectation does not credit.
    for check in ["m.assert_called_once()", "m.assert_called_with(1)"] {
        assert_eq!(
            on_parse_class(
                ON_PARSE,
                2,
                ON_PARSE_OLD,
                &format!("def test_on_parse():\n    m = on_parse('1', Mock())\n    {check}\n")
            )?,
            ExposureClass::WeaklyExposed,
            "{check}"
        );
    }
    Ok(())
}

#[test]
fn a_mock_passed_to_a_classmethod_owner_pins_its_arguments() -> Result<(), String> {
    // #7634: a classmethod or staticmethod owner is called on its imported
    // class (rule 9), and its binding decorator leaves the returned mock
    // unchanged.
    const IMPORTS: &str = "from unittest.mock import Mock\n\nfrom src.other import Other\nfrom src.shop import Service";
    // `decorators` are the owner's decorator lines; the changed line is the
    // first call in the body.
    let class = |decorators: &[&str], signature: &str, test: &str| {
        let head: String = decorators
            .iter()
            .map(|decorator| format!("    @{decorator}\n"))
            .collect();
        classify(
            &format!(
                "class Service:\n{head}    def run({signature}):\n        handler(2)\n        return handler\n"
            ),
            decorators.len() + 3,
            "        handler(1)",
            &test_module(IMPORTS, test),
        )
    };
    let returned =
        "def test_run():\n    m = Service.run(Mock())\n    m.assert_called_once_with(2)\n";
    let passed =
        "def test_run():\n    h = Mock()\n    Service.run(h)\n    h.assert_called_once_with(2)\n";
    for (decorator, signature) in [("classmethod", "cls, handler"), ("staticmethod", "handler")] {
        for test in [returned, passed] {
            assert_eq!(
                class(&[decorator], signature, test)?,
                ExposureClass::Exposed,
                "@{decorator}: {test}"
            );
        }
    }
    // The class through an import alias, and a staticmethod whose handler is
    // not its first parameter.
    assert_eq!(
        classify(
            "class Service:\n    @classmethod\n    def run(cls, handler):\n        handler(2)\n",
            4,
            "        handler(1)",
            &test_module(
                "from unittest.mock import Mock\n\nfrom src.shop import Service as S",
                "def test_run():\n    h = Mock()\n    S.run(h)\n    h.assert_called_once_with(2)\n",
            ),
        )?,
        ExposureClass::Exposed
    );
    assert_eq!(
        class(
            &["staticmethod"],
            "tag, handler",
            "def test_run():\n    h = Mock()\n    Service.run(1, h)\n    h.assert_called_once_with(2)\n"
        )?,
        ExposureClass::Exposed
    );
    // A classmethod's receiver binds the class whatever it is named, so the
    // local passed second is the handler, not the first parameter after it.
    let klass = "klass, handler, audit";
    assert_eq!(
        class(
            &["classmethod"],
            klass,
            "def test_run():\n    h = Mock()\n    a = Mock()\n    Service.run(h, a)\n    h.assert_called_once_with(2)\n"
        )?,
        ExposureClass::Exposed
    );
    // `audit` gets 2 before and after, so its expectation cannot fail.
    assert_eq!(
        classify(
            "class Service:\n    @classmethod\n    def run(klass, handler, audit):\n        handler(2)\n        audit(2)\n",
            4,
            "        handler(1)",
            &test_module(
                IMPORTS,
                "def test_run():\n    h = Mock()\n    a = Mock()\n    Service.run(h, a)\n    a.assert_called_once_with(2)\n",
            ),
        )?,
        ExposureClass::WeaklyExposed
    );
    // `@classmethod` over `@staticmethod` binds no receiver: `h` is the
    // handler, and `a`, which gets 2 either way, is not read.
    assert_ne!(
        classify(
            "class Service:\n    @classmethod\n    @staticmethod\n    def run(handler, audit):\n        handler(2)\n        audit(2)\n",
            5,
            "        handler(1)",
            &test_module(
                IMPORTS,
                "def test_run():\n    h = Mock()\n    a = Mock()\n    Service.run(h, a)\n    a.assert_called_once_with(2)\n",
            ),
        )?,
        ExposureClass::Exposed
    );
    // A later import of the same name binds another class.
    assert_ne!(
        class(
            &["classmethod"],
            "cls, handler",
            "from src.other import Other as Service\n\n\ndef test_run():\n    h = Mock()\n    Service.run(h)\n    h.assert_called_once_with(2)\n"
        )?,
        ExposureClass::Exposed
    );
    // Another decorator stacked on the classmethod may change what the call
    // returns, so the result is not the mock passed; the passed local still
    // reads it.
    let cached = ["classmethod", "functools.cache"];
    assert_eq!(
        class(&cached, "cls, handler", returned)?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        class(&cached, "cls, handler", passed)?,
        ExposureClass::Exposed
    );
    // Not the owner on that class: an instance method called on the class, a
    // decorator that only ends in `classmethod` or `staticmethod`, the same method on another
    // class, and a class name the test rebinds (rule 9).
    for (decorators, signature, test) in [
        (&[][..], "self, handler", passed),
        (&["lib.fake_classmethod"][..], "cls, handler", passed),
        (
            &["lib.fake_staticmethod"][..],
            "handler",
            "def test_run():\n    s = Service()\n    h = Mock()\n    s.run(h)\n    h.assert_called_once_with(2)\n",
        ),
        (
            &["classmethod"][..],
            "cls, handler",
            "def test_run():\n    h = Mock()\n    Other.run(h)\n    h.assert_called_once_with(2)\n",
        ),
        (
            &["classmethod"][..],
            "cls, handler",
            "def test_run():\n    Service = Other\n    h = Mock()\n    Service.run(h)\n    h.assert_called_once_with(2)\n",
        ),
    ] {
        // Either reading fails closed; neither credits the mock.
        assert!(
            matches!(
                class(decorators, signature, test)?,
                ExposureClass::WeaklyExposed | ExposureClass::StaticUnknown
            ),
            "{decorators:?} {signature}: {test}"
        );
    }
    Ok(())
}

#[test]
fn a_wrapped_argument_does_not_credit_a_mocked_collaborator() -> Result<(), String> {
    // The gate applies to `p.m(...)` too: `abs(user)` may hand the mock the
    // same value, while a scaled or added argument differs at a nonzero input
    // (RIPR-SPEC-0238 Decision 15).
    //
    // Each test pins the arguments the new call passes at `user == 3`, so it
    // passes on the new code; whether it also passes on the old code is what
    // the verdict reads.
    let class = |new: &str, old: &str, pinned: &str| {
        classify(
            &format!("def notify(client, user, other=5):\n    client.send({new})\n"),
            2,
            &format!("    client.send({old})"),
            &test_module(
                "from unittest.mock import Mock\n\nfrom src.shop import notify",
                &format!(
                    "def test_notify():\n    client = Mock()\n    notify(client, 3)\n    client.send.assert_called_once_with({pinned})\n"
                ),
            ),
        )
    };
    // Any rewrite that keeps the old argument inside it, other than literal
    // arithmetic, can return it unchanged at the test's input.
    for (new, old, pinned) in [
        ("abs(user)", "user", "3"),
        ("user", "int(user)", "3"),
        ("abs(abs(user))", "user", "3"),
        ("float(abs(user))", "user", "3.0"),
        ("max(user, 0)", "user", "3"),
        ("user or 0", "user", "3"),
        ("(user)", "user", "3"),
        ("user.real", "user", "3"),
        ("2.0", "2", "2"),
        ("*[user]", "user", "3"),
        // An identity operation or a requoted string hands the mock the same
        // value.
        ("user + 0", "user", "3"),
        ("user * 1", "user", "3"),
        ("user ** 1", "user", "3"),
        ("0 + user", "user", "3"),
        ("1 * user", "user", "3"),
        ("str(user) + ''", "str(user)", "'3'"),
        ("\"hi\"", "'hi'", "'hi'"),
        ("1_000", "1000", "1000"),
    ] {
        assert_eq!(
            class(new, old, pinned)?,
            ExposureClass::WeaklyExposed,
            "{old} -> {new}"
        );
    }
    for (new, old, pinned) in [
        ("user * 100", "user", "300"),
        ("user + 1", "user", "4"),
        ("1 + user", "user", "4"),
        ("user", "user - 1", "3"),
        ("user, 1", "user", "3, 1"),
        ("abs(user), 1", "user, 2", "3, 1"),
        ("2.5", "2", "2.5"),
        ("'welcome'", "'hi'", "'welcome'"),
        ("other", "user", "5"),
        ("user, tag=1", "user", "3, tag=1"),
        ("0 - user", "user", "-3"),
        ("user - 1", "user", "2"),
        ("'1'", "1", "'1'"),
        ("user, tag=1", "user, kind=1", "3, tag=1"),
    ] {
        assert_eq!(
            class(new, old, pinned)?,
            ExposureClass::Exposed,
            "{old} -> {new}"
        );
    }
    // An old line that is not a call statement cannot be compared: it fails
    // closed rather than reading the new call as a change.
    for old in [
        "    client.send(user)  # notify",
        "    sent = client.send(user)",
        "    self.sent = client.send(user)",
        "    log(); client.send(user)",
    ] {
        assert_eq!(
            classify(
                "def notify(client, user):\n    client.send(abs(user))\n",
                2,
                old,
                &test_module(
                    "from unittest.mock import Mock\n\nfrom src.shop import notify",
                    "def test_notify():\n    client = Mock()\n    notify(client, 3)\n    client.send.assert_called_once_with(3)\n",
                ),
            )?,
            ExposureClass::WeaklyExposed,
            "{old}"
        );
    }
    Ok(())
}
