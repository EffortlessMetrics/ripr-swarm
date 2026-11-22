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
        // #15295: the Summary bucket carries the same reach judgement as the
        // reason line, in the bucket this finding is counted under.
        let summary = compact
            .lines()
            .find(|line| line.starts_with("Summary: "))
            .ok_or_else(|| format!("{name}: no Summary line: {compact}"))?;
        let note = " (1 through a test link ripr could not confirm)";
        let bucket = match class {
            "weakly_exposed" => format!("1 partly noticed{note}, 0 reached but not noticed, "),
            _ => format!("0 partly noticed, 1 reached but not noticed{note}, "),
        };
        if reach == "yes" {
            assert!(!summary.contains("could not confirm"), "{name}: {summary}");
            assert!(summary.contains("1 partly noticed, "), "{name}: {summary}");
        } else {
            assert!(summary.contains(&bucket), "{name}: {summary}");
        }
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
        assert!(
            human.contains("1 partly noticed (1 through a test link ripr could not confirm)"),
            "{state:?}: {human}"
        );
    }
    Ok(())
}

#[test]
fn guard_refusal_does_not_override_unestablished_reach() -> Result<(), String> {
    let original = check_fixture("rust_trait_impl_shadow")?;
    assert_eq!(original.findings.len(), 1);
    for state in [
        StageState::Weak,
        StageState::No,
        StageState::Unknown,
        StageState::Opaque,
        StageState::NotApplicable,
    ] {
        let mut output = original.clone();
        let finding = &mut output.findings[0];
        finding.ripr.reach = StageEvidence::new(
            state.clone(),
            Confidence::Low,
            "owner reach not established",
        );
        finding.ripr.reveal.observe.summary = "No statically established oracle: assertion execution or macro binding is unestablished (rust_assertion_context_unestablished)".into();
        finding.evidence.push("assertion not credited: `assert_eq!` in render_is_empty at src/lib.rs:29: an earlier guard spares the value the assertion expects".into());
        let human = render_check(&output, &OutputFormat::Human)?;
        assert!(
            human.contains("Why unrevealed: test reach to this change is not established"),
            "{state:?}: {human}"
        );
    }
    let mut reaching = original.clone();
    let finding = &mut reaching.findings[0];
    finding.ripr.reach = StageEvidence::new(StageState::Yes, Confidence::Medium, "owner reached");
    finding.ripr.reveal.observe.summary = "No statically established oracle: assertion execution or macro binding is unestablished (rust_assertion_context_unestablished)".into();
    finding.evidence.push("assertion not credited: `assert_eq!` in render_is_empty at src/lib.rs:29: an earlier guard spares the value the assertion expects".into());
    let human = render_check(&reaching, &OutputFormat::Human)?;
    assert!(
        human.contains("Why unrevealed: a related test asserts here"),
        "{human}"
    );
    Ok(())
}

/// A withheld finding has left its Summary bucket, so the unconfirmed-link
/// note must leave with it. Otherwise the note lands on the bucket's remaining
/// member, whose reach is established (#15295).
#[test]
fn a_withheld_unconfirmed_finding_is_not_counted_in_the_summary_note() -> Result<(), String> {
    let name = "mixed_rust_typescript_preview";
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name);
    let summary_of = |policy: Option<&Path>| -> Result<String, String> {
        // The CLI, not the library entry point: the fixture's `ripr.toml`
        // enables the TypeScript preview, and the binary is what reads it.
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .current_dir(fixture.join("input"))
            .args(["check", "--root", ".", "--diff"])
            .arg(fixture.join("diff.patch"));
        if let Some(policy) = policy {
            command.arg("--suppression-policy").arg(policy);
        }
        let output = command.output().map_err(|error| error.to_string())?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        stdout
            .lines()
            .find(|line| line.starts_with("Summary: "))
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "{name}: no Summary line: {stdout}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                )
            })
    };

    // Control: one Rust finding with established reach and one TypeScript
    // finding with a heuristic link share the bucket.
    let unsuppressed = summary_of(None)?;
    assert!(
        unsuppressed.contains("2 partly noticed (1 through a test link ripr could not confirm), "),
        "{unsuppressed}"
    );

    let policy = Path::new(env!("CARGO_TARGET_TMPDIR")).join("weak_reach_hint_15295_policy.toml");
    std::fs::write(
        &policy,
        "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"web/discount.ts\"\nreason = \"accepted preview debt\"\nowner = \"team-web\"\n",
    )
    .map_err(|error| error.to_string())?;
    let suppressed = summary_of(Some(&policy))?;
    assert!(
        suppressed.contains("1 partly noticed, 0 reached but not noticed, "),
        "{suppressed}"
    );
    assert!(!suppressed.contains("could not confirm"), "{suppressed}");
    assert!(
        suppressed.contains("1 suppressed by your settings"),
        "{suppressed}"
    );
    Ok(())
}
