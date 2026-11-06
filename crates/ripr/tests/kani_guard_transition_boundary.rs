//! Staged #12512 correspondence inputs, not a Kani transition implementation.
//! The comparator tests reach the existing gap owner. The source/capture
//! oracles do not reach an admitted guard producer, CLI, MCP, cache, or Kani.

use ripr::domain::{
    AttributionBasis, CanonicalDelta, CanonicalEvidenceState, ComparisonConfidence,
    DeltaAttribution, GapState, OracleStrength, compare_fixture_delta,
};
use sha2::{Digest, Sha256};

#[path = "fixtures/kani_transition_boundary/assertion_removed.rs"]
mod removed;
#[path = "fixtures/kani_transition_boundary/cochange.rs"]
mod cochange;
#[path = "fixtures/kani_guard_transitions/strong.rs"]
mod strong;

const STRONG: &str = include_str!("fixtures/kani_guard_transitions/strong.rs");
const REMOVED: &str = include_str!("fixtures/kani_transition_boundary/assertion_removed.rs");
const DELETED: &str = include_str!("fixtures/kani_transition_boundary/proof_deleted.rs");
const OWNER_DELETED: &str = include_str!("fixtures/kani_transition_boundary/owner_deleted.rs");
const OPAQUE: &str = include_str!("fixtures/kani_transition_boundary/opaque_replacement.rs");
const MOVED: &str = include_str!("fixtures/kani_transition_boundary/relocated.rs");
const WRONG: &str = include_str!("fixtures/kani_transition_boundary/wrong_owner.rs");
const COCHANGE: &str = include_str!("fixtures/kani_transition_boundary/cochange.rs");
const DISABLED: &str = include_str!("fixtures/kani_transition_boundary/disabled.rs");
const UNKNOWN_CFG: &str = include_str!("fixtures/kani_transition_boundary/unknown_cfg.rs");
const BODY_PROVIDER: &str = include_str!("fixtures/kani_transition_boundary/body_provider.rs");
const NARROW: &str = include_str!("fixtures/kani_guard_transitions/narrow_correct.rs");
const FALSE_DOMAIN: &str = include_str!("fixtures/kani_guard_transitions/false_domain.rs");
const ASSERTION: &str = "        assert_eq!(super::fits(n, max), n < max);";
const OWNER: &str = "src/lib.rs::fits";
const SUBJECT: &str = "fits:strict-u8-capacity";

fn production(source: &str) -> Result<&str, String> {
    source
        .split_once("#[cfg(")
        .map(|(implementation, _)| implementation)
        .ok_or_else(|| "expected a gated proof after the production owner".to_string())
}

fn state(gap: GapState) -> CanonicalEvidenceState {
    // This is an authored input to the existing comparator, not an inferred
    // proof relationship or a synthesized ordinary test registration.
    CanonicalEvidenceState::new(
        OWNER,
        SUBJECT,
        "return:bool",
        gap,
        OracleStrength::Unknown,
    )
}

/// Oracle for the first conservative adapter capture. This does not implement
/// a transition: it only rejects a result that drops or overcredits its subject.
/// The eventual production-path test must call this on the actual producer's
/// delta, with independently captured source identities and limitations too.
fn require_retained_unknown(delta: &CanonicalDelta) -> Result<(), String> {
    if delta.canonical_gap_id != SUBJECT
        || delta.delta_attribution != DeltaAttribution::ComparisonUnknown
        || delta.comparison_confidence != ComparisonConfidence::Unknown
    {
        return Err("guard subject was lost, resolved, or given unsupported credit".to_string());
    }
    for side in [&delta.base_state, &delta.head_state] {
        let Some(side) = side else {
            return Err("affected behavior must remain present on both sides".to_string());
        };
        if side.canonical_owner != OWNER || side.behavior_identity != SUBJECT {
            return Err("wrong affected owner or behavior".to_string());
        }
    }
    if delta.attribution_basis.is_empty() {
        return Err("missing comparison provenance".to_string());
    }
    Ok(())
}

#[test]
fn two_sided_guard_changes_keep_exact_production_bytes() -> Result<(), String> {
    let expected = "pub fn fits(n: u8, max: u8) -> bool {\n    n < max\n}\n\n";
    assert_eq!(production(STRONG)?, expected);
    let heads = [
        REMOVED, OPAQUE, MOVED, WRONG, DISABLED, UNKNOWN_CFG, BODY_PROVIDER, NARROW, FALSE_DOMAIN,
    ];
    assert_eq!(heads.len(), 9);
    for head in heads {
        assert_eq!(production(head)?, expected);
        assert_ne!(head, STRONG);
    }
    assert_eq!(DELETED, expected);
    assert_eq!(
        REMOVED,
        STRONG.replace(ASSERTION, "        let _ = super::fits(n, max);")
    );
    assert_eq!(
        OPAQUE,
        STRONG.replace(ASSERTION, "        opaque_guard!(super::fits(n, max));")
    );
    assert!(!REMOVED.contains("assert"));
    assert!(!DELETED.contains("kani"));
    Ok(())
}

