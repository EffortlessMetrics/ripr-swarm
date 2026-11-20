//! Split `assertion_not_credited` into inert vs admission-unproven (#5446).
use ripr::domain::RelatedTestMiss;
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, explain_finding_with_input};
use std::path::PathBuf;

fn fixture_input(name: &str) -> CheckInput {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    CheckInput {
        root: root.join("input"),
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    }
}

#[test]
fn shadowed_assert_macro_emits_inert_miss() -> Result<(), String> {
    let output = check_workspace(fixture_input("match_arm_shadowed_assert"))?;
    let finding = output.findings.first().ok_or("expected one finding")?;
    let miss = finding.related_tests[0]
        .miss
        .ok_or("expected a related-test miss")?;
    assert_eq!(
        miss,
        RelatedTestMiss::AssertionInert,
        "shadowed assert! is an inert refusal, not admission-unproven"
    );
    assert_eq!(miss.as_str(), "assertion_inert");
    Ok(())
}

#[test]
fn explain_recommends_replacing_only_the_inert_assertion() -> Result<(), String> {
    let recipe = "use a plain, always-run `assert_eq!`/`assert!`";
    let inert =
        explain_finding_with_input(fixture_input("match_arm_shadowed_assert"), "src/lib.rs:61")?;
    assert!(inert.contains("checks_unknown"), "{inert}");
    assert!(inert.contains(recipe), "{inert}");

    let unknown = explain_finding_with_input(
        fixture_input("rust_macro_wrapped_assertion_limitation"),
        "src/internal.rs:2",
    )?;
    assert!(
        unknown.contains("test_inner_with_custom_assertion_macro"),
        "{unknown}"
    );
    assert!(
        unknown.contains("assertion admission unproven"),
        "{unknown}"
    );
    assert!(!unknown.contains(recipe), "{unknown}");
    assert!(
        unknown.contains("use an input that reaches `a == b`"),
        "{unknown}"
    );
    Ok(())
}
