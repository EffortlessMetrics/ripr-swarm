//! Rust canonical-gap identity shaping.
//!
//! Mirrors the Python producer's `probe_shape` module: the Rust diff and repo
//! production loops call [`canonical_rust_gap_for`] after the finding's typed
//! static limitation is settled, so a finding that names no producer-named
//! missing discriminator still carries the canonical gap identity the MCP
//! discriminator gate reads (`#5268`).
//!
//! Honesty boundaries, shared with the Python/Perl producers:
//!
//! - the gap is an identity, not an establishment claim. Its
//!   `normalized_discriminator` names the changed behavior a focused test
//!   would need to discriminate (the same boundary the classifier's
//!   missing-discriminator statement renders for a comparison seam); whether
//!   a test observes it stays a separate, downstream producer fact.
//! - a finding withheld behind its own typed static limitation keeps
//!   `canonical_gap: None` (the caller gates on `static_limit_kind`), exactly
//!   like the Python producer, so the MCP readiness refusal stays
//!   `static_limitation`.
//! - a probe with no owner identity anchors no canonical gap (`None`): an
//!   ownerless line names no canonical owner to burn down against.
//!
//! The key normalization mirrors the Python producer's conventions (word runs
//! joined with `_`, operator characters preserved, lowercase) so both
//! languages project the same compact identity shape from their own changed
//! line; each language keeps its own matcher by contract.

use crate::analysis::classify::comparison_operands;
use crate::domain::{FindingCanonicalGap, ProbeFamily, SymbolId};
use std::path::Path;

/// Build the canonical gap identity for one Rust finding's probe, or `None`
/// when the probe carries no owner to anchor it.
///
/// `file` is the producer's workspace-relative path (the same projection the
/// index builds [`crate::domain::SymbolId`]s from); `expression` is the
/// changed line text the probe carries (`Probe::expression`).
pub(crate) fn canonical_rust_gap_for(
    file: &Path,
    owner: Option<&SymbolId>,
    probe_family: &ProbeFamily,
    expression: &str,
) -> Option<FindingCanonicalGap> {
    let owner = qualified_owner(owner, file)?;
    let file = crate::analysis::stable_path_text(file);
    let behavior_kind = rust_behavior_kind(probe_family).to_string();
    let probe_kind = probe_family.as_str().to_string();
    let normalized_discriminator = normalize_rust_gap_discriminator(probe_family, expression);
    let id =
        format!("gap:rust:{file}:{owner}:{behavior_kind}:{probe_kind}:{normalized_discriminator}");
    Some(FindingCanonicalGap {
        id,
        language: "rust".to_string(),
        file,
        owner,
        behavior_kind,
        probe_kind,
        normalized_discriminator,
    })
}

/// The module-qualified owner path (`pricing::score`) a repo seam's owner
/// projection uses: the index builds symbol ids as
/// `{stable file text}::{module path}::{name}`, so the file prefix strips off.
/// The full symbol id survives when it does not carry the expected prefix.
fn qualified_owner(owner: Option<&SymbolId>, file: &Path) -> Option<String> {
    let id = owner.as_ref()?.0.as_str();
    let prefix = format!("{}::", crate::analysis::stable_path_text(file));
    Some(id.strip_prefix(&prefix).unwrap_or(id).to_string())
}

fn rust_behavior_kind(probe_family: &ProbeFamily) -> &'static str {
    match probe_family {
        ProbeFamily::Predicate => "predicate_boundary",
        ProbeFamily::ReturnValue => "return_value",
        ProbeFamily::ErrorPath => "error_variant",
        ProbeFamily::FieldConstruction => "field_construction",
        ProbeFamily::SideEffect => "side_effect",
        ProbeFamily::MatchArm => "match_arm",
        ProbeFamily::CallDeletion => "call_deletion",
        ProbeFamily::StaticUnknown => "static_unknown",
    }
}

/// Derive the normalized discriminator from the changed line text.
///
/// For a comparison predicate this is the equality boundary form the
/// classifier's missing-discriminator statement renders
/// (`amount >= threshold` renders `amount == threshold`), extracted by the
/// same operand reader, so one changed seam cannot name two different
/// discriminators. Other families strip the family's statement prefix, again
/// mirroring the Python matcher's shape.
fn normalize_rust_gap_discriminator(probe_family: &ProbeFamily, expression: &str) -> String {
    let text = expression.trim();
    let stripped = match probe_family {
        ProbeFamily::Predicate => comparison_operands(text)
            .map(|(left, right)| format!("{left} == {right}"))
            .unwrap_or_else(|| strip_predicate_statement(text)),
        ProbeFamily::ReturnValue | ProbeFamily::ErrorPath => strip_return_statement(text),
        ProbeFamily::MatchArm => strip_match_arm_arrow(text),
        _ => text.to_string(),
    };
    rust_gap_key_text(&stripped)
}

fn strip_predicate_statement(text: &str) -> String {
    let mut text = text
        .trim_start_matches("if let ")
        .trim_start_matches("if ")
        .trim_start_matches("while let ")
        .trim_start_matches("while ")
        .trim();
    if let Some((before, _)) = text.split_once('{') {
        text = before.trim();
    }
    text.trim_end_matches(';').trim().to_string()
}

fn strip_return_statement(text: &str) -> String {
    text.trim()
        .strip_prefix("return ")
        .unwrap_or(text)
        .trim()
        .trim_end_matches(';')
        .trim()
        .to_string()
}

fn strip_match_arm_arrow(text: &str) -> String {
    text.trim().trim_end_matches("=>").trim().to_string()
}

