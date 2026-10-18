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
fn a_helper_returned_mutable_record_selects_its_exact_asserted_call()
-> Result<(), Box<dyn std::error::Error>> {
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11 +11 @@\n-        Some(ms) => 1,\n+        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),\n";
    let original = concat!(
        "        let mut lease = fixture();\n",
        "        assert_eq!(charge(&lease), 600);\n",
        "        lease.settled_elapsed_ms = Some(1001);\n",
        "        assert_eq!(charge(&lease), 2);",
    );
    for source in [SOURCE.to_string(), SOURCE.replace(original,
        "        let lease = Lease { timeout_sec: 600, settled_elapsed_ms: Some(1001) };\n        assert_eq!(charge(&lease), 2);") ] {
    let fixture = Fixture::new(&source, diff)?;
    let output = fixture.check()?;
    let arms = output.findings.iter().filter(|finding|
        finding.probe.family == ripr::ProbeFamily::MatchArm && finding.probe.location.line == 11
    ).collect::<Vec<_>>();
    assert!(!arms.is_empty(), "the changed Some arm must be analyzed");
    for arm in arms {
        assert_eq!(arm.class, ExposureClass::Exposed, "{}", arm.id);
        assert!(arm.static_limit_kind.is_none());
    }
    }
    Ok(())
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

#[test]
fn ffi_binding_keeps_its_prior_limitation_without_field_selector_detail()
-> Result<(), Box<dyn std::error::Error>> {
    let source = SOURCE.replace("fn charge(", "#[unsafe(no_mangle)]\nfn charge(");
    assert_ne!(source, SOURCE);
    let diff = DIFF.replace("@@ -10 +10,4 @@", "@@ -11 +11,4 @@");
    assert_ne!(diff, DIFF);
    let fixture = Fixture::new(&source, &diff)?;
    let output = fixture.check()?;
    assert_eq!(output.findings.len(), 4);
    let weak = output
        .findings
        .iter()
        .filter(|finding| finding.class == ExposureClass::WeaklyExposed)
        .collect::<Vec<_>>();
    assert_eq!(weak.len(), 3);
    for finding in weak {
        assert_eq!(
            finding.static_limit_kind.map(|kind| kind.as_str()),
            Some("cross_language_oracle_visibility_unresolved")
        );
        assert!(finding.canonical_gap.is_none());
        assert!(
            !finding.evidence.iter().any(|line| line
                .contains("limitation_analyzer_route: analysis/rust-field-match-selector")),
            "FFI priority must not leave mismatched field-selector detail"
        );
        assert!(
            !finding
                .recommended_next_step
                .as_deref()
                .unwrap_or_default()
                .contains("Inspect `lease.settled_elapsed_ms` at the existing owner call")
        );
    }
    Ok(())
}

#[test]
fn a_direct_record_selector_is_paired_with_its_own_asserted_call()
-> Result<(), Box<dyn std::error::Error>> {
    let original = concat!(
        "        let mut lease = fixture();\n",
        "        assert_eq!(charge(&lease), 600);\n",
        "        lease.settled_elapsed_ms = Some(1001);\n",
        "        assert_eq!(charge(&lease), 2);",
    );
    assert!(SOURCE.contains(original));
    let source = SOURCE.replace(original,
        "        assert_eq!(charge(&Lease { timeout_sec: 600, settled_elapsed_ms: Some(1001) }), 2);");
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11 +11 @@\n-        Some(ms) => 1,\n+        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),\n";
    let fixture = Fixture::new(&source, diff)?;
    let output = fixture.check()?;
    let arms = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ripr::ProbeFamily::MatchArm && finding.probe.location.line == 11
        })
        .collect::<Vec<_>>();
    assert!(!arms.is_empty(), "the changed arm must be analyzed");
    for arm in arms {
        assert_eq!(arm.class, ExposureClass::Exposed, "{}", arm.id);
        assert!(arm.static_limit_kind.is_none());
    }
    Ok(())
}

#[test]
fn a_direct_record_sibling_names_the_missing_arm_and_wrong_calls_never_credit_it()
-> Result<(), Box<dyn std::error::Error>> {
    let original = concat!(
        "        let mut lease = fixture();\n",
        "        assert_eq!(charge(&lease), 600);\n",
        "        lease.settled_elapsed_ms = Some(1001);\n",
        "        assert_eq!(charge(&lease), 2);",
    );
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11 +11 @@\n-        Some(ms) => 1,\n+        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),\n";
    for (case, body) in [
        "        assert_eq!(charge(&Lease { timeout_sec: 1001, settled_elapsed_ms: None }), 1001);",
        "        let _ = charge(&Lease { timeout_sec: 600, settled_elapsed_ms: Some(1001) });\n        assert_eq!(charge(&Lease { timeout_sec: 600, settled_elapsed_ms: None }), 600);",
        "        assert_eq!(charge(&Lease { timeout_sec: 600, settled_elapsed_ms: Some(1000 + 1) }), 2);",
    ].into_iter().enumerate() {
        let source = SOURCE.replace(original, body);
        let end = source.find("    #[test]\n    fn unrelated_owner_with_same_none_literal()")
            .ok_or("fixture unrelated test boundary")?;
        let source = format!("{}}}\n", &source[..end]);
        let fixture = Fixture::new(&source, diff)?;
        let output = fixture.check()?;
        let arms = output.findings.iter().filter(|finding|
            finding.probe.family == ripr::ProbeFamily::MatchArm && finding.probe.location.line == 11
        ).collect::<Vec<_>>();
        assert!(!arms.is_empty(), "the changed arm must be analyzed");
        for arm in arms {
            assert_ne!(arm.class, ExposureClass::Exposed, "case {case}: {}", arm.id);
            if case == 0 {
                assert!(arm.activation.missing_discriminators.iter().any(|missing| missing.value == "Some(ms)"), "{}", arm.id);
                assert!(arm.static_limit_kind.is_none(),
                    "an established sibling input is a real gap, not an unresolved field value");
                assert!(arm.canonical_gap.is_some(),
                    "the supported missing-arm discriminator must retain its repair identity");
            }
        }
    }
    Ok(())
}

