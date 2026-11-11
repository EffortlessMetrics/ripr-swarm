//! The `check_diff` library example runs and shows what it claims (#14098).
//!
//! The example is compiled into this test crate by path, so the code a reader
//! runs with `cargo run --example check_diff` is the code checked here. Each
//! example has its own test file because all three load the shared
//! `examples/support/sample_workspace.rs` module, and one crate may load a
//! file as a module only once. A wrong bundled diff or a scan of the
//! wrong root changes the finding lines pinned below.

#[path = "../examples/check_diff.rs"]
mod check_diff;

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
