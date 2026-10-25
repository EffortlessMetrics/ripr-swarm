//! Per-file identifier-write summaries must stay linear in the number of
//! writes (#10017): a 5 MB minified test file has tens of thousands of them,
//! and the earlier per-name rescan made extraction cost ~190 s in debug.

use super::tests_extract::{
    assigned_identifier_names, identifier_writes, rewritten_identifier_names,
    strict_identifier_writes,
};

/// The definition the summaries must keep: names in first-write order, the
/// rewritten ones being those with at least two strict writes.
fn reference_rewritten(text: &str) -> Vec<String> {
    let writes = strict_identifier_writes(text);
    let mut names: Vec<String> = Vec::new();
    for (name, _) in &writes {
        if !names.contains(name) && writes.iter().filter(|(other, _)| other == name).count() > 1 {
            names.push(name.clone());
        }
    }
    names
}

fn reference_assigned(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (name, _) in identifier_writes(text) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

#[test]
fn summaries_match_the_reference_definition_in_first_write_order() {
    let sources = [
        "",
        "let a = 1; a = 2; let b = 3; c = 4; c += 1; b = 5; d++;",
        "let z = 1; let y = 2; y = 3; z = 4; z = 5; for (let i = 0; i < 3; i++) {}",
        "const only = 1; let w; w = 1; ({ w } = other); [w] = [2];",
        "x = 1; x = 2; x = 3; y = 1; x = 4; y = 2;",
    ];
    for source in sources {
        assert_eq!(
            rewritten_identifier_names(source),
            reference_rewritten(source),
            "rewritten names diverged for {source:?}"
        );
        assert_eq!(
            assigned_identifier_names(source),
            reference_assigned(source),
            "assigned names diverged for {source:?}"
        );
    }
    // Control: the named fixtures are not all trivially empty, so the
    // equality above compares real order and membership.
    assert_eq!(
        rewritten_identifier_names("x = 1; x = 2; y = 1; y = 2;"),
        vec!["x".to_string(), "y".to_string()]
    );
}

#[test]
fn many_distinct_writes_summarize_without_a_quadratic_rescan() {
    // 60k distinct single-write names plus two rewritten ones at the end.
    // The old per-name rescan took minutes in debug here; this finishes in
    // well under a second, so a regression shows up as a test timeout.
    let mut text = String::new();
    for index in 0..60_000 {
        text.push_str(&format!("v{index} = {index};"));
    }
    text.push_str("tail = 1; tail = 2; last = 1; last = 2;");
    assert_eq!(
        rewritten_identifier_names(&text),
        vec!["tail".to_string(), "last".to_string()]
    );
    assert_eq!(assigned_identifier_names(&text).len(), 60_002);
}
