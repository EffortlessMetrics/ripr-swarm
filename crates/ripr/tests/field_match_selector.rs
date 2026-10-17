//! #8479: an unsupported field selector is a named limitation, not a
//! request to repeat an existing assertion or permission to credit it.
#![cfg(feature = "lang-rust")]

use ripr::{CheckInput, CheckOutput, ExposureClass, Mode, OutputFormat, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SOURCE: &str = "#[derive(Debug)]
struct Lease { timeout_sec: u64, settled_elapsed_ms: Option<u128> }
fn new_lease(timeout_sec: u64) -> Lease {
    Lease {
        timeout_sec,
        settled_elapsed_ms: None,
    }
}
fn charge(lease: &Lease) -> u64 {
    match lease.settled_elapsed_ms {
        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),
        None => lease.timeout_sec,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Lease { new_lease(600) }
    #[test]
    fn charge_uses_elapsed_and_retains_legacy_reservation() {
        let mut lease = fixture();
        assert_eq!(charge(&lease), 600);
        lease.settled_elapsed_ms = Some(1001);
        assert_eq!(charge(&lease), 2);
    }
    #[test]
    fn unrelated_owner_with_same_none_literal() {
        let value: Option<u32> = None;
        assert_eq!(value, None);
    }
}
";

const DIFF: &str = concat!(
    "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -10 +10,4 @@\n",
    "-    lease.timeout_sec\n",
    "+    match lease.settled_elapsed_ms {\n",
    "+        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),\n",
    "+        None => lease.timeout_sec,\n+    }\n",
);

struct Fixture(PathBuf);

impl Fixture {
    fn new(source: &str, diff: &str) -> Result<Self, Box<dyn std::error::Error>> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "ripr-field-selector-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed),
        )));
        std::fs::create_dir_all(fixture.0.join("src"))?;
        std::fs::write(
            fixture.0.join("Cargo.toml"),
            "[package]\nname='field_selector'\nversion='0.0.0'\nedition='2024'\n[workspace]\n",
        )?;
        std::fs::write(fixture.0.join("src/lib.rs"), source)?;
        std::fs::write(fixture.0.join("input.diff"), diff)?;
        Ok(fixture)
    }

    fn check(&self) -> Result<CheckOutput, String> {
        check_workspace(CheckInput {
            root: self.0.clone(),
            diff_file: Some(self.0.join("input.diff")),
            mode: Mode::Ready,
            format: OutputFormat::Json,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn field_match_selector_names_all_three_rows_without_credit_or_wire_shape_change()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = Fixture::new(SOURCE, DIFF)?;
    let output = fixture.check()?;
    assert_eq!(output.findings.len(), 4);
    let weak = output
        .findings
        .iter()
        .filter(|finding| finding.class == ExposureClass::WeaklyExposed)
        .collect::<Vec<_>>();
    assert_eq!(weak.len(), 3);
    assert_eq!(
        weak.iter()
            .map(|finding| finding.probe.location.line)
            .collect::<Vec<_>>(),
        [10, 11, 12],
    );
    let json: serde_json::Value = serde_json::from_str(&ripr::output::json::render(&output))?;
    assert_eq!(json["schema_version"], "0.2");
    assert_eq!(json["summary"]["weakly_exposed"], 3);
    assert_eq!(
        weak.iter()
            .map(|finding| finding.id.as_str())
            .collect::<Vec<_>>(),
        [
            "probe:src_lib.rs:match_arm:931816b8",
            "probe:src_lib.rs:match_arm:afe43b21",
            "probe:src_lib.rs:match_arm:23efc786",
        ]
    );
    for finding in weak {
        assert_eq!(finding.id, finding.probe.id.0);
        assert!(
            finding.canonical_gap.is_none(),
            "#5268: a typed limitation must withhold automatic repair-gap projection"
        );
        assert_eq!(
            finding.static_limit_kind.map(|kind| kind.as_str()),
            Some("rust_field_match_selector_unresolved"),
            "{} must name the field-selector boundary",
            finding.id,
        );
        for state in [
            &finding.ripr.reach.state,
            &finding.ripr.infect.state,
            &finding.ripr.propagate.state,
            &finding.ripr.reveal.observe.state,
        ] {
            assert_eq!(state, &ripr::domain::StageState::Yes);
        }
        assert_eq!(
            finding.ripr.reveal.discriminate.state,
            ripr::domain::StageState::Weak
        );
        assert!(finding.activation.missing_discriminators.is_empty());
        let row = json["findings"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["id"] == finding.id))
            .ok_or("missing rendered finding")?;
        assert_eq!(
            row["static_limitation"]["kind"],
            "rust_field_match_selector_unresolved"
        );
        assert!(
            row["static_limitation"]["first_unresolved_edge"]
                .as_str()
                .is_some_and(|edge| edge.contains("lease.settled_elapsed_ms"))
        );
        assert_eq!(
            row["static_limitation"]["analyzer_route"],
            "analysis/rust-field-match-selector"
        );
        let next = finding
            .recommended_next_step
            .as_deref()
            .ok_or("missing next step")?;
        assert!(next.contains("Inspect") && next.contains("lease.settled_elapsed_ms"));
        assert!(!next.contains("Strengthen the related assertion"));
        let human = ripr::output::human::render_finding(finding);
        assert!(human.contains("rust_field_match_selector_unresolved"));
        assert!(human.contains("lease.settled_elapsed_ms"));
    }
    Ok(())
}

#[test]
fn field_match_selector_never_borrows_an_unrelated_or_overwritten_observation()
-> Result<(), Box<dyn std::error::Error>> {
    let original = concat!(
        "        let mut lease = fixture();\n",
        "        assert_eq!(charge(&lease), 600);\n",
        "        lease.settled_elapsed_ms = Some(1001);\n",
        "        assert_eq!(charge(&lease), 2);",
    );
    for (case, body) in [
        "        let mut lease = fixture();\n        lease.timeout_sec = 1001;\n        assert_eq!(charge(&lease), 1001);",
        "        let mut changed = fixture();\n        changed.settled_elapsed_ms = Some(1001);\n        let sibling = fixture();\n        assert_eq!(charge(&sibling), 600);",
        "        let lease = fixture();\n        let _ = charge(&lease);\n        let value: Option<u32> = None;\n        assert_eq!(value, None);",
        "        let mut lease = fixture();\n        let result = charge(&lease);\n        lease.settled_elapsed_ms = Some(1001);\n        assert_eq!(result, 600);",
        "        let mut lease = fixture();\n        lease.settled_elapsed_ms = Some(1001);\n        let result = charge(&lease);\n        let result = 600;\n        assert_eq!(result, 600);",
    ].into_iter().enumerate() {
        assert!(SOURCE.contains(original));
        let fixture = Fixture::new(&SOURCE.replace(original, body), DIFF)?;
        let output = fixture.check()?;
        assert_eq!(output.findings.len(), 4, "{body}");
        let named = output.findings.iter().filter(|finding| finding.static_limit_kind == Some(ripr::domain::StaticLimitKind::RustFieldMatchSelectorUnresolved)).collect::<Vec<_>>();
        if case == 2 {
            assert_eq!(named.len(), 3, "unrelated strong assertion must exercise conditional guidance");
        }
        for finding in named {
            let advice = finding.recommended_next_step.as_deref().ok_or("missing inspection guidance")?;
            assert!(advice.contains("whether a related assertion observes that call's returned value"));
            assert!(!advice.contains("and its returned-value assertion"));
        }
        assert!(
            output
                .findings
                .iter()
                .all(|finding| finding.class != ExposureClass::Exposed),
            "{body}"
        );
    }
    Ok(())
}

#[test]
fn a_direct_parameter_match_keeps_its_genuine_unselected_arm_gap()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "fn charge(elapsed: Option<u64>) -> u64 {\n    match elapsed {\n        Some(ms) => ms,\n        None => 600,\n    }\n}\n#[cfg(test)] mod tests {\nuse super::*;\n#[test] fn legacy_only() { assert_eq!(charge(None), 600); }\n}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -3 +3 @@\n-        Some(ms) => ms + 1,\n+        Some(ms) => ms,\n";
    let fixture = Fixture::new(source, diff)?;
    let output = fixture.check()?;
    let arms = output
        .findings
        .iter()
        .filter(|finding| finding.probe.family == ripr::ProbeFamily::MatchArm)
        .collect::<Vec<_>>();
    assert_eq!(
        arms.len(),
        2,
        "both the current and deleted arm subjects survive"
    );
    assert_eq!(
        arms.iter()
            .filter(
                |arm| arm.source_currentness == ripr::domain::SourceCurrentness::CandidateCurrent
            )
            .count(),
        1
    );
    for arm in arms {
        assert_ne!(arm.class, ExposureClass::Exposed);
        assert!(arm.static_limit_kind.is_none());
        assert!(
            arm.canonical_gap.is_some(),
            "the genuine direct-parameter gap retains its repair identity"
        );
        assert!(
            !arm.activation.missing_discriminators.is_empty(),
            "the real unselected-arm discriminator must remain actionable"
        );
    }
    Ok(())
}
