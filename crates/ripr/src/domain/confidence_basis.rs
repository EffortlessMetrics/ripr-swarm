//! Why a preview-adapter finding's `confidence` is capped (#10700).
//!
//! TypeScript/JavaScript (and, by adoption, Python) preview findings carry a
//! per-class constant `confidence`. The number alone cannot tell a reader
//! whether it is low because the oracle is weak, because the adapter does not
//! model a stage, or because the finding is unlikely to be right. The
//! `confidence_basis` list names each capping reason as a stable token so the
//! score is never read bare. The score is a preview-only static ordinal; it is
//! not on the same scale as Rust's `confidence` and is not a probability.

use serde::{Deserialize, Serialize};

use super::{ExposureClass, LanguageId};

/// One reason a preview finding's `confidence` is held at its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceCap {
    /// The adapter does not model value-level infection or propagation.
    PreviewStagesNotModeled,
    /// Syntax-first analysis with no type or import resolution.
    SyntaxFirstNoTypeResolution,
    /// No related test reaches the owner, so no oracle was inspected.
    NoRelatedTest,
    /// Related tests exist but none holds a strong (exact-value) oracle.
    OracleBelowStrong,
    /// A strong oracle exists, but no observed call supplies the changed
    /// boundary input.
    BoundaryInputNotMatched,
    /// A strong oracle and the boundary input exist, but the assertion was
    /// not confirmed to observe the changed value.
    ObservationNotConfirmed,
    /// The discriminator is not established either way.
    DiscriminatorNotEstablished,
    /// The adapter could not establish that the changed value alters behavior.
    InfectionNotEstablished,
    /// The adapter could not establish that the changed effect reaches an
    /// observable output.
    PropagationNotEstablished,
}

/// Evidence the semantic owner reads to derive the basis. Each field is a
/// fact the adapter already computed for the classification.
#[derive(Clone, Copy, Debug)]
pub struct ConfidenceBasisInputs {
    /// Some related test reaches the owner with an oracle-eligible relation.
    pub has_related_test: bool,
    /// The strongest oracle matching the changed behavior family is strong
    /// (exact value or exact error variant).
    pub strong_oracle_present: bool,
    /// A strong oracle's observed inputs include the changed boundary input.
    /// `true` when there is no strong oracle or no boundary to witness.
    pub boundary_input_matched: bool,
}

impl ConfidenceCap {
    /// Stable snake_case wire token.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PreviewStagesNotModeled => "preview_stages_not_modeled",
            Self::SyntaxFirstNoTypeResolution => "syntax_first_no_type_resolution",
            Self::NoRelatedTest => "no_related_test",
            Self::OracleBelowStrong => "oracle_below_strong",
            Self::BoundaryInputNotMatched => "boundary_input_not_matched",
            Self::ObservationNotConfirmed => "observation_not_confirmed",
            Self::DiscriminatorNotEstablished => "discriminator_not_established",
            Self::InfectionNotEstablished => "infection_not_established",
            Self::PropagationNotEstablished => "propagation_not_established",
        }
    }

    /// Plain-English reason, without trailing punctuation. `language` is the
    /// finding's adapter language (`None` reads as a generic preview adapter).
    pub fn summary(self, language: Option<LanguageId>) -> String {
        let name = language.map_or("preview", LanguageId::display_name);
        match self {
            Self::PreviewStagesNotModeled => {
                format!("preview adapter: infection/propagation not modeled ({name})")
            }
            Self::SyntaxFirstNoTypeResolution => {
                "syntax-first, no type or import resolution".to_string()
            }
            Self::NoRelatedTest => "no related test reaches the changed owner".to_string(),
            Self::OracleBelowStrong => {
                "related tests hold no exact-value oracle (weak or smoke assertion)".to_string()
            }
            Self::BoundaryInputNotMatched => {
                "no observed call supplies the changed boundary input".to_string()
            }
            Self::ObservationNotConfirmed => {
                "the assertion is not confirmed to observe the changed value".to_string()
            }
            Self::DiscriminatorNotEstablished => {
                "whether a test checks this behavior is not established".to_string()
            }
            Self::InfectionNotEstablished => {
                "whether the changed value alters behavior is not established".to_string()
            }
            Self::PropagationNotEstablished => {
                "whether the changed effect reaches an output is not established".to_string()
            }
        }
    }

    /// Derives the capping reasons for a preview finding. The first two
    /// reasons hold for every preview finding; the rest name the evidence
    /// that distinguishes findings sharing a per-class score. This only
    /// explains the number: it never changes it and never feeds actionability.
    pub fn for_preview(class: &ExposureClass, inputs: &ConfidenceBasisInputs) -> Vec<Self> {
        let mut caps = vec![
            Self::PreviewStagesNotModeled,
            Self::SyntaxFirstNoTypeResolution,
        ];
        match class {
            ExposureClass::StaticUnknown => caps.push(Self::DiscriminatorNotEstablished),
            ExposureClass::InfectionUnknown => caps.push(Self::InfectionNotEstablished),
            ExposureClass::PropagationUnknown => caps.push(Self::PropagationNotEstablished),
            _ if !inputs.has_related_test => caps.push(Self::NoRelatedTest),
            _ if !inputs.strong_oracle_present => caps.push(Self::OracleBelowStrong),
            ExposureClass::Exposed => {}
            _ if !inputs.boundary_input_matched => caps.push(Self::BoundaryInputNotMatched),
            _ => caps.push(Self::ObservationNotConfirmed),
        }
        caps
    }
}

