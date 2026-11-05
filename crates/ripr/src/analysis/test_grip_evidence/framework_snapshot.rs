//! Carry the fresh Finding's field-snapshot uncertainty into repository
//! evidence (#13278). The framework reader remains the semantic authority;
//! this adapter only names its existing limit for a candidate field change.
//!
//! Class and stage states stay intact. The full and compact producers pass
//! every directly related test before their display caps, so a late witness
//! cannot disappear from recommendation or repair admission.

use super::TestGripEvidence;
use crate::analysis::classify::{insta_field_snapshot_limitation, is_ignored};
use crate::analysis::rust_index::{FunctionSummary, TestSummary};
use crate::analysis::seam_classification::classify_seam;
use crate::analysis::seams::{RepoSeam, SeamGripClass, SeamKind};
use crate::domain::StaticLimitKind;

const LIMIT: StaticLimitKind = StaticLimitKind::RustFrameworkOracleUnresolved;

/// Name only field-snapshot uncertainty recognized by the shared reader.
/// An exact discriminator, unrelated snapshot or literal redaction naming
/// the changed field does not acquire a limitation through this adapter.
pub(super) fn withhold<'a>(
    seam: &RepoSeam,
    evidence: &mut TestGripEvidence,
    owner: Option<&FunctionSummary>,
    direct_related: impl IntoIterator<Item = &'a TestSummary>,
) -> bool {
    // Panic uncertainty is established first by both producers. Preserve
    // its reserved prefix and reason instead of wrapping it out of view.
    if seam.kind() != SeamKind::FieldConstruction
        || evidence.names_static_limit(LIMIT)
        || evidence.names_static_limit(StaticLimitKind::RustPanicOutcomeUnresolved)
        || !matches!(
            classify_seam(seam, evidence),
            SeamGripClass::WeaklyGripped | SeamGripClass::ReachableUnrevealed
        )
    {
        return false;
    }
    let changed_line = seam.display_line();
    let Some(owner) =
        owner.filter(|owner| owner.start_line <= changed_line && changed_line <= owner.end_line)
    else {
        return false;
    };
    let Some(changed_text) = owner.body.lines().nth(changed_line - owner.start_line) else {
        return false;
    };
    // One witness establishes uncertainty. Search the full set, but retain
    // only that witness so compact evidence cannot grow with every test.
    let Some(judged) = direct_related
        .into_iter()
        .filter(|test| !is_ignored(test))
        .find_map(|test| {
            let advice = insta_field_snapshot_limitation(test, changed_text, &owner.name)?;
            Some(format!(
                "`{}` ({}:{}) {advice}",
                test.name,
                test.file.display().to_string().replace('\\', "/"),
                test.start_line
            ))
        })
    else {
        return false;
    };
    evidence.discriminate.summary = format!(
        "{}: {judged}; Original discrimination: {}",
        LIMIT.as_str(),
        evidence.discriminate.summary
    );
    true
}
