//! F1: an edit inside an inline branch value on an `if`'s condition line
//! (`if price > 100 { 10 } else { 0 }` to `{ 15 }`) is probed as the value it
//! changed, never as the unchanged predicate's boundary.

use super::replacement_identity_tests::git_probes;
use crate::domain::ProbeFamily;

fn rows(before: &str, after: &str) -> Result<Vec<(ProbeFamily, String, Option<String>)>, String> {
    Ok(git_probes(before, after)?
        .into_iter()
        .map(|seeded| {
            (
                seeded.probe.family,
                seeded.probe.expression,
                seeded.probe.before,
            )
        })
        .collect())
}

fn owner(body: &str) -> String {
    format!("pub fn discount_cents(price: u32) -> u32 {{\n{body}}}\n")
}

fn flags_owner(body: &str) -> String {
    format!("pub fn pick(a: bool, b: bool, x: u32) -> u32 {{\n{body}}}\n")
}

fn keeps_predicate(rows: &[(ProbeFamily, String, Option<String>)], expression: &str) -> bool {
    rows.iter()
        .any(|(family, text, _)| *family == ProbeFamily::Predicate && text == expression)
}

#[test]
fn an_inline_branch_value_edit_is_the_returned_value_not_the_predicate() -> Result<(), String> {
    let rows = rows(
        &owner("    if price > 100 { 10 } else { 0 }\n"),
        &owner("    if price > 100 { 15 } else { 0 }\n"),
    )?;
    assert_eq!(
        rows,
        vec![(
            ProbeFamily::ReturnValue,
            "15".to_string(),
            Some("10".to_string())
        )],
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn an_inline_else_value_edit_is_the_returned_value_not_the_predicate() -> Result<(), String> {
    let rows = rows(
        &owner("    if price > 100 { 10 } else { 0 }\n"),
        &owner("    if price > 100 { 10 } else { 7 }\n"),
    )?;
    assert_eq!(
        rows,
        vec![(
            ProbeFamily::ReturnValue,
            "7".to_string(),
            Some("0".to_string())
        )],
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn a_changed_predicate_on_the_same_line_keeps_its_predicate_probe() -> Result<(), String> {
    let rows = rows(
        &owner("    if price > 100 { 10 } else { 0 }\n"),
        &owner("    if price >= 100 { 10 } else { 0 }\n"),
    )?;
    assert!(
        rows.iter()
            .any(|(family, expression, _)| *family == ProbeFamily::Predicate
                && expression == "price >= 100"),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .all(|(family, _, _)| *family != ProbeFamily::ReturnValue),
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn an_own_line_branch_value_edit_keeps_its_return_value_probe() -> Result<(), String> {
    let rows = rows(
        &owner("    if price > 100 {\n        10\n    } else {\n        0\n    }\n"),
        &owner("    if price > 100 {\n        15\n    } else {\n        0\n    }\n"),
    )?;
    assert_eq!(
        rows,
        vec![(
            ProbeFamily::ReturnValue,
            "15".to_string(),
            Some("10".to_string())
        )],
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn a_non_returned_inline_branch_value_edit_keeps_its_prior_probe() -> Result<(), String> {
    // A value bound by `let` has no shape of its own to stand for the edit, so
    // the line keeps the reading it had rather than a new, unmodeled one.
    let rows = rows(
        &owner("    let cut = if price > 100 { 10 } else { 0 };\n    log(cut);\n    0\n"),
        &owner("    let cut = if price > 100 { 15 } else { 0 };\n    log(cut);\n    0\n"),
    )?;
    assert!(keeps_predicate(&rows, "price > 100"), "{rows:?}");
    assert!(
        rows.iter()
            .all(|(family, _, _)| *family != ProbeFamily::ReturnValue),
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn an_inline_early_return_edit_is_the_return_not_the_predicate() -> Result<(), String> {
    let rows = rows(
        &owner("    if price > 100 { return 10; }\n    0\n"),
        &owner("    if price > 100 { return 15; }\n    0\n"),
    )?;
    assert!(
        rows.iter().any(
            |(family, expression, _)| *family == ProbeFamily::ReturnValue
                && expression.contains("15")
        ),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .all(|(family, _, _)| *family != ProbeFamily::Predicate),
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn an_edit_in_an_else_if_link_is_not_read_as_the_outer_branch() -> Result<(), String> {
    // The nested `if` holds a predicate of its own; this reader leaves it.
    let rows = rows(
        &owner("    if price > 100 { 10 } else if price > 50 { 5 } else { 0 }\n"),
        &owner("    if price > 100 { 10 } else if price >= 50 { 5 } else { 0 }\n"),
    )?;
    assert!(
        rows.iter()
            .any(|(family, _, _)| *family == ProbeFamily::Predicate),
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn a_shrunk_condition_before_a_returned_if_keeps_its_predicate() -> Result<(), String> {
    // The shared ` { 1 }` tail pulls the deletion of ` && b` (or `0`) onto the `{`
    // byte as an empty edit; it is the condition's change, not the block's,
    // so the spanning `return` shape must not take the line from it.
    for (before, after, predicate) in [
        (
            "    return if x > 1 && b { 1 } else { 0 };\n",
            "    return if x > 1 { 1 } else { 0 };\n",
            "x > 1",
        ),
        (
            "    if x > 10{ 1 } else { 0 }\n",
            "    if x > 1{ 1 } else { 0 }\n",
            "x > 1",
        ),
        (
            "    compute(if x > 1 && b { 1 } else { 0 })\n",
            "    compute(if x > 1 { 1 } else { 0 })\n",
            "x > 1",
        ),
    ] {
        let rows = rows(&flags_owner(before), &flags_owner(after))?;
        assert!(keeps_predicate(&rows, predicate), "{after:?}: {rows:?}");
    }
    Ok(())
}

#[test]
fn a_whitespace_only_branch_edit_is_no_value_edit() -> Result<(), String> {
    let rows = rows(
        &owner("    if price > 100 { 10 } else { 0 }\n"),
        &owner("    if price > 100 { 10} else { 0 }\n"),
    )?;
    assert!(
        rows.iter()
            .all(|(family, _, _)| *family != ProbeFamily::ReturnValue),
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn a_returned_inline_if_value_edit_is_the_return_not_the_predicate() -> Result<(), String> {
    let rows = rows(
        &flags_owner("    return if a { 1 } else { 0 };\n"),
        &flags_owner("    return if a { 2 } else { 0 };\n"),
    )?;
    assert!(
        rows.iter()
            .any(|(family, _, _)| *family == ProbeFamily::ReturnValue),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .all(|(family, _, _)| *family != ProbeFamily::Predicate),
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn a_nested_inline_if_value_edit_in_a_tail_block_is_the_returned_value() -> Result<(), String> {
    let body = |value: &str| {
        flags_owner(&format!(
            "    if a {{\n        if b {{ {value} }} else {{ 2 }}\n    }} else {{\n        0\n    }}\n"
        ))
    };
    let rows = rows(&body("1"), &body("3"))?;
    assert_eq!(
        rows,
        vec![(
            ProbeFamily::ReturnValue,
            "3".to_string(),
            Some("1".to_string())
        )],
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn a_multibyte_line_places_the_value_edit_by_bytes() -> Result<(), String> {
    let body = |value: &str| {
        format!(
            "pub fn tag_cents(name: &str) -> u32 {{\n    if name == \"é\" {{ {value} }} else {{ 0 }}\n}}\n"
        )
    };
    let rows = rows(&body("10"), &body("15"))?;
    assert_eq!(
        rows,
        vec![(
            ProbeFamily::ReturnValue,
            "15".to_string(),
            Some("10".to_string())
        )],
        "{rows:?}"
    );
    Ok(())
}

#[test]
fn a_non_tail_inline_if_statement_keeps_its_predicate() -> Result<(), String> {
    let rows = rows(
        &owner("    if price > 100 { 10 } else { 0 };\n    0\n"),
        &owner("    if price > 100 { 15 } else { 0 };\n    0\n"),
    )?;
    assert!(keeps_predicate(&rows, "price > 100"), "{rows:?}");
    assert!(
        rows.iter()
            .all(|(family, _, _)| *family != ProbeFamily::ReturnValue),
        "{rows:?}"
    );
    Ok(())
}