/// Same key conventions as the Python producer's `normalize_gap_key_text`:
/// word runs keep their characters lowercased and rejoin with `_`, operator
/// characters survive verbatim, any other character separates, and an empty
/// result falls back to `unknown` so an id component is never empty.
fn rust_gap_key_text(text: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_word = false;
    let mut pending_separator = false;

    for character in text.chars() {
        if character.is_ascii_alphanumeric() || character == '_' || character == '.' {
            if pending_separator && previous_was_word {
                normalized.push('_');
            }
            normalized.push(character.to_ascii_lowercase());
            previous_was_word = true;
            pending_separator = false;
        } else if matches!(
            character,
            '=' | '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '[' | ']'
        ) {
            normalized.push(character);
            previous_was_word = false;
            pending_separator = false;
        } else {
            pending_separator = true;
        }
    }

    let trimmed = normalized.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn owner(file: &str, qualified: &str) -> SymbolId {
        SymbolId(format!("{file}::{qualified}"))
    }

    fn gap_for(
        file: &Path,
        owner: Option<SymbolId>,
        probe_family: &ProbeFamily,
        expression: &str,
    ) -> Result<FindingCanonicalGap, String> {
        canonical_rust_gap_for(file, owner.as_ref(), probe_family, expression).ok_or_else(|| {
            format!("an owned {probe_family:?} probe must carry a canonical gap: {expression:?}")
        })
    }

    #[test]
    fn predicate_comparison_names_the_rendered_equality_boundary() -> Result<(), String> {
        // The changed line `if amount >= discount_threshold {` renders the
        // same `amount == discount_threshold` boundary the classifier's
        // missing-discriminator statement renders (activation.rs), normalized
        // through the shared key conventions.
        let gap = gap_for(
            Path::new("src/pricing.rs"),
            Some(owner("src/pricing.rs", "pricing::discounted_total")),
            &ProbeFamily::Predicate,
            "if amount >= discount_threshold {",
        )?;
        assert_eq!(gap.language, "rust");
        assert_eq!(gap.file, "src/pricing.rs");
        assert_eq!(gap.owner, "pricing::discounted_total");
        assert_eq!(gap.behavior_kind, "predicate_boundary");
        assert_eq!(gap.probe_kind, "predicate");
        assert_eq!(gap.normalized_discriminator, "amount==discount_threshold");
        assert_eq!(
            gap.id,
            "gap:rust:src/pricing.rs:pricing::discounted_total:\
             predicate_boundary:predicate:amount==discount_threshold"
        );
        Ok(())
    }

    #[test]
    fn reversed_and_braced_predicates_keep_the_boundary_readable() -> Result<(), String> {
        let gap = gap_for(
            Path::new("src/lib.rs"),
            Some(owner("src/lib.rs", "gate")),
            &ProbeFamily::Predicate,
            "100 < amount",
        )?;
        // Equality is symmetric, so a reversed comparison still names the
        // same boundary case.
        assert_eq!(gap.normalized_discriminator, "100==amount");

        let braced = gap_for(
            Path::new("src/lib.rs"),
            Some(owner("src/lib.rs", "gate")),
            &ProbeFamily::Predicate,
            "if amount == discount_threshold {",
        )?;
        assert_eq!(
            braced.normalized_discriminator,
            "amount==discount_threshold"
        );
        Ok(())
    }

    #[test]
    fn non_comparison_predicates_fall_back_to_the_statement_text() -> Result<(), String> {
        let gap = gap_for(
            Path::new("src/lib.rs"),
            Some(owner("src/lib.rs", "gate")),
            &ProbeFamily::Predicate,
            "if cache.is_empty() {",
        )?;
        assert_eq!(gap.normalized_discriminator, "cache.is_empty");
        Ok(())
    }

    #[test]
    fn return_and_match_families_strip_their_statement_shapes() -> Result<(), String> {
        let returned = gap_for(
            Path::new("src/lib.rs"),
            Some(owner("src/lib.rs", "rate")),
            &ProbeFamily::ReturnValue,
            "return amount - 10;",
        )?;
        assert_eq!(returned.behavior_kind, "return_value");
        assert_eq!(returned.normalized_discriminator, "amount-10");

        let arm = gap_for(
            Path::new("src/lib.rs"),
            Some(owner("src/lib.rs", "parse")),
            &ProbeFamily::MatchArm,
            "OutputFormat::Json =>",
        )?;
        assert_eq!(arm.behavior_kind, "match_arm");
        assert_eq!(arm.normalized_discriminator, "outputformat_json");
        Ok(())
    }

    #[test]
    fn windows_separators_normalize_into_the_identity() -> Result<(), String> {
        let gap = gap_for(
            &PathBuf::from("src\\pricing.rs"),
            Some(owner("src/pricing.rs", "discounted_total")),
            &ProbeFamily::Predicate,
            "if amount >= discount_threshold {",
        )?;
        assert_eq!(gap.file, "src/pricing.rs");
        assert_eq!(gap.owner, "discounted_total");
        Ok(())
    }

    #[test]
    fn ownerless_probes_anchor_no_canonical_gap() {
        assert!(
            canonical_rust_gap_for(
                Path::new("src/lib.rs"),
                None,
                &ProbeFamily::Predicate,
                "if amount >= threshold {",
            )
            .is_none()
        );
    }

    #[test]
    fn foreign_symbol_id_survives_as_the_owner() -> Result<(), String> {
        // An id that does not carry the expected file prefix (an exotic
        // producer) degrades to the full symbol id instead of a wrong owner.
        let gap = gap_for(
            Path::new("src/lib.rs"),
            Some(SymbolId("impl W::f".to_string())),
            &ProbeFamily::ReturnValue,
            "return 1;",
        )?;
        assert_eq!(gap.owner, "impl W::f");
        Ok(())
    }

    #[test]
    fn empty_key_text_falls_back_to_unknown() {
        assert_eq!(rust_gap_key_text("   ()  "), "unknown");
    }
}
