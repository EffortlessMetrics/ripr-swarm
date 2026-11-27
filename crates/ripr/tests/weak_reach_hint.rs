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
            "Why unrevealed: no test is confirmed to reach this change; related tests are linked only by a shared file or name",
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
        // A weak reach whose summary is not the proximity-only marker names no
        // file/name link, so it keeps the bare "not established" hint (#15336).
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

/// #15336: the summary bucket "reached but not noticed" and the `Why` line of
/// the same finding must not disagree. With weak reach the `Why` line names the
/// shared file/name link instead of the bare "not established"; a genuinely
/// reached unrevealed finding keeps the reaching-test wording.
#[test]
fn unrevealed_weak_reach_why_line_does_not_contradict_summary() -> Result<(), String> {
    let original = check_fixture("rust_trait_impl_shadow")?;
    assert_eq!(original.findings.len(), 1);
    assert_eq!(
        original.findings[0].class.as_str(),
        "reachable_unrevealed",
        "fixture subject"
    );
    let human = render_check(&original, &OutputFormat::Human)?;
    assert!(human.contains("1 reached but not noticed"), "{human}");
    assert!(
        human.contains("no test is confirmed to reach this change"),
        "{human}"
    );
    assert!(
        !human.contains("Why unrevealed: test reach to this change is not established"),
        "{human}"
    );

    // Negative control: reach is established, so the old class wording stays.
    let mut reached = original.clone();
    reached.findings[0].ripr.reach =
        StageEvidence::new(StageState::Yes, Confidence::Medium, "owner reached");
    let human = render_check(&reached, &OutputFormat::Human)?;
    assert!(
        !human.contains("no test is confirmed to reach this change"),
        "{human}"
    );
    assert!(human.contains("Why unrevealed: a related test"), "{human}");
    Ok(())
}

/// A withheld finding leaves its Summary bucket, so its unconfirmed-link note
/// must leave with it. The Rust-only fixture keeps this check in the small profile.
#[test]
fn a_withheld_unconfirmed_finding_is_not_counted_in_the_summary_note() -> Result<(), String> {
    exercise_summary_suppression(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/weak_reach_summary"),
        "src/heuristic.rs",
        "rust",
    )
}

// Keep the real mixed-language CLI configuration and suppression-policy control.
#[cfg(feature = "lang-typescript")]
#[test]
fn mixed_language_suppression_excludes_the_unconfirmed_link() -> Result<(), String> {
    exercise_summary_suppression(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed_rust_typescript_preview"),
        "web/discount.ts",
        "mixed",
    )
}

fn exercise_summary_suppression(
    fixture: &Path,
    suppressed_path: &str,
    profile: &str,
) -> Result<(), String> {
    let summary_of = |policy: Option<&Path>| -> Result<String, String> {
        // Exercise the CLI's config and suppression-policy plumbing as well as rendering.
        let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_ripr"));
        command
            .current_dir(fixture.join("input"))
            .args(["check", "--root", ".", "--diff"])
            .arg(fixture.join("diff.patch"));
        if let Some(policy) = policy {
            command.arg("--suppression-policy").arg(policy);
        }
        let output = command.output().map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "{profile}: CLI exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        stdout
            .lines()
            .find(|line| line.starts_with("Summary: "))
            .map(str::to_string)
            .ok_or_else(|| {
                format!(
                    "{profile}: no Summary line: {stdout}\n{}",
                    String::from_utf8_lossy(&output.stderr)
                )
            })
    };

    // Control: only one of two partly noticed findings has established reach.
    let unsuppressed = summary_of(None)?;
    assert!(
        unsuppressed.contains("2 partly noticed (1 through a test link ripr could not confirm), "),
        "{unsuppressed}"
    );

    let policy_for = |label: &str, path: &str| -> Result<std::path::PathBuf, String> {
        let policy = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join(format!("weak_reach_hint_15295_{profile}_{label}.toml"));
        std::fs::write(
            &policy,
            format!(
                "schema_version = 1\n\n[[suppressions]]\nkind = \"exposure_gap\"\npath = \"{path}\"\nreason = \"accepted fixture debt\"\nowner = \"test-fixture\"\n"
            ),
        )
        .map_err(|error| error.to_string())?;
        Ok(policy)
    };

    // The finding without established reach is suppressed: it leaves its
    // Summary bucket, and the unconfirmed-link note leaves with it.
    let policy = policy_for("policy", suppressed_path)?;
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

    // The other way round: the finding with established reach is suppressed
    // (`src/lib.rs` in each fixture) and the other stays, so the note stays.
    // Under the first policy alone, a Summary that dropped the note whenever
    // anything is suppressed would read alike.
    let policy = policy_for("reaching", "src/lib.rs")?;
    let other_suppressed = summary_of(Some(&policy))?;
    assert!(
        other_suppressed
            .contains("1 partly noticed (1 through a test link ripr could not confirm), "),
        "{other_suppressed}"
    );
    assert!(
        other_suppressed.contains("1 suppressed by your settings"),
        "{other_suppressed}"
    );
    Ok(())
}
