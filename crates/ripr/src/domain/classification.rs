/// RIPR-SPEC-0113: honest per-finding guidance for `no_static_path` findings.
///
/// Defined once here in `domain/` so both `analysis::classify::decision` (production path)
/// and `analysis::classifier` (test assertions) can reference the same literal without
/// duplicating it. Exported via `crate::domain::*`.
pub(crate) const NO_STATIC_PATH_NEXT_STEP: &str = "ripr found no static test path to this change \u{2014} this is not a coverage assessment. \
A test may already exercise it through macros, helper-call chains, or integration tests that \
ripr's static model does not yet trace. If none does, add a co-located test that reaches and \
observes the changed behavior so an assertion can notice if it is wrong.";

/// #9173: a root build script runs under Cargo before the package compiles and
/// no package source mounts it, so "add a co-located test" cannot close the
/// gap; point at what can.
pub(crate) const BUILD_SCRIPT_NO_STATIC_PATH_NEXT_STEP: &str = "ripr found no static test path to this change \u{2014} this is not a coverage assessment. \
A Cargo build script runs before the package compiles and no package source includes it, so no package test can call it. \
Check the change by building the package and inspecting the script's output or effect (its `cargo:` directives or generated files), \
or move the logic into a library function that a test can call.";

/// RIPR-SPEC-0133: shared one-sentence reason an owner counts as an
/// assertion-shaped oracle. Kept in `domain/` so the detector
/// (`analysis::classify::owner_shape`), the `Finding.evidence` disclosure line,
/// and the guidance strings below state the rule identically (a unit test pins
/// each guidance string to contain this clause).
pub(crate) const ASSERTION_SHAPED_OWNER_REASON: &str =
    "its body is dominated by assert*/expect calls and nothing outside tests calls it";

/// RIPR-SPEC-0133: per-class `recommended_next_step` text when the changed
/// owner is itself an assertion-shaped helper (an oracle), so the standard
/// code-under-test advice would ask for a test of the test. The exposure class
/// is unchanged; only the guidance is reframed for oracle-shaped owners. Each
/// string embeds `ASSERTION_SHAPED_OWNER_REASON` verbatim so the rule is
/// explainable in one sentence in output.
pub(crate) const ASSERTION_SHAPED_NO_STATIC_PATH_NEXT_STEP: &str = "The changed owner is itself an assertion helper (its body is dominated by assert*/expect calls and \
nothing outside tests calls it), so a test that observes it would be a test of the oracle. Sharpen its \
assertions, or exercise it indirectly through the code it checks.";

/// Weak predicate whose related test already supplies the changed boundary
/// input (#10622); the advisory keys off this exact Next step.
pub(crate) const BOUNDARY_OBSERVED_WEAK_ORACLE_NEXT_STEP: &str = "A related test already reaches the boundary input, but ripr grades its assertion as a weak check; write it as an exact-value assertion ripr can credit, for example assert_eq!(result, expected), so the old and changed behavior differ.";

pub(crate) const ASSERTION_SHAPED_WEAKLY_EXPOSED_NEXT_STEP: &str = "The changed owner is an assertion helper (its body is dominated by assert*/expect calls and nothing \
outside tests calls it), so exact-equality advice for code under test may not apply — boolean invariants \
have no exact equality. Tighten the loosest assertion in this helper: a sharper predicate, an exact \
expected value where one exists, or a case the current assertions would not flag.";

pub(crate) const ASSERTION_SHAPED_REACHABLE_UNREVEALED_NEXT_STEP: &str = "The changed owner is an assertion helper (its body is dominated by assert*/expect calls and nothing \
outside tests calls it); it is the observation, not the code under observation. Ensure at least one test \
calls this helper over inputs that exercise the changed check.";

pub(crate) const ASSERTION_SHAPED_INFECTION_UNKNOWN_NEXT_STEP: &str = "The changed owner is an assertion helper (its body is dominated by assert*/expect calls and nothing \
outside tests calls it); inspect the inputs and behavior this helper checks, including any existing \
boundary or negative-path assertions.";

