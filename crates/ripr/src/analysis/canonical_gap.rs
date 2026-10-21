//! Canonical behavioral gap identity for Lane 1 evidence.
//!
//! `RepoSeam` IDs stay tied to source locations so before/after movement can
//! track exact static seams. Canonical gap IDs are a separate burn-down key:
//! they group visible gap records by the behavior a focused test would need to
//! discriminate, while leaving line numbers as locators.

use super::ClassifiedSeam;
use super::language::producer_gap_names_seam_shape;
use super::seams::{RequiredDiscriminator, SeamId, SeamKind};
use std::collections::BTreeMap;

pub(crate) const CANONICAL_GAP_REASON: &str =
    "same owner, seam kind, flow sink, missing discriminator, and assertion shape";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CanonicalGapIdentity {
    pub(crate) id: String,
    pub(crate) group_size: usize,
    pub(crate) reason: &'static str,
    pub(crate) owner: String,
    pub(crate) seam_kind: String,
    pub(crate) flow_sink: String,
    pub(crate) missing_discriminator: String,
    pub(crate) assertion_shape: String,
}

/// Return canonical gap identities keyed by seam ID.
///
/// Only headline-eligible seam classes receive canonical gap IDs. Strong,
/// intentional, suppressed, and opaque static-limitation records remain visible
/// but are not grouped as actionable behavioral debt.
pub(crate) fn canonical_gap_identities(
    classified: &[ClassifiedSeam],
) -> BTreeMap<SeamId, CanonicalGapIdentity> {
    let mut pending = Vec::new();
    let mut group_counts = BTreeMap::<String, usize>::new();

    for entry in classified {
        let Some(identity) = canonical_gap_identity(entry) else {
            continue;
        };
        *group_counts.entry(identity.id.clone()).or_insert(0) += 1;
        pending.push((entry.seam.id().clone(), identity));
    }

    pending
        .into_iter()
        .map(|(seam_id, mut identity)| {
            identity.group_size = group_counts.get(&identity.id).copied().unwrap_or(1);
            (seam_id, identity)
        })
        .collect()
}

pub(crate) fn canonical_gap_identity(entry: &ClassifiedSeam) -> Option<CanonicalGapIdentity> {
    if !entry.class.is_headline_eligible() {
        return None;
    }

    let owner = entry.seam.owner().to_string();
    let seam_kind = entry.seam.kind().as_str().to_string();
    let flow_sink = entry.seam.expected_sink().as_str().to_string();
    let missing_discriminator = missing_discriminator_key(entry);
    let assertion_shape = assertion_shape_kind_for(entry.seam.kind()).to_string();
    let id = compute_canonical_gap_id([
        owner.as_str(),
        seam_kind.as_str(),
        flow_sink.as_str(),
        missing_discriminator.as_str(),
        assertion_shape.as_str(),
    ]);

    Some(CanonicalGapIdentity {
        id,
        group_size: 1,
        reason: CANONICAL_GAP_REASON,
        owner,
        seam_kind,
        flow_sink,
        missing_discriminator,
        assertion_shape,
    })
}

