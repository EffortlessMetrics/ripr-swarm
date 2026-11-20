//! Pins for the literal-collection membership member rule (#15632).
//!
//! An edit to the members of `val in (...)` credits `exposed` only when a
//! strong related call passes an altered member; each case below has a
//! control that keeps the credit once such an input exists.

use super::classify::{PythonNoBehaviorContext, classify_change_with_context};
use super::owners_tests::{extract_owners, extract_tests};
use super::*;
use std::path::Path;

const OWNER_FILE: &str = "src/conv.py";
const TEST_FILE: &str = "tests/test_conv.py";
const IMPORTS: &str = "import pytest\nfrom src.conv import to_bool";

/// The attrs `to_bool` shape after removing `"y"` from the truthy set; the
/// changed line is line 5.
const TO_BOOL: &str = "def to_bool(val):\n    if isinstance(val, str):\n        val = val.lower()\n\n    if val in (True, \"true\", \"t\", \"yes\", \"on\", \"1\", 1):\n        return True\n    if val in (False, \"false\", \"f\", \"no\", \"n\", \"off\", \"0\", 0):\n        return False\n\n    raise ValueError(val)\n";
const TO_BOOL_OLD: &str =
    "    if val in (True, \"true\", \"t\", \"yes\", \"y\", \"on\", \"1\", 1):";

fn finding_at(
    owner: &str,
    line: usize,
    old_line: &str,
    test_body: &str,
) -> Result<Finding, String> {
    let new_line = owner
        .lines()
        .nth(line - 1)
        .ok_or_else(|| format!("owner has no line {line}"))?;
    let owners = extract_owners(Path::new(OWNER_FILE), owner);
    let tests = extract_tests(Path::new(TEST_FILE), &format!("{IMPORTS}\n\n\n{test_body}"));
    if tests.is_empty() {
        return Err("fixture should parse at least one test".to_string());
    }
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
    .ok_or_else(|| "changed line should classify".to_string())
}

fn class_at(
    owner: &str,
    line: usize,
    old_line: &str,
    test_body: &str,
) -> Result<ExposureClass, String> {
    finding_at(owner, line, old_line, test_body).map(|finding| finding.class)
}

fn discriminators(finding: &Finding) -> Vec<String> {
    finding
        .activation
        .missing_discriminators
        .iter()
        .map(|fact| fact.value.clone())
        .collect()
}

fn truthy_cases(cases: &str) -> String {
    format!(
        "@pytest.mark.parametrize(\"val\", [{cases}])\ndef test_truthy(val):\n    assert to_bool(val) == True\n"
    )
}

#[test]
fn a_removed_member_no_case_passes_does_not_credit() -> Result<(), String> {
    let body = truthy_cases("\"true\", \"t\", \"yes\", \"on\", \"1\"");
    let finding = finding_at(TO_BOOL, 5, TO_BOOL_OLD, &body)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(discriminators(&finding), vec!["val == \"y\"".to_string()]);
    // A case that passes the removed member discriminates the edit.
    let reached = truthy_cases("\"true\", \"y\", \"on\"");
    assert_eq!(
        class_at(TO_BOOL, 5, TO_BOOL_OLD, &reached)?,
        ExposureClass::Exposed
    );
    Ok(())
}

