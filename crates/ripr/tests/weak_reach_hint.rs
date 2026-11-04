//! The public check renderers must not turn uncertain owner reach into a claim
//! that a test reaches the change (#7330).

use ripr::domain::{Confidence, StageEvidence, StageState};
use ripr::{CheckInput, Mode, OutputFormat, check_workspace, render_check};
use std::path::Path;

fn check_fixture(name: &str) -> Result<ripr::CheckOutput, String> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    check_workspace(CheckInput {
        root: fixture.join("input"),
        diff_file: Some(fixture.join("diff.patch")),
        mode: Mode::Fast,
        ..CheckInput::default()
    })
}

#[test]
fn producer_evidence_and_public_renderers_agree_about_reach() -> Result<(), String> {
    for (name, reach, class, relation, hint) in [
        (
            "rust_trait_impl_shadow",
            "weak",
            "reachable_unrevealed",
            "weak_token_substring",
            "Why unrevealed: test reach to this change is not established",
        ),
        (
            "owner_return_pin_pointer_glob_shadow",
            "weak",
            "weakly_exposed",
            "same_test_file",
            "Why weak: test reach to this change is not established",
        ),
        (
            "owner_self_body_same_name_with_caller_control",
            "weak",
            "weakly_exposed",
            "same_test_file",
            "Why weak: test reach to this change is not established",
        ),
        (
            "boundary_gap",
            "yes",
            "weakly_exposed",
            "direct_owner_call",
            "Why weak: a related test reaches this change",
        ),
        (
            "inequality_assertion_not_exact",
            "yes",
            "weakly_exposed",
            "direct_owner_call",
            "Why weak: a related test reaches this change but does not observe the exact changed value",
        ),
    ] {
        let output = check_fixture(name)?;
        assert_eq!(output.findings.len(), 1, "{name}: nonempty subject");
        let json: serde_json::Value =
            serde_json::from_str(&render_check(&output, &OutputFormat::Json)?)
                .map_err(|error| error.to_string())?;
        let finding = &json["findings"][0];
        assert_eq!(finding["classification"], class, "{name}");
        assert_eq!(finding["ripr"]["reach"]["state"], reach, "{name}");
        assert!(
            finding["related_tests"]
                .as_array()
                .is_some_and(|tests| tests.iter().any(|test| test["relation_reason"] == relation)),
            "{name}: {finding}"
        );
        let compact = render_check(&output, &OutputFormat::Human)?;
        let full = render_check(&output, &OutputFormat::HumanFull)?;
        assert!(compact.contains(hint), "{name}: {compact}");
        assert!(full.contains(&format!("reach {reach}:")), "{name}: {full}");
    }
    Ok(())
}

#[test]
fn unestablished_reach_never_takes_the_weak_discriminator_shortcut() -> Result<(), String> {
    let original = check_fixture("owner_return_pin_pointer_glob_shadow")?;
    assert_eq!(original.findings.len(), 1);
    assert_eq!(
        original.findings[0].ripr.reveal.discriminate.state,
        StageState::Weak
    );
    for state in [
        StageState::Weak,
        StageState::No,
        StageState::Unknown,
        StageState::Opaque,
        StageState::NotApplicable,
    ] {
        let mut output = original.clone();
        output.findings[0].ripr.reach = StageEvidence::new(
            state.clone(),
            Confidence::Low,
            "owner reach not established",
        );
        let human = render_check(&output, &OutputFormat::Human)?;
        assert!(
            human.contains("Why weak: test reach to this change is not established"),
            "{state:?}: {human}"
        );
    }
    Ok(())
}
