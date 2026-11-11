//! The library examples under `examples/` run and show what they claim
//! (#14098).
//!
//! Each example is compiled into this test crate by path, so the code a
//! reader runs with `cargo run --example <name>` is the code checked here.
//! The examples analyze a scratch copy of the bundled sample; a malformed
//! bundled diff or a scan of the wrong root shows up as an incomplete
//! analysis or a finding on the wrong line.

#[path = "../examples/check_diff.rs"]
mod check_diff;
#[path = "../examples/explain.rs"]
mod explain;
#[path = "../examples/render_json.rs"]
mod render_json;

/// The finding lines of one `check_diff` section, without their detail lines.
fn section<'a>(report: &'a str, heading: &str) -> Result<Vec<&'a str>, String> {
    let mut lines = report.lines().skip_while(|line| !line.starts_with(heading));
    lines
        .next()
        .ok_or_else(|| format!("no `{heading}` section in:\n{report}"))?;
    Ok(lines
        .take_while(|line| line.starts_with("- ") || line.starts_with("    "))
        .filter(|line| line.starts_with("- "))
        .collect())
}

#[test]
fn check_diff_shows_the_weak_findings_and_the_strengthened_tests_closing_them() -> Result<(), String>
{
    let report = check_diff::report()?;

    let before = section(&report, "tests as shipped")?;
    assert_eq!(before.len(), 4, "{report}");
    assert!(
        before.iter().all(|line| line.ends_with(": weakly_exposed")),
        "{report}"
    );
    assert!(
        before.contains(&"- src/lib.rs:8 predicate: weakly_exposed"),
        "{report}"
    );
    assert!(
        before.contains(&"- src/lib.rs:22 error_path: weakly_exposed"),
        "{report}"
    );
    assert!(
        report.contains("    missing: Missing check value: amount == discount_threshold"),
        "{report}"
    );

    let after = section(&report, "tests strengthened")?;
    assert_eq!(after.len(), 4, "{report}");
    assert!(
        after.contains(&"- src/lib.rs:8 predicate: exposed"),
        "{report}"
    );
    assert!(
        after.contains(&"- src/lib.rs:22 error_path: exposed"),
        "{report}"
    );

    check_diff::main()
}

#[test]
fn explain_names_the_changed_line_the_missing_boundary_and_a_next_step() -> Result<(), String> {
    let explanation = explain::report()?;

    for expected in [
        "./src/lib.rs:8",
        "after:  amount >= discount_threshold",
        "weakly_exposed",
        "missing discriminator amount == discount_threshold",
        "Next step",
    ] {
        assert!(
            explanation.contains(expected),
            "missing `{expected}` in:\n{explanation}"
        );
    }

    explain::main()
}

#[test]
fn render_json_reports_a_complete_analysis_of_the_bundled_diff() -> Result<(), String> {
    let rendered = render_json::report()?;
    let document: serde_json::Value =
        serde_json::from_str(&rendered).map_err(|error| format!("invalid JSON: {error}"))?;

    let outcome = &document["analysis_outcome"];
    assert_eq!(outcome["analysis_complete"], true, "{outcome}");
    assert_eq!(
        outcome["outcome"]["kind"], "complete_with_findings",
        "{outcome}"
    );
    assert_eq!(
        outcome["outcome"]["limitations"],
        serde_json::json!([]),
        "{outcome}"
    );

    let lines: Vec<_> = document["findings"]
        .as_array()
        .ok_or("findings is not an array")?
        .iter()
        .map(|finding| {
            (
                finding["probe"]["file"].as_str().unwrap_or_default(),
                finding["probe"]["line"].as_u64().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        lines,
        [
            ("./src/lib.rs", 8),
            ("./src/lib.rs", 22),
            ("./src/lib.rs", 22),
            ("./src/lib.rs", 22)
        ]
    );

    render_json::main()
}
