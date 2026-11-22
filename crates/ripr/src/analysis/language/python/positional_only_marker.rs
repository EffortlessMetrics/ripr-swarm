//! A `def` header edit that only moves the positional-only `/` marker (#16011).
//!
//! `def key(self, *args)` to `def key(self, /, *args)` changes behavior only
//! for a call that passes `self` by keyword: the old header binds it, the new
//! one refuses it (or routes it into `**kwargs`). A related call that passes
//! every argument by position runs the same way on both headers, so a strong
//! oracle on such a call does not discriminate the edit.

use super::PythonOwner;
use super::assertion_selection::{PythonAssertionFocus, select_relevant_assertion};
use super::no_behavior::{analyze_call_args, call_arglists_with_offsets, def_signature_skeleton};
use super::related_tests::{PythonRelatedCandidate, owner_module_callees};
use crate::domain::OracleStrength;

/// The parameters whose positional-only status differs between two one-line
/// `def` headers, when that is the only runtime difference: same name,
/// async-ness, parameter names, defaults, keyword-only group and
/// `*args`/`**kwargs`. None for anything else.
pub(super) fn positional_only_marker_params(old_line: &str, new_line: &str) -> Option<Vec<String>> {
    let (old_async, old_name, old_pos, old_kw, old_params, old_va, old_kwa) =
        def_signature_skeleton(old_line)?;
    let (new_async, new_name, new_pos, new_kw, new_params, new_va, new_kwa) =
        def_signature_skeleton(new_line)?;
    if old_pos == new_pos
        || old_async != new_async
        || old_name != new_name
        || old_kw != new_kw
        || old_params != new_params
        || old_va != new_va
        || old_kwa != new_kwa
    {
        return None;
    }
    let (low, high) = (old_pos.min(new_pos), old_pos.max(new_pos));
    let moved: Vec<String> = new_params
        .get(low..high)?
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    (!moved.is_empty()).then_some(moved)
}

/// For a marker-only header edit, the moved parameters when no strong related
/// test calls the owner with one of them by keyword. Such a test is the only
/// kind that runs differently on the two headers. A call whose arguments this
/// reader cannot account for (`**options`) is not counted as binding by
/// keyword, so the edit stays unconfirmed: the result is a weaker class, never
/// a false `exposed`.
pub(super) fn positional_only_marker_gap(
    old_line_text: Option<&str>,
    new_line_text: &str,
    owner: &PythonOwner,
    related_candidates: &[PythonRelatedCandidate<'_>],
    focus: &PythonAssertionFocus,
) -> Option<Vec<String>> {
    let moved = positional_only_marker_params(old_line_text?, new_line_text)?;
    let binds_by_keyword = related_candidates.iter().any(|candidate| {
        candidate.relation.uses_oracle()
            && select_relevant_assertion(&candidate.test.assertions, Some(focus))
                .assertion()
                .is_some_and(|assertion| {
                    assertion.oracle_strength.rank() >= OracleStrength::Strong.rank()
                })
            && owner_call_spellings(candidate, owner)
                .iter()
                .any(|(callee, method_call)| {
                    call_arglists_with_offsets(&candidate.test.body_text, callee, *method_call)
                        .into_iter()
                        .filter_map(|(_, arglist)| analyze_call_args(arglist))
                        .any(|shape| shape.keywords.iter().any(|name| moved.contains(name)))
                })
    });
    (!binds_by_keyword).then_some(moved)
}

/// How a related test may spell a call of the owner: its own name as a free
/// or attribute call, and every import alias or module receiver the related
/// test matcher resolves to it (`from m import key as lookup`, `m.key`).
fn owner_call_spellings(
    candidate: &PythonRelatedCandidate<'_>,
    owner: &PythonOwner,
) -> Vec<(String, bool)> {
    let mut spellings = vec![(owner.name.clone(), false), (owner.name.clone(), true)];
    spellings.extend(
        owner_module_callees(candidate.test, owner)
            .into_iter()
            .map(|callee| (callee, false)),
    );
    spellings
}
