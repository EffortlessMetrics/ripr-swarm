//! Compare ordinary swarm and reusable external-tree product routes at the
//! proposition level (#3332, consuming #3330; live reusable execution awaits
//! #3331).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::product_gate_plan::{
    ProductGatePlan, ProductGateRole, ProductGateSelection, ProductGateSubject,
    ProductGateTrustClass, ProductSurface,
};
use crate::work_portfolio::workspace_path;

pub(crate) const PRODUCT_ROUTE_PARITY_CORPUS_SCHEMA: &str = "ripr.product_route_parity_corpus.v1";
pub(crate) const PRODUCT_ROUTE_PARITY_REPORT_SCHEMA: &str = "ripr.product_route_parity.v1";
pub(crate) const DEFAULT_PRODUCT_ROUTE_PARITY_CORPUS: &str =
    "fixtures/product-route-parity/corpus.json";

pub(crate) const PRODUCT_ROUTE_PARITY_CLAIM_BOUNDARY: &str = "Compares normalized gate \
 propositions from the typed #3330 plan for the ordinary repository route and the reusable \
 external-tree route projection; runner identity and cost stay observational only. This check \
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
    #[serde(default)]
    terminals: BTreeMap<String, GateTerminalState>,
    expect_semantic_parity: bool,
    #[serde(default)]
    expect_mismatch_owner: Option<MismatchOwner>,
    #[serde(default)]
    expect_public_fork_trust: bool,
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
    pub role: ProductGateRole,
    pub terminal_state: GateTerminalState,
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
    pub subjects: Vec<SubjectParityResult>,
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
        return *state;
    }
    match selection {
        ProductGateSelection::NoOp => GateTerminalState::NotApplicable,
        ProductGateSelection::Selected => GateTerminalState::NotProven,
        ProductGateSelection::Quarantined => GateTerminalState::NotProven,
        ProductGateSelection::NotProven => GateTerminalState::NotProven,
    }
}

pub(crate) fn build_route_observation(
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
    ordinary: &ProductGateSubject,
    ordinary_row: &GateProposition,
    reusable_row: &GateProposition,
) -> MismatchOwner {
    if ordinary_row.command != reusable_row.command || ordinary_row.role != reusable_row.role {
        return MismatchOwner::CanonicalPlanDefect;
    }
    if ordinary_row.selection != reusable_row.selection
        && ordinary.selectors_authoritative
        && reusable_row.selection == ProductGateSelection::Selected
        && ordinary_row.selection == ProductGateSelection::NoOp
    {
        return MismatchOwner::ExpectedInfrastructureDifference;
    }
    if ordinary_row.terminal_state != reusable_row.terminal_state {
        return MismatchOwner::OrdinaryRouteDefect;
    }
    MismatchOwner::NotProven
}

pub(crate) fn compare_route_observations(
    ordinary_subject: &ProductGateSubject,
    ordinary: &RouteObservation,
    reusable: &RouteObservation,
) -> (bool, Vec<PropositionMismatch>, Option<MismatchOwner>) {
    let mut by_ordinary: BTreeMap<&str, &GateProposition> = BTreeMap::new();
    for row in &ordinary.propositions {
        by_ordinary.insert(row.gate_id.as_str(), row);
    }
    let mut by_reusable: BTreeMap<&str, &GateProposition> = BTreeMap::new();
    for row in &reusable.propositions {
        by_reusable.insert(row.gate_id.as_str(), row);
    }
    let gate_ids: BTreeSet<_> = by_ordinary
        .keys()
        .chain(by_reusable.keys())
        .copied()
        .collect();
    let mut mismatches = Vec::new();
    for gate_id in gate_ids {
        let Some(ordinary_row) = by_ordinary.get(gate_id).copied() else {
            mismatches.push(PropositionMismatch {
                gate_id: gate_id.to_string(),
                ordinary: GateProposition {
                    gate_id: gate_id.to_string(),
                    selection: ProductGateSelection::NotProven,
                    command: String::new(),
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
                    role: ProductGateRole::Required,
                    terminal_state: GateTerminalState::NotProven,
                },
                owner: MismatchOwner::ReusableRouteDefect,
            });
            continue;
        };
        let semantic_match = ordinary_row.selection == reusable_row.selection
            && ordinary_row.command == reusable_row.command
            && ordinary_row.role == reusable_row.role
            && ordinary_row.terminal_state == reusable_row.terminal_state;
        if !semantic_match {
            mismatches.push(PropositionMismatch {
                gate_id: gate_id.to_string(),
                ordinary: (*ordinary_row).clone(),
                reusable: (*reusable_row).clone(),
                owner: classify_mismatch(ordinary_subject, ordinary_row, reusable_row),
            });
        }
    }
    if mismatches.is_empty() {
        return (true, mismatches, None);
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
    (false, mismatches, owner)
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
    Ok(corpus)
}

pub(crate) fn run_product_route_parity_check(
    corpus_path: &Path,
) -> Result<ProductRouteParityReport, String> {
    let corpus = load_corpus(corpus_path)?;
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
            compare_route_observations(&gate_subject, &ordinary, &reusable);
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
    let proof_equivalence_verdict = if mismatched == 0 {
        "equivalent_on_corpus".to_string()
    } else {
        "equivalent_with_classified_differences".to_string()
    };
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
            "Runner/cache/wall-clock deltas are recorded separately and do not affect semantic parity.".to_string(),
            "Live reusable workflow execution and portable receipts remain owned by #3331/#3329.".to_string(),
        ],
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
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|err| err.to_string())?
        );
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
    use crate::product_gate_plan::ProductGateId;
    use crate::product_gate_plan::ProductGateParity;

    #[test]
    fn committed_corpus_passes() -> Result<(), String> {
        run_product_route_parity_check(&workspace_path(DEFAULT_PRODUCT_ROUTE_PARITY_CORPUS))?;
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
        let (parity, mismatches, _) = compare_route_observations(&subject, &ordinary, &reusable);
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
        let (parity, _, _) = compare_route_observations(&subject, &ordinary, &reusable);
        assert!(!parity);
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
    fn ordinary_inventory_still_aligns_with_canonical_plan() {
        let producer_ids: BTreeSet<_> = crate::product_gate_plan::product_gate_definitions()
            .into_iter()
            .map(|gate| gate.id)
            .collect();
        assert!(ProductGateParity::compare(producer_ids).aligned);
    }
}