/// Whether a `ripr check` finding ID (`probe:...`, or `repo-probe:...` for a
/// whole-repo probe) names this seam (#8166 residual).
///
/// Like [`producer_gap_id_names_seam`], the ID is read from the seam side:
/// the probe fingerprint is re-minted from the seam's file, the probe family
/// its kind names, its owner, and its own source expression, with the same
/// content-addressed fingerprint the probe producer uses. A changed line the
/// seam does not carry therefore never matches. Several seams can still
/// share one re-minted ID (the fingerprint carries no line); the caller owns
/// that uniqueness decision.
///
/// The collision ordinal (`.2`, ...) the probe producers append to the
/// 2nd+ occurrence of a repeated ID is handled per ID family, because the
/// two producers mint over different sets:
///
/// * `repo-probe:` — rank-binding. The repo producer
///   (`probes/repo.rs:82`) scans every probe shape in the repo, so its
///   mint set is the seam set: the first occurrence of a base ID keeps it
///   bare and each later occurrence takes its 1-based encounter rank.
///   The seam side mirrors that ordering exactly: `seams` is scanned in
///   slice order (the classified inventory order, which sorts by file
///   then byte offset in `seam_inventory.rs:2800`, i.e. source order
///   within the one file a base ID can name, since the fingerprint
///   carries that file), and an explicit ordinal `N` names only the seam
///   whose rank among the seams re-minting the same base ID equals `N`.
///   An invented ordinal (`.999`) or a `.2` whose second occurrence was
///   seam-filtered so only the first seam remains therefore names
///   nothing (stale).
/// * `probe:` — NOT rank-binding. The diff producer
///   (`dedup_probe_ids` in `probes/diff.rs:582`) ranks only the probes
///   for changed lines, while the seam inventory also contains unchanged
///   seams re-minting the same base ID. A diff ordinal therefore cannot
///   be rank-bound from the full inventory: with three identical
///   predicates where the diff touches only occurrences 1 and 3, the
///   second changed probe is `.2` but inventory rank 2 is the unchanged
///   middle seam, so rank-binding would silently start repair on a seam
///   the diff never touched. Instead an explicit diff ordinal names
///   every seam re-minting the shared base, so the caller refuses it as
///   ambiguous (bounded seam list, no attempt). The sole exception is a
///   single sharing seam, where an explicit ordinal (`.2`, `.999`) still
///   names nothing (stale): its second occurrence is not in the
///   inventory at all.
///
/// With no ordinal, the bare base ID names every sharing seam for both
/// families, so the caller refuses it as ambiguous rather than picking
/// an occurrence.
pub(crate) fn probe_id_names_seam(
    item: &str,
    entry: &ClassifiedSeam,
    seams: &[ClassifiedSeam],
) -> bool {
    let prefix = if item.starts_with("probe:") {
        "probe"
    } else if item.starts_with("repo-probe:") {
        "repo-probe"
    } else {
        return false;
    };
    let explicit_ordinal = item
        .rsplit_once(':')
        .and_then(|(_, fp)| fp.rsplit_once('.'))
        .and_then(|(_, ordinal)| ordinal.parse::<u32>().ok());
    let Some(ordinal) = explicit_ordinal else {
        // No ordinal: the bare base ID names every seam re-minting it;
        // the caller owns the resulting ambiguity.
        if reminted_probe_id(entry, prefix, 1).as_deref() != Some(item) {
            return false;
        }
        let Some(base) = reminted_probe_id(entry, prefix, 1) else {
            return false;
        };
        return seams
            .iter()
            .any(|candidate| reminted_probe_id(candidate, prefix, 1).as_deref() == Some(&*base));
    };
    if reminted_probe_id(entry, prefix, ordinal).as_deref() != Some(item) {
        return false;
    }
    let Some(base) = reminted_probe_id(entry, prefix, 1) else {
        return false;
    };
    let sharing = seams
        .iter()
        .filter(|candidate| reminted_probe_id(candidate, prefix, 1).as_deref() == Some(&*base))
        .count();
    match prefix {
        // `repo-probe:` ordinals CAN be rank-bound: the repo producer's
        // mint set is the seam set (see the doc comment above), so the
        // seam at 1-based rank `ordinal` among the seams re-minting
        // `base`, in slice (inventory/source) order, is the named seam.
        "repo-probe" => {
            let mut rank = 0u32;
            for candidate in seams {
                if reminted_probe_id(candidate, prefix, 1).as_deref() == Some(&*base) {
                    rank += 1;
                    if candidate.seam.id() == entry.seam.id() {
                        return rank == ordinal;
                    }
                }
            }
            false
        }
        // `probe:` ordinals CANNOT be rank-bound from the full inventory:
        // the diff producer ranked only changed-line probes, so inventory
        // rank can name an unchanged seam the diff never touched. Fall
        // back to the shared-base seam set (the caller refuses it as
        // ambiguous); a sole sharing seam still names nothing, so a
        // filtered-twin `.2` refuses as stale.
        _ => sharing > 1,
    }
}

