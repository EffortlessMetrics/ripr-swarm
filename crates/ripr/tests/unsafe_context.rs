#![cfg(feature = "lang-rust")]

use ripr::app::{CheckInput, CheckOutput, OutputFormat, check_workspace, render_check};
use std::path::Path;

fn check(root: &Path, diff: &str) -> Result<CheckOutput, String> {
    check_workspace(CheckInput {
        root: root.to_path_buf(),
        diff_file: Some(root.join(diff)),
        ..CheckInput::default()
    })
}

/// The findings of a report as (line, family, expression), sorted.
fn finding_rows(report: &CheckOutput) -> Vec<(usize, String, String)> {
    let mut rows = report
        .findings
        .iter()
        .map(|finding| {
            let probe = &finding.probe;
            (
                probe.location.line,
                probe.family.as_str().to_owned(),
                probe.expression.as_str().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    rows.sort_unstable();
    rows
}

#[test]
fn unsafe_context_keeps_the_edited_behavior_primary() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unsafe_context");
    let predicate = (3, "predicate", "value >= 3");
    let returned = (12, "return_value", "return value + 2");
    let inline = (19, "return_value", "unsafe { value + 2 }");
    let cases: &[(&str, &[_])] = &[
        ("predicate.diff", &[predicate]),
        (
            "adjacent.diff",
            &[
                (25, "predicate", "enabled"),
                (25, "static_unknown", "unsafe block"),
            ],
        ),
        ("added.diff", &[(25, "predicate", "enabled")]),
        (
            "unpaired.diff",
            &[
                (25, "predicate", "enabled"),
                (25, "static_unknown", "unsafe block"),
                (25, "static_unknown", "value += 1;"),
            ],
        ),
        ("returned.diff", &[returned]),
        ("inline.diff", &[inline]),
        ("spacing.diff", &[inline]),
        ("comment.diff", &[inline]),
        (
            "contract.diff",
            &[(10, "static_unknown", "unsafe fn score")],
        ),
        (
            "wrap.diff",
            &[inline, (19, "static_unknown", "unsafe block")],
        ),
        (
            "mixed.diff",
            &[
                predicate,
                returned,
                (14, "static_unknown", "unsafe fn score"),
            ],
        ),
    ];
    for &(diff, expected) in cases {
        let report = check(&root, diff)?;
        let expected = expected
            .iter()
            .map(|&(line, family, expression)| (line, family.to_owned(), expression.to_owned()))
            .collect::<Vec<_>>();
        assert_eq!(finding_rows(&report), expected, "{diff}");
        if diff == "predicate.diff" {
            assert!(
                report.findings[0]
                    .related_tests
                    .iter()
                    .any(|test| test.name == "checks_boundary")
            );
        }
        if let [(_, _, expression)] = expected.as_slice() {
            let digest = render_check(&report, &OutputFormat::Human)?;
            assert!(digest.contains("Start here:"), "{diff}: {digest}");
            assert!(
                digest.contains(&format!("Changed behavior: {expression}")),
                "{diff}: {digest}"
            );
        }
    }
    Ok(())
}

#[test]
fn unsafe_context_keeps_one_note_per_boundary_and_hides_none_behind_a_filtered_shape()
-> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unsafe_context");
    // Two lines of one block change and neither has a classified probe: the
    // block keeps its single note, on the first of them.
    let report = check(&root, "twice.diff")?;
    assert_eq!(
        finding_rows(&report),
        vec![(32, "static_unknown".to_owned(), "unsafe block".to_owned())],
        "twice.diff"
    );
    // A return that only respells its integer is filtered out before it is
    // emitted, so it is no classified probe and cannot hide the note. The
    // case has a function of its own: the fixture file is the added side.
    let report = check(&root, "respelled.diff")?;
    assert_eq!(
        finding_rows(&report),
        vec![(39, "static_unknown".to_owned(), "unsafe block".to_owned())],
        "respelled.diff"
    );
    Ok(())
}

#[test]
fn every_diff_names_the_fixture_file_as_its_added_side() -> Result<(), Box<dyn std::error::Error>> {
    // A probe is read from the file on disk. A diff whose added line is not
    // the fixture's line would be checked against a line the fixture does not
    // have, and could pass for a reason other than the rule it pins.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unsafe_context");
    let lines = std::fs::read_to_string(root.join("src/lib.rs"))?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut diffs = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "diff")
        {
            diffs.push(path);
        }
    }
    diffs.sort();
    assert!(!diffs.is_empty());
    for diff in &diffs {
        let name = diff
            .file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default();
        let mut next_added_line = None;
        for text in std::fs::read_to_string(diff)?.lines() {
            if let Some(after_minus) = text.strip_prefix("@@ -") {
                let plus_side = after_minus.split_once(" +").map(|(_, plus)| plus);
                next_added_line = plus_side
                    .and_then(|plus| plus.split(' ').next())
                    .and_then(|number| number.parse::<usize>().ok());
                continue;
            }
            if let (Some(line), Some(added)) = (next_added_line, text.strip_prefix('+')) {
                assert_eq!(
                    lines.get(line - 1),
                    Some(&added.to_owned()),
                    "{name}: line {line}"
                );
                next_added_line = Some(line + 1);
            }
        }
    }
    Ok(())
}
