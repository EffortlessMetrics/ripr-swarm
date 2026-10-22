//! RIPR-SPEC-0240: a trait-impl method no test names is not a gap.
//!
//! A method in `impl Trait for Type` runs through machinery that never spells
//! its name: `Money(2) + Money(3)` calls `Add::add`, a block end calls
//! `Drop::drop`, `collect()` calls `Iterator::next`, and `shapes[0].area()`
//! on a `Box<dyn Shape>` dispatches to one of several `area` impls. The reach
//! stage sees only name tokens for such an owner ("No test is seen calling
//! `add`"), so it reads `weak`. A `weak` reach that rests on proximity cannot
//! support the claim that an assertion is too weak: the test that pins the
//! value may be the one running the method.
//!
//! The gap is withheld under `rust_implicit_trait_call_unresolved`. Nothing is
//! credited, and like every RIPR-SPEC-0240 rule this only withholds, so the
//! actionability flip stays fail-closed.

use super::gap_admission::withhold_gap;
use crate::analysis::rust_index::RustIndex;
use crate::analysis::test_grip_evidence::text_calls;
use crate::domain::*;

const LIMIT: StaticLimitKind = StaticLimitKind::RustImplicitTraitCallUnresolved;
const FIRST_UNRESOLVED_EDGE: &str = "whether a related test runs this trait-impl method through an operator, drop glue, iteration or dynamic dispatch";
const PROXIMITY_REACH_PREFIX: &str = "No test is seen calling ";

/// Withhold a gap on a trait-impl method whose reach is name-token proximity
/// only. Returns `true` when the finding was rewritten to `static_unknown`.
pub(in crate::analysis) fn withhold_implicit_trait_call_gap(
    finding: &mut Finding,
    index: &RustIndex,
) -> bool {
    if finding.static_limit_kind.is_some()
        || !matches!(
            finding.class,
            ExposureClass::WeaklyExposed | ExposureClass::ReachableUnrevealed
        )
        || finding.ripr.reach.state != StageState::Weak
        || !finding
            .ripr
            .reach
            .summary
            .starts_with(PROXIMITY_REACH_PREFIX)
        || finding
            .related_tests
            .iter()
            .any(|related| related.relation_reason == Some(RelationReason::DirectOwnerCall))
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
    let Some(trait_name) = implemented_trait(&owner.id.0) else {
        return false;
    };
    let trait_ident = trait_name.split(" for ").next().unwrap_or_default();
    // A test that calls the method name on a concrete type reaches it, or a
    // same-named method on another type, by an ordinary call the reach stage
    // already reads. Only a call through a trait object of this trait, or no
    // call by name at all, leaves the reach to implicit dispatch.
    let named_concrete_call = finding.related_tests.iter().any(|related| {
        index
            .tests()
            .iter()
            .find(|test| {
                test.name == related.name
                    && test.file == related.file
                    && test.start_line == related.line
            })
            .is_some_and(|test| {
                text_calls(&test.body, &owner.name)
                    && !calls_through_trait_object(&test.body, trait_ident)
            })
    });
    if named_concrete_call {
        return false;
    }
    // A name-tied test that pins an exact value is the one that may run the
    // method implicitly. Without one (a smoke test, a relational check) the
    // weak oracle is the finding and the gap stands.
    let labels: Vec<String> = finding
        .related_tests
        .iter()
        .filter(|related| {
            matches!(
                related.relation_reason,
                Some(RelationReason::OwnerNamedTest | RelationReason::WeakTokenSubstring)
            ) && related.oracle_strength == OracleStrength::Strong
        })
        .take(3)
        .map(|related| {
            format!(
                "`{}` ({}:{})",
                related.name,
                related.file.display().to_string().replace('\\', "/"),
                related.line
            )
        })
        .collect();
    if labels.is_empty() {
        return false;
    }
    let next_step = format!(
        "`{}` is a method of `impl {trait_name}`, which a test can run without writing its name (an operator, drop glue, iteration, or a call through a trait object), so ripr does not report a gap. Check that a test builds the implementing type and exercises the method with an input the changed code treats differently; add a test only if none does.",
        owner.name
    );
    withhold_gap(finding, LIMIT, FIRST_UNRESOLVED_EDGE, next_step, &labels);
    true
}

/// The trait of the innermost `impl Trait for Type` segment of a function id
/// (`src/ops.rs::impl Add for Money::add` -> `Add for Money`).
fn implemented_trait(id: &str) -> Option<String> {
    id.split("::impl ")
        .skip(1)
        .filter(|segment| segment.contains(" for "))
        .last()
        .map(|segment| segment.rsplit_once("::").map_or(segment, |(head, _)| head))
        .map(str::to_string)
}

/// Whether `body` builds or names a `dyn <trait>` object.
fn calls_through_trait_object(body: &str, trait_ident: &str) -> bool {
    !trait_ident.is_empty()
        && body.match_indices("dyn ").any(|(at, _)| {
            body[at + 4..]
                .trim_start()
                .strip_prefix(trait_ident)
                .is_some_and(|rest| !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_'))
        })
}