#[test]
fn a_case_the_normalization_changes_is_not_placed_on_the_member() -> Result<(), String> {
    // `"Y"` lowers to `"y"` before the test, so it never reaches a removed
    // `"Y"` member, and it is not placed on a removed `"y"` either.
    let old_upper = "    if val in (True, \"true\", \"t\", \"yes\", \"Y\", \"on\", \"1\", 1):";
    let body = truthy_cases("\"true\", \"Y\"");
    assert_eq!(
        class_at(TO_BOOL, 5, old_upper, &body)?,
        ExposureClass::WeaklyExposed
    );
    assert_eq!(
        class_at(TO_BOOL, 5, TO_BOOL_OLD, &body)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

#[test]
fn a_non_string_counts_only_under_an_isinstance_guarded_normalization() -> Result<(), String> {
    let old = "    if val in (True, \"true\", \"t\", \"yes\", \"on\", \"1\", 1, 2):";
    let body = truthy_cases("\"yes\", 2");
    assert_eq!(class_at(TO_BOOL, 5, old, &body)?, ExposureClass::Exposed);
    // Without the guard `2 .lower()` raises before the membership test.
    let unguarded = "def to_bool(val):\n    val = val.lower()\n    if val in (\"yes\", \"on\"):\n        return True\n    return False\n";
    let old = "    if val in (\"yes\", \"on\", 2):";
    assert_eq!(
        class_at(unguarded, 3, old, &body)?,
        ExposureClass::WeaklyExposed
    );
    Ok(())
}

const PLAIN: &str = "def to_bool(val):\n    if val in (True, \"yes\", \"on\"):\n        return True\n    return False\n";

#[test]
fn an_added_member_needs_a_case_that_passes_it() -> Result<(), String> {
    let old = "    if val in (True, \"yes\"):";
    let missed = truthy_cases("True, \"yes\", \"off\"");
    let finding = finding_at(PLAIN, 2, old, &missed)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(discriminators(&finding), vec!["val == \"on\"".to_string()]);
    let reached = truthy_cases("\"yes\", \"on\"");
    assert_eq!(class_at(PLAIN, 2, old, &reached)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_changed_member_needs_a_case_for_each_side() -> Result<(), String> {
    let old = "    if val in (True, \"yes\", \"y\"):";
    // Only the removed `"y"` is passed; the added `"on"` is not.
    let half = truthy_cases("\"yes\", \"y\"");
    let finding = finding_at(PLAIN, 2, old, &half)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(discriminators(&finding), vec!["val == \"on\"".to_string()]);
    let both = truthy_cases("\"y\", \"on\"");
    assert_eq!(class_at(PLAIN, 2, old, &both)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn not_in_list_and_set_displays_read_the_same_way() -> Result<(), String> {
    for (new, old) in [
        (
            "    if val not in [\"yes\", \"on\"]:",
            "    if val not in [\"yes\", \"y\", \"on\"]:",
        ),
        (
            "    if val in {\"yes\", \"on\"}:",
            "    if val in {\"yes\", \"y\", \"on\"}:",
        ),
    ] {
        let owner = format!("def to_bool(val):\n{new}\n        return True\n    return False\n");
        let missed = truthy_cases("\"yes\", \"no\"");
        let finding = finding_at(&owner, 2, old, &missed)?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{new}");
        assert_eq!(discriminators(&finding), vec!["val == \"y\"".to_string()]);
        let reached = truthy_cases("\"yes\", \"y\"");
        assert_eq!(
            class_at(&owner, 2, old, &reached)?,
            ExposureClass::Exposed,
            "{new}"
        );
    }
    Ok(())
}

#[test]
fn a_member_an_equal_bool_still_holds_is_not_hit_by_its_alias() -> Result<(), String> {
    // Removing `"y"` and `1` while `True` stays: a case passing `1` still
    // takes the branch (`1 == True`), so it does not discriminate the edit.
    let owner = "def to_bool(val):\n    if val in (True, \"yes\"):\n        return True\n    return False\n";
    let old = "    if val in (True, \"yes\", \"y\", 1):";
    let aliased = truthy_cases("\"yes\", 1");
    let finding = finding_at(owner, 2, old, &aliased)?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert_eq!(discriminators(&finding), vec!["val == \"y\"".to_string()]);
    let reached = truthy_cases("\"yes\", \"y\", 1");
    assert_eq!(class_at(owner, 2, old, &reached)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_subject_that_is_not_a_parameter_keeps_the_existing_verdict() -> Result<(), String> {
    let owner = "MODE = \"yes\"\n\n\ndef to_bool(val):\n    if MODE in (\"yes\", \"on\"):\n        return val\n    return None\n";
    let old = "    if MODE in (\"yes\", \"y\", \"on\"):";
    let body = "def test_truthy():\n    assert to_bool(3) == 3\n";
    assert_eq!(class_at(owner, 5, old, body)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_non_literal_display_keeps_the_existing_verdict() -> Result<(), String> {
    let owner = "TRUTHY = (\"yes\", \"on\")\n\n\ndef to_bool(val):\n    if val in TRUTHY + (\"t\",):\n        return True\n    return False\n";
    let old = "    if val in TRUTHY + (\"t\", \"y\"):";
    let body = truthy_cases("\"yes\", \"t\"");
    assert_eq!(class_at(owner, 5, old, &body)?, ExposureClass::Exposed);
    Ok(())
}

#[test]
fn a_whole_collection_replacement_keeps_the_existing_verdict() -> Result<(), String> {
    let old = "    if val in (\"y\", \"n\"):";
    let body = truthy_cases("True, \"yes\"");
    assert_eq!(class_at(PLAIN, 2, old, &body)?, ExposureClass::Exposed);
    Ok(())
}

/// `owner` reads `weakly_exposed` naming `discriminator` when the rows pass
/// only `missed`, and `exposed` once `hit` passes the altered member.
fn pins_member(
    owner: &str,
    line: usize,
    old: &str,
    (missed, hit): (&str, &str),
    discriminator: &str,
) -> Result<(), String> {
    let finding = finding_at(owner, line, old, &truthy_cases(missed))?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{owner}");
    assert_eq!(
        discriminators(&finding),
        vec![discriminator.to_string()],
        "{owner}"
    );
    assert_eq!(
        class_at(owner, line, old, &truthy_cases(hit))?,
        ExposureClass::Exposed,
        "{owner}"
    );
    Ok(())
}

const CASES: (&str, &str) = ("\"yes\", \"on\"", "\"yes\", \"y\"");

#[test]
fn a_compound_or_wrapped_condition_reads_the_same_way() -> Result<(), String> {
    for (new, old) in [
        (
            "    if val in (True, \"yes\", \"on\") and flag:",
            "    if val in (True, \"yes\", \"y\", \"on\") and flag:",
        ),
        (
            "    if flag or val in (True, \"yes\", \"on\"):",
            "    if flag or val in (True, \"yes\", \"y\", \"on\"):",
        ),
        (
            "    if (val in (True, \"yes\", \"on\")):",
            "    if (val in (True, \"yes\", \"y\", \"on\")):",
        ),
    ] {
        let owner = format!(
            "def to_bool(val, flag=False):\n{new}\n        return True\n    return False\n"
        );
        pins_member(&owner, 2, old, CASES, "val == \"y\"")?;
    }
    // `flag` defaults to True here, so `and flag` holds for every row.
    let owner = "def to_bool(val, flag=True):\n    if val in (True, \"yes\", \"on\") and flag:\n        return True\n    return False\n";
    pins_member(
        owner,
        2,
        "    if val in (True, \"yes\", \"y\", \"on\") and flag:",
        CASES,
        "val == \"y\"",
    )
}

#[test]
fn a_negated_membership_reads_the_same_way() -> Result<(), String> {
    let owner = "def to_bool(val):\n    if not val in (True, \"yes\", \"on\"):\n        return False\n    return True\n";
    pins_member(
        owner,
        2,
        "    if not val in (True, \"yes\", \"y\", \"on\"):",
        CASES,
        "val == \"y\"",
    )
}

#[test]
fn a_returned_or_assigned_membership_reads_the_same_way() -> Result<(), String> {
    let returned = "def to_bool(val):\n    return val in (True, \"yes\", \"on\")\n";
    pins_member(
        returned,
        2,
        "    return val in (True, \"yes\", \"y\", \"on\")",
        CASES,
        "val == \"y\"",
    )?;
    let assigned = "def to_bool(val):\n    ok = val in (True, \"yes\", \"on\")\n    return ok\n";
    pins_member(
        assigned,
        2,
        "    ok = val in (True, \"yes\", \"y\", \"on\")",
        CASES,
        "val == \"y\"",
    )
}

#[test]
fn a_one_line_if_and_an_elif_read_the_same_way() -> Result<(), String> {
    let one_line =
        "def to_bool(val):\n    if val in (True, \"yes\", \"on\"): return True\n    return False\n";
    pins_member(
        one_line,
        2,
        "    if val in (True, \"yes\", \"y\", \"on\"): return True",
        CASES,
        "val == \"y\"",
    )?;
    let elif = "def to_bool(val):\n    if val is None:\n        return False\n    elif val in (True, \"yes\", \"on\"):\n        return True\n    return False\n";
    pins_member(
        elif,
        4,
        "    elif val in (True, \"yes\", \"y\", \"on\"):",
        CASES,
        "val == \"y\"",
    )
}

#[test]
fn a_transformed_subject_places_only_strings_it_leaves_unchanged() -> Result<(), String> {
    for (subject, discriminator) in [
        ("val.lower()", "val.lower() == \"y\""),
        ("val.casefold()", "val.casefold() == \"y\""),
        ("val.strip()", "val.strip() == \"y\""),
        ("str(val)", "str(val) == \"y\""),
    ] {
        let owner = format!(
            "def to_bool(val):\n    if {subject} in (\"yes\", \"on\"):\n        return True\n    return False\n"
        );
        let old = format!("    if {subject} in (\"yes\", \"y\", \"on\"):");
        pins_member(&owner, 2, &old, CASES, discriminator)?;
    }
    // A row equal to the removed member that the transform changes never
    // compares as that member (`"Y".lower()` is `"y"`), and a non-string row
    // is never placed under a transforming subject (`str(1)` is `"1"`).
    for (subject, member) in [
        ("val.lower()", "\"Y\""),
        ("val.strip()", "\" y\""),
        ("str(val)", "1"),
    ] {
        let owner = format!(
            "def to_bool(val):\n    if {subject} in (\"yes\", \"on\"):\n        return True\n    return False\n"
        );
        let old = format!("    if {subject} in (\"yes\", {member}, \"on\"):");
        let body = truthy_cases(&format!("\"yes\", {member}"));
        assert_eq!(
            class_at(&owner, 2, &old, &body)?,
            ExposureClass::WeaklyExposed,
            "{subject}"
        );
    }
    Ok(())
}

#[test]
fn a_member_line_inside_a_multi_line_display_reads_the_same_way() -> Result<(), String> {
    let owner = "def to_bool(val):\n    if val in (\n        True,\n        \"yes\",\n        \"on\",\n    ):\n        return True\n    return False\n";
    pins_member(owner, 4, "        \"yes\", \"y\",", CASES, "val == \"y\"")?;
    // A changed member line in the middle of the display.
    pins_member(
        owner,
        4,
        "        \"y\",",
        ("\"on\", \"yes\"", "\"y\", \"yes\""),
        "val == \"y\"",
    )
}

#[test]
fn an_unmodelled_rebinding_withholds_credit_even_when_a_row_passes_the_member() -> Result<(), String>
{
    let owner = "def to_bool(val):\n    val = normalize(val)\n    if val in (True, \"yes\", \"on\"):\n        return True\n    return False\n";
    let old = "    if val in (True, \"yes\", \"y\", \"on\"):";
    for cases in [CASES.0, CASES.1] {
        let finding = finding_at(owner, 3, old, &truthy_cases(cases))?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{cases}");
        assert_eq!(discriminators(&finding), vec!["val == \"y\"".to_string()]);
    }
    Ok(())
}

#[test]
fn a_line_that_also_changes_outside_the_display_withholds_credit() -> Result<(), String> {
    // A second edit on the line: even a row that passes the removed member
    // does not establish that the whole edit is observed.
    for (new, old) in [
        (
            "    if val in (True, \"yes\", \"on\") and flag:",
            "    if val in (True, \"yes\", \"y\", \"on\") or flag:",
        ),
        (
            "    if not val in (True, \"yes\", \"on\"):",
            "    if val in (True, \"yes\", \"y\", \"on\"):",
        ),
        (
            "    if (val in (True, \"yes\", \"on\")) and flag:",
            "    if val in (True, \"yes\", \"y\", \"on\") and flag:",
        ),
        (
            "    if val in (True, \"yes\", \"on\"): return False",
            "    if val in (True, \"yes\", \"y\", \"on\"): return True",
        ),
    ] {
        let body = if new.ends_with(':') {
            "        return True\n    return False\n"
        } else {
            "    return False\n"
        };
        let owner = format!("def to_bool(val, flag=True):\n{new}\n{body}");
        for cases in [CASES.0, CASES.1] {
            let finding = finding_at(&owner, 2, old, &truthy_cases(cases))?;
            assert_eq!(
                finding.class,
                ExposureClass::WeaklyExposed,
                "{new} / {cases}"
            );
            assert_eq!(
                discriminators(&finding),
                vec!["val == \"y\"".to_string()],
                "{new}"
            );
        }
    }
    Ok(())
}

#[test]
fn an_unreadable_member_withholds_credit() -> Result<(), String> {
    let owner = "def to_bool(val):\n    if val in (True, \"yes\", \"on\"):\n        return True\n    return False\n";
    // Removing an escaped string: static evidence does not read `"\t"`.
    let escaped = "    if val in (True, \"yes\", \"\\t\", \"on\"):";
    for cases in [CASES.0, "\"yes\", \"\\t\""] {
        assert_eq!(
            class_at(owner, 2, escaped, &truthy_cases(cases))?,
            ExposureClass::WeaklyExposed,
            "{cases}"
        );
    }
    // Adding a name: no readable member to name, still no credit.
    let named = "EXTRA = \"y\"\n\n\ndef to_bool(val):\n    if val in (True, \"yes\", \"on\", EXTRA):\n        return True\n    return False\n";
    let finding = finding_at(
        named,
        5,
        "    if val in (True, \"yes\", \"on\"):",
        &truthy_cases(CASES.1),
    )?;
    assert_eq!(finding.class, ExposureClass::WeaklyExposed);
    assert!(discriminators(&finding).is_empty(), "{finding:?}");
    Ok(())
}

#[test]
fn a_member_line_whose_old_side_cannot_be_paired_withholds_credit() -> Result<(), String> {
    // The old line also closed the display, so the restored old source does
    // not parse; the changed line still only edits literal members.
    let owner = "def to_bool(val):\n    if val in (\n        True,\n        \"yes\",\n        \"on\",\n    ):\n        return True\n    return False\n";
    for cases in [CASES.0, CASES.1] {
        let finding = finding_at(owner, 4, "        \"yes\", \"y\",)", &truthy_cases(cases))?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{cases}");
        assert_eq!(discriminators(&finding), vec!["val == \"y\"".to_string()]);
    }
    Ok(())
}

#[test]
fn a_walrus_or_while_body_rebinding_withholds_credit_even_when_a_row_passes_the_member()
-> Result<(), String> {
    // A walrus on the comparison line rebinds the subject before `in` reads
    // it, and a `while` comparison re-reads it after its body rebinds it.
    let walrus = "def to_bool(val):\n    if (val := normalize(val)) and val in (True, \"yes\", \"on\"):\n        return True\n    return False\n";
    let walrus_old = "    if (val := normalize(val)) and val in (True, \"yes\", \"y\", \"on\"):";
    let looped = "def to_bool(val):\n    while val in (True, \"yes\", \"on\"):\n        val = normalize(val)\n    return False\n";
    let looped_old = "    while val in (True, \"yes\", \"y\", \"on\"):";
    let chained =
        "def to_bool(val):\n    val = normalize(val); return val in (True, \"yes\", \"on\")\n";
    let chained_old = "    val = normalize(val); return val in (True, \"yes\", \"y\", \"on\")";
    for (owner, old) in [
        (walrus, walrus_old),
        (looped, looped_old),
        (chained, chained_old),
    ] {
        let finding = finding_at(owner, 2, old, &truthy_cases(CASES.1))?;
        assert_eq!(finding.class, ExposureClass::WeaklyExposed, "{owner}");
        assert_eq!(discriminators(&finding), vec!["val == \"y\"".to_string()]);
    }
    // Control: a plain self-assignment binds only after `in` has read the
    // call input, so a row passing the removed member keeps the credit.
    let assigned = "def to_bool(val):\n    val = val in (True, \"yes\", \"on\")\n    return val\n";
    let assigned_old = "    val = val in (True, \"yes\", \"y\", \"on\")";
    assert_eq!(
        class_at(assigned, 2, assigned_old, &truthy_cases(CASES.1))?,
        ExposureClass::Exposed
    );
    // Control: the same `while` without a body rebinding keeps the credit.
    let plain = "def to_bool(val):\n    while val in (True, \"yes\", \"on\"):\n        return True\n    return False\n";
    assert_eq!(
        class_at(plain, 2, looped_old, &truthy_cases(CASES.1))?,
        ExposureClass::Exposed
    );
    Ok(())
}