/// RIPR-SPEC-0115: stable leading phrase of the transitive-reach *witness*
/// pointer. The producer (`analysis::classify::transitive_reach`) begins the
/// pointer with this phrase, and the human renderer (`output::human`) recognizes
/// it in `Finding.evidence` to surface a concrete "Where to look" line. Shared
/// here in `domain/` so the producer and renderer agree on one literal across
/// the analysis/output seam (reuse, don't fork).
pub(crate) const TRANSITIVE_REACH_WITNESS_PREFIX: &str = "For example, the test ";

/// Evidence prefix naming the first related `assert_eq!` ripr found but did
/// not credit, and why (`rust_assertion_context_unestablished`). The
/// classifier writes it; the human renderer quotes it so a refusal says what
/// blocked it instead of reading as "no assertion".
pub(crate) const ASSERTION_NOT_CREDITED_PREFIX: &str = "assertion not credited: ";

/// Observe summary when a related `assert_eq!` exists but its execution or
/// macro binding is not statically established (RIPR-SPEC-0197). Shared so
/// the renderer can tell this refusal from "no assertion at all".
pub(crate) const ASSERTION_CONTEXT_UNESTABLISHED: &str = "No statically established oracle: assertion execution or macro binding is unestablished (rust_assertion_context_unestablished)";

/// Reach summary prefix when every related test is a `SeamCalleeCall`: the
/// tests exercise the seam's converted callee and never invoke the changed
/// owner (#3714). The producer (`analysis::classify::reach`) builds the
/// summary from this literal, and the human renderer recognizes it so no
/// classification hint claims a reaching test for such a finding (#7003).
/// Shared here in `domain/` so the two sides agree on one literal across
/// the analysis/output seam (reuse, don't fork).
pub(crate) const CALLEE_ONLY_REACH_PREFIX: &str = "Related tests exercise the wrapper seam's converted callee (the changed owner is not invoked by them)";

/// RIPR-SPEC-0114/0117: stable evidence prefixes for named static-limitation
/// detail. Producers append these lines to `Finding.evidence`; renderers and
/// corpus checks consume the same prefixes so the unresolved edge stays visible
/// across JSON and human projections.
pub(crate) const LIMITATION_LAST_ESTABLISHED_EDGE_PREFIX: &str =
    "limitation_last_established_edge: ";
pub(crate) const LIMITATION_FIRST_UNRESOLVED_EDGE_PREFIX: &str =
    "limitation_first_unresolved_edge: ";
pub(crate) const LIMITATION_ANALYZER_ROUTE_PREFIX: &str = "limitation_analyzer_route: ";
pub(crate) const LIMITATION_NON_CLAIM_PREFIX: &str = "limitation_non_claim: ";
/// Additive declaration context, kept separate from the canonical edge text.
pub(crate) const LIMITATION_HELPER_BOUNDARY_PREFIX: &str = "limitation_helper_boundary: ";

/// Static exposure classification for a changed behavior.
///
/// This is draft static evidence about whether current tests appear to
/// contain a discriminator for the change; it never claims a test was
/// executed or that test adequacy was demonstrated. See
/// `docs/STATIC_EXPOSURE_MODEL.md` for the full model.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExposureClass {
    /// Static evidence suggests a complete RIPR path to a strong oracle
    /// that observes the changed sink.
    Exposed,
    /// A path exists, but infection or discrimination appears weak.
    WeaklyExposed,
    /// Related tests appear reachable, but no meaningful oracle was found.
    ReachableUnrevealed,
    /// No static test path was found for the changed owner.
    NoStaticPath,
    /// Reachability exists, but input or fixture evidence is opaque.
    InfectionUnknown,
    /// The changed behavior crosses an opaque propagation boundary.
    PropagationUnknown,
    /// Syntax-first analysis cannot make a credible judgment.
    StaticUnknown,
}

