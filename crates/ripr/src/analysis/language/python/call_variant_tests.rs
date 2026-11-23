//! Tests for the strip-side and `round`-digits gate (#16203): the edit is
//! discriminated only by an argument the two versions treat differently.

use super::call_variant::{CallVariant, call_variant, variant_agrees};
use super::owners_tests::{extract_owners, extract_tests};
use super::*;

const SOURCE: &str =
    "def clean(s):\n    return s.strip()\n\n\ndef money(x):\n    return round(x, 2)\n";

fn classify(line: usize, old: &str, new: &str, test_source: &str) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/text.py"), SOURCE);
    let tests = extract_tests(Path::new("tests/test_text.py"), test_source);
    classify_change_with_old(
        Path::new("src/text.py"),
        line,
        new,
        Some(old),
        &owners,
        &tests,
    )
    .ok_or_else(|| "a call change should classify".to_string())
}

fn classify_clean(test_source: &str) -> Result<Finding, String> {
    classify(
        2,
        "    return s.strip()",
        "    return s.lstrip()",
        test_source,
    )
}

fn classify_money(test_source: &str) -> Result<Finding, String> {
    classify(
        6,
        "    return round(x, 2)",
        "    return round(x, 3)",
        test_source,
    )
}

#[test]
fn inputs_without_trailing_whitespace_do_not_expose_strip_to_lstrip() -> Result<(), String> {
    let finding = classify_clean(
        "from src.text import clean\n\n\ndef test_clean():\n    assert clean(\"  a\") == \"a\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `clean` with `s` with trailing whitespace"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    Ok(())
}

#[test]
fn an_input_with_trailing_whitespace_keeps_strip_to_lstrip_exposed() -> Result<(), String> {
    let finding = classify_clean(
        "from src.text import clean\n\n\ndef test_clean():\n    assert clean(\"  a\") == \"a\"\n    assert clean(\"b \") == \"b\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    Ok(())
}

#[test]
fn inputs_within_the_smaller_digit_count_do_not_expose_a_round_change() -> Result<(), String> {
    let finding = classify_money(
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.5) == 1.5\n    assert money(3) == 3\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `money` with `x` with more than 2 decimal places"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

#[test]
fn an_input_with_more_decimals_keeps_a_round_change_exposed() -> Result<(), String> {
    let finding = classify_money(
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.2345) == 1.23\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_unread_argument_keeps_its_reading() -> Result<(), String> {
    let finding = classify_clean(
        "from src.text import clean\n\nWORDS = [\"a \"]\n\n\ndef test_clean():\n    assert clean(\"a\") == \"a\"\n    assert clean(*WORDS) == \"a\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn only_a_strip_side_or_round_digits_edit_on_a_name_is_read() {
    assert_eq!(
        call_variant("return s.strip()", "return s.rstrip()"),
        Some(CallVariant::Strip {
            param: "s".to_string(),
            old: "strip".to_string(),
            new: "rstrip".to_string(),
        })
    );
    assert_eq!(
        call_variant("total = round(x, 2) + 1", "total = round(x, 0) + 1"),
        Some(CallVariant::Round {
            param: "x".to_string(),
            old: 2,
            new: 0,
        })
    );
    // An argument to strip changes which characters go.
    assert_eq!(
        call_variant("return s.strip(\"x\")", "return s.lstrip(\"x\")"),
        None
    );
    // An attribute receiver is not a parameter value.
    assert_eq!(
        call_variant("return self.s.strip()", "return self.s.lstrip()"),
        None
    );
    // Another method name.
    assert_eq!(call_variant("return s.upper()", "return s.lower()"), None);
    // A digit count on a computed argument, or a module's own `round`.
    assert_eq!(
        call_variant("return round(x * 2, 2)", "return round(x * 2, 3)"),
        None
    );
    assert_eq!(
        call_variant("return np.round(x, 2)", "return np.round(x, 3)"),
        None
    );
    // Two edits.
    assert_eq!(
        call_variant("return s.strip() + t", "return s.lstrip() + u"),
        None
    );
}

#[test]
fn agreement_follows_python_strip_and_round() {
    let strip = |old: &str, new: &str| CallVariant::Strip {
        param: "s".to_string(),
        old: old.to_string(),
        new: new.to_string(),
    };
    assert_eq!(
        variant_agrees(&strip("strip", "lstrip"), "\"  a\""),
        Some(true)
    );
    assert_eq!(
        variant_agrees(&strip("strip", "lstrip"), "\"a \""),
        Some(false)
    );
    assert_eq!(
        variant_agrees(&strip("strip", "rstrip"), "'a\t'"),
        Some(true)
    );
    assert_eq!(
        variant_agrees(&strip("lstrip", "rstrip"), "\"   \""),
        Some(true)
    );
    assert_eq!(
        variant_agrees(&strip("lstrip", "rstrip"), "\" a\""),
        Some(false)
    );
    // Escapes and prefixed literals are not read.
    assert_eq!(variant_agrees(&strip("strip", "lstrip"), "\"a\\n\""), None);
    assert_eq!(variant_agrees(&strip("strip", "lstrip"), "b\"a \""), None);
    let round = CallVariant::Round {
        param: "x".to_string(),
        old: 2,
        new: 3,
    };
    assert_eq!(variant_agrees(&round, "1.50"), Some(true));
    assert_eq!(variant_agrees(&round, "-7"), Some(true));
    assert_eq!(variant_agrees(&round, "1.255"), Some(false));
    assert_eq!(variant_agrees(&round, "\"1.5\""), None);
    assert_eq!(variant_agrees(&round, "1e3"), None);
}