/// A capping reason rendered for the wire: stable `code` plus plain-English
/// `summary`. Shared by check, explain and context so the shape is one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfidenceBasisEntry {
    pub code: &'static str,
    pub summary: String,
}

/// Wire entries for a finding's capping reasons.
pub fn basis_entries(
    caps: &[ConfidenceCap],
    language: Option<LanguageId>,
) -> Vec<ConfidenceBasisEntry> {
    caps.iter()
        .map(|cap| ConfidenceBasisEntry {
            code: cap.as_str(),
            summary: cap.summary(language),
        })
        .collect()
}

/// One plain-English "why capped" line for a finding, or `None` when the
/// finding carries no basis. States that the score is not Rust-comparable.
pub fn why_capped_line(caps: &[ConfidenceCap], language: Option<LanguageId>) -> Option<String> {
    if caps.is_empty() {
        return None;
    }
    let reasons = caps
        .iter()
        .map(|cap| cap.summary(language))
        .collect::<Vec<_>>()
        .join("; ");
    Some(format!(
        "Why capped: {reasons}. Preview score, not on the same scale as Rust confidence."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(related: bool, strong: bool, boundary: bool) -> ConfidenceBasisInputs {
        ConfidenceBasisInputs {
            has_related_test: related,
            strong_oracle_present: strong,
            boundary_input_matched: boundary,
        }
    }

    #[test]
    fn distinct_evidence_gets_distinct_basis_for_the_same_class() {
        let weak =
            ConfidenceCap::for_preview(&ExposureClass::WeaklyExposed, &inputs(true, false, true));
        let off_boundary =
            ConfidenceCap::for_preview(&ExposureClass::WeaklyExposed, &inputs(true, true, false));
        assert_ne!(weak, off_boundary);
        assert!(weak.contains(&ConfidenceCap::OracleBelowStrong));
        assert!(off_boundary.contains(&ConfidenceCap::BoundaryInputNotMatched));
    }

    #[test]
    fn exposed_with_exact_oracle_carries_only_the_adapter_caps() {
        let caps = ConfidenceCap::for_preview(&ExposureClass::Exposed, &inputs(true, true, true));
        assert_eq!(
            caps,
            vec![
                ConfidenceCap::PreviewStagesNotModeled,
                ConfidenceCap::SyntaxFirstNoTypeResolution
            ]
        );
    }

    #[test]
    fn line_names_language_and_denies_rust_comparability() {
        let caps =
            ConfidenceCap::for_preview(&ExposureClass::NoStaticPath, &inputs(false, false, true));
        let line = why_capped_line(&caps, Some(LanguageId::TypeScript)).unwrap_or_default();
        assert!(
            line.contains("infection/propagation not modeled (TypeScript)"),
            "{line}"
        );
        assert!(line.contains("no related test"), "{line}");
        assert!(line.contains("not on the same scale as Rust"), "{line}");
        assert!(why_capped_line(&[], None).is_none());
    }

    #[test]
    fn exposed_without_a_strong_oracle_is_capped_on_the_oracle() {
        let caps = ConfidenceCap::for_preview(&ExposureClass::Exposed, &inputs(true, false, true));
        assert!(caps.contains(&ConfidenceCap::OracleBelowStrong), "{caps:?}");
        let strong = ConfidenceCap::for_preview(&ExposureClass::Exposed, &inputs(true, true, true));
        assert_ne!(caps, strong);
    }

    #[test]
    fn remaining_arms_name_their_own_reason() {
        let unknown =
            ConfidenceCap::for_preview(&ExposureClass::StaticUnknown, &inputs(true, true, true));
        assert_eq!(
            unknown.last(),
            Some(&ConfidenceCap::DiscriminatorNotEstablished)
        );
        let none =
            ConfidenceCap::for_preview(&ExposureClass::WeaklyExposed, &inputs(false, false, true));
        assert_eq!(none.last(), Some(&ConfidenceCap::NoRelatedTest));
        let observed =
            ConfidenceCap::for_preview(&ExposureClass::WeaklyExposed, &inputs(true, true, true));
        assert_eq!(
            observed.last(),
            Some(&ConfidenceCap::ObservationNotConfirmed)
        );
        for cap in unknown.iter().chain(&none).chain(&observed) {
            assert!(!cap.as_str().is_empty() && !cap.summary(None).is_empty());
        }
    }

    #[test]
    fn infection_and_propagation_unknown_name_their_own_reason() {
        let infection =
            ConfidenceCap::for_preview(&ExposureClass::InfectionUnknown, &inputs(true, true, true));
        assert_eq!(
            infection.last(),
            Some(&ConfidenceCap::InfectionNotEstablished)
        );
        let propagation = ConfidenceCap::for_preview(
            &ExposureClass::PropagationUnknown,
            &inputs(false, false, true),
        );
        assert_eq!(
            propagation.last(),
            Some(&ConfidenceCap::PropagationNotEstablished)
        );
    }
}
