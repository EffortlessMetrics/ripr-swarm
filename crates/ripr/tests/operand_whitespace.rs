//! Whitespace cannot supply the missing discriminator in SPEC-0197 rule 7.
//! Native Rust controls and public RIPR findings are separate assertions.

#![cfg(feature = "lang-rust")]

use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxKind};
use ripr::domain::{RelationReason, StageState};
use ripr::{CheckInput, ExposureClass, Finding, Mode, OutputFormat, ProbeFamily, check_workspace};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const SPACED: &str = include_str!("fixtures/operand_whitespace/spaced.rs");
const COMPACT: &str = include_str!("fixtures/operand_whitespace/compact.rs");
const MARKER: &str = "field_pinned_equal_to_operand";

mod native_spaced {
    include!("fixtures/operand_whitespace/spaced.rs");
}

mod native_compact {
    include!("fixtures/operand_whitespace/compact.rs");
}

struct TempRepo(PathBuf);

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn field_finding(source: &str, expression: &str) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo(std::env::temp_dir().join(format!(
        "ripr-operand-whitespace-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(repo.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        repo.0.join("Cargo.toml"),
        "[package]\nname = \"operand-whitespace\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(repo.0.join("src/lib.rs"), source).map_err(|error| error.to_string())?;
    let changed = source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim() == format!("{expression},"))
        .collect::<Vec<_>>();
    let [(offset, added)] = changed.as_slice() else {
        return Err("fixture must contain exactly one changed initializer".to_string());
    };
    let line = offset + 1;
    let removed = "        total_cents: subtotal,";
    let patch = format!(
        "diff --git a/src/lib.rs b/src/lib.rs\n--- a/src/lib.rs\n+++ b/src/lib.rs\n@@ -{line},1 +{line},1 @@\n-{removed}\n+{added}\n"
    );
    std::fs::write(repo.0.join("diff.patch"), patch).map_err(|error| error.to_string())?;
    let output = check_workspace(CheckInput {
        root: repo.0.clone(),
        base: None,
        diff_file: Some(repo.0.join("diff.patch")),
        mode: Mode::Ready,
        format: OutputFormat::Json,
        include_unchanged_tests: true,
        ..CheckInput::default()
    })?;
    let selected = output
        .findings
        .iter()
        .filter(|finding| {
            finding.probe.location.line == line
                && finding.probe.family == ProbeFamily::FieldConstruction
                && finding.probe.expression.trim().trim_end_matches(',') == expression
        })
        .collect::<Vec<_>>();
    let [finding] = selected.as_slice() else {
        return Err(format!(
            "premise: one field-construction finding for {expression}: {:?}",
            output.findings
        ));
    };
    assert!(
        finding
            .related_tests
            .iter()
            .any(|test| test.relation_reason == Some(RelationReason::DirectOwnerCall)),
        "premise: the fixture directly calls the owner: {finding:?}"
    );
    Ok((*finding).clone())
}

fn significant_tokens(source: &str) -> Vec<(SyntaxKind, String)> {
    let parsed = SourceFile::parse(source, Edition::Edition2024);
    assert!(parsed.errors().is_empty(), "fixture syntax: {parsed:?}");
    assert_eq!(
        parsed
            .tree()
            .syntax()
            .descendants()
            .filter(|node| node.kind() == SyntaxKind::BIN_EXPR)
            .count(),
        1,
        "premise: exactly one binary expression"
    );
    parsed
        .tree()
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| token.kind() != SyntaxKind::WHITESPACE)
        .map(|token| (token.kind(), token.text().to_string()))
        .collect()
}

