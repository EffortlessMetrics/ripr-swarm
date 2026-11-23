//! RIPR-SPEC-0240 / #14935: an owner compiled only under a cfg that also
//! gates a doc example ripr could not read is not a gap.
//!
//! The doctest admission (RIPR-SPEC-0279) reads a doc example only when it can
//! prove rustdoc compiles it in a plain `cargo test`. An example behind a
//! custom or target cfg, or any `#[cfg_attr(..)]` (anyhow's nightly-only
//! examples wrap themselves in
//! `#[cfg_attr(not(error_generic_member_access), doc = "# _ = stringify! {")]`),
//! is refused and never becomes a test fact. When the changed function itself
//! carries a gate on the same cfg atom (`#[cfg(error_generic_member_access)]`),
//! the configuration that compiles the owner is exactly the one that runs
//! those unread examples. A weak reading (no indexed test is seen calling the
//! owner; related tests only share its file or a name token) then rests on
//! tests ripr skipped, not on a missing test, so the gap is withheld under
//! `rust_gated_doctest_unread`.
//!
//! The rule needs both links, read from syntax:
//!
//! - the owner's own or enclosing `#[cfg(..)]` gates, as
//!   `unestablished_cfg_gates_of_fn` reads them (gates provably on in a test
//!   build are not among them), require a custom cfg name on: every
//!   combinator above it is a conjunction once negations are pushed inward
//!   and an even number of `not(..)` encloses it, and it is not a
//!   `key = "value"` pair or a built-in name such as `unix`
//!   (`facts::doctests::gated`); and
//! - a runnable fenced doc example in the same doctested library is gated by
//!   that name (`RustIndex::refused_doctest_gate_atoms`). The match is
//!   library-wide, not per item, which is why only custom names count:
//!   anyhow's examples sit on
//!   `into_boxed_dyn_error` in `error.rs` and exercise `provide` in
//!   `context.rs` through std's provider dispatch, which no call edge shows.
//!
//! A finding a test is seen calling (reach `yes`), a `weakly_exposed` finding
//! (a credited weak oracle is read evidence, RIPR-SPEC-0240's scope
//! boundary), an owner whose gates do not read, and an owner outside a
//! doctested library keep today's reading. Like
//! every RIPR-SPEC-0240 rule this only withholds: nothing is credited and the
//! actionability flip stays fail-closed.

use super::gap_admission::withhold_gap;
use crate::analysis::facts::owner_gate_atoms;
use crate::analysis::rust_index::RustIndex;
use crate::domain::*;
use std::collections::BTreeSet;

const LIMIT: StaticLimitKind = StaticLimitKind::RustGatedDoctestUnread;
const FIRST_UNRESOLVED_EDGE: &str = "whether a doc example ripr did not read, gated by the owner's own cfg, checks the changed line";

/// Withhold a `reachable_unrevealed` gap whose owner is
/// compiled only under a cfg that also gates an unread doc example. Returns
/// `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_gated_doctest_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || finding.class != ExposureClass::ReachableUnrevealed
        || finding.ripr.reach.state != StageState::Weak
    {
        return false;
    }
    let Some(owner_id) = finding.probe.owner.as_ref() else {
        return false;
    };
    let changed_line = finding.probe.location.line;
    let Some(owner) = index.functions().iter().find(|function| {
        function.id == *owner_id
            && function.start_line <= changed_line
            && changed_line <= function.end_line
    }) else {
        return false;
    };
    let Some(refused) = index.refused_doctest_gate_atoms(&owner.file) else {
        return false;
    };
    let Some(facts) = index.files().get(&owner.file) else {
        return false;
    };
    let Some(gates) = crate::analysis::syntax::unestablished_cfg_gates_of_fn(
        &facts.source,
        &owner.name,
        owner.start_line,
    ) else {
        return false;
    };
    let shared: BTreeSet<String> = gates
        .iter()
        .flat_map(|gate| owner_gate_atoms(gate))
        .filter(|atom| refused.contains(atom))
        .collect();
    if shared.is_empty() {
        return false;
    }
    let named = shared
        .iter()
        .map(|atom| format!("`{atom}`"))
        .collect::<Vec<_>>()
        .join(", ");
    let next_step = format!(
        "`{}` is compiled only under {named}, and a doc example in this crate is gated by the same cfg, so ripr did not read it and does not report a gap. Check whether that example, or a test run with {named} on, asserts on the changed behavior; add a test only if none does.",
        owner.name,
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &[]);
    true
}