/// Re-mint the probe ID for one seam at `ordinal`, or `None` when the
/// seam's kind names no probe family.
fn reminted_probe_id(entry: &ClassifiedSeam, prefix: &str, ordinal: u32) -> Option<String> {
    const FAMILIES: [crate::domain::ProbeFamily; 8] = [
        crate::domain::ProbeFamily::Predicate,
        crate::domain::ProbeFamily::ReturnValue,
        crate::domain::ProbeFamily::ErrorPath,
        crate::domain::ProbeFamily::CallDeletion,
        crate::domain::ProbeFamily::FieldConstruction,
        crate::domain::ProbeFamily::SideEffect,
        crate::domain::ProbeFamily::MatchArm,
        crate::domain::ProbeFamily::StaticUnknown,
    ];
    let sanitized = super::sanitize_path(entry.seam.file());
    let owner = crate::domain::SymbolId(entry.seam.owner().to_string());
    FAMILIES.iter().find_map(|family| {
        (super::seam_kind_for_probe_family(family.as_str()) == Some(entry.seam.kind())).then(|| {
            super::fingerprint_probe_id(
                prefix,
                &sanitized,
                family.as_str(),
                &owner.0,
                &super::normalize_rust_expression(entry.seam.expression()),
                ordinal,
            )
            .0
        })
    })
}

/// Whether a Rust producer gap id as `ripr check` prints it names this seam.
/// That id is minted from the changed line, not from the seam, so it joins on
/// the seam's stable file, owner, kind and source expression. A shared id
/// still resolves to several seams; the caller decides that ambiguity.
pub(crate) fn producer_gap_id_names_seam(item: &str, entry: &ClassifiedSeam) -> bool {
    let file = crate::analysis::stable_path_text(entry.seam.file());
    producer_gap_names_seam_shape(
        item,
        &file,
        entry.seam.owner(),
        entry.seam.kind(),
        entry.seam.expression(),
    )
}

/// The readable producer gap id (`gap:rust:{file}:{owner}:...`) `ripr check`,
/// `ripr_list_gaps` and `ripr_get_gap` print for the finding that names this
/// seam, re-minted from the seam through the producer's own minter (#8638).
/// This is the id agents receive and pass back; the content-hash id from
/// [`canonical_gap_identity`] stays an internal join key.
pub(crate) fn readable_gap_id_for_seam(entry: &ClassifiedSeam) -> Option<String> {
    let file = crate::analysis::stable_path_text(entry.seam.file());
    // The readable id is the Rust producer's spelling; a non-Rust path mints
    // no `gap:rust:` id.
    if !file.ends_with(".rs") {
        return None;
    }
    super::language::rust_gap_id_for_seam_shape(
        &file,
        entry.seam.owner(),
        entry.seam.kind(),
        entry.seam.expression(),
    )
}

fn missing_discriminator_key(entry: &ClassifiedSeam) -> String {
    let mut missing = entry
        .evidence
        .missing_discriminators
        .iter()
        .map(|fact| fact.value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    missing.sort();
    missing.dedup();
    if missing.is_empty() {
        required_discriminator_text(entry.seam.required_discriminator())
    } else {
        missing.join(" | ")
    }
}

fn required_discriminator_text(discriminator: &RequiredDiscriminator) -> String {
    match discriminator {
        RequiredDiscriminator::BoundaryValue { description } => description.clone(),
        RequiredDiscriminator::ErrorVariant { variant } => variant.clone(),
        RequiredDiscriminator::ReturnValue { description } => description.clone(),
        RequiredDiscriminator::FieldValue { field } => field.clone(),
        RequiredDiscriminator::Effect { sink } => sink.clone(),
        RequiredDiscriminator::MatchArmTaken { arm } => arm.clone(),
        RequiredDiscriminator::CallSite { target } => target.clone(),
    }
}

fn assertion_shape_kind_for(kind: SeamKind) -> &'static str {
    match kind {
        SeamKind::PredicateBoundary => "exact_return_value",
        SeamKind::ErrorVariant => "exact_error_variant",
        SeamKind::ReturnValue => "exact_return_value",
        SeamKind::FieldConstruction => "field_equality",
        SeamKind::SideEffect => "side_effect_observer",
        SeamKind::MatchArm => "match_result",
        SeamKind::CallPresence => "call_expectation",
    }
}