#[test]
fn relocation_wrong_owner_and_domain_controls_are_distinct() {
    assert_eq!(
        MOVED,
        STRONG.replace("mod proofs {", "mod relocated_proofs {")
    );
    assert!(WRONG.contains("let _ = super::fits(n, max);"));
    assert!(WRONG.contains("assert_eq!(super::decoy::fits(n, max), n < max);"));
    assert!(!WRONG.contains(ASSERTION));
    assert_eq!(
        NARROW,
        STRONG.replace(ASSERTION, &format!("        kani::assume(n < max);\n{ASSERTION}"))
    );
    assert_eq!(
        FALSE_DOMAIN,
        STRONG.replace(ASSERTION, &format!("        kani::assume(false);\n{ASSERTION}"))
    );
    // Narrowed correct code remains a legitimate partial-spec sibling.
    // Nonempty inputs do not establish coverage of the excluded equality edge.
    assert!(NARROW.contains(ASSERTION));
    assert!(FALSE_DOMAIN.contains(ASSERTION));
    assert_eq!(
        DISABLED,
        STRONG.replace("#[cfg(kani)]", "#[cfg(all(kani, any()))]")
    );
    assert_eq!(
        UNKNOWN_CFG,
        STRONG.replace("#[cfg(kani)]", "#[cfg(all(kani, feature = \"unresolved\"))]")
    );
    assert_eq!(
        BODY_PROVIDER,
        STRONG.replace("        let n:", "        mod kani {}\n        let n:")
    );
    assert_eq!(OWNER_DELETED, &STRONG[DELETED.len()..]);
    assert!(OWNER_DELETED.contains(ASSERTION));
    assert!(!OWNER_DELETED.contains("pub fn fits"));
}

#[test]
fn ordinary_execution_separates_assertion_removal_from_code_cochange() {
    let mut visited = 0_usize;
    let mut cochange_faults = 0_usize;
    for n in u8::MIN..=u8::MAX {
        for max in u8::MIN..=u8::MAX {
            visited += 1;
            let required = n.cmp(&max).is_lt();
            assert_eq!(strong::fits(n, max), required);
            assert_eq!(removed::fits(n, max), required);
            let differs = cochange::fits(n, max) != required;
            assert_eq!(differs, n == max);
            cochange_faults += usize::from(differs);
        }
    }
    assert_eq!(visited, 65_536);
    assert_eq!(cochange_faults, 256);
    assert_eq!(
        COCHANGE,
        STRONG
            .replace("    n < max\n", "    n <= max\n")
            .replace("super::fits(n, max), n < max", "super::fits(n, max), n <= max")
    );
}

#[test]
fn ordinary_repaired_gap_keeps_existing_resolution_semantics() {
    let delta = compare_fixture_delta(
        SUBJECT,
        true,
        true,
        Some(state(GapState::Actionable)),
        None,
    );
    assert_eq!(delta.delta_attribution, DeltaAttribution::ResolvedByChange);
    assert_eq!(
        delta.attribution_basis,
        vec![AttributionBasis::HeadItemAbsent]
    );
    assert_eq!(delta.comparison_confidence, ComparisonConfidence::FixtureBacked);
    // Valid for the existing gap contract; explicitly wrong for a removed
    // protective guard. Do not reverse the comparator's global absence rule.
    assert!(require_retained_unknown(&delta).is_err());
}

#[test]
fn retained_unsupported_guard_uses_existing_unknown_comparator() -> Result<(), String> {
    let delta = compare_fixture_delta(
        SUBJECT,
        true,
        true,
        Some(state(GapState::Unknown)),
        Some(state(GapState::Unknown)),
    );
    require_retained_unknown(&delta)?;
    assert_eq!(delta.base_state, Some(state(GapState::Unknown)));
    assert_eq!(delta.head_state, Some(state(GapState::Unknown)));
    Ok(())
}

#[test]
fn capture_oracle_rejects_empty_wrong_owner_and_overcrediting_results() -> Result<(), String> {
    let valid = compare_fixture_delta(
        SUBJECT,
        true,
        true,
        Some(state(GapState::Unknown)),
        Some(state(GapState::Unknown)),
    );
    require_retained_unknown(&valid)?;
    let mut bad = valid.clone();
    bad.head_state = None;
    assert!(require_retained_unknown(&bad).is_err());
    bad = valid.clone();
    bad.head_state = Some(CanonicalEvidenceState::new(
        "src/lib.rs::decoy::fits",
        SUBJECT,
        "return:bool",
        GapState::Unknown,
        OracleStrength::Unknown,
    ));
    assert!(require_retained_unknown(&bad).is_err());
    for attribution in [
        DeltaAttribution::ResolvedByChange,
        DeltaAttribution::WeakenedByChange,
    ] {
        bad = valid.clone();
        bad.delta_attribution = attribution;
        assert!(require_retained_unknown(&bad).is_err());
    }
    bad = valid.clone();
    bad.comparison_confidence = ComparisonConfidence::FixtureBacked;
    assert!(require_retained_unknown(&bad).is_err());
    bad = valid;
    bad.attribution_basis.clear();
    assert!(require_retained_unknown(&bad).is_err());
    Ok(())
}

#[test]
fn same_head_dependency_inputs_are_distinct_and_restorable() {
    // Source input oracle only. No cache is consulted and no receipt is
    // authenticated by these digests. A production cache test is still due.
    let base = [STRONG, "helper:v1", "kani=true", "worktree:clean"];
    let changed = [
        [REMOVED, base[1], base[2], base[3]],
        [COCHANGE, base[1], base[2], base[3]],
        [base[0], "helper:v2", base[2], base[3]],
        [base[0], base[1], "kani=false", base[3]],
        [base[0], base[1], base[2], "worktree:dirty"],
    ];
    let digest = |inputs: [&str; 4]| {
        let mut hash = Sha256::new();
        for input in inputs {
            hash.update(input.len().to_le_bytes());
            hash.update(input.as_bytes());
        }
        hash.finalize()
    };
    for inputs in changed {
        assert_ne!(digest(base), digest(inputs));
    }
    let restored = REMOVED.replace("        let _ = super::fits(n, max);", ASSERTION);
    assert_eq!(restored, STRONG);
    assert_eq!(digest([&restored, base[1], base[2], base[3]]), digest(base));
}
