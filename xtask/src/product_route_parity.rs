//! Compare synthetic ordinary and external-tree gate-plan projections and
//! inspect ordinary workflow declarations (#3332). Live route execution and
//! portable receipts await #3331 and #3329.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use repo_policy::product_gate_plan::composites::{CompositeProducer, CompositeSubject, Event};
use serde::{Deserialize, Deserializer, Serialize, de};

use crate::work_portfolio::workspace_path;
use repo_policy::product_gate_plan::{
    ProductGatePlan, ProductGateRole, ProductGateSelection, ProductGateSubject,
    ProductGateTrustClass, ProductSurface,
};

pub(crate) const PRODUCT_ROUTE_PARITY_CORPUS_SCHEMA: &str = "ripr.product_route_parity_corpus.v1";
pub(crate) const PRODUCT_ROUTE_PARITY_REPORT_SCHEMA: &str = "ripr.product_route_parity.v1";
pub(crate) const DEFAULT_PRODUCT_ROUTE_PARITY_CORPUS: &str =
    "fixtures/product-route-parity/corpus.json";

pub(crate) const PRODUCT_ROUTE_PARITY_CLAIM_BOUNDARY: &str = "Compares normalized gate \
 propositions from the typed #3330 plan for the ordinary repository route and the reusable \
 external-tree route projection; runner, cache, and cost observations are not recorded. This check \
 does not execute either route, emit portable product proof (#3329), or change required checks.";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RouteKind {
    OrdinarySwarm,
    ReusableExternalTree,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum GateTerminalState {
    Pass,
    Fail,
    Skipped,
    Unavailable,
    NotProven,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MismatchOwner {
    OrdinaryRouteDefect,
    ReusableRouteDefect,
    CanonicalPlanDefect,
    ExpectedInfrastructureDifference,
    NotProven,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusSubjectSpec {
    changed_surfaces: Vec<String>,
    selectors_authoritative: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CorpusSubject {
    id: String,
    title: String,
    subject: CorpusSubjectSpec,
    public_fork: bool,
    ordinary_runner: String,
    reusable_runner: String,
    #[serde(default, deserialize_with = "deserialize_unique_terminals")]
    terminals: BTreeMap<String, GateTerminalState>,
    expect_semantic_parity: bool,
    #[serde(default)]
    expect_mismatch_owner: Option<MismatchOwner>,
    #[serde(default)]
    expect_public_fork_trust: bool,
}

fn deserialize_unique_terminals<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, GateTerminalState>, D::Error>
where
    D: Deserializer<'de>,
{
    struct TerminalsVisitor;
    impl<'de> de::Visitor<'de> for TerminalsVisitor {
        type Value = BTreeMap<String, GateTerminalState>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a terminal map with unique gate IDs")
        }

        fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
        where
            M: de::MapAccess<'de>,
        {
            let mut terminals = BTreeMap::new();
            while let Some((id, state)) = map.next_entry::<String, GateTerminalState>()? {
                if terminals.insert(id.clone(), state).is_some() {
                    return Err(de::Error::custom(format!("duplicate terminal gate `{id}`")));
                }
            }
            Ok(terminals)
        }
    }
    deserializer.deserialize_map(TerminalsVisitor)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductRouteParityCorpus {
    schema_version: String,
    claim_boundary: String,
    subjects: Vec<CorpusSubject>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct GateProposition {
    pub gate_id: String,
    pub selection: ProductGateSelection,
    pub command: String,
    pub claim: String,
    pub non_claim: String,
    pub composite_projection: Option<CompositeProjection>,
    pub role: ProductGateRole,
    pub terminal_state: GateTerminalState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CompositeProjection {
    pub producer: String,
    pub events: Vec<CompositeEventProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CompositeEventProjection {
    pub event: String,
    pub subject: String,
    pub pr_events_only: bool,
    pub steps: Vec<CompositeStepProjection>,
    pub commands: Vec<String>,
    pub artifact_edges: Vec<CompositeArtifactEdgeProjection>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CompositeStepProjection {
    pub name: String,
    pub id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct CompositeArtifactEdgeProjection {
    pub path: String,
    pub producer: String,
    pub produced_by: usize,
    pub consumed_by: usize,
    pub schema: Option<String>,
}

fn project_composite(producer: CompositeProducer) -> CompositeProjection {
    let producer_name = match producer {
        CompositeProducer::BaseUserText => "base_user_text",
        CompositeProducer::PrEvidence => "pr_evidence",
        CompositeProducer::SchemaSweep => "schema_sweep",
    };
    let events = [
        ("pull_request", Event::PullRequest),
        ("merge_group", Event::MergeGroup),
        ("other", Event::Other),
    ]
    .into_iter()
    .map(|(event, kind)| {
        let contract = producer.contract(kind);
        CompositeEventProjection {
            event: event.to_string(),
            subject: match contract.subject {
                CompositeSubject::FirstParentMatcherAgainstCheckout => {
                    "first_parent_matcher_against_checkout"
                }
                CompositeSubject::AdmittedComparison => "admitted_comparison",
                CompositeSubject::CheckoutWithEventDependentEvidence => {
                    "checkout_with_event_dependent_evidence"
                }
            }
            .to_string(),
            pr_events_only: contract.pr_events_only,
            steps: contract
                .steps
                .into_iter()
                .map(|step| CompositeStepProjection {
                    name: step.name.to_string(),
                    id: step.id.map(str::to_string),
                })
                .collect(),
            commands: contract.commands.into_iter().map(str::to_string).collect(),
            artifact_edges: contract
                .artifacts
                .into_iter()
                .map(|edge| CompositeArtifactEdgeProjection {
                    path: edge.path.to_string(),
                    producer: edge.producer.as_str().to_string(),
                    produced_by: edge.produced_by,
                    consumed_by: edge.consumed_by,
                    schema: edge.schema.map(str::to_string),
                })
                .collect(),
        }
    })
    .collect();
    CompositeProjection {
        producer: producer_name.to_string(),
        events,
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct RouteObservation {
    pub route: RouteKind,
    pub trust_class: ProductGateTrustClass,
    pub runner_identity: String,
    pub public_fork: bool,
    pub propositions: Vec<GateProposition>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct PropositionMismatch {
    pub gate_id: String,
    pub ordinary: GateProposition,
    pub reusable: GateProposition,
    pub owner: MismatchOwner,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SubjectParityResult {
    pub id: String,
    pub title: String,
    pub semantic_parity: bool,
    pub matched_propositions: usize,
    pub mismatched_propositions: usize,
    pub mismatch_owner: Option<MismatchOwner>,
    pub mismatches: Vec<PropositionMismatch>,
    pub public_fork_trust_ok: bool,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProductRouteParityReport {
    pub schema_version: String,
    pub claim_boundary: String,
    pub subjects_total: usize,
    pub subjects_matched: usize,
    pub subjects_mismatched: usize,
    pub proof_equivalence_verdict: String,
    pub safe_consumers: Vec<String>,
    pub limitations: Vec<String>,
    pub ordinary_workflow_declarations: WorkflowDeclarationAudit,
    pub subjects: Vec<SubjectParityResult>,
}

/// A structural observation of the checked-in ordinary Rust workflow, not an
/// execution receipt or a decision about which unrepresented steps are product
/// propositions. Summary bindings and literal command lines are separate from
/// the plan that supplies the canonical commands.
#[derive(Clone, Debug, Serialize)]
pub(crate) struct WorkflowDeclarationAudit {
    pub workflow: String,
    pub claim_boundary: String,
    pub canonical_commands_found_once: Vec<String>,
    pub canonical_commands_missing: Vec<String>,
    pub canonical_commands_ambiguous: Vec<String>,
    pub canonical_commands_without_summary_binding: Vec<String>,
    pub composite_owners_not_validated: Vec<String>,
    pub summarized_steps_without_canonical_command: Vec<String>,
}

impl WorkflowDeclarationAudit {
    fn require_canonical_commands(&self) -> Result<(), String> {
        if !self.canonical_commands_missing.is_empty()
            || !self.canonical_commands_ambiguous.is_empty()
        {
            return Err(format!(
                "ordinary workflow literal commands missing {:?}, ambiguous {:?}",
                self.canonical_commands_missing, self.canonical_commands_ambiguous
            ));
        }
        Ok(())
    }
}

fn audit_ordinary_workflow_declarations(
    workflow: &str,
) -> Result<WorkflowDeclarationAudit, String> {
    let mut steps = BTreeMap::new();
    let mut commands = Vec::new();
    for block in workflow.split("\n      - ").skip(1) {
        let id = block
            .lines()
            .find_map(|line| line.strip_prefix("        id: "));
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .map(|line| line.strip_prefix("run: ").unwrap_or(line))
            .collect();
        commands.extend(lines.iter().copied());
        if let Some(id) = id
            && steps.insert(id, lines).is_some()
        {
            return Err(format!("duplicate ordinary workflow step ID `{id}`"));
        }
    }
    let summary = steps
        .get("gate_summary")
        .ok_or("ordinary workflow gate summary missing")?;
    let mut summarized = BTreeSet::new();
    for line in summary {
        if let Some((_, binding)) = line.split_once("${{ steps.")
            && let Some((id, _)) = binding.split_once(".outcome }}")
            && !summarized.insert(id)
        {
            return Err(format!("duplicate ordinary summary outcome `{id}`"));
        }
    }
    if summarized.is_empty() {
        return Err("ordinary workflow summary has no step outcomes".to_string());
    }
    let definitions = repo_policy::product_gate_plan::product_gate_definitions();
    let mut found = Vec::new();
    let mut missing = Vec::new();
    let mut ambiguous = Vec::new();
    for gate in definitions.iter().filter(|gate| gate.composite.is_none()) {
        let count = commands
            .iter()
            .filter(|line| **line == gate.command)
            .count();
        let destination = match count {
            0 => &mut missing,
            1 => &mut found,
            _ => &mut ambiguous,
        };
        destination.push(gate.id.as_str().to_string());
    }
    let without_summary = definitions
        .iter()
        .filter(|gate| gate.composite.is_none())
        .filter(|gate| {
            !summarized.iter().any(|id| {
                steps
                    .get(*id)
                    .is_some_and(|lines| lines.contains(&gate.command))
            })
        })
        .map(|gate| gate.id.as_str().to_string())
        .collect();
    let unclassified = summarized
        .iter()
        .filter(|id| {
            !steps.get(**id).is_some_and(|lines| {
                definitions
                    .iter()
                    .filter(|gate| gate.composite.is_none())
                    .any(|gate| lines.contains(&gate.command))
            })
        })
        .map(|id| (*id).to_string())
        .collect();
    Ok(WorkflowDeclarationAudit {
        workflow: ".github/workflows/rust-gates.yml".to_string(),
        claim_boundary: "Literal step command and summary-binding observations only; no execution, applicability, completeness, shell or product-proposition classification is established.".to_string(),
        canonical_commands_found_once: found,
        canonical_commands_missing: missing,
        canonical_commands_ambiguous: ambiguous,
        canonical_commands_without_summary_binding: without_summary,
        composite_owners_not_validated: definitions.iter().filter(|gate| gate.composite.is_some()).map(|gate| gate.id.as_str().to_string()).collect(),
        summarized_steps_without_canonical_command: unclassified,
    })
}

fn parse_surface(name: &str) -> Result<ProductSurface, String> {
    match name {
        "rust" => Ok(ProductSurface::Rust),
        "evidence" => Ok(ProductSurface::Evidence),
        "repository_policy" => Ok(ProductSurface::RepositoryPolicy),
        other => Err(format!("unknown product surface `{other}`")),
    }
}

fn subject_from_spec(spec: &CorpusSubjectSpec) -> Result<ProductGateSubject, String> {
    let mut changed_surfaces = BTreeSet::new();
    for surface in &spec.changed_surfaces {
        changed_surfaces.insert(parse_surface(surface)?);
    }
    if changed_surfaces.is_empty() {
        return Err("subject must name at least one changed surface".to_string());
    }
    Ok(ProductGateSubject {
        changed_surfaces,
        selectors_authoritative: spec.selectors_authoritative,
        trust_class: ProductGateTrustClass::Repository,
    })
}

fn terminal_for_row(
    gate_id: &str,
    selection: ProductGateSelection,
    terminals: &BTreeMap<String, GateTerminalState>,
) -> GateTerminalState {
    if let Some(state) = terminals.get(gate_id) {
        if *state == GateTerminalState::NotApplicable && selection == ProductGateSelection::Selected
        {
            return GateTerminalState::NotProven;
        }
        return *state;
    }
    match selection {
        ProductGateSelection::NoOp => GateTerminalState::NotApplicable,
        ProductGateSelection::Selected => GateTerminalState::NotProven,
        ProductGateSelection::Quarantined => GateTerminalState::NotProven,
        ProductGateSelection::NotProven => GateTerminalState::NotProven,
    }
}

fn build_route_observation(
    route: RouteKind,
    subject: &ProductGateSubject,
    runner_identity: &str,
    public_fork: bool,
    terminals: &BTreeMap<String, GateTerminalState>,
) -> RouteObservation {
    let mut routed_subject = subject.clone();
    routed_subject.trust_class = match route {
        RouteKind::OrdinarySwarm => ProductGateTrustClass::Repository,
        RouteKind::ReusableExternalTree => ProductGateTrustClass::ExternalTree,
    };
    let plan = ProductGatePlan::for_subject(&routed_subject);
    let propositions = plan
        .rows
        .into_iter()
        .map(|row| {
            let gate_id = row.definition.id.as_str().to_string();
            let terminal_state = terminal_for_row(&gate_id, row.selection, terminals);
            GateProposition {
                gate_id: gate_id.clone(),
                selection: row.selection,
                command: row.definition.command.to_string(),
                claim: row.definition.claim.to_string(),
                non_claim: row.definition.non_claim.to_string(),
                composite_projection: row.definition.composite.map(project_composite),
                role: row.definition.role,
                terminal_state,
            }
        })
        .collect();
    RouteObservation {
        route,
        trust_class: routed_subject.trust_class,
        runner_identity: runner_identity.to_string(),
        public_fork,
        propositions,
    }
}

fn classify_mismatch(
    ordinary_row: &GateProposition,
    reusable_row: &GateProposition,
) -> MismatchOwner {
    if ordinary_row.command != reusable_row.command
        || ordinary_row.role != reusable_row.role
        || ordinary_row.claim != reusable_row.claim
        || ordinary_row.non_claim != reusable_row.non_claim
        || ordinary_row.composite_projection != reusable_row.composite_projection
    {
        return MismatchOwner::CanonicalPlanDefect;
    }
    // Applicability is part of the product proposition, not an infrastructure
    // difference. Without independent route evidence its owner is unresolved.
    if ordinary_row.selection != reusable_row.selection {
        // Do not let a selection disagreement hide a false green for a gate
        // the other route actually selected and did not pass.
        if ordinary_row.selection == ProductGateSelection::Selected
            && reusable_row.terminal_state == GateTerminalState::Pass
            && ordinary_row.terminal_state != GateTerminalState::Pass
        {
            return MismatchOwner::ReusableRouteDefect;
        }
        if reusable_row.selection == ProductGateSelection::Selected
            && ordinary_row.terminal_state == GateTerminalState::Pass
            && reusable_row.terminal_state != GateTerminalState::Pass
        {
            return MismatchOwner::OrdinaryRouteDefect;
        }
        return MismatchOwner::NotProven;
    }
    if ordinary_row.terminal_state != reusable_row.terminal_state {
        let ordinary_terminal = ordinary_row.terminal_state;
        let reusable_terminal = reusable_row.terminal_state;
        if reusable_terminal == GateTerminalState::Pass
            && ordinary_terminal != GateTerminalState::Pass
        {
            return MismatchOwner::ReusableRouteDefect;
        }
        if ordinary_terminal == GateTerminalState::Pass
            && reusable_terminal != GateTerminalState::Pass
        {
            return MismatchOwner::OrdinaryRouteDefect;
        }
        if matches!(
            reusable_terminal,
            GateTerminalState::Skipped | GateTerminalState::NotApplicable
        ) && matches!(
            ordinary_terminal,
            GateTerminalState::Fail | GateTerminalState::Unavailable
        ) {
            return MismatchOwner::ReusableRouteDefect;
        }
        if matches!(
            ordinary_terminal,
            GateTerminalState::Skipped | GateTerminalState::NotApplicable
        ) && matches!(
            reusable_terminal,
            GateTerminalState::Fail | GateTerminalState::Unavailable
        ) {
            return MismatchOwner::OrdinaryRouteDefect;
        }
        return MismatchOwner::NotProven;
    }
    MismatchOwner::NotProven
}

fn mismatch_owner_is_route_defect(owner: MismatchOwner) -> bool {
    matches!(
        owner,
        MismatchOwner::OrdinaryRouteDefect
            | MismatchOwner::ReusableRouteDefect
            | MismatchOwner::CanonicalPlanDefect
    )
}

fn unobserved_proposition(gate_id: &str) -> GateProposition {
    GateProposition {
        gate_id: gate_id.to_string(),
        selection: ProductGateSelection::NotProven,
        command: String::new(),
        claim: String::new(),
        non_claim: String::new(),
        composite_projection: None,
        role: ProductGateRole::Required,
        terminal_state: GateTerminalState::NotProven,
    }
}

pub(crate) fn compare_route_observations(
    _ordinary_subject: &ProductGateSubject,
    ordinary: &RouteObservation,
    reusable: &RouteObservation,
) -> Result<(bool, Vec<PropositionMismatch>, Option<MismatchOwner>), String> {
    if ordinary.propositions.is_empty() || reusable.propositions.is_empty() {
        return Err("route observations must each contain propositions".to_string());
    }
    let mut by_ordinary: BTreeMap<&str, &GateProposition> = BTreeMap::new();
    for row in &ordinary.propositions {
        if row.gate_id.trim().is_empty() {
            return Err("ordinary gate ID must not be empty".to_string());
        }
        if by_ordinary.insert(row.gate_id.as_str(), row).is_some() {
            return Err(format!("duplicate ordinary gate `{}`", row.gate_id));
        }
    }
    let mut by_reusable: BTreeMap<&str, &GateProposition> = BTreeMap::new();
    for row in &reusable.propositions {
        if row.gate_id.trim().is_empty() {
            return Err("reusable gate ID must not be empty".to_string());
        }
        if by_reusable.insert(row.gate_id.as_str(), row).is_some() {
            return Err(format!("duplicate reusable gate `{}`", row.gate_id));
        }
    }
    let definitions = repo_policy::product_gate_plan::product_gate_definitions();
    for gate in &definitions {
        let id = gate.id.as_str();
        match (by_ordinary.get(id), by_reusable.get(id)) {
            (None, None) => {
                return Err(format!(
                    "canonical gate `{id}` is absent from both observations"
                ));
            }
            (Some(left), Some(right))
                if gate.composite.is_some()
                    && left.composite_projection.is_none()
                    && right.composite_projection.is_none() =>
            {
                return Err(format!(
                    "composite model `{id}` is absent from both observations"
                ));
            }
            _ => {}
        }
    }
    let canonical_ids: BTreeSet<_> = definitions.iter().map(|gate| gate.id.as_str()).collect();
    let gate_ids: BTreeSet<_> = by_ordinary
        .keys()
        .chain(by_reusable.keys())
        .copied()
        .collect();
    let mut mismatches = Vec::new();
    for gate_id in gate_ids {
        if !canonical_ids.contains(gate_id) {
            let left = by_ordinary.get(gate_id).copied();
            let right = by_reusable.get(gate_id).copied();
            mismatches.push(PropositionMismatch {
                gate_id: gate_id.to_string(),
                ordinary: left
                    .cloned()
                    .unwrap_or_else(|| unobserved_proposition(gate_id)),
                reusable: right
                    .cloned()
                    .unwrap_or_else(|| unobserved_proposition(gate_id)),
                owner: MismatchOwner::CanonicalPlanDefect,
            });
            continue;
        }
        let Some(ordinary_row) = by_ordinary.get(gate_id).copied() else {
            mismatches.push(PropositionMismatch {
                gate_id: gate_id.to_string(),
                ordinary: GateProposition {
                    gate_id: gate_id.to_string(),
                    selection: ProductGateSelection::NotProven,
                    command: String::new(),
                    claim: String::new(),
                    non_claim: String::new(),
                    composite_projection: None,
                    role: ProductGateRole::Required,
                    terminal_state: GateTerminalState::NotProven,
                },
                reusable: (*by_reusable[gate_id]).clone(),
                owner: MismatchOwner::OrdinaryRouteDefect,
            });
            continue;
        };
        let Some(reusable_row) = by_reusable.get(gate_id).copied() else {
            mismatches.push(PropositionMismatch {
                gate_id: gate_id.to_string(),
                ordinary: (*ordinary_row).clone(),
                reusable: GateProposition {
                    gate_id: gate_id.to_string(),
                    selection: ProductGateSelection::NotProven,
                    command: String::new(),
                    claim: String::new(),
                    non_claim: String::new(),
                    composite_projection: None,
                    role: ProductGateRole::Required,
                    terminal_state: GateTerminalState::NotProven,
                },
                owner: MismatchOwner::ReusableRouteDefect,
            });
            continue;
        };
        let semantic_match = ordinary_row.selection == reusable_row.selection
            && ordinary_row.command == reusable_row.command
            && ordinary_row.claim == reusable_row.claim
            && ordinary_row.non_claim == reusable_row.non_claim
            && ordinary_row.composite_projection == reusable_row.composite_projection
            && ordinary_row.role == reusable_row.role
            && ordinary_row.terminal_state == reusable_row.terminal_state;
        if !semantic_match {
            mismatches.push(PropositionMismatch {
                gate_id: gate_id.to_string(),
                ordinary: (*ordinary_row).clone(),
                reusable: (*reusable_row).clone(),
                owner: classify_mismatch(ordinary_row, reusable_row),
            });
        }
    }
    if mismatches.is_empty() {
        return Ok((true, mismatches, None));
    }
    let owner = mismatches
        .iter()
        .map(|m| m.owner)
        .max_by_key(|owner| match owner {
            MismatchOwner::CanonicalPlanDefect => 5,
            MismatchOwner::OrdinaryRouteDefect => 4,
            MismatchOwner::ReusableRouteDefect => 4,
            MismatchOwner::ExpectedInfrastructureDifference => 3,
            MismatchOwner::NotProven => 1,
        });
    Ok((false, mismatches, owner))
}

pub(crate) fn verify_public_fork_trust(observation: &RouteObservation) -> Result<(), String> {
    if !observation.public_fork {
        return Ok(());
    }
    if observation.route != RouteKind::ReusableExternalTree {
        return Err("public/fork trust controls apply to the reusable route only".to_string());
    }
    if observation.trust_class != ProductGateTrustClass::ExternalTree {
        return Err("public/fork reusable route must keep external-tree trust class".to_string());
    }
    for label in ["self-hosted", "contabo", "trusted", "secret"] {
        if observation
            .runner_identity
            .to_ascii_lowercase()
            .contains(label)
        {
            return Err(format!(
                "public/fork reusable route must not name trusted runner or secret surface `{label}`"
            ));
        }
    }
    Ok(())
}

fn load_corpus(path: &Path) -> Result<ProductRouteParityCorpus, String> {
    let body = fs::read_to_string(path)
        .map_err(|err| format!("read product route parity corpus {}: {err}", path.display()))?;
    let corpus: ProductRouteParityCorpus = serde_json::from_str(&body)
        .map_err(|err| format!("parse product route parity corpus: {err}"))?;
    if corpus.schema_version != PRODUCT_ROUTE_PARITY_CORPUS_SCHEMA {
        return Err(format!(
            "unsupported product route parity corpus schema `{}`",
            corpus.schema_version
        ));
    }
    if corpus.subjects.is_empty() {
        return Err("product route parity corpus must include at least one subject".to_string());
    }
    let canonical_ids: BTreeSet<_> = repo_policy::product_gate_plan::product_gate_definitions()
        .into_iter()
        .map(|gate| gate.id.as_str())
        .collect();
    for subject in &corpus.subjects {
        for id in subject.terminals.keys() {
            if !canonical_ids.contains(id.as_str()) {
                return Err(format!(
                    "subject `{}` has unknown terminal gate `{id}`",
                    subject.id
                ));
            }
        }
        if !subject.expect_semantic_parity && subject.expect_mismatch_owner.is_none() {
            return Err(format!(
                "subject `{}` must set expect_mismatch_owner when expect_semantic_parity is false",
                subject.id
            ));
        }
    }
    Ok(corpus)
}

pub(crate) fn run_product_route_parity_check(
    corpus_path: &Path,
) -> Result<ProductRouteParityReport, String> {
    let corpus = load_corpus(corpus_path)?;
    let workflow_path = workspace_path(".github/workflows/rust-gates.yml");
    let workflow = fs::read_to_string(&workflow_path)
        .map_err(|err| format!("read ordinary workflow {}: {err}", workflow_path.display()))?;
    let ordinary_workflow_declarations = audit_ordinary_workflow_declarations(&workflow)?;
    ordinary_workflow_declarations.require_canonical_commands()?;
    let mut subject_results = Vec::new();
    let mut matched = 0usize;
    let mut mismatched = 0usize;
    for subject in &corpus.subjects {
        let gate_subject = subject_from_spec(&subject.subject)?;
        let ordinary = build_route_observation(
            RouteKind::OrdinarySwarm,
            &gate_subject,
            &subject.ordinary_runner,
            false,
            &subject.terminals,
        );
        let reusable = build_route_observation(
            RouteKind::ReusableExternalTree,
            &gate_subject,
            &subject.reusable_runner,
            subject.public_fork,
            &subject.terminals,
        );
        let (semantic_parity, mismatches, owner) =
            compare_route_observations(&gate_subject, &ordinary, &reusable)?;
        let public_fork_trust_ok = verify_public_fork_trust(&reusable).is_ok();
        if subject.expect_semantic_parity && !semantic_parity {
            return Err(format!(
                "subject `{}` expected semantic parity but mismatched ({owner:?})",
                subject.id
            ));
        }
        if !subject.expect_semantic_parity && semantic_parity {
            return Err(format!(
                "subject `{}` expected a mismatch but propositions matched",
                subject.id
            ));
        }
        if let Some(expected_owner) = subject.expect_mismatch_owner
            && owner != Some(expected_owner)
        {
            return Err(format!(
                "subject `{}` expected mismatch owner `{expected_owner:?}` but observed `{owner:?}`",
                subject.id
            ));
        }
        if subject.expect_public_fork_trust && !public_fork_trust_ok {
            return Err(format!(
                "subject `{}` expected public/fork trust controls to pass",
                subject.id
            ));
        }
        let matched_propositions = ordinary.propositions.len().saturating_sub(mismatches.len());
        if semantic_parity {
            matched += 1;
        } else {
            mismatched += 1;
        }
        subject_results.push(SubjectParityResult {
            id: subject.id.clone(),
            title: subject.title.clone(),
            semantic_parity,
            matched_propositions,
            mismatched_propositions: mismatches.len(),
            mismatch_owner: owner,
            mismatches,
            public_fork_trust_ok,
        });
    }
    let unexpected_defect = subject_results.iter().any(|subject| {
        subject.mismatches.iter().any(|mismatch| {
            mismatch_owner_is_route_defect(mismatch.owner)
                && subject.mismatch_owner != Some(mismatch.owner)
        })
    });
    if unexpected_defect {
        return Err(
            "corpus produced route or plan defects without a matching expect_mismatch_owner"
                .to_string(),
        );
    }
    // These observations are projections, not independently executed routes
    // or subject-bound receipts, regardless of whether their rows match.
    let proof_equivalence_verdict = "not_proven".to_string();
    Ok(ProductRouteParityReport {
        schema_version: PRODUCT_ROUTE_PARITY_REPORT_SCHEMA.to_string(),
        claim_boundary: PRODUCT_ROUTE_PARITY_CLAIM_BOUNDARY.to_string(),
        subjects_total: corpus.subjects.len(),
        subjects_matched: matched,
        subjects_mismatched: mismatched,
        proof_equivalence_verdict,
        safe_consumers: vec![
            "periodic equivalence planning (#1592)".to_string(),
            "parity denominator expansion after #3331 lands".to_string(),
        ],
        limitations: vec![
            corpus.claim_boundary.clone(),
            "Runner/cache/wall-clock differences are excluded from semantic comparison and are not recorded by this report.".to_string(),
            "Live reusable workflow execution and portable receipts remain owned by #3331/#3329.".to_string(),
            "Public/fork trust checks inspect synthetic runner labels, not actual runner or secret delivery.".to_string(),
            "Composite projections describe the canonical source model for three event classes; they do not validate live shell control flow, history-sync exceptions, artifact authenticity, or execution.".to_string(),
        ],
        ordinary_workflow_declarations,
        subjects: subject_results,
    })
}

pub(crate) fn check_product_route_parity(args: &[String]) -> Result<(), String> {
    const USAGE: &str = "usage: cargo xtask check-product-route-parity [--corpus <path>] [--json]";
    let mut corpus = workspace_path(DEFAULT_PRODUCT_ROUTE_PARITY_CORPUS);
    let mut json = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" => {
                index += 1;
                corpus = Path::new(
                    args.get(index)
                        .ok_or_else(|| format!("{USAGE}: --corpus requires a path"))?,
                )
                .to_path_buf();
            }
            "--json" => json = true,
            flag => return Err(format!("{USAGE}: unknown argument `{flag}`")),
        }
        index += 1;
    }
    let report = run_product_route_parity_check(&corpus)?;
    let report_json = serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?;
    crate::write_report("product-route-parity.json", &report_json)?;
    if json {
        println!("{report_json}");
    } else {
        println!("product route parity: {}", report.proof_equivalence_verdict);
        println!(
            "subjects: {} matched / {} mismatched of {}",
            report.subjects_matched, report.subjects_mismatched, report.subjects_total
        );
        for subject in &report.subjects {
            println!(
                "- {}: parity={} mismatches={}",
                subject.id, subject.semantic_parity, subject.mismatched_propositions
            );
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvenanceFile {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductRouteParityProvenance {
    schema_version: String,
    files: Vec<ProvenanceFile>,
}

const PRODUCT_ROUTE_PARITY_PROVENANCE_FILES: &[&str] = &["corpus.json", "SPEC.md"];

/// Validate committed corpus digests for `check-fixture-contracts`.
pub(crate) fn validate_product_route_parity_fixture_corpus(violations: &mut Vec<String>) {
    let root = workspace_path("fixtures/product-route-parity");
    let provenance_path = root.join("provenance.json");
    let provenance_body = match fs::read_to_string(&provenance_path) {
        Ok(body) => body,
        Err(error) => {
            violations.push(format!(
                "fixtures/product-route-parity: provenance.json is unreadable: {error}"
            ));
            return;
        }
    };
    let provenance: ProductRouteParityProvenance = match serde_json::from_str(&provenance_body) {
        Ok(provenance) => provenance,
        Err(error) => {
            violations.push(format!("fixtures/product-route-parity: {error}"));
            return;
        }
    };
    if provenance.schema_version != "ripr.product_route_parity_provenance.v1" {
        violations.push(format!(
            "fixtures/product-route-parity: unsupported provenance schema `{}`",
            provenance.schema_version
        ));
    }
    let mut manifest_paths: BTreeSet<String> = BTreeSet::new();
    for file in &provenance.files {
        if !manifest_paths.insert(file.path.clone()) {
            violations.push(format!(
                "fixtures/product-route-parity: duplicate provenance entry for `{}`",
                file.path
            ));
        }
    }
    for required in PRODUCT_ROUTE_PARITY_PROVENANCE_FILES {
        if !manifest_paths.contains(*required) {
            violations.push(format!(
                "fixtures/product-route-parity: provenance.json must bind `{required}`"
            ));
        }
    }
    for file in &provenance.files {
        let path = root.join(&file.path);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                violations.push(format!(
                    "fixtures/product-route-parity: {} is unreadable: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if file.sha256 != crate::blind_journey::sha256_hex(&bytes) {
            violations.push(format!(
                "fixtures/product-route-parity: {} digest drifted: recorded `{}`, recomputed `{}`",
                path.display(),
                file.sha256,
                crate::blind_journey::sha256_hex(&bytes)
            ));
        }
    }
    if let Err(error) = run_product_route_parity_check(&root.join("corpus.json")) {
        violations.push(format!(
            "fixtures/product-route-parity: corpus check failed: {error}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use repo_policy::product_gate_plan::ProductGateId;

    #[test]
    fn committed_corpus_passes() -> Result<(), String> {
        let report =
            run_product_route_parity_check(&workspace_path(DEFAULT_PRODUCT_ROUTE_PARITY_CORPUS))?;
        assert_eq!(report.proof_equivalence_verdict, "not_proven");
        assert!(
            report
                .subjects
                .iter()
                .flat_map(|subject| &subject.mismatches)
                .all(|mismatch| mismatch.owner != MismatchOwner::ExpectedInfrastructureDifference)
        );
        Ok(())
    }

    #[test]
    fn canonical_atomic_and_composite_terminals_reach_projection() -> Result<(), String> {
        struct TempCorpus(std::path::PathBuf);
        impl Drop for TempCorpus {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let body = fs::read_to_string(workspace_path(DEFAULT_PRODUCT_ROUTE_PARITY_CORPUS))
            .map_err(|error| error.to_string())?;
        let mut corpus: serde_json::Value =
            serde_json::from_str(&body).map_err(|error| error.to_string())?;
        let terminals = corpus["subjects"][0]["terminals"]
            .as_object_mut()
            .ok_or("missing terminal object")?;
        terminals.insert(
            ProductGateId::Formatting.as_str().to_string(),
            serde_json::json!("fail"),
        );
        terminals.insert(
            ProductGateId::PrEvidence.as_str().to_string(),
            serde_json::json!("unavailable"),
        );
        terminals.remove(ProductGateId::WorkspaceTests.as_str());
        let temp = TempCorpus(crate::tests::temp_dir("product-route-parity-loader"));
        let path = temp.0.join("corpus.json");
        fs::write(
            &path,
            serde_json::to_vec(&corpus).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let loaded = load_corpus(&path)?;
        let spec = loaded.subjects.first().ok_or("missing loaded subject")?;
        let subject = subject_from_spec(&spec.subject)?;
        let observation = build_route_observation(
            RouteKind::OrdinarySwarm,
            &subject,
            "runner",
            false,
            &spec.terminals,
        );
        for (gate, expected) in [
            (ProductGateId::Formatting, GateTerminalState::Fail),
            (ProductGateId::PrEvidence, GateTerminalState::Unavailable),
            (ProductGateId::WorkspaceTests, GateTerminalState::NotProven),
        ] {
            let row = observation
                .propositions
                .iter()
                .find(|row| row.gate_id == gate.as_str())
                .ok_or("missing gate")?;
            assert_eq!(row.terminal_state, expected);
        }
        Ok(())
    }

    #[test]
    fn applicability_difference_is_not_infrastructure() -> Result<(), String> {
        let subject = ProductGateSubject {
            changed_surfaces: [ProductSurface::Rust].into_iter().collect(),
            selectors_authoritative: true,
            trust_class: ProductGateTrustClass::Repository,
        };
        let ordinary = build_route_observation(
            RouteKind::OrdinarySwarm,
            &subject,
            "a",
            false,
            &BTreeMap::new(),
        );
        let reusable = build_route_observation(
            RouteKind::ReusableExternalTree,
            &subject,
            "b",
            false,
            &BTreeMap::new(),
        );
        let (parity, mismatches, owner) =
            compare_route_observations(&subject, &ordinary, &reusable)?;
        assert!(!parity);
        assert_eq!(owner, Some(MismatchOwner::NotProven));
        assert!(!mismatches.is_empty());
        assert!(
            mismatches
                .iter()
                .all(|mismatch| mismatch.owner == MismatchOwner::NotProven)
        );
        Ok(())
    }

    #[test]
    fn applicability_disagreement_does_not_hide_a_false_pass() {
        let ordinary = GateProposition {
            gate_id: "gate".to_string(),
            selection: ProductGateSelection::Selected,
            command: "test".to_string(),
            claim: "claim".to_string(),
            non_claim: "non-claim".to_string(),
            composite_projection: None,
            role: ProductGateRole::Required,
            terminal_state: GateTerminalState::Unavailable,
        };
        let mut reusable = ordinary.clone();
        reusable.selection = ProductGateSelection::NoOp;
        reusable.terminal_state = GateTerminalState::Pass;
        assert_eq!(
            classify_mismatch(&ordinary, &reusable),
            MismatchOwner::ReusableRouteDefect
        );
        assert_eq!(
            classify_mismatch(&reusable, &ordinary),
            MismatchOwner::OrdinaryRouteDefect
        );
    }

    #[test]
    fn empty_and_duplicate_observations_cannot_match() -> Result<(), String> {
        let subject = ProductGateSubject {
            changed_surfaces: [ProductSurface::Rust].into_iter().collect(),
            selectors_authoritative: false,
            trust_class: ProductGateTrustClass::Repository,
        };
        let ordinary = build_route_observation(
            RouteKind::OrdinarySwarm,
            &subject,
            "a",
            false,
            &BTreeMap::new(),
        );
        let reusable = build_route_observation(
            RouteKind::ReusableExternalTree,
            &subject,
            "b",
            false,
            &BTreeMap::new(),
        );
        let mut empty = ordinary.clone();
        empty.propositions.clear();
        assert!(compare_route_observations(&subject, &empty, &empty).is_err());
        assert!(compare_route_observations(&subject, &ordinary, &empty).is_err());
        let mut duplicate = ordinary.clone();
        duplicate
            .propositions
            .push(duplicate.propositions[0].clone());
        assert!(compare_route_observations(&subject, &duplicate, &reusable).is_err());
        let mut duplicate = reusable.clone();
        duplicate
            .propositions
            .push(duplicate.propositions[0].clone());
        assert!(compare_route_observations(&subject, &ordinary, &duplicate).is_err());
        let mut invalid = ordinary.clone();
        invalid.propositions[0].gate_id.clear();
        assert!(compare_route_observations(&subject, &invalid, &reusable).is_err());
        let mut ordinary_missing = ordinary.clone();
        let mut reusable_missing = reusable.clone();
        ordinary_missing
            .propositions
            .retain(|row| row.gate_id != ProductGateId::Clippy.as_str());
        reusable_missing
            .propositions
            .retain(|row| row.gate_id != ProductGateId::Clippy.as_str());
        assert!(
            compare_route_observations(&subject, &ordinary_missing, &reusable_missing).is_err()
        );
        let mut ordinary_extra = ordinary.clone();
        let mut reusable_extra = reusable.clone();
        let mut extra = ordinary_extra.propositions[0].clone();
        extra.gate_id = "product.unknown.required".to_string();
        ordinary_extra.propositions.push(extra.clone());
        reusable_extra.propositions.push(extra);
        let (parity, mismatches, _) =
            compare_route_observations(&subject, &ordinary_extra, &reusable_extra)?;
        assert!(!parity);
        assert!(
            mismatches
                .iter()
                .any(|m| m.gate_id == "product.unknown.required"
                    && m.owner == MismatchOwner::CanonicalPlanDefect)
        );
        let (_, mismatches, _) = compare_route_observations(&subject, &ordinary_extra, &reusable)?;
        let missing = mismatches
            .iter()
            .find(|m| m.gate_id == "product.unknown.required")
            .ok_or("unknown gate must be reported")?;
        assert_eq!(
            missing.reusable.terminal_state,
            GateTerminalState::NotProven
        );
        assert!(missing.reusable.command.is_empty());
        Ok(())
    }

    #[test]
    fn omitted_gate_is_a_defect() -> Result<(), String> {
        let subject = ProductGateSubject {
            changed_surfaces: [ProductSurface::Rust].into_iter().collect(),
            selectors_authoritative: false,
            trust_class: ProductGateTrustClass::Repository,
        };
        let ordinary = build_route_observation(
            RouteKind::OrdinarySwarm,
            &subject,
            "ordinary",
            false,
            &BTreeMap::new(),
        );
        let mut reusable = build_route_observation(
            RouteKind::ReusableExternalTree,
            &subject,
            "reusable",
            false,
            &BTreeMap::new(),
        );
        reusable
            .propositions
            .retain(|row| row.gate_id != ProductGateId::Formatting.as_str());
        let (parity, mismatches, _) = compare_route_observations(&subject, &ordinary, &reusable)?;
        assert!(!parity);
        assert!(
            mismatches
                .iter()
                .any(|m| m.owner == MismatchOwner::ReusableRouteDefect)
        );
        Ok(())
    }

    #[test]
    fn skipped_as_pass_is_a_defect() -> Result<(), String> {
        let subject = ProductGateSubject {
            changed_surfaces: [ProductSurface::Rust].into_iter().collect(),
            selectors_authoritative: false,
            trust_class: ProductGateTrustClass::Repository,
        };
        let mut terminals = BTreeMap::new();
        terminals.insert(
            ProductGateId::Clippy.as_str().to_string(),
            GateTerminalState::Pass,
        );
        let ordinary = build_route_observation(
            RouteKind::OrdinarySwarm,
            &subject,
            "ordinary",
            false,
            &BTreeMap::new(),
        );
        let reusable = build_route_observation(
            RouteKind::ReusableExternalTree,
            &subject,
            "reusable",
            false,
            &terminals,
        );
        let (parity, mismatches, owner) =
            compare_route_observations(&subject, &ordinary, &reusable)?;
        assert!(!parity);
        assert_eq!(owner, Some(MismatchOwner::ReusableRouteDefect));
        assert!(
            mismatches
                .iter()
                .any(|m| m.owner == MismatchOwner::ReusableRouteDefect)
        );
        Ok(())
    }

    #[test]
    fn public_fork_rejects_trusted_runner_label() {
        let subject = ProductGateSubject {
            changed_surfaces: [ProductSurface::Rust].into_iter().collect(),
            selectors_authoritative: false,
            trust_class: ProductGateTrustClass::Repository,
        };
        let reusable = build_route_observation(
            RouteKind::ReusableExternalTree,
            &subject,
            "contabo-self-hosted",
            true,
            &BTreeMap::new(),
        );
        assert!(verify_public_fork_trust(&reusable).is_err());
    }

    #[test]
    fn ordinary_workflow_declarations_expose_unmapped_steps_and_mutations() -> Result<(), String> {
        let workflow = include_str!("../../.github/workflows/rust-gates.yml");
        let audit = audit_ordinary_workflow_declarations(workflow)?;
        let expected_atomic: BTreeSet<_> = [
            "product.rust.formatting",
            "product.rust.all_features_check",
            "product.rust.clippy",
            "product.rust.workspace_tests",
            "product.rust.workspace_doc_tests",
            "product.repository.precommit",
            "product.evidence.promotion_honesty",
            "product.repository.agent_skills",
            "product.repository.dependencies",
            "product.repository.process_policy",
            "product.repository.network_policy",
            "product.evidence.goldens",
            "product.evidence.fixtures",
            "product.evidence.capability_producers",
            "product.evidence.capability_docs",
            "product.evidence.verdict_corpora",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(
            audit
                .canonical_commands_found_once
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>(),
            expected_atomic
        );
        assert_eq!(
            audit.composite_owners_not_validated,
            [
                "product.repository.base_user_text",
                "product.evidence.pr_evidence",
                "product.evidence.schema_producer_sweep"
            ]
        );
        assert!(audit.canonical_commands_missing.is_empty());
        assert!(audit.canonical_commands_ambiguous.is_empty());
        assert_eq!(
            audit.canonical_commands_without_summary_binding,
            ["product.rust.workspace_doc_tests"]
        );
        audit.require_canonical_commands()?;
        for step in ["pr_evidence", "schema_producer_sweep"] {
            assert!(
                audit
                    .summarized_steps_without_canonical_command
                    .contains(&step.to_string()),
                "{step}"
            );
        }
        let removed = workflow.replacen("run: cargo fmt --check", "run: echo missing", 1);
        assert_ne!(removed, workflow);
        let audit = audit_ordinary_workflow_declarations(&removed)?;
        assert!(
            audit
                .canonical_commands_missing
                .contains(&"product.rust.formatting".to_string())
        );
        assert!(audit.require_canonical_commands().is_err());
        let duplicated = workflow.replacen(
            "run: cargo fmt --check",
            "run: |\n          cargo fmt --check\n          cargo fmt --check",
            1,
        );
        assert_ne!(duplicated, workflow);
        let audit = audit_ordinary_workflow_declarations(&duplicated)?;
        assert!(
            audit
                .canonical_commands_ambiguous
                .contains(&"product.rust.formatting".to_string())
        );
        assert!(audit.require_canonical_commands().is_err());
        let extra = workflow.replacen("      - name: Write gate summary", "      - name: New product step\n        id: new_product_step\n        run: cargo xtask check-new-product\n\n      - name: Write gate summary", 1)
            .replacen("          FORMATTING_RESULT: ${{ steps.formatting.outcome }}", "          EXTRA_RESULT: ${{ steps.new_product_step.outcome }}\n          FORMATTING_RESULT: ${{ steps.formatting.outcome }}", 1);
        assert_ne!(extra, workflow);
        let audit = audit_ordinary_workflow_declarations(&extra)?;
        assert!(
            audit
                .summarized_steps_without_canonical_command
                .contains(&"new_product_step".to_string())
        );
        Ok(())
    }

    #[test]
    fn composite_projection_detects_changed_order_and_artifact_edge() -> Result<(), String> {
        let subject = ProductGateSubject {
            changed_surfaces: [ProductSurface::Rust].into_iter().collect(),
            selectors_authoritative: false,
            trust_class: ProductGateTrustClass::Repository,
        };
        let ordinary = build_route_observation(
            RouteKind::OrdinarySwarm,
            &subject,
            "a",
            false,
            &BTreeMap::new(),
        );
        let reusable = build_route_observation(
            RouteKind::ReusableExternalTree,
            &subject,
            "b",
            false,
            &BTreeMap::new(),
        );
        let pr = reusable
            .propositions
            .iter()
            .find(|row| row.gate_id == ProductGateId::PrEvidence.as_str())
            .ok_or("PR evidence row missing")?;
        assert_eq!(
            pr.composite_projection
                .as_ref()
                .ok_or("composite missing")?
                .events[0]
                .commands
                .len(),
            10
        );
        let mut changed = reusable.clone();
        let pr = changed
            .propositions
            .iter_mut()
            .find(|row| row.gate_id == ProductGateId::PrEvidence.as_str())
            .ok_or("PR evidence row missing")?;
        pr.composite_projection
            .as_mut()
            .ok_or("composite missing")?
            .events[0]
            .commands
            .swap(0, 1);
        let (matches, mismatches, _) = compare_route_observations(&subject, &ordinary, &changed)?;
        assert!(!matches);
        assert!(
            mismatches
                .iter()
                .any(|row| row.gate_id == ProductGateId::PrEvidence.as_str()
                    && row.owner == MismatchOwner::CanonicalPlanDefect)
        );
        let mut ordinary_without_model = ordinary.clone();
        let mut reusable_without_model = reusable.clone();
        for observation in [&mut ordinary_without_model, &mut reusable_without_model] {
            observation
                .propositions
                .iter_mut()
                .find(|row| row.gate_id == ProductGateId::PrEvidence.as_str())
                .ok_or("PR evidence row missing")?
                .composite_projection = None;
        }
        assert!(
            compare_route_observations(&subject, &ordinary_without_model, &reusable_without_model)
                .is_err()
        );
        let mut changed = reusable.clone();
        let schema = changed
            .propositions
            .iter_mut()
            .find(|row| row.gate_id == ProductGateId::SchemaProducerSweep.as_str())
            .ok_or("schema row missing")?;
        schema
            .composite_projection
            .as_mut()
            .ok_or("composite missing")?
            .events[0]
            .artifact_edges[0]
            .path = "other.json".to_string();
        let (matches, mismatches, _) = compare_route_observations(&subject, &ordinary, &changed)?;
        assert!(!matches);
        assert!(mismatches.iter().any(|row| row.gate_id
            == ProductGateId::SchemaProducerSweep.as_str()
            && row.owner == MismatchOwner::CanonicalPlanDefect));
        let mut changed = reusable.clone();
        changed.propositions[0].non_claim = "altered boundary".to_string();
        let (matches, mismatches, _) = compare_route_observations(&subject, &ordinary, &changed)?;
        assert!(!matches);
        assert!(
            mismatches
                .iter()
                .any(|row| row.owner == MismatchOwner::CanonicalPlanDefect)
        );
        Ok(())
    }
}