fn compute_canonical_gap_id<'a>(parts: impl IntoIterator<Item = &'a str>) -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash: u64 = FNV_OFFSET;
    for part in parts {
        for byte in part.as_bytes().iter().chain([0].iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    format!("gap:{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::seams::{ExpectedSink, RepoSeam, SeamGripClass};
    use crate::analysis::test_grip_evidence::TestGripEvidence;
    use crate::domain::{
        Confidence, MissingDiscriminatorFact, StageEvidence, StageState, ValueFact,
    };

    fn stage(state: StageState) -> StageEvidence {
        StageEvidence::new(state, Confidence::Medium, "test stage")
    }

    fn classified_gap(owner: &str, line: usize, missing: &str) -> ClassifiedSeam {
        classified_gap_with_expression(owner, line, "amount >= threshold", missing)
    }

    fn classified_gap_with_expression(
        owner: &str,
        line: usize,
        expression: &str,
        missing: &str,
    ) -> ClassifiedSeam {
        let seam = RepoSeam::new(
            "src/pricing.rs",
            owner,
            SeamKind::PredicateBoundary,
            line * 10,
            line,
            expression,
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let evidence = TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: Vec::new(),
            reach: stage(StageState::Yes),
            activate: stage(StageState::Yes),
            propagate: stage(StageState::Yes),
            observe: stage(StageState::Yes),
            discriminate: stage(StageState::Weak),
            observed_values: Vec::<ValueFact>::new(),
            missing_discriminators: vec![MissingDiscriminatorFact {
                value: missing.to_string(),
                reason: "missing equality boundary".to_string(),
                flow_sink: None,
            }],
            new_test_target: None,
            statically_contradicted_related_tests: 0,
        };
        ClassifiedSeam {
            seam,
            evidence,
            class: SeamGripClass::WeaklyGripped,
        }
    }

    /// #8638: the readable `gap:rust:` id is the Rust producer's spelling, so
    /// a Rust seam mints it and a non-Rust path mints nothing.
    #[test]
    fn readable_gap_id_is_minted_for_rust_seams_only() {
        let rust = classified_gap("pricing::score", 3, "amount == threshold");
        assert!(
            readable_gap_id_for_seam(&rust)
                .as_deref()
                .is_some_and(|id| id.starts_with("gap:rust:src/pricing.rs:"))
        );
        let python_seam = RepoSeam::new(
            "src/pricing.py",
            "pricing::score",
            SeamKind::PredicateBoundary,
            30,
            3,
            "amount >= threshold",
            RequiredDiscriminator::BoundaryValue {
                description: "amount >= threshold".to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let python = ClassifiedSeam {
            seam: python_seam,
            evidence: rust.evidence.clone(),
            class: SeamGripClass::WeaklyGripped,
        };
        assert_eq!(readable_gap_id_for_seam(&python), None);
    }

    fn classified_match_arm_gap(owner: &str, line: usize, arm: &str) -> ClassifiedSeam {
        let seam = RepoSeam::new(
            "src/parser.rs",
            owner,
            SeamKind::MatchArm,
            line * 10,
            line,
            arm,
            RequiredDiscriminator::MatchArmTaken {
                arm: arm.to_string(),
            },
            ExpectedSink::ReturnValue,
        );
        let evidence = TestGripEvidence {
            seam_id: seam.id().clone(),
            related_tests: Vec::new(),
            reach: stage(StageState::Yes),
            activate: stage(StageState::Unknown),
            propagate: stage(StageState::Yes),
            observe: stage(StageState::Yes),
            discriminate: stage(StageState::Yes),
            observed_values: Vec::<ValueFact>::new(),
            missing_discriminators: Vec::new(),
            statically_contradicted_related_tests: 0,
            new_test_target: None,
        };
        ClassifiedSeam {
            seam,
            evidence,
            class: SeamGripClass::ActivationUnknown,
        }
    }

    #[test]
    fn canonical_gap_id_is_stable_across_line_movement() -> Result<(), String> {
        let before = classified_gap("pricing::discounted_total", 88, "amount == threshold");
        let after = classified_gap("pricing::discounted_total", 118, "amount == threshold");

        assert_ne!(before.seam.id(), after.seam.id());
        let before_gap =
            canonical_gap_identity(&before).ok_or_else(|| "missing before gap".to_string())?;
        let after_gap =
            canonical_gap_identity(&after).ok_or_else(|| "missing after gap".to_string())?;
        assert_eq!(before_gap.id, after_gap.id);
        assert_eq!(before_gap.owner, "pricing::discounted_total");
        assert_eq!(before_gap.flow_sink, "return_value");
        assert_eq!(before_gap.assertion_shape, "exact_return_value");
        Ok(())
    }

    #[test]
    fn canonical_gap_id_ignores_expression_locator_text() -> Result<(), String> {
        let before = classified_gap_with_expression(
            "pricing::discounted_total",
            88,
            "amount >= threshold",
            "amount == threshold",
        );
        let after = classified_gap_with_expression(
            "pricing::discounted_total",
            118,
            "(amount) >= threshold",
            "amount == threshold",
        );

        assert_ne!(before.seam.id(), after.seam.id());
        let before_gap =
            canonical_gap_identity(&before).ok_or_else(|| "missing before gap".to_string())?;
        let after_gap =
            canonical_gap_identity(&after).ok_or_else(|| "missing after gap".to_string())?;
        assert_eq!(before_gap.id, after_gap.id);
        Ok(())
    }

    #[test]
    fn canonical_gap_id_changes_when_missing_discriminator_changes() -> Result<(), String> {
        let equality = classified_gap("pricing::discounted_total", 88, "amount == threshold");
        let payload = classified_gap("pricing::discounted_total", 89, "exact discount payload");
        let equality_gap =
            canonical_gap_identity(&equality).ok_or_else(|| "missing equality gap".to_string())?;
        let payload_gap =
            canonical_gap_identity(&payload).ok_or_else(|| "missing payload gap".to_string())?;
        assert_ne!(equality_gap.id, payload_gap.id);
        Ok(())
    }

    #[test]
    fn canonical_gap_id_sorts_multiple_missing_discriminators() -> Result<(), String> {
        let mut first = classified_gap("pricing::discounted_total", 88, "amount == threshold");
        first.evidence.missing_discriminators = vec![
            MissingDiscriminatorFact {
                value: "exact discount payload".to_string(),
                reason: "missing payload".to_string(),
                flow_sink: None,
            },
            MissingDiscriminatorFact {
                value: "amount == threshold".to_string(),
                reason: "missing equality boundary".to_string(),
                flow_sink: None,
            },
        ];
        let mut second = classified_gap("pricing::discounted_total", 118, "amount == threshold");
        second.evidence.missing_discriminators = vec![
            MissingDiscriminatorFact {
                value: "amount == threshold".to_string(),
                reason: "missing equality boundary".to_string(),
                flow_sink: None,
            },
            MissingDiscriminatorFact {
                value: "exact discount payload".to_string(),
                reason: "missing payload".to_string(),
                flow_sink: None,
            },
        ];

        let first_gap =
            canonical_gap_identity(&first).ok_or_else(|| "missing first gap".to_string())?;
        let second_gap =
            canonical_gap_identity(&second).ok_or_else(|| "missing second gap".to_string())?;

        assert_eq!(first_gap.id, second_gap.id);
        assert_eq!(
            first_gap.missing_discriminator,
            "amount == threshold | exact discount payload"
        );
        Ok(())
    }

    #[test]
    fn canonical_gap_id_uses_required_discriminator_when_missing_fact_is_absent()
    -> Result<(), String> {
        let mut entry = classified_gap("pricing::discounted_total", 88, "amount == threshold");
        entry.evidence.missing_discriminators.clear();

        let gap = canonical_gap_identity(&entry).ok_or_else(|| "missing gap".to_string())?;

        assert_eq!(gap.missing_discriminator, "amount >= threshold");
        Ok(())
    }

    #[test]
    fn canonical_gap_id_distinguishes_same_function_name_in_different_modules() -> Result<(), String>
    {
        let pricing = classified_gap("pricing::discount", 88, "amount == threshold");
        let billing = classified_gap("billing::discount", 88, "amount == threshold");
        let pricing_gap =
            canonical_gap_identity(&pricing).ok_or_else(|| "missing pricing gap".to_string())?;
        let billing_gap =
            canonical_gap_identity(&billing).ok_or_else(|| "missing billing gap".to_string())?;
        assert_ne!(pricing_gap.id, billing_gap.id);
        Ok(())
    }

    #[test]
    fn canonical_gap_id_distinguishes_different_match_arm_discriminators() -> Result<(), String> {
        let zero = classified_match_arm_gap("parser::parse_format", 42, "OutputFormat::Json =>");
        let markdown = classified_match_arm_gap("parser::parse_format", 43, "OutputFormat::Md =>");

        let zero_gap =
            canonical_gap_identity(&zero).ok_or_else(|| "missing zero gap".to_string())?;
        let markdown_gap =
            canonical_gap_identity(&markdown).ok_or_else(|| "missing markdown gap".to_string())?;

        assert_ne!(zero_gap.id, markdown_gap.id);
        assert_eq!(zero_gap.missing_discriminator, "OutputFormat::Json =>");
        assert_eq!(markdown_gap.missing_discriminator, "OutputFormat::Md =>");
        assert_eq!(zero_gap.assertion_shape, "match_result");
        Ok(())
    }

    #[test]
    fn canonical_gap_id_groups_same_match_arm_across_line_movement() -> Result<(), String> {
        let before = classified_match_arm_gap("parser::parse_format", 42, "OutputFormat::Json =>");
        let after = classified_match_arm_gap("parser::parse_format", 88, "OutputFormat::Json =>");

        assert_ne!(before.seam.id(), after.seam.id());
        let before_gap =
            canonical_gap_identity(&before).ok_or_else(|| "missing before gap".to_string())?;
        let after_gap =
            canonical_gap_identity(&after).ok_or_else(|| "missing after gap".to_string())?;

        assert_eq!(before_gap.id, after_gap.id);
        assert_eq!(before_gap.seam_kind, "match_arm");
        assert_eq!(before_gap.flow_sink, "return_value");
        Ok(())
    }

    #[test]
    fn canonical_gap_identities_report_group_size_for_equivalent_gaps() -> Result<(), String> {
        let first = classified_gap("pricing::discounted_total", 88, "amount == threshold");
        let second = classified_gap("pricing::discounted_total", 118, "amount == threshold");
        let identities = canonical_gap_identities(&[first.clone(), second.clone()]);
        let first_gap = identities
            .get(first.seam.id())
            .ok_or_else(|| "missing first gap".to_string())?;
        let second_gap = identities
            .get(second.seam.id())
            .ok_or_else(|| "missing second gap".to_string())?;
        assert_eq!(first_gap.id, second_gap.id);
        assert_eq!(first_gap.group_size, 2);
        assert_eq!(second_gap.group_size, 2);
        assert_eq!(first_gap.reason, CANONICAL_GAP_REASON);
        Ok(())
    }

    /// #8166 residual: the `probe:` ID `ripr check` prints re-mints from the
    /// seam it names (file, family-from-kind, owner, seam expression), and
    /// from no other seam. Two seams sharing those fields share the ID; the
    /// resolver caller owns that ambiguity.
    #[test]
    fn probe_id_names_only_the_seam_it_was_minted_from() {
        let entry = classified_gap("src/pricing.rs::pricing::discounted_total", 88, "x");
        // Mint the ID exactly as the diff probe producer does: the
        // content-addressed fingerprint over sanitized path, family, owner
        // and normalized expression.
        let id = crate::analysis::fingerprint_probe_id(
            "probe",
            &crate::analysis::sanitize_path(entry.seam.file()),
            "predicate",
            entry.seam.owner(),
            &crate::analysis::normalize_rust_expression(entry.seam.expression()),
            1,
        )
        .0;
        assert!(
            probe_id_names_seam(&id, &entry, std::slice::from_ref(&entry)),
            "own probe ID must name its seam: {id}"
        );
        // A fingerprint over a different expression names no seam here.
        let other = classified_gap_with_expression(
            "src/pricing.rs::pricing::discounted_total",
            88,
            "amount < threshold",
            "x",
        );
        assert!(!probe_id_names_seam(
            &id,
            &other,
            std::slice::from_ref(&other)
        ));
        // A stale fingerprint (no seam carries it) names nothing.
        assert!(!probe_id_names_seam(
            "probe:src_pricing.rs:predicate:00000000",
            &entry,
            std::slice::from_ref(&entry)
        ));
        // The duplicate-expression twin shares the re-minted ID: the matcher
        // credits both, so the resolver must refuse as ambiguous.
        let twin = classified_gap("src/pricing.rs::pricing::discounted_total", 99, "x");
        let seams = [entry.clone(), twin.clone()];
        assert!(probe_id_names_seam(&id, &twin, &seams));
    }

    #[test]
    fn line_comment_terminators_do_not_reverse_to_the_other_value() {
        let commented = classified_gap_with_expression(
            "src/pricing.rs::equal",
            88,
            "x == concat!(\"a\", // \"b\",\n        \"c\")",
            "concat value",
        );
        let active = classified_gap_with_expression(
            "src/pricing.rs::equal",
            88,
            "x == concat!(\"a\", //\n        \"b\", \"c\")",
            "concat value",
        );
        // Fixed expected payload hashes use this fixture's path and owner;
        // the old flat payload was shared by both source expressions.
        for prefix in ["probe", "repo-probe"] {
            let old = format!("{prefix}:src_pricing.rs:predicate:e4b79a1b");
            let commented_id = format!("{prefix}:src_pricing.rs:predicate:60ee2017");
            let active_id = format!("{prefix}:src_pricing.rs:predicate:1862f384");
            for (entry, own, other) in [
                (&commented, &commented_id, &active_id),
                (&active, &active_id, &commented_id),
            ] {
                let sole = std::slice::from_ref(entry);
                assert!(
                    !probe_id_names_seam(&old, entry, sole),
                    "ambiguous old ID must be stale"
                );
                assert!(probe_id_names_seam(own, entry, sole));
                assert!(!probe_id_names_seam(other, entry, sole));
            }
        }
    }

    /// #8166 review: a sole sharing seam plus an explicit ordinal names
    /// nothing for either family. An invented `.999` is stale, and a real
    /// `.2` whose second occurrence was seam-filtered (so only the first
    /// seam remains) must not resolve to that first seam.
    #[test]
    fn probe_id_ordinal_must_be_assignable_to_the_seam_set() {
        let entry = classified_gap("src/pricing.rs::pricing::discounted_total", 88, "x");
        let mint = |prefix: &str, ordinal: u32| {
            crate::analysis::fingerprint_probe_id(
                prefix,
                &crate::analysis::sanitize_path(entry.seam.file()),
                "predicate",
                entry.seam.owner(),
                &crate::analysis::normalize_rust_expression(entry.seam.expression()),
                ordinal,
            )
            .0
        };
        let sole = std::slice::from_ref(&entry);
        for prefix in ["probe", "repo-probe"] {
            assert!(probe_id_names_seam(&mint(prefix, 1), &entry, sole));
            assert!(
                !probe_id_names_seam(&mint(prefix, 999), &entry, sole),
                "an invented .999 ordinal must be refused as stale ({prefix})"
            );
            assert!(
                !probe_id_names_seam(&mint(prefix, 2), &entry, sole),
                "a .2 whose second occurrence is seam-filtered must not name the sole remaining seam ({prefix})"
            );
        }
        // Without an ordinal the bare ID still names both seams of a pair,
        // so the resolver refuses it as ambiguous.
        let twin = classified_gap("src/pricing.rs::pricing::discounted_total", 99, "x");
        let pair = [entry.clone(), twin.clone()];
        for prefix in ["probe", "repo-probe"] {
            assert!(probe_id_names_seam(&mint(prefix, 1), &entry, &pair));
            assert!(probe_id_names_seam(&mint(prefix, 1), &twin, &pair));
        }
    }

    /// #8166 round 3: the two ID families handle explicit ordinals
    /// differently. A `repo-probe:` `.2` rank-binds to exactly the second
    /// seam (its mint set is the seam set) and `.3` exceeds the set. A
    /// diff `probe:` `.2` instead names every sharing seam, so the
    /// resolver refuses it as ambiguous rather than rank-binding it to a
    /// possibly unchanged seam.
    #[test]
    fn probe_id_second_ordinal_resolves_to_the_second_seam() {
        let first = classified_gap("src/pricing.rs::pricing::discounted_total", 88, "x");
        let second = classified_gap("src/pricing.rs::pricing::discounted_total", 99, "x");
        let pair = [first.clone(), second.clone()];
        let mint = |prefix: &str, ordinal: u32| {
            crate::analysis::fingerprint_probe_id(
                prefix,
                &crate::analysis::sanitize_path(first.seam.file()),
                "predicate",
                first.seam.owner(),
                &crate::analysis::normalize_rust_expression(first.seam.expression()),
                ordinal,
            )
            .0
        };
        let repo_resolved: Vec<&ClassifiedSeam> = pair
            .iter()
            .filter(|entry| probe_id_names_seam(&mint("repo-probe", 2), entry, &pair))
            .collect();
        assert_eq!(
            repo_resolved.len(),
            1,
            "repo-probe .2 must resolve to exactly one seam"
        );
        assert_eq!(repo_resolved[0].seam.id(), second.seam.id());
        assert!(!probe_id_names_seam(&mint("repo-probe", 3), &first, &pair));
        assert!(!probe_id_names_seam(&mint("repo-probe", 3), &second, &pair));
        // Diff `probe:`: `.2` (and any explicit ordinal) names the whole
        // shared-base set, never just the inventory-rank-2 seam.
        for ordinal in [2, 3, 999] {
            let diff_resolved: Vec<&ClassifiedSeam> = pair
                .iter()
                .filter(|entry| probe_id_names_seam(&mint("probe", ordinal), entry, &pair))
                .collect();
            assert_eq!(
                diff_resolved.len(),
                2,
                "diff probe .{ordinal} must name both sharing seams (ambiguous), not rank-bind"
            );
        }
    }

    /// #8166 round 3 (reviewer's concrete case): three identical
    /// predicates, a diff touching only occurrences 1 and 3, so the
    /// second changed probe is `.2`. That `.2` must not bind occurrence
    /// 2's (unchanged) seam: the diff matcher names all three sharing
    /// seams so the caller refuses as ambiguous, while `repo-probe:`
    /// `.2` still rank-binds to exactly occurrence 2.
    #[test]
    fn diff_probe_second_ordinal_does_not_bind_the_middle_of_three_seams() {
        let seams = [
            classified_gap("src/pricing.rs::pricing::discounted_total", 2, "x"),
            classified_gap("src/pricing.rs::pricing::discounted_total", 3, "x"),
            classified_gap("src/pricing.rs::pricing::discounted_total", 4, "x"),
        ];
        let mint = |prefix: &str, ordinal: u32| {
            crate::analysis::fingerprint_probe_id(
                prefix,
                &crate::analysis::sanitize_path(seams[0].seam.file()),
                "predicate",
                seams[0].seam.owner(),
                &crate::analysis::normalize_rust_expression(seams[0].seam.expression()),
                ordinal,
            )
            .0
        };
        let diff_resolved: Vec<&ClassifiedSeam> = seams
            .iter()
            .filter(|entry| probe_id_names_seam(&mint("probe", 2), entry, &seams))
            .collect();
        assert_eq!(
            diff_resolved.len(),
            3,
            "diff probe .2 must name all sharing seams (ambiguous), not bind occurrence 2"
        );
        let repo_resolved: Vec<&ClassifiedSeam> = seams
            .iter()
            .filter(|entry| probe_id_names_seam(&mint("repo-probe", 2), entry, &seams))
            .collect();
        assert_eq!(repo_resolved.len(), 1);
        assert_eq!(repo_resolved[0].seam.id(), seams[1].seam.id());
    }

    #[test]
    fn strongly_gripped_seams_do_not_get_canonical_gap_identities() {
        let mut entry = classified_gap("pricing::discounted_total", 88, "amount == threshold");
        entry.class = SeamGripClass::StronglyGripped;
        assert!(canonical_gap_identity(&entry).is_none());
        assert!(canonical_gap_identities(&[entry]).is_empty());
    }
}