impl ExposureClass {
    /// Every class in vocabulary order, for adapters that validate a class
    /// wire token instead of re-spelling the vocabulary.
    pub(crate) const ALL: [ExposureClass; 7] = [
        ExposureClass::Exposed,
        ExposureClass::WeaklyExposed,
        ExposureClass::ReachableUnrevealed,
        ExposureClass::NoStaticPath,
        ExposureClass::InfectionUnknown,
        ExposureClass::PropagationUnknown,
        ExposureClass::StaticUnknown,
    ];

    /// The stable snake_case wire token for this class (for example
    /// `weakly_exposed`), as used in JSON output and configuration.
    pub fn as_str(&self) -> &'static str {
        profile::for_class(self).label
    }

    /// The diagnostic severity associated with this class (`info`,
    /// `warning`, or `note`).
    pub fn severity(&self) -> &'static str {
        profile::for_class(self).severity
    }

    /// Whether a finding with this class must carry an explicit stop
    /// reason explaining why static analysis could not judge further.
    pub fn requires_stop_reason(&self) -> bool {
        profile::for_class(self).requires_stop_reason
    }

    /// The plain word human output leads with. It is the vocabulary of the
    /// `ripr check` summary line, and `SeamGripClass::plain_label` maps the
    /// seam-level grip classes onto the same words, so `check` and `pilot`
    /// name one gap the same way while the schema values stay distinct.
    pub(crate) fn plain_label(&self) -> &'static str {
        match self {
            ExposureClass::Exposed => "exposed",
            ExposureClass::WeaklyExposed => "weak",
            ExposureClass::ReachableUnrevealed => "unrevealed",
            ExposureClass::NoStaticPath => "no path",
            ExposureClass::InfectionUnknown
            | ExposureClass::PropagationUnknown
            | ExposureClass::StaticUnknown => "unknown",
        }
    }

    /// #9373: the plain label for a row that lists a finding without room for a
    /// full explanation (the default digest's `Hidden:` rows). The shared
    /// `plain_label` (#8165) is unchanged; the three unknown classes keep the
    /// word `unknown` and add which stage the analysis could not judge, so a
    /// reader can tell them apart without rerunning `--format human-full`.
    pub(crate) fn plain_label_with_stage(&self) -> &'static str {
        match self {
            ExposureClass::InfectionUnknown => "unknown: infection unclear",
            ExposureClass::PropagationUnknown => "unknown: propagation unclear",
            ExposureClass::StaticUnknown => "unknown: static analysis limited",
            other => other.plain_label(),
        }
    }

    /// #9373: a one-line plain meaning for this class, the vocabulary of
    /// `plain_label_with_stage`. Prose only, no schema tokens.
    pub(crate) fn plain_gloss(&self) -> &'static str {
        match self {
            ExposureClass::Exposed => "tests appear to notice the change",
            ExposureClass::WeaklyExposed => "tests reach it but check it only loosely",
            ExposureClass::ReachableUnrevealed => {
                "tests reach it but nothing appears to notice a change"
            }
            ExposureClass::NoStaticPath => "no test path was found",
            ExposureClass::InfectionUnknown => {
                "ripr could not tell what input makes the change take effect"
            }
            ExposureClass::PropagationUnknown => {
                "ripr could not follow the change through an opaque boundary"
            }
            ExposureClass::StaticUnknown => {
                "reading the code was not enough for ripr to judge (analysis limited, not a verdict)"
            }
        }
    }

    /// `plain word (` for the bounded (default) human digest (#8165): the
    /// schema value (`as_str`) is not repeated here; it stays available in
    /// `--format human-full`, JSON, and `ripr explain`.
    pub(crate) fn human_lead(&self) -> String {
        format!("{} (", self.plain_label())
    }
}

mod profile {
    use super::ExposureClass;

    pub(super) struct ExposureProfile {
        pub(super) label: &'static str,
        pub(super) severity: &'static str,
        pub(super) requires_stop_reason: bool,
    }

