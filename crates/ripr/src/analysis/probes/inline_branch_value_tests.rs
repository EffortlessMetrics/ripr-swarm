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
    assert!(!rows.is_empty(), "the changed line keeps a probe");
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
