//! Inactive pre-allocation seam for the canonical ordinary product portfolio.
//!
//! Inputs are observations from a trusted controller, not self-authenticating
//! attestations. That controller must independently read Git and request state,
//! capture the complete inventory (including generated files), and authenticate
//! result provenance. Equality here cannot establish those external facts.
//! No workflow consumes this seam yet; the wider caller portfolio is not covered.

use std::collections::BTreeSet;

use super::{ProductGateDefinition, ProductGateId, product_gate_definitions};

pub(crate) const PLAN_GENERATION: &str = "canonical-product-admission-v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Provenance {
    pub(crate) producer: String,
    pub(crate) reference: String,
    pub(crate) digest: String,
}

impl Provenance {
    fn valid(&self) -> bool {
        !self.producer.trim().is_empty()
            && !self.reference.trim().is_empty()
            && hex(&self.digest, 64)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InputObject {
    pub(crate) path: String,
    pub(crate) object: String,
    pub(crate) mode: String,
}

impl InputObject {
    fn valid(&self) -> bool {
        !self.path.is_empty()
            && !self.path.starts_with('/')
            && !self.path.contains(['\0', '\\'])
            && self
                .path
                .split('/')
                .all(|part| !matches!(part, "" | "." | ".."))
            && hex(&self.object, 40)
            && matches!(
                self.mode.as_str(),
                "100644" | "100755" | "120000" | "160000"
            )
    }
}

/// Old/new sides retain additions, deletions, renames, copies and mode changes.
/// Generated inputs have no exemption. Inventory order is canonicalized by its
/// producer; this seam requires exact equality rather than dropping any entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ChangedInput {
    pub(crate) before: Option<InputObject>,
    pub(crate) after: Option<InputObject>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Configuration {
    pub(crate) toolchain: String,
    pub(crate) platform: String,
    pub(crate) features: BTreeSet<String>,
    pub(crate) default_features: bool,
    pub(crate) cargo_digest: String,
    pub(crate) configuration_digest: String,
    pub(crate) controller_generation: String,
    pub(crate) plan_generation: String,
    pub(crate) result_generation: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Binding {
    pub(crate) repository: String,
    pub(crate) request_id: String,
    pub(crate) head: String,
    pub(crate) base: String,
    pub(crate) integration: String,
    pub(crate) ordered_parents: Vec<String>,
    pub(crate) tree: String,
    pub(crate) configuration: Configuration,
    pub(crate) inventory: Vec<ChangedInput>,
    pub(crate) inventory_provenance: Provenance,
    /// The authenticated inventory may be empty; absence is not an empty inventory.
    /// This generation binds the snapshot even though this version never reuses it.
    pub(crate) evidence_inventory: Provenance,
}

impl Binding {
    fn valid(&self) -> bool {
        let c = &self.configuration;
        let repository: Vec<_> = self.repository.split('/').collect();
        let mut before = BTreeSet::new();
        let mut after = BTreeSet::new();
        repository.len() == 2
            && repository.iter().all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            })
            && !self.request_id.trim().is_empty()
            && [&self.head, &self.base, &self.integration, &self.tree]
                .into_iter()
                .all(|value| hex(value, 40))
            && !self.ordered_parents.is_empty()
            && self.ordered_parents.iter().all(|value| hex(value, 40))
            && self.ordered_parents.iter().collect::<BTreeSet<_>>().len()
                == self.ordered_parents.len()
            && !c.toolchain.trim().is_empty()
            && !c.platform.trim().is_empty()
            && c.features.iter().all(|feature| !feature.trim().is_empty())
            && hex(&c.cargo_digest, 64)
            && hex(&c.configuration_digest, 64)
            && !c.controller_generation.trim().is_empty()
            && c.plan_generation == PLAN_GENERATION
            && !c.result_generation.trim().is_empty()
            && self.inventory_provenance.valid()
            && self.evidence_inventory.valid()
            && self.inventory.iter().all(|input| {
                (input.before.is_some() || input.after.is_some())
                    && input.before != input.after
                    && input
                        .before
                        .as_ref()
                        .is_none_or(|object| object.valid() && before.insert(&object.path))
                    && input
                        .after
                        .as_ref()
                        .is_none_or(|object| object.valid() && after.insert(&object.path))
            })
    }
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// These are explicit states, never a boolean claiming externally verified truth.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PolicyTransition {
    Unchanged,
    Changed,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Observation {
    Complete {
        binding: Box<Binding>,
        policy: PolicyTransition,
    },
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlockReason {
    NoExplicitRequest,
    ObservationUnavailable,
    InvalidBinding,
    StaleRequest,
    PolicyTransitionNotProven,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Disposition {
    Execute,
    ReuseVerified,
    NotApplicable,
    Blocked(BlockReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExpectedRow {
    pub(crate) definition: ProductGateDefinition,
    pub(crate) disposition: Disposition,
}

/// Fields cannot be replaced by callers or deserialized from candidate output.
/// Only admission constructs the denominator. This version emits Execute or
/// Blocked; neither legacy NoOp nor cache availability can exempt an obligation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AdmissionPlan {
    binding: Option<Binding>,
    rows: Vec<ExpectedRow>,
}

impl AdmissionPlan {
    pub(crate) fn admit(request: Option<&Binding>, observation: &Observation) -> Self {
        let (binding, block) = match (request, observation) {
            (None, _) => (None, Some(BlockReason::NoExplicitRequest)),
            (Some(request), Observation::Unavailable) => (
                Some(request.clone()),
                Some(BlockReason::ObservationUnavailable),
            ),
            (Some(request), Observation::Complete { binding, policy }) => {
                let block = if !request.valid() || !binding.valid() {
                    Some(BlockReason::InvalidBinding)
                } else if request != binding.as_ref() {
                    Some(BlockReason::StaleRequest)
                } else if *policy != PolicyTransition::Unchanged {
                    Some(BlockReason::PolicyTransitionNotProven)
                } else {
                    None
                };
                (Some(binding.as_ref().clone()), block)
            }
        };
        let rows = product_gate_definitions()
            .into_iter()
            .map(|definition| ExpectedRow {
                definition,
                disposition: block.map_or(Disposition::Execute, Disposition::Blocked),
            })
            .collect();
        Self { binding, rows }
    }

    pub(crate) fn rows(&self) -> &[ExpectedRow] {
        &self.rows
    }

    /// Typed per-gate input, not a second selector. Any blocked row prevents all
    /// allocation; missing identities never yield a misleading partial route.
    pub(crate) fn allocations(&self) -> Vec<Allocation<'_>> {
        let Some(binding) = self.binding.as_ref() else {
            return Vec::new();
        };
        if self.rows.is_empty()
            || self
                .rows
                .iter()
                .map(|row| row.definition.id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.rows.len()
            || self
                .rows
                .iter()
                .any(|row| row.disposition != Disposition::Execute)
        {
            return Vec::new();
        }
        self.rows
            .iter()
            .map(|row| Allocation {
                binding,
                definition: &row.definition,
            })
            .collect()
    }

    /// Reconcile against the immutable admitted plan, not an actual-result set
    /// supplied as its own denominator. All non-success outcomes remain failures.
    pub(crate) fn reconcile(&self, actual: &[ActualRow]) -> Reconciliation {
        let allocations = self.allocations();
        let mut problems = Vec::new();
        if allocations.is_empty() {
            problems.push(ResultProblem::BlockedPlan);
        }
        let mut seen = BTreeSet::new();
        for result in actual {
            let id = result.definition.id;
            if !seen.insert(id) {
                problems.push(ResultProblem::Duplicate(id));
            }
            match allocations
                .iter()
                .find(|allocation| allocation.definition.id == id)
            {
                None => problems.push(ResultProblem::Unexpected(id)),
                Some(expected) => {
                    if expected.binding != &result.binding
                        || expected.definition != &result.definition
                    {
                        problems.push(ResultProblem::IdentityMismatch(id));
                    }
                    if result.outcome != Outcome::Passed
                        || result.evidence_kind != EvidenceKind::ExecutedGateResult
                        || !result.provenance.valid()
                    {
                        problems.push(ResultProblem::NotProven(id));
                    }
                }
            }
        }
        for expected in allocations {
            if !seen.contains(&expected.definition.id) {
                problems.push(ResultProblem::Missing(expected.definition.id));
            }
        }
        Reconciliation { problems }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Allocation<'a> {
    pub(crate) binding: &'a Binding,
    pub(crate) definition: &'a ProductGateDefinition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Passed,
    Failed,
    Cancelled,
    Pending,
    Incomplete,
    Skipped,
}

/// Classify authenticated evidence before reconciliation. A cache hit may speed
/// execution but is never an executed gate-result receipt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EvidenceKind {
    ExecutedGateResult,
    CompilerCache,
    DependencyCache,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActualRow {
    pub(crate) binding: Binding,
    pub(crate) definition: ProductGateDefinition,
    pub(crate) outcome: Outcome,
    pub(crate) evidence_kind: EvidenceKind,
    pub(crate) provenance: Provenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ResultProblem {
    BlockedPlan,
    Duplicate(ProductGateId),
    Unexpected(ProductGateId),
    IdentityMismatch(ProductGateId),
    NotProven(ProductGateId),
    Missing(ProductGateId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Reconciliation {
    pub(crate) problems: Vec<ResultProblem>,
}

impl Reconciliation {
    pub(crate) fn accepted(&self) -> bool {
        self.problems.is_empty()
    }
}

#[cfg(test)]
mod tests;
