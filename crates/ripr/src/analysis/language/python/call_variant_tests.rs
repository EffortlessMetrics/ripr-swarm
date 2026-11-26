//! Tests for the strip-side and `round`-digits gate (#16203): the edit is
//! discriminated only by an argument the two versions treat differently.

use super::call_variant::{
    CallVariant, call_variant, elements_agree, round_example, round_half_even, variant_agrees,
};
use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::reexport_tests::{unique_tempdir, write_file};
use super::*;

const SOURCE: &str = "def clean(s):\n    return s.strip()\n\n\ndef money(x):\n    return round(x, 2)\n\n\ndef top(xs):\n    return sorted(xs)[0]\n\n\ndef low(xs):\n    xs.append(9)\n    return sorted(xs)[0]\n";

fn classify(line: usize, old: &str, new: &str, test_source: &str) -> Result<Finding, String> {
    classify_in(SOURCE, line, old, new, test_source)
}

/// Classifies a change with the owner file's new-side source, as the diff
/// path does, so the builtin-name check can read it.
fn classify_in(
    source: &str,
    line: usize,
    old: &str,
    new: &str,
    test_source: &str,
) -> Result<Finding, String> {
    let owners = extract_owners(Path::new("src/text.py"), source);
    let tests = extract_tests(Path::new("tests/test_text.py"), test_source);
    classify_change_with_source(
        Path::new("src/text.py"),
        line,
        new,
        Some(old),
        &owners,
        &tests,
        Some(source),
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

fn classify_top(test_source: &str) -> Result<Finding, String> {
    classify(
        10,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        test_source,
    )
}

fn classify_source(
    source: &str,
    line: usize,
    old: &str,
    new: &str,
    test_source: &str,
) -> Result<Finding, String> {
    classify_in(source, line, old, new, test_source)
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
            .any(|fact| fact.value
                == "call `clean` with `s` with text followed by trailing whitespace"),
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
            .any(|fact| fact.value
                == "call `money` with `x` with a value that `round(x, 2)` and `round(x, 3)` round differently, such as `1.234`"),
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
fn a_literal_both_counts_round_alike_does_not_expose_a_round_change() -> Result<(), String> {
    // `round(1.0001, 2)` and `round(1.0001, 3)` both return `1.0`, so the
    // exact assertion passes either way although the literal has four
    // decimal places (#16221).
    let finding = classify_money(
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.0001) == 1.0\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value
                == "call `money` with `x` with a value that `round(x, 2)` and `round(x, 3)` round differently, such as `1.234`"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    // The literal has four decimal places and both counts round it alike, so the
    // message cannot say that no call with more than two was seen.
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .all(|fact| !fact.value.contains("decimal place")),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

#[test]
fn a_negative_count_agreeing_input_does_not_expose_a_round_change() -> Result<(), String> {
    // `round(100, -1)` and `round(100, -2)` both return `100` (#16221).
    let source = "def money(x):\n    return round(x, -1)\n";
    let finding = classify_source(
        source,
        2,
        "    return round(x, -1)",
        "    return round(x, -2)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(100) == 100\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact
                .value
                .contains(
                    "`x` with a value that `round(x, -1)` and `round(x, -2)` round differently, such as `15`",
                )),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

#[test]
fn an_example_the_reader_cannot_read_is_left_out_of_the_message() -> Result<(), String> {
    // `round(0, -20)` and `round(0, -19)` are both 0. No input of these
    // counts is inside what the reader evaluates, so the message names the
    // difference and gives no example.
    let source = "def money(x):\n    return round(x, -20)\n";
    let finding = classify_source(
        source,
        2,
        "    return round(x, -20)",
        "    return round(x, -19)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(0) == 0\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value
                == "call `money` with `x` with a value that `round(x, -20)` and `round(x, -19)` round differently"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    Ok(())
}

#[test]
fn the_missing_check_names_an_input_the_counts_round_differently() {
    // `1.0001` has more than two decimal places and both counts round it to
    // `1.0`; `101` is no multiple of 100 and both counts round it to `100`.
    // Each is where a difference can be, not one. The examples are inputs the
    // reader finds the counts round apart.
    let round = |old: i32, new: i32| CallVariant::Round {
        param: "x".to_string(),
        old,
        new,
    };
    assert_eq!(variant_agrees(&round(2, 3), "1.0001"), Some(true));
    assert_eq!(variant_agrees(&round(2, 3), "1.234"), Some(false));
    assert_eq!(variant_agrees(&round(-1, -2), "101"), Some(true));
    assert_eq!(variant_agrees(&round(-1, -2), "15"), Some(false));
    assert_eq!(variant_agrees(&round(-1, 0), "1.23"), Some(false));
    // The example the rule finds for each pair.
    assert_eq!(round_example(2, 3).as_deref(), Some("1.234"));
    assert_eq!(round_example(-1, -2).as_deref(), Some("15"));
    assert_eq!(round_example(-1, 0).as_deref(), Some("1.6"));
    assert_eq!(round_example(-1, 15).as_deref(), Some("1.6"));
    assert_eq!(round_example(3, -2).as_deref(), Some("1.6"));
    assert_eq!(round_example(14, 15).as_deref(), Some("0.0000000000000012"));
    assert_eq!(round_example(-2, -15).as_deref(), Some("150"));
    assert_eq!(round_example(-15, -16).as_deref(), Some("600000000000001"));
    assert_eq!(round_example(-16, -17), None);
    assert_eq!(round_example(-20, -19), None);
}

#[test]
fn every_example_is_an_input_the_counts_round_differently() {
    // Every pair of distinct counts inside the rule's bounds, in both orders.
    // An example is given only for a pair the rule finds it separates; CPython
    // 3.14 rounds each of these the same way.
    let mut without = Vec::new();
    for old in -20..=20 {
        for new in -20..=20 {
            if old == new {
                continue;
            }
            match round_example(old, new) {
                Some(example) => assert_eq!(
                    variant_agrees(
                        &CallVariant::Round {
                            param: "x".to_string(),
                            old,
                            new
                        },
                        &example
                    ),
                    Some(false),
                    "{old}, {new}: {example}"
                ),
                None => without.push((old, new)),
            }
        }
    }
    assert!(
        without
            .iter()
            .all(|&(old, new)| old < 0 && new < 0 && old.max(new) <= -16),
        "pairs without an example: {without:?}"
    );
}

#[test]
fn a_count_that_changes_its_sign_is_read_as_one_edit() -> Result<(), String> {
    // `round(100, -1)` and `round(100, 0)` both return `100`: the `-`
    // belongs to the count, so removing it changes the count and nothing else.
    let source = "def money(x):\n    return round(x, -1)\n";
    let finding = classify_source(
        source,
        2,
        "    return round(x, -1)",
        "    return round(x, 0)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(100) == 100\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value
                == "call `money` with `x` with a value that `round(x, -1)` and `round(x, 0)` round differently, such as `1.6`"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    // `round(15, -1)` is `20` and `round(15, 0)` is `15`: read and apart,
    // the gate leaves the finding exposed. The first half is what holds the
    // reading; an unread edit would leave this one exposed as well.
    let finding = classify_source(
        source,
        2,
        "    return round(x, -1)",
        "    return round(x, 0)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(15) == 20\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_sign_added_to_a_count_is_read_as_one_edit() -> Result<(), String> {
    // `round(100, 0)` and `round(100, -1)` both return `100`.
    let source = "def money(x):\n    return round(x, 0)\n";
    let finding = classify_source(
        source,
        2,
        "    return round(x, 0)",
        "    return round(x, -1)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(100) == 100\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_negative_count_off_grid_input_keeps_a_round_change_exposed() -> Result<(), String> {
    // `round(120, -1)` is `120` and `round(120, -2)` is `100`.
    let source = "def money(x):\n    return round(x, -1)\n";
    let finding = classify_source(
        source,
        2,
        "    return round(x, -1)",
        "    return round(x, -2)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(120) == 120\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_count_past_the_leading_digit_keeps_a_round_change_exposed() -> Result<(), String> {
    // `round(60, -2)` is `100` and `round(60, -3)` is `0`.
    let source = "def money(x):\n    return round(x, -2)\n";
    let finding = classify_source(
        source,
        2,
        "    return round(x, -2)",
        "    return round(x, -3)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(60) == 0\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_float_on_a_decimal_tie_keeps_a_round_change_exposed() -> Result<(), String> {
    // Python rounds `1.05` by its binary value: `round(1.05, 1)` is `1.1`,
    // where half-to-even on the spelling gives `1.0`.
    let source = "def money(x):\n    return round(x, 0)\n";
    let finding = classify_source(
        source,
        2,
        "    return round(x, 0)",
        "    return round(x, 1)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.05) == 1.1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_recursive_owner_keeps_the_entry_rows_from_reading_agreement() -> Result<(), String> {
    // The recursive call hands `clean` an argument no test passes, so the
    // entry rows do not cover what the versions see (#16221).
    let source = "def clean(text):\n    if \" \" in text:\n        return clean(text.replace(\" \", \"\"))\n    return text.strip()\n";
    let finding = classify_source(
        source,
        4,
        "    return text.strip()",
        "    return text.lstrip()",
        "from src.text import clean\n\n\ndef test_clean():\n    assert clean(\"a\") == \"a\"\n",
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
    // A negative count lexes as a `-` before the digits.
    assert_eq!(
        call_variant("return round(x, -1)", "return round(x, -2)"),
        Some(CallVariant::Round {
            param: "x".to_string(),
            old: -1,
            new: -2,
        })
    );
    // A call that begins the line has nothing before it and is bare.
    assert_eq!(
        call_variant("round(x, 2)", "round(x, 3)"),
        Some(CallVariant::Round {
            param: "x".to_string(),
            old: 2,
            new: 3,
        })
    );
    assert_eq!(
        call_variant("round(x, -1)", "round(x, 0)"),
        Some(CallVariant::Round {
            param: "x".to_string(),
            old: -1,
            new: 0,
        })
    );
    // The sign belongs to the count: adding or removing it is still one edit.
    assert_eq!(
        call_variant("return round(x, -1)", "return round(x, 0)"),
        Some(CallVariant::Round {
            param: "x".to_string(),
            old: -1,
            new: 0,
        })
    );
    assert_eq!(
        call_variant("return round(x, 2)", "return round(x, -3)"),
        Some(CallVariant::Round {
            param: "x".to_string(),
            old: 2,
            new: -3,
        })
    );
    // `-0` and `0` are one count.
    assert_eq!(
        call_variant("return round(x, -0)", "return round(x, 0)"),
        None
    );
    // A count that changes with its parameter is two edits.
    assert_eq!(
        call_variant("return round(x, -1)", "return round(y, 0)"),
        None
    );
    // A count beside a subtraction.
    assert_eq!(
        call_variant("return round(x, 1 - 2)", "return round(x, -2)"),
        None
    );
    // A binary subtraction is not a signed count.
    assert_eq!(
        call_variant("return round(x, 1 - 2)", "return round(x, 1 - 3)"),
        None
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
    // `round(1.0001, 2)` and `round(1.0001, 3)` both return `1.0`: digits
    // beyond both counts can round away under either.
    assert_eq!(variant_agrees(&round, "1.0001"), Some(true));
    // A float spelling on an exact decimal tie is not read: Python rounds
    // the double it names, which may sit on either side of the tie.
    assert_eq!(variant_agrees(&round, "1.255"), None);
    let coarse = CallVariant::Round {
        param: "x".to_string(),
        old: 0,
        new: 1,
    };
    assert_eq!(variant_agrees(&coarse, "2.5"), None);
    assert_eq!(variant_agrees(&coarse, "1.05"), None);
    assert_eq!(variant_agrees(&coarse, "0.45"), None);
    assert_eq!(variant_agrees(&coarse, "1.06"), Some(false));
    assert_eq!(variant_agrees(&coarse, "1.004"), Some(true));
    // An int spelling is exact: `round(3, 0)` and `round(3, 1)` are both `3`.
    assert_eq!(variant_agrees(&coarse, "3"), Some(true));
    // A negative count rounds to tens and hundreds.
    let negative = CallVariant::Round {
        param: "x".to_string(),
        old: -1,
        new: -2,
    };
    assert_eq!(variant_agrees(&negative, "100"), Some(true));
    assert_eq!(variant_agrees(&negative, "120"), Some(false));
    assert_eq!(variant_agrees(&negative, "-140"), Some(false));
    // An int spelling's exact tie rounds half to even: `round(250, -2)` is
    // `200`.
    assert_eq!(variant_agrees(&negative, "250"), Some(false));
    assert_eq!(variant_agrees(&negative, "1200"), Some(true));
    // A count past the value's leading digit rounds to zero.
    let past_leading = CallVariant::Round {
        param: "x".to_string(),
        old: -2,
        new: -3,
    };
    assert_eq!(variant_agrees(&past_leading, "60"), Some(false));
    assert_eq!(variant_agrees(&past_leading, "600"), Some(false));
    assert_eq!(variant_agrees(&past_leading, "20"), Some(true));
    let coarser = CallVariant::Round {
        param: "x".to_string(),
        old: -3,
        new: -4,
    };
    assert_eq!(variant_agrees(&coarser, "60"), Some(true));
    assert_eq!(variant_agrees(&coarser, "600"), Some(false));
    // An int spelling's exact tie with nothing kept rounds to zero, to even.
    assert_eq!(variant_agrees(&coarser, "500"), Some(true));
    assert_eq!(variant_agrees(&coarser, "1500"), Some(false));
    let to_zero = CallVariant::Round {
        param: "x".to_string(),
        old: 0,
        new: -1,
    };
    assert_eq!(variant_agrees(&to_zero, "0.6"), Some(false));
    // Beyond the read's bounds the rule refuses rather than claim agreement.
    assert_eq!(variant_agrees(&round, "123456789012345678901"), None);
    // A spelling of more than 48 digits is not read, leading zeros counted.
    assert_eq!(
        variant_agrees(&round, &format!("0.{}1", "0".repeat(47))),
        None
    );
    assert_eq!(
        variant_agrees(&round, &format!("0.{}1", "0".repeat(46))),
        Some(true)
    );
    let unreachable = CallVariant::Round {
        param: "x".to_string(),
        old: 21,
        new: 3,
    };
    assert_eq!(variant_agrees(&unreachable, "1.5"), None);
    assert_eq!(variant_agrees(&round, "\"1.5\""), None);
    // A scientific spelling is not read: the argument reader passes plain
    // decimal literals only, so the gate does not apply to one (#16221 review).
    assert_eq!(variant_agrees(&round, "1e3"), None);
    assert_eq!(variant_agrees(&round, "1.5E3"), None);
    assert_eq!(variant_agrees(&round, "1e5x"), None);
    assert_eq!(variant_agrees(&round, "1e"), None);
}

#[test]
fn an_exact_tie_rounds_to_the_even_digit() {
    // A tie the binary value holds exactly: Python rounds 2.5 to 2 and 3.5
    // to 4, 0.125 to 0.12 and 0.375 to 0.38. A digit after the tie rounds up.
    assert_eq!(round_half_even(&[2, 5], 1, 0), (vec![2], 0));
    assert_eq!(round_half_even(&[3, 5], 1, 0), (vec![4], 0));
    assert_eq!(round_half_even(&[1, 2, 5], 3, 2), (vec![1, 2], -2));
    assert_eq!(round_half_even(&[3, 7, 5], 3, 2), (vec![3, 8], -2));
    assert_eq!(round_half_even(&[2, 5, 1], 2, 0), (vec![3], 0));
    // A count past the leading digit rounds to zero, and so does an exact
    // tie with nothing kept (`round(500, -3)`); with a digit kept, the tie
    // rounds up (`round(1500, -3)` is `2000`).
    assert_eq!(round_half_even(&[6, 0], 0, -3), (vec![0], 0));
    assert_eq!(round_half_even(&[6], 1, -1), (vec![0], 0));
    assert_eq!(round_half_even(&[5, 0, 0], 0, -3), (vec![0], 0));
    assert_eq!(round_half_even(&[1, 5, 0, 0], 0, -3), (vec![2], 3));
}

#[test]
fn a_module_that_defines_round_is_not_read_as_the_builtin() -> Result<(), String> {
    // A local `round` is another call: `money(1.5)` returns `3.5` and `4.5`,
    // so the assertion discriminates and the gate must not model the builtin
    // (#16221 review).
    let source = "def round(x, digits):
    return x + digits


def money(x):
    return round(x, 2)
";
    let finding = classify_source(
        source,
        6,
        "    return round(x, 2)",
        "    return round(x, 3)",
        "from src.text import money


def test_money():
    assert money(1.5) == 3.5
",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn lists_without_two_different_elements_do_not_expose_a_sort_direction_flip() -> Result<(), String>
{
    let finding = classify_top(
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n    assert top([2, 2]) == 2\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value == "call `top` with `xs` holding two or more different elements"),
        "{:?}",
        finding.activation.missing_discriminators
    );
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    Ok(())
}

#[test]
fn a_list_with_different_elements_keeps_a_sort_direction_flip_exposed() -> Result<(), String> {
    let finding = classify_top(
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n    assert top([4, 1]) == 1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    assert_eq!(finding.ripr.infect.state, StageState::Yes);
    Ok(())
}

#[test]
fn an_unread_list_keeps_a_sort_direction_flip_exposed() -> Result<(), String> {
    let finding = classify_top(
        "from src.text import top\n\nNUMS = [4, 1]\n\n\ndef test_top():\n    assert top([4]) == 4\n    assert top(NUMS) == 1\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_list_the_body_changes_where_the_text_reader_stops_short_keeps_a_sort_direction_flip_exposed()
-> Result<(), String> {
    // Python 3.12 lets a comment sit inside an f-string's replacement field.
    // The text reader cannot prove that field closed and hides the rest of
    // the statement, so the first-mention scan would miss the append and take
    // the sort for the list's first mention: `top([4])` sorts `[4, 9]`, where
    // the direction decides the result.
    let source = "def top(xs):\n    rows = f\"\"\"{\n        xs.append(9)  # note\n    }\"\"\"\n    return sorted(xs)[0]\n";
    let test = "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n";
    let finding = classify_source(
        source,
        5,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        test,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // The same owner with a field the reader can follow to its end and that
    // does not name the list: the gate reads the list and withholds, so it
    // is live.
    let closed_field =
        "def top(xs):\n    rows = f\"\"\"{\n        9\n    }\"\"\"\n    return sorted(xs)[0]\n";
    let finding = classify_source(
        closed_field,
        5,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        test,
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    // A call the reader can see in a field the parser reads is a mention.
    let visible = "def top(xs):\n    rows = f\"{xs.append(9)}\"\n    return sorted(xs)[0]\n";
    let finding = classify_source(
        visible,
        3,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        test,
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_list_the_body_changes_before_the_sort_keeps_its_reading() -> Result<(), String> {
    // `low([4])` sorts `[4, 9]`, where the direction decides the result.
    let finding = classify(
        15,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        "from src.text import low\n\n\ndef test_low():\n    assert low([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn an_unrelated_non_ascii_string_does_not_block_the_sort_reading() -> Result<(), String> {
    // Non-ASCII string data cannot name the parameter, so the scan reads the
    // changed line as the first mention and the agreeing list downgrades
    // (#16221).
    let source = "def top(xs):\n    label = \"r\u{e9}sultat\"\n    return sorted(xs)[0]\n";
    let finding = classify_source(
        source,
        3,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(finding.ripr.infect.state, StageState::Weak);
    Ok(())
}

#[test]
fn a_list_python_compares_unequal_keeps_a_sort_direction_flip_exposed() -> Result<(), String> {
    // Python stores `9007199254740993.0` as `...992`, so the two elements are
    // unequal and the direction decides which comes first (#16221).
    let finding = classify_top(
        "from src.text import top\n\n\ndef test_top():\n    assert top([9007199254740993, 9007199254740993.0]) == 9007199254740993\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_list_python_compares_equal_does_not_expose_a_sort_direction_flip() -> Result<(), String> {
    // `9007199254740992.0` is exact, so both elements compare equal and the
    // direction cannot matter.
    let finding = classify_top(
        "from src.text import top\n\n\ndef test_top():\n    assert top([9007199254740992, 9007199254740992.0]) == 9007199254740992\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_line_inside_a_multiline_string_is_not_read_as_a_call() -> Result<(), String> {
    // The interior line is data; how the methods treat the argument cannot
    // gate a change to it (#16221).
    let source = "def template(s):\n    return \"\"\"a\nreturn s.strip()\nb\"\"\"\n";
    let owners = extract_owners(Path::new("src/text.py"), source);
    let tests = extract_tests(
        Path::new("tests/test_text.py"),
        "from src.text import template\n\n\ndef test_template():\n    assert template(\"a\") == \"a\\nreturn s.strip()\\nb\"\n",
    );
    let finding = classify_change_with_context(
        Path::new("src/text.py"),
        3,
        "return s.strip()",
        Some("return s.lstrip()"),
        &owners,
        &tests,
        PythonNoBehaviorContext {
            new_line_in_multiline_string: true,
            ..PythonNoBehaviorContext::default()
        },
        Some(source),
    )
    .ok_or_else(|| "a call change should classify".to_string())?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_prior_use_spelled_with_a_normalized_identifier_keeps_the_sort_exposed() -> Result<(), String> {
    // #16221: Python NFKC-normalises identifiers, so `K` and `K` are the same
    // parameter. The body appends to the list before the sort, so the sort
    // sees `[4, 9]` and the direction decides the result. The raw prior-use
    // scan must not miss the append and reason from the original singleton.
    let source = "def top(K):\n    K.append(9)\n    return sorted(K)[0]\n";
    let finding = classify_source(
        source,
        3,
        "    return sorted(K)[0]",
        "    return sorted(K, reverse=True)[0]",
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn only_a_lone_reverse_flag_flip_on_a_named_sort_is_read() {
    assert_eq!(
        call_variant("return sorted(xs)[0]", "return sorted(xs, reverse=True)[0]"),
        Some(CallVariant::Sort {
            param: "xs".to_string(),
            old_call: "sorted(xs)".to_string(),
            new_call: "sorted(xs, reverse=True)".to_string(),
        })
    );
    assert_eq!(
        call_variant("    xs.sort(reverse=True)", "    xs.sort()"),
        Some(CallVariant::Sort {
            param: "xs".to_string(),
            old_call: "xs.sort(reverse=True)".to_string(),
            new_call: "xs.sort()".to_string(),
        })
    );
    // A key changes the order the flag reverses.
    assert_eq!(
        call_variant(
            "return sorted(xs, key=len)",
            "return sorted(xs, key=len, reverse=True)"
        ),
        None
    );
    // An explicit flag that keeps its value is no change of direction.
    assert_eq!(
        call_variant("return sorted(xs)", "return sorted(xs, reverse=False)"),
        None
    );
    // A computed argument, an attribute receiver, or a module's own sort.
    assert_eq!(
        call_variant(
            "return sorted(xs + ys)",
            "return sorted(xs + ys, reverse=True)"
        ),
        None
    );
    assert_eq!(
        call_variant("    self.xs.sort()", "    self.xs.sort(reverse=True)"),
        None
    );
    assert_eq!(
        call_variant("return np.sorted(xs)", "return np.sorted(xs, reverse=True)"),
        None
    );
    // Switching between the two forms, or another edit beside the flag.
    assert_eq!(
        call_variant("return sorted(xs)", "return xs.sort(reverse=True)"),
        None
    );
    assert_eq!(
        call_variant("return sorted(xs)[0]", "return sorted(xs, reverse=True)[1]"),
        None
    );
    // Two sort calls on the line.
    assert_eq!(
        call_variant(
            "return sorted(xs) + sorted(xs)",
            "return sorted(xs, reverse=True) + sorted(xs)"
        ),
        None
    );
}

#[test]
fn sorting_agrees_only_without_two_different_elements() {
    let list = |items: &[&str]| {
        items
            .iter()
            .map(|item| item.to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(elements_agree(&list(&[])), Some(true));
    assert_eq!(elements_agree(&list(&["4"])), Some(true));
    assert_eq!(elements_agree(&list(&["2", "2"])), Some(true));
    assert_eq!(elements_agree(&list(&["1_000", "1000"])), Some(true));
    // An int and a float compare as Python compares them (#16221): `2**53 + 1`
    // is not its float spelling, which names `2**53`, and `2 == 2.0`.
    assert_eq!(
        elements_agree(&list(&["9007199254740993", "9007199254740993.0"])),
        Some(false)
    );
    assert_eq!(elements_agree(&list(&["2", "2.0"])), Some(true));
    assert_eq!(elements_agree(&list(&["\"a\"", "'a'"])), Some(true));
    assert_eq!(elements_agree(&list(&["4", "1"])), Some(false));
    assert_eq!(elements_agree(&list(&["\"a\"", "\"b\""])), Some(false));
    // Two strings whose bodies are spelled alike are one element, whatever
    // quotes hold them. An escape is compared as spelled, not decoded: two
    // spellings of one text (`"a\n"` and `"a\x0a"`) read as two elements,
    // which keeps the finding rather than downgrading it.
    assert_eq!(elements_agree(&list(&["\"a\\n\"", "'a\\n'"])), Some(true));
    assert_eq!(
        elements_agree(&list(&["\"a\\n\"", "\"a\\x0a\""])),
        Some(false)
    );
}

#[test]
fn a_module_round_or_sorted_keeps_its_reading() -> Result<(), String> {
    // The module's own `round` adds its digits, so `money(1.5)` gives 3.5
    // before and 4.5 after; the builtin model must not be applied.
    let round_source =
        "def round(x, n):\n    return x + n\n\n\ndef money(x):\n    return round(x, 2)\n";
    let finding = classify_in(
        round_source,
        6,
        "    return round(x, 2)",
        "    return round(x, 3)",
        "from src.text import money\n\n\ndef test_money():\n    assert money(1.5) == 3.5\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // The module's own `sorted` ignores the flag here, but the rule cannot
    // know what a rebound name does, so it leaves the reading alone.
    let sorted_source = "def sorted(xs, reverse=False):\n    return list(xs)\n\n\ndef top(xs):\n    return sorted(xs)[0]\n";
    let finding = classify_in(
        sorted_source,
        6,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    // The same sort with the builtin intact still reads weakly.
    let finding = classify_in(
        "def top(xs):\n    return sorted(xs)[0]\n",
        2,
        "    return sorted(xs)[0]",
        "    return sorted(xs, reverse=True)[0]",
        "from src.text import top\n\n\ndef test_top():\n    assert top([4]) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    Ok(())
}

#[test]
fn a_parametrized_list_keeps_a_sort_direction_flip_exposed() -> Result<(), String> {
    // The test changes the decorator's list in place before the call.
    let finding = classify_top(
        "import pytest\n\nfrom src.text import top\n\n\n@pytest.mark.parametrize(\"xs\", [[4]])\ndef test_top(xs):\n    xs.append(9)\n    assert top(xs) == 4\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_recursive_owner_keeps_its_reading() -> Result<(), String> {
    // `tidy("a!")` recurses with `"a "`, where `strip` and `lstrip` differ.
    let source = "def tidy(s):\n    if s.endswith(\"!\"):\n        return tidy(s[:-1] + \" \")\n    return s.strip()\n";
    let finding = classify_in(
        source,
        4,
        "    return s.strip()",
        "    return s.lstrip()",
        "from src.text import tidy\n\n\ndef test_tidy():\n    assert tidy(\"a!\") == \"a\"\n",
    )?;
    assert_eq!(finding.class, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_call_shaped_line_inside_a_multiline_string_is_not_read_as_a_call() -> Result<(), String> {
    let root = unique_tempdir("call-variant-multiline-string")?;
    let production = PathBuf::from("src/text.py");
    write_file(
        &root.join(&production),
        "def template(s):\n    return \"\"\"\n    return s.lstrip()\n    \"\"\".strip() + s\n",
    )?;
    write_file(
        &root.join("tests/test_text.py"),
        "from src.text import template\n\n\ndef test_template():\n    assert template(\"a\") == \"return s.strip()a\"\n",
    )?;
    let options = AnalysisOptions {
        root: root.clone(),
        base: None,
        diff_file: None,
        mode: crate::analysis::AnalysisMode::Draft,
        include_unchanged_tests: false,
        resolve_tsconfig_paths: false,
        perl_facts_path: None,
        perl_producer_failure: None,
        git_timeout: None,
        git_candidate: None,
        production_like_targets: Default::default(),
        test_harnesses: Vec::new(),
        resolved_subject_identity: None,
        open_rust_index_paths: Default::default(),
    };
    let changed_line = |text: &str| crate::analysis::diff::ChangedLine {
        line: 3,
        new_side_line: 3,
        text: text.to_string(),
    };
    let changed_files = vec![ChangedFile {
        path: production,
        added_lines: vec![changed_line("    return s.lstrip()")],
        removed_lines: vec![changed_line("    return s.strip()")],
    }];
    let result = PythonAdapter.analyze_diff(&options, &OraclePolicy::default(), &changed_files);
    let cleanup = std::fs::remove_dir_all(&root);
    let result = result?;
    cleanup.map_err(|err| format!("remove_dir_all({}): {err}", root.display()))?;
    let read_as_call = result.findings.iter().any(|finding| {
        finding
            .activation
            .missing_discriminators
            .iter()
            .any(|fact| fact.value.contains("trailing whitespace"))
    });
    assert!(!read_as_call, "{:#?}", result.findings);
    Ok(())
}
