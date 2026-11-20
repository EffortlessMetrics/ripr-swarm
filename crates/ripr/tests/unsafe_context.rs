#![cfg(feature = "lang-rust")]

use ripr::app::{CheckInput, OutputFormat, check_workspace, render_check};
use std::path::Path;

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
        let report = check_workspace(CheckInput {
            root: root.clone(),
            diff_file: Some(root.join(diff)),
            ..CheckInput::default()
        })?;
        let mut actual = report
            .findings
            .iter()
            .map(|finding| {
                let probe = &finding.probe;
                (
                    probe.location.line,
                    probe.family.as_str(),
                    probe.expression.as_str(),
                )
            })
            .collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(actual, expected, "{diff}");
        if diff == "predicate.diff" {
            assert!(
                report.findings[0]
                    .related_tests
                    .iter()
                    .any(|test| test.name == "checks_boundary")
            );
        }
        if let [(_, _, expression)] = expected {
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