#[test]
fn a_foreign_option_constant_cannot_name_the_selected_field_arm()
-> Result<(), Box<dyn std::error::Error>> {
    let source = "mod foreign;\nstruct Lease { settled_elapsed_ms: Option<u128>, timeout_sec: u64 }\nfn charge(lease: &Lease) -> u64 {\n    match lease.settled_elapsed_ms {\n        foreign::None => 1,\n        Some(ms) => ms as u64,\n        None => lease.timeout_sec,\n    }\n}\n#[cfg(test)] mod tests {\n    use super::*;\n    #[test] fn legacy_only() {\n        assert_eq!(charge(&Lease { settled_elapsed_ms: None, timeout_sec: 600 }), 600);\n    }\n}\n";
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -5 +5 @@\n-        foreign::None => 0,\n+        foreign::None => 1,\n";
    let fixture = Fixture::new(source, diff)?;
    std::fs::write(
        fixture.0.join("src/foreign.rs"),
        "#[allow(non_upper_case_globals)]\npub const None: Option<u128> = Some(1001);\n",
    )?;
    let output = fixture.check()?;
    let arms = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ripr::ProbeFamily::MatchArm && finding.probe.location.line == 5
        })
        .collect::<Vec<_>>();
    assert!(
        !arms.is_empty(),
        "the qualified changed arm must be analyzed"
    );
    for arm in arms {
        assert_ne!(
            arm.class,
            ExposureClass::Exposed,
            "a constant is not the prelude variant: {}",
            arm.id
        );
    }
    Ok(())
}

#[test]
fn raw_identifier_option_shadows_cannot_borrow_prelude_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("../../../fixtures/field_match_raw_option_shadow/input/src/lib.rs");
    let diff = include_str!("../../../fixtures/field_match_raw_option_shadow/diff.patch");
    let fixture = Fixture::new(source, diff)?;
    let output = fixture.check()?;
    let arms = output
        .findings
        .iter()
        .filter(|finding| finding.probe.family == ripr::ProbeFamily::MatchArm)
        .collect::<Vec<_>>();
    assert_eq!(arms.len(), 2, "both changed-arm subjects must be analyzed");
    for arm in arms {
        assert_ne!(
            arm.class,
            ExposureClass::Exposed,
            "raw identifiers retain their binding identity: {}",
            arm.id
        );
    }
    Ok(())
}

#[test]
fn an_imported_statement_macro_cannot_supply_an_unread_option_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let source = include_str!("../../../fixtures/field_match_macro_option_shadow/input/src/lib.rs");
    let diff = include_str!("../../../fixtures/field_match_macro_option_shadow/diff.patch");
    let fixture = Fixture::new(source, diff)?;
    std::fs::write(
        fixture.0.join("src/foreign.rs"),
        include_str!("../../../fixtures/field_match_macro_option_shadow/input/src/foreign.rs"),
    )?;
    let output = fixture.check()?;
    let arms = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ripr::ProbeFamily::MatchArm && finding.probe.location.line == 5
        })
        .collect::<Vec<_>>();
    assert_eq!(
        arms.len(),
        2,
        "current and deleted arm subjects must be analyzed"
    );
    for arm in arms {
        assert_ne!(
            arm.class,
            ExposureClass::Exposed,
            "an unread statement expansion can shadow None: {}",
            arm.id
        );
    }
    Ok(())
}

#[test]
fn a_field_selector_cannot_credit_an_assertion_that_does_not_execute()
-> Result<(), Box<dyn std::error::Error>> {
    let source =
        include_str!("../../../fixtures/field_match_unexecuted_assertion/input/src/lib.rs");
    let diff = include_str!("../../../fixtures/field_match_unexecuted_assertion/diff.patch");
    let fixture = Fixture::new(source, diff)?;
    let output = fixture.check()?;
    let arms = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.family == ripr::ProbeFamily::MatchArm && finding.probe.location.line == 4
        })
        .collect::<Vec<_>>();
    assert_eq!(arms.len(), 2, "current and deleted subjects must survive");
    for arm in arms {
        assert_ne!(
            arm.class,
            ExposureClass::Exposed,
            "an unreachable assertion is not same-invocation proof: {}",
            arm.id
        );
    }
    Ok(())
}