    pub(super) fn for_class(class: &ExposureClass) -> ExposureProfile {
        match class {
            ExposureClass::Exposed => ExposureProfile {
                label: "exposed",
                severity: "info",
                requires_stop_reason: false,
            },
            ExposureClass::WeaklyExposed => ExposureProfile {
                label: "weakly_exposed",
                severity: "warning",
                requires_stop_reason: false,
            },
            ExposureClass::ReachableUnrevealed => ExposureProfile {
                label: "reachable_unrevealed",
                severity: "warning",
                requires_stop_reason: false,
            },
            ExposureClass::NoStaticPath => ExposureProfile {
                label: "no_static_path",
                severity: "warning",
                requires_stop_reason: false,
            },
            ExposureClass::InfectionUnknown => ExposureProfile {
                label: "infection_unknown",
                severity: "warning",
                requires_stop_reason: true,
            },
            ExposureClass::PropagationUnknown => ExposureProfile {
                label: "propagation_unknown",
                severity: "note",
                requires_stop_reason: true,
            },
            ExposureClass::StaticUnknown => ExposureProfile {
                label: "static_unknown",
                severity: "note",
                requires_stop_reason: true,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExposureClass;
    use std::collections::HashSet;

    fn all_exposure_classes() -> [ExposureClass; 7] {
        [
            ExposureClass::Exposed,
            ExposureClass::WeaklyExposed,
            ExposureClass::ReachableUnrevealed,
            ExposureClass::NoStaticPath,
            ExposureClass::InfectionUnknown,
            ExposureClass::PropagationUnknown,
            ExposureClass::StaticUnknown,
        ]
    }

    #[test]
    fn exposure_class_strings_match_contract_terms() {
        let cases = [
            (ExposureClass::Exposed, "exposed"),
            (ExposureClass::WeaklyExposed, "weakly_exposed"),
            (ExposureClass::ReachableUnrevealed, "reachable_unrevealed"),
            (ExposureClass::NoStaticPath, "no_static_path"),
            (ExposureClass::InfectionUnknown, "infection_unknown"),
            (ExposureClass::PropagationUnknown, "propagation_unknown"),
            (ExposureClass::StaticUnknown, "static_unknown"),
        ];

        for (class, expected) in cases {
            assert_eq!(class.as_str(), expected);
        }
    }

    #[test]
    fn exposure_class_severities_match_output_expectations() {
        let cases = [
            (ExposureClass::Exposed, "info"),
            (ExposureClass::WeaklyExposed, "warning"),
            (ExposureClass::ReachableUnrevealed, "warning"),
            (ExposureClass::NoStaticPath, "warning"),
            (ExposureClass::InfectionUnknown, "warning"),
            (ExposureClass::PropagationUnknown, "note"),
            (ExposureClass::StaticUnknown, "note"),
        ];

        for (class, expected) in cases {
            assert_eq!(class.severity(), expected);
        }
    }

    #[test]
    fn stop_reason_requirement_is_only_for_unknown_classes() {
        assert!(!ExposureClass::Exposed.requires_stop_reason());
        assert!(!ExposureClass::WeaklyExposed.requires_stop_reason());
        assert!(!ExposureClass::ReachableUnrevealed.requires_stop_reason());
        assert!(!ExposureClass::NoStaticPath.requires_stop_reason());
        assert!(ExposureClass::InfectionUnknown.requires_stop_reason());
        assert!(ExposureClass::PropagationUnknown.requires_stop_reason());
        assert!(ExposureClass::StaticUnknown.requires_stop_reason());
    }

    #[test]
    fn exposure_class_contract_terms_are_unique() {
        let mut seen = HashSet::new();

        for class in all_exposure_classes() {
            assert!(
                seen.insert(class.as_str()),
                "duplicate contract term found for {}",
                class.as_str()
            );
        }

        assert_eq!(seen.len(), 7, "every class should map to a unique term");
    }

    #[test]
    fn exposure_class_severity_values_stay_in_supported_set() {
        let supported = HashSet::from(["info", "warning", "note"]);
        for class in all_exposure_classes() {
            assert!(
                supported.contains(class.severity()),
                "unsupported severity {} for {}",
                class.severity(),
                class.as_str()
            );
        }
    }
}
