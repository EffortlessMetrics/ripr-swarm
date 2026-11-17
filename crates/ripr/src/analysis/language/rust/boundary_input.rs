//! #14951: the typed boundary-input limit on a diff finding for an owner no
//! test calls.
//!
//! The repo-seam producer (`test_grip_evidence::activate_evidence`) names
//! `activation_boundary_input_unresolved` when a changed predicate compares an
//! operand that binds to no owner argument (`cents % 100 > 49`), and agent
//! guidance withholds that seam as a static limitation. `ripr check` reads the
//! same change as a diff finding. When the producer has established that
//! nothing in the workspace names the owner, that finding asks the same
//! question the seam answered, through the same decision
//! ([`boundary_input_unresolved_for_uncalled_owner`]), and carries the same
//! typed kind, so `check`, the agent header and PR guidance agree on whether
//! the change is a gap or a withheld limitation.
//!
//! The class, stages and related tests are untouched: a `no_static_path`
//! finding stays `no_static_path`, as for every other named Rust reach
//! limit. Any other finding keeps today's reading.

use crate::analysis::facts::RustIndex;
use crate::analysis::seams::{ExpectedSink, RepoSeam, RequiredDiscriminator, SeamKind};
use crate::analysis::test_grip_evidence::boundary_input_unresolved_for_uncalled_owner;
use crate::domain::{
    ExposureClass, Finding, Probe, ProbeFamily, StageState, StaticLimitKind,
    UNREFERENCED_IN_WORKSPACE,
};

const LIMIT: StaticLimitKind = StaticLimitKind::ActivationBoundaryInputUnresolved;

/// Attach the typed limit when the finding is a changed predicate in an owner
/// nothing in the workspace names, some test is related to it (as the seam's
/// activate stage requires), and the shared decision says its boundary input
/// is unresolved. Returns whether the limit was attached.
pub(super) fn apply_limit(finding: &mut Finding, probe: &Probe, index: &RustIndex) -> bool {
    if probe.family != ProbeFamily::Predicate
        || finding.class != ExposureClass::NoStaticPath
        || finding.static_limit_kind.is_some()
        || finding.related_tests.is_empty()
        || finding.ripr.reach.state != StageState::No
        || !finding
            .ripr
            .reach
            .summary
            .ends_with(UNREFERENCED_IN_WORKSPACE)
    {
        return false;
    }
    // The seam below is rebuilt from the diff line. A predicate spanning
    // lines or joining comparisons may not split the way the repo producer
    // splits its parsed expression, so those keep today's reading.
    if probe.expression.contains('\n')
        || probe.expression.contains("&&")
        || probe.expression.contains("||")
    {
        return false;
    }
    let Some(owner) = probe
        .owner
        .as_ref()
        .and_then(|id| index.functions().iter().find(|function| function.id == *id))
    else {
        return false;
    };
    // The diff probe's own expression and line, read as the predicate seam
    // the repo producer would build for it. Identity fields are unused here.
    let seam = RepoSeam::new(
        &owner.file,
        owner.name.clone(),
        SeamKind::PredicateBoundary,
        0,
        probe.location.line,
        probe.expression.clone(),
        RequiredDiscriminator::BoundaryValue {
            description: probe.expression.clone(),
        },
        ExpectedSink::ReturnValue,
    );
    if !boundary_input_unresolved_for_uncalled_owner(&seam, index, owner) {
        return false;
    }
    finding.static_limit_kind = Some(LIMIT);
    finding.evidence.push(format!(
        "{}: the changed boundary `{}` compares a local or computed operand that ripr cannot map to a test call's arguments; the repair is withheld as a named limitation, not a coverage claim",
        LIMIT.as_str(),
        probe.expression
    ));
    true
}