#[test]
fn helper_mutable_field_projection_refuses_unrelated_or_invalidated_values()
-> Result<(), Box<dyn std::error::Error>> {
    let original = concat!(
        "        let mut lease = fixture();\n",
        "        assert_eq!(charge(&lease), 600);\n",
        "        lease.settled_elapsed_ms = Some(1001);\n",
        "        assert_eq!(charge(&lease), 2);",
    );
    let diff = "--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -11 +11 @@\n-        Some(ms) => 1,\n+        Some(ms) => u64::try_from(ms.div_ceil(1000)).unwrap_or(u64::MAX).max(1),\n";
    let cases = [
        (
            "wrong field",
            "let mut lease = fixture(); lease.timeout_sec = 1001; assert_eq!(charge(&lease), 1001);",
        ),
        (
            "wrong receiver",
            "let mut changed = fixture(); changed.settled_elapsed_ms = Some(1001); let lease = fixture(); assert_eq!(charge(&lease), 600);",
        ),
        (
            "discarded selecting call",
            "let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); let _ = charge(&lease); lease.settled_elapsed_ms = None; assert_eq!(charge(&lease), 600);",
        ),
        (
            "write after call",
            "let mut lease = fixture(); let result = charge(&lease); lease.settled_elapsed_ms = Some(1001); assert_eq!(result, 600);",
        ),
        (
            "overwritten result",
            "let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); let result = charge(&lease); let result = 600; assert_eq!(result, 600);",
        ),
        (
            "uncalled helper",
            "fn unused() -> Lease { Lease { timeout_sec: 600, settled_elapsed_ms: Some(1001) } } let lease = fixture(); assert_eq!(charge(&lease), 600);",
        ),
        (
            "mutable alias",
            "let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); let alias = &mut lease; alias.settled_elapsed_ms = None; assert_eq!(charge(&lease), 600);",
        ),
        (
            "implicit field borrow",
            "let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); lease.settled_elapsed_ms.take(); assert_eq!(charge(&lease), 600);",
        ),
        (
            "implicit borrow in macro",
            "let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); assert_eq!(lease.settled_elapsed_ms.take(), Some(1001)); assert_eq!(charge(&lease), 600);",
        ),
        (
            "conditional write",
            "let mut lease = fixture(); if false { lease.settled_elapsed_ms = Some(1001); } assert_eq!(charge(&lease), 600);",
        ),
        (
            "uncalled closure write",
            "let mut lease = fixture(); let _unused = || { lease.settled_elapsed_ms = Some(1001); }; assert_eq!(charge(&lease), 600);",
        ),
        (
            "whole object overwrite",
            "let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); lease = fixture(); assert_eq!(charge(&lease), 600);",
        ),
        (
            "whole object shadow",
            "let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); let lease = fixture(); assert_eq!(charge(&lease), 600);",
        ),
        (
            "shadowed helper",
            "let fixture = || new_lease(600); let mut lease = fixture(); lease.settled_elapsed_ms = Some(1001); assert_eq!(charge(&lease), 2);",
        ),
        (
            "same line earlier call",
            "let mut lease = fixture(); assert_eq!(charge(&lease), 600); lease.settled_elapsed_ms = Some(1001);",
        ),
    ];
    assert!(SOURCE.contains(original));
    for (case, body) in cases {
        let fixture = Fixture::new(&SOURCE.replace(original, body), diff)?;
        let output = fixture.check()?;
        let arms = output
            .findings
            .iter()
            .filter(|finding| {
                finding.probe.family == ripr::ProbeFamily::MatchArm
                    && finding.probe.location.line == 11
            })
            .collect::<Vec<_>>();
        assert!(!arms.is_empty(), "{case}: changed arm must survive");
        assert!(
            arms.iter().all(|arm| arm.class != ExposureClass::Exposed),
            "{case}"
        );
    }
    for (case, helper) in [
        (
            "unreachable helper record",
            "fn fixture() -> Lease { if false { return Lease { timeout_sec: 600, settled_elapsed_ms: Some(1001) }; } new_lease(600) }",
        ),
        (
            "conditional helper declaration",
            "#[cfg(all())] fn fixture() -> Lease { new_lease(600) }",
        ),
    ] {
        let source = SOURCE.replace("fn fixture() -> Lease { new_lease(600) }", helper);
        let fixture = Fixture::new(&source, diff)?;
        let output = fixture.check()?;
        assert!(
            output
                .findings
                .iter()
                .all(|finding| finding.class != ExposureClass::Exposed),
            "{case}"
        );
        assert!(
            output
                .findings
                .iter()
                .any(|finding| finding.static_limit_kind
                    == Some(ripr::domain::StaticLimitKind::RustFieldMatchSelectorUnresolved)),
            "{case}: unsupported input stays named"
        );
    }
    Ok(())
}
