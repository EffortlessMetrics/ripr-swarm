//! A non-substituting related test beside mock-patched siblings (#13633).
//!
//! `mocked_module` withholds credit from any oracle-eligible test that uses
//! patch syntax. A test with no substitution at all runs the real code, so it
//! is read independently of mock-patched siblings: [`drop_substituting_siblings`]
//! sets the siblings aside, and the caller keeps the original limit unless the
//! independent tests alone earn `exposed`.

use super::literal_patch::literal_patch_calls;
use super::related_tests::PythonRelatedCandidate;
use super::static_limits::test_has_mocked_module;
use super::{PythonOwner, PythonTest};

/// Set aside the oracle-eligible related tests that substitute behavior when
/// an oracle-eligible test with no substitution at all also reaches the owner.
/// Returns whether anything was dropped. A sibling that leaves a patch running
/// (`.start(`) is never set aside, since the patch may outlive its test.
pub(super) fn drop_substituting_siblings(
    candidates: &mut Vec<PythonRelatedCandidate<'_>>,
    owner: &PythonOwner,
    owner_source: &str,
    old_line: Option<&str>,
    new_line: &str,
) -> bool {
    let substitutes = |test: &PythonTest| {
        test_has_mocked_module(test) || test.constant_rebinding.fixture_substitution
    };
    let independent = candidates
        .iter()
        .any(|candidate| candidate.relation.uses_oracle() && !substitutes(candidate.test));
    if !independent {
        return false;
    }
    let droppable = |candidate: &PythonRelatedCandidate<'_>| {
        candidate.relation.uses_oracle()
            && substitutes(candidate.test)
            && literal_patch_calls(candidate.test, owner, owner_source, old_line, new_line)
                .is_none()
    };
    if candidates
        .iter()
        .any(|candidate| droppable(candidate) && candidate.test.body_text.contains(".start("))
    {
        return false;
    }
    let before = candidates.len();
    candidates.retain(|candidate| !droppable(candidate));
    candidates.len() != before
}
