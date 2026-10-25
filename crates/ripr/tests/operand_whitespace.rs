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
    field_finding_at(source, expression, "src/lib.rs", "2024", &[])
}

fn field_finding_at(
    source: &str,
    expression: &str,
    file: &str,
    edition: &str,
    extra_files: &[(&str, &str)],
) -> Result<Finding, String> {
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let repo = TempRepo(std::env::temp_dir().join(format!(
        "ripr-operand-whitespace-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    std::fs::create_dir_all(repo.0.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        repo.0.join("Cargo.toml"),
        format!("[package]\nname = \"operand-whitespace\"\nversion = \"0.0.0\"\nedition = \"{edition}\"\n\n[workspace]\n"),
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(repo.0.join(file), source).map_err(|error| error.to_string())?;
    for (path, text) in extra_files {
        std::fs::write(repo.0.join(path), text).map_err(|error| error.to_string())?;
    }
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
        "diff --git a/{file} b/{file}\n--- a/{file}\n+++ b/{file}\n@@ -{line},1 +{line},1 @@\n-{removed}\n+{added}\n"
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

fn assert_binding_class(source: &str, expected: ExposureClass) -> Result<(), String> {
    for spelling in [
        "subtotal + shipping",
        "subtotal+shipping",
        "subtotal +shipping",
        "subtotal+ shipping",
        "subtotal\t+\tshipping",
    ] {
        assert_eq!(source.matches("subtotal + shipping").count(), 1);
        let source = source.replace("subtotal + shipping", spelling);
        let finding = field_finding(&source, &format!("total_cents: {spelling}"))?;
        assert_eq!(finding.class, expected, "{spelling}: {finding:?}");
        assert_eq!(
            finding.ripr.reveal.discriminate.summary.contains(MARKER),
            expected == ExposureClass::WeaklyExposed,
            "{finding:?}"
        );
    }
    Ok(())
}

#[test]
fn primitive_type_aliases_do_not_hide_observed_operator_effects() -> Result<(), String> {
    let sources = [
        include_str!("fixtures/operand_whitespace/callee_shadow_annotation_alias.rs"),
        include_str!("fixtures/operand_whitespace/local_annotation_alias.rs"),
        include_str!("fixtures/operand_whitespace/module_param_alias.rs"),
        include_str!("fixtures/operand_whitespace/module_return_alias.rs"),
    ];
    let mut findings = Vec::new();
    for (case, source) in sources.into_iter().enumerate() {
        assert_binding_class(source, ExposureClass::Exposed)?;
        for spelling in ["subtotal + shipping", "subtotal+shipping"] {
            let source = source.replace("subtotal + shipping", spelling);
            let finding = field_finding(&source, &format!("total_cents: {spelling}"))?;
            findings.push(serde_json::json!({
                "id": format!("{case}:{spelling}"),
                "classification": finding.class.as_str(),
                "probe": { "family": "field_construction" },
                "discriminate": finding.ripr.reveal.discriminate.state.as_str(),
                "operand_pin": finding.ripr.reveal.discriminate.summary.contains(MARKER),
            }));
        }
    }
    // Expected contract projection, not a stored execution report. The native
    // effect-removal witnesses establish why these pins retain their credit.
    let projected = serde_json::json!({
        "kind": "operand_binding_expected_test_projection",
        "summary": { "findings": findings.len() },
        "findings": findings,
    });
    let expected: serde_json::Value = serde_json::from_str(include_str!(
        "../../../fixtures/evidence-promotion-honesty-corpus/reports/operand-binding.json"
    ))
    .map_err(|error| error.to_string())?;
    assert_eq!(projected, expected);
    Ok(())
}

#[test]
fn primitive_generic_nominal_and_import_bindings_refuse_the_downgrade() -> Result<(), String> {
    let source = include_str!("fixtures/operand_whitespace/module_param_alias.rs");
    let without_alias =
        source.replace("#[allow(non_camel_case_types)]\ntype u64 = Shipping;\n", "");
    let generic = without_alias
        .replace("fn quote(", "fn quote<u64>(")
        .replace(
            "shipping: u64) -> Quote {",
            "shipping: u64) -> Quote where i64: std::ops::Add<u64, Output = i64> {",
        );
    assert_binding_class(&generic, ExposureClass::Exposed)?;
    let nominal = without_alias.replace("Shipping", "u64");
    assert_binding_class(&nominal, ExposureClass::Exposed)?;
    let enumeration = nominal
        .replace("pub struct u64;", "pub enum u64 { Value }")
        .replace("    u64\n", "    u64::Value\n")
        .replace("3, u64)", "3, u64::Value)");
    assert_binding_class(&enumeration, ExposureClass::Exposed)?;
    for binding in [
        "use crate::Shipping as u64;",
        "use crate::{Shipping as u64};",
    ] {
        let source = format!("{binding}\n{without_alias}");
        assert_binding_class(&source, ExposureClass::Exposed)?;
    }
    let local = include_str!("fixtures/operand_whitespace/local_annotation_alias.rs");
    let opaque = local.replace(
        "type u64 = Shipping;",
        "macro_rules! bind { () => { type u64 = Shipping; } } bind!();",
    );
    assert_binding_class(&opaque, ExposureClass::Exposed)
}

#[test]
fn value_namespace_and_nested_test_type_bindings_keep_primitive_credit() -> Result<(), String> {
    for addition in [
        "const u64: u64 = 0;",
        "fn u64() {}",
        "static u64: u64 = 0;",
        "mod unrelated { type u64 = (); }",
        "#[cfg(test)] mod other_tests { type u64 = (); }",
    ] {
        assert_binding_class(
            &format!("{addition}\n{SPACED}"),
            ExposureClass::WeaklyExposed,
        )?;
    }
    for addition in ["let u64 = 0;", "let u64 = || ();", "const u64: u64 = 0;"] {
        let source = SPACED.replace("    Quote {", &format!("    {addition}\n    Quote {{"));
        assert_binding_class(&source, ExposureClass::WeaklyExposed)?;
    }
    for import in [
        "use std::primitive::u64;",
        "use core::primitive::{u64};",
        "use std::primitive::*;",
        "use core::primitive::{u64 as u64};",
    ] {
        assert_binding_class(&format!("{import}\n{SPACED}"), ExposureClass::WeaklyExposed)?;
    }
    let parameter = SPACED
        .replace("shipping: u64)", "shipping: u64, u64: u64)")
        .replace("quote(9_000, 0)", "quote(9_000, 0, 0)");
    assert_binding_class(&parameter, ExposureClass::WeaklyExposed)?;
    Ok(())
}

#[test]
fn unestablished_alias_terminals_and_standard_roots_stay_conservative() -> Result<(), String> {
    for binding in [
        "type u64 = core::primitive::u64;",
        "type Word = core::primitive::u64; type u64 = Word;",
        "use ::core::primitive::u64;",
    ] {
        assert_binding_class(&format!("{binding}\n{SPACED}"), ExposureClass::Exposed)?;
    }
    let source = include_str!("fixtures/operand_whitespace/module_param_alias.rs");
    let without_alias =
        source.replace("#[allow(non_camel_case_types)]\ntype u64 = Shipping;\n", "");
    for binding in [
        "mod core { pub mod primitive { pub use crate::Shipping as u64; } } use core::primitive::u64;",
        "extern crate self as core; pub mod primitive { pub use crate::Shipping as u64; } use core::primitive::u64;",
        "mod custom { pub use crate::Shipping as u64; } use custom::*;",
    ] {
        assert_binding_class(
            &format!("{binding}\n{without_alias}"),
            ExposureClass::Exposed,
        )?;
    }
    Ok(())
}

#[test]
fn out_of_line_standard_imports_need_the_crate_root_context() -> Result<(), String> {
    let source = include_str!("fixtures/operand_whitespace/module_param_alias.rs")
        .replace("#[allow(non_camel_case_types)]\ntype u64 = Shipping;\n", "")
        .replace("std::", "::real_std::");
    let source = format!("use std::primitive::u64;\n{source}");
    let root = "#![no_std]\n#[macro_use] extern crate std as real_std;\nmod std { pub mod primitive { pub use pricing::Shipping as u64; } }\npub mod pricing;\n";
    for spelling in ["subtotal + shipping", "subtotal+shipping"] {
        let source = source.replace("subtotal + shipping", spelling);
        let finding = field_finding_at(
            &source,
            &format!("total_cents: {spelling}"),
            "src/pricing.rs",
            "2015",
            &[("src/lib.rs", root)],
        )?;
        assert_eq!(finding.class, ExposureClass::Exposed, "{finding:?}");
        assert!(!finding.ripr.reveal.discriminate.summary.contains(MARKER));
    }
    Ok(())
}

#[test]
fn disjoint_block_bindings_do_not_rebind_outer_annotations_or_casts() -> Result<(), String> {
    let primitive = include_str!("fixtures/operand_callee_identity/primitive_control.rs");
    for binding in [
        "{ type u64 = (); }",
        "{ use std::string::String as u64; }",
        "{ use std::option::*; }",
        "let _unused = || { type u64 = (); };",
    ] {
        for initializer in [
            "let shipping: u64 = shipping_cents(subtotal);",
            "let shipping = 0 as u64;",
        ] {
            let source = primitive.replace(
                "let shipping = shipping_cents(subtotal);",
                &format!("{initializer}\n    {binding}"),
            );
            assert_binding_class(&source, ExposureClass::WeaklyExposed)?;
        }
    }
    Ok(())
}

#[test]
fn actual_type_sites_keep_comment_offsets_and_item_hoisting() -> Result<(), String> {
    let primitive = include_str!("fixtures/operand_callee_identity/primitive_control.rs");
    for initializer in [
        "let shipping: u64 = shipping_cents(subtotal)  ;",
        "let shipping = 0 as u64  ;",
    ] {
        let source = primitive.replace(
            "let shipping = shipping_cents(subtotal);",
            &format!(
                "// preceding Unicode comment: λ\n    {initializer}\n    {{ type u64 = (); }}"
            ),
        );
        assert_binding_class(&source, ExposureClass::WeaklyExposed)?;
    }
    let custom = include_str!("fixtures/operand_whitespace/local_annotation_alias.rs");
    let alias = custom.replace(
        "type u64 = Shipping;\n    let shipping: u64 = flat_rate(subtotal);",
        "let shipping: u64 = flat_rate(subtotal);\n    type u64 = Shipping;",
    );
    assert_ne!(
        alias, custom,
        "premise: move the applicable alias after its use"
    );
    assert_binding_class(&alias, ExposureClass::Exposed)?;
    let import = alias.replace("type u64 = Shipping;", "use crate::Shipping as u64;");
    assert_binding_class(&import, ExposureClass::Exposed)
}
