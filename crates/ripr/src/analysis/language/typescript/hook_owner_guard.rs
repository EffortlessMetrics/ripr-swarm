//! A mocha setup hook that calls the changed owner (RIPR-SPEC-0246 rule 7,
//! #9496).
//!
//! `before(() => { register("  alpha "); })` runs the changed owner outside
//! every test body, so a test that asserts on the state the hook built never
//! names the owner and is not a related candidate. The related tests that are
//! candidates read an unchanged value, and the guard downgrades the finding to
//! `weakly_exposed`: an actionable gap that may be false. The rule only
//! withdraws that actionable signal. It credits nothing, adds no relation and
//! reads each hook through the same owner-identity gates a test body gets
//! (`owner_call_relation`), so a wrong tie only moves toward abstention.

use super::related_tests::{ReExportIndex, owner_call_relation};
use super::*;

/// `true` when a synchronous mocha setup hook in the describe chain of an
/// oracle-eligible related candidate calls the changed owner. The hook body
/// is masked (comments and string contents blanked), then read as the body of
/// a test in the same file, scope and imports, so shadowing, module mocks,
/// spy fabrication and the declaration anchor apply unchanged. Only a plain
/// call of the owner counts: a method receiver, a module-entry reach or a
/// same-file proximity link is not read here.
pub(crate) fn setup_hook_calls_owner(
    candidates: &[TypeScriptRelatedCandidate<'_>],
    owner: &TypeScriptOwner,
    reexport_index: &ReExportIndex,
    alias_map: Option<&TsAliasMap>,
    workspace_root: Option<&Path>,
) -> bool {
    candidates
        .iter()
        .filter(|candidate| candidate.relation.uses_oracle())
        .any(|candidate| {
            candidate.test.setup_hook_bodies.iter().any(|hook| {
                let mut view = candidate.test.clone();
                view.body_text = ts_mask_literals_and_comments(hook);
                view.reach_body_text = None;
                view.assertions = Vec::new();
                matches!(
                    owner_call_relation(&view, owner, reexport_index, alias_map, workspace_root),
                    Some(
                        TypeScriptRelationKind::DirectOwnerCall
                            | TypeScriptRelationKind::ImportAliasOwnerCall
                            | TypeScriptRelationKind::ImportedOwnerCall
                            | TypeScriptRelationKind::ReExportChainFollowed
                    )
                )
            })
        })
}
