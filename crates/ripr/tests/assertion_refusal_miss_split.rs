//! Split `assertion_not_credited` into inert vs admission-unproven (#5446).
use ripr::domain::RelatedTestMiss;
use ripr::{CheckInput, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;

fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

#[test]
fn shadowed_assert_macro_emits_inert_miss() -> Result<(), String> {
    let root = fixture_root("match_arm_shadowed_assert");
    let output = check_workspace(CheckInput {
        root: root.join("input"),
        diff_file: Some(root.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
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