#[test]
fn both_spellings_have_the_same_rust_tokens_and_native_values() {
    assert_eq!(significant_tokens(SPACED), significant_tokens(COMPACT));
    let spaced_zero = native_spaced::quote(9_000, 0);
    let compact_zero = native_compact::quote(9_000, 0);
    assert_eq!(spaced_zero.subtotal_cents, 9_000);
    assert_eq!(compact_zero.subtotal_cents, 9_000);
    assert_eq!(spaced_zero.total_cents, 9_000);
    assert_eq!(compact_zero.total_cents, 9_000);
    let spaced_nonzero = native_spaced::quote(9_000, 499);
    let compact_nonzero = native_compact::quote(9_000, 499);
    assert_eq!(spaced_nonzero.subtotal_cents, 9_000);
    assert_eq!(compact_nonzero.subtotal_cents, 9_000);
    assert_eq!(spaced_nonzero.total_cents, 9_499);
    assert_eq!(compact_nonzero.total_cents, 9_499);
    // The dropped-operand implementation passes zero pins and fails these pins.
    assert_ne!(spaced_nonzero.total_cents, spaced_nonzero.subtotal_cents);
    assert_ne!(compact_nonzero.total_cents, compact_nonzero.subtotal_cents);
}

#[test]
fn whitespace_does_not_supply_the_missing_shipping_discriminator() -> Result<(), String> {
    let mut observed = Vec::new();
    for spelling in [
        "subtotal + shipping",
        "subtotal+shipping",
        "subtotal+ shipping",
        "subtotal +shipping",
    ] {
        let source = SPACED.replace("subtotal + shipping", spelling);
        assert_eq!(significant_tokens(SPACED), significant_tokens(&source));
        let expression = format!("total_cents: {spelling}");
        let finding = field_finding(&source, &expression)?;
        observed.push((
            spelling,
            finding.class,
            finding.ripr.reveal.discriminate.state,
            finding.ripr.reveal.discriminate.summary.contains(MARKER),
        ));
    }
    let expected = observed
        .iter()
        .map(|(spelling, _, _, _)| {
            (
                *spelling,
                ExposureClass::WeaklyExposed,
                StageState::Weak,
                true,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        observed, expected,
        "zero-shipping pins cannot notice the dropped operand"
    );
    // This expected projection is independently constrained by the honesty
    // corpus. It is not a captured execution or a complete check report.
    let projected = serde_json::json!({
        "kind": "operand_whitespace_expected_test_projection",
        "summary": { "findings": observed.len() },
        "findings": observed.iter().map(|(spelling, class, state, gap)| {
            serde_json::json!({
                "id": spelling,
                "classification": class.as_str(),
                "probe": { "family": "field_construction" },
                "discriminate": state.as_str(),
                "operand_pin": gap,
            })
        }).collect::<Vec<_>>(),
    });
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/evidence-promotion-honesty-corpus/reports/operand-whitespace.json"
    ))
    .map_err(|error| error.to_string())?;
    assert_eq!(
        projected, expected,
        "current public producer must satisfy the registered contract"
    );
    Ok(())
}

#[test]
fn a_nonzero_shipping_pin_discriminates_both_spellings() -> Result<(), String> {
    for (source, expression) in [
        (SPACED, "total_cents: subtotal + shipping"),
        (COMPACT, "total_cents: subtotal+shipping"),
    ] {
        let source = source
            .replace("quote(9_000, 0)", "quote(9_000, 499)")
            .replace(
                "assert_eq!(q.total_cents, 9_000)",
                "assert_eq!(q.total_cents, 9_499)",
            );
        let finding = field_finding(&source, expression)?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert_eq!(
            finding.ripr.reveal.discriminate.state,
            StageState::Yes,
            "{finding:?}"
        );
        assert!(
            !finding.ripr.reveal.discriminate.summary.contains(MARKER),
            "{finding:?}"
        );
    }
    Ok(())
}

#[test]
fn compact_spelling_preserves_callee_identity_refusals() -> Result<(), String> {
    for source in [
        include_str!("fixtures/operand_callee_identity/let_shadow.rs"),
        include_str!("fixtures/operand_callee_identity/method_confusion.rs"),
    ] {
        assert_eq!(source.matches("subtotal + shipping").count(), 1);
        let source = source.replace("subtotal + shipping", "subtotal+shipping");
        let finding = field_finding(&source, "total_cents: subtotal+shipping")?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert_eq!(
            finding.ripr.reveal.discriminate.state,
            StageState::Yes,
            "{finding:?}"
        );
        assert!(
            !finding.ripr.reveal.discriminate.summary.contains(MARKER),
            "an observed overloaded operator must retain credit: {finding:?}"
        );
    }
    Ok(())
}
