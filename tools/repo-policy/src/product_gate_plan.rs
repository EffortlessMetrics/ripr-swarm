//! Typed product-gate authority for ordinary PR qualification.
//!
//! This module deliberately does not execute gates or select runners. It owns
//! the proposition, applicability, and claim boundary so those meanings can
//! later be consumed by several routes without deriving product truth from
//! workflow YAML.

#![allow(
    dead_code,
    reason = "canonical authority is introduced before workflow consumers migrate"
)]

use std::collections::BTreeSet;

pub mod composites;
use composites::CompositeProducer;

/// The selected #3825 runner contract. Nextest owns compiled lib, bin,
/// integration, and example test binaries; Cargo and rustdoc own doctests,
/// which nextest cannot execute. Both rows are required, both run with default
/// features, and neither stands in for the other. Non-default features are
/// owned outside the required lane (Test Analytics all-features telemetry, the
/// `lang-perl` job, and the Windows advisory feature matrix).
pub const TEST_RUNNER_CONTRACT: &str = "canonical_nextest_plus_cargo_doc";

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProductGateId {
    Formatting,
    AllFeaturesCheck,
    Clippy,
    WorkspaceTests,
    WorkspaceDocTests,
    Precommit,
    EvidencePromotionHonesty,
    AgentSkills,
    Dependencies,
    ProcessPolicy,
    NetworkPolicy,
    Goldens,
    Fixtures,
    CapabilityProducers,
    CapabilityDocs,
    VerdictCorpora,
    BaseUserText,
    PrEvidence,
    SchemaProducerSweep,
}

impl ProductGateId {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Formatting => "product.rust.formatting",
            Self::AllFeaturesCheck => "product.rust.all_features_check",
            Self::Clippy => "product.rust.clippy",
            Self::WorkspaceTests => "product.rust.workspace_tests",
            Self::WorkspaceDocTests => "product.rust.workspace_doc_tests",
            Self::Precommit => "product.repository.precommit",
            Self::EvidencePromotionHonesty => "product.evidence.promotion_honesty",
            Self::AgentSkills => "product.repository.agent_skills",
            Self::Dependencies => "product.repository.dependencies",
            Self::ProcessPolicy => "product.repository.process_policy",
            Self::NetworkPolicy => "product.repository.network_policy",
            Self::Goldens => "product.evidence.goldens",
            Self::Fixtures => "product.evidence.fixtures",
            Self::CapabilityProducers => "product.evidence.capability_producers",
            Self::CapabilityDocs => "product.evidence.capability_docs",
            Self::VerdictCorpora => "product.evidence.verdict_corpora",
            Self::BaseUserText => "product.repository.base_user_text",
            Self::PrEvidence => "product.evidence.pr_evidence",
            Self::SchemaProducerSweep => "product.evidence.schema_producer_sweep",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProductSurface {
    Rust,
    RepositoryPolicy,
    Evidence,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductGateRole {
    Required,
    Advisory,
    Scheduled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductGateSelection {
    Selected,
    NoOp,
    Quarantined,
    NotProven,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductGateTrustClass {
    Repository,
    ExternalTree,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductGateDefinition {
    pub id: ProductGateId,
    pub role: ProductGateRole,
    pub surfaces: BTreeSet<ProductSurface>,
    pub command: &'static str,
    pub claim: &'static str,
    pub non_claim: &'static str,
    /// Ordered composite semantics, not independent per-command receipts.
    pub composite: Option<CompositeProducer>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductGateRow {
    pub definition: ProductGateDefinition,
    pub selection: ProductGateSelection,
    pub reason: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductGateSubject {
    pub changed_surfaces: BTreeSet<ProductSurface>,
    pub selectors_authoritative: bool,
    pub trust_class: ProductGateTrustClass,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductGatePlan {
    pub rows: Vec<ProductGateRow>,
    pub full_route_reason: Option<&'static str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductGateParity {
    pub aligned: bool,
    pub plan_gate_ids: BTreeSet<String>,
    pub producer_gate_ids: BTreeSet<String>,
    pub duplicate_producer_gate_ids: BTreeSet<String>,
    pub missing_from_producer: BTreeSet<String>,
    pub unrepresented_producer_gates: BTreeSet<String>,
}

impl ProductGateParity {
    pub fn compare<I, S>(producer_gate_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let plan_gate_ids: BTreeSet<String> = product_gate_definitions()
            .into_iter()
            .map(|gate| gate.id.as_str().to_owned())
            .collect();
        let mut producers = BTreeSet::new();
        let mut duplicate_producer_gate_ids = BTreeSet::new();
        for id in producer_gate_ids {
            let id = id.as_ref().to_owned();
            if !producers.insert(id.clone()) {
                duplicate_producer_gate_ids.insert(id);
            }
        }
        let producer_gate_ids = producers;
        let missing_from_producer = plan_gate_ids
            .difference(&producer_gate_ids)
            .cloned()
            .collect::<BTreeSet<_>>();
        let unrepresented_producer_gates = producer_gate_ids
            .difference(&plan_gate_ids)
            .cloned()
            .collect::<BTreeSet<_>>();

        Self {
            aligned: missing_from_producer.is_empty()
                && unrepresented_producer_gates.is_empty()
                && duplicate_producer_gate_ids.is_empty(),
            plan_gate_ids,
            producer_gate_ids,
            duplicate_producer_gate_ids,
            missing_from_producer,
            unrepresented_producer_gates,
        }
    }
}

impl ProductGatePlan {
    pub fn for_subject(subject: &ProductGateSubject) -> Self {
        let definitions = product_gate_definitions();
        let full_route_reason = if !subject.selectors_authoritative {
            Some("selector authority is missing; the complete product route is required")
        } else if subject.trust_class == ProductGateTrustClass::ExternalTree {
            Some(
                "external-tree applicability is not yet demonstrated; the complete product route is required",
            )
        } else {
            None
        };

        let rows = definitions
            .into_iter()
            .map(|definition| {
                let selection = if full_route_reason.is_some()
                    || definition
                        .surfaces
                        .iter()
                        .any(|surface| subject.changed_surfaces.contains(surface))
                {
                    ProductGateSelection::Selected
                } else {
                    ProductGateSelection::NoOp
                };
                ProductGateRow {
                    definition,
                    selection,
                    reason: if selection == ProductGateSelection::Selected {
                        full_route_reason.unwrap_or("changed surface is applicable")
                    } else {
                        "no changed surface is applicable"
                    },
                }
            })
            .collect();

        Self {
            rows,
            full_route_reason,
        }
    }
}

pub fn product_gate_definitions() -> Vec<ProductGateDefinition> {
    let mut rust = BTreeSet::new();
    rust.insert(ProductSurface::Rust);
    let mut policy = BTreeSet::new();
    policy.insert(ProductSurface::RepositoryPolicy);
    let mut evidence = BTreeSet::new();
    evidence.insert(ProductSurface::Evidence);

    vec![
        gate(
            ProductGateId::Formatting,
            ProductGateRole::Required,
            rust.clone(),
            "cargo fmt --check",
            "formatting is accepted as repository product input",
            "does not prove runtime behavior",
        ),
        gate(
            ProductGateId::AllFeaturesCheck,
            ProductGateRole::Required,
            rust.clone(),
            "cargo check -p ripr --all-targets --all-features",
            "feature-gated ripr targets, including tests, type-check",
            "does not run or lint feature-gated tests",
        ),
        gate(
            ProductGateId::Clippy,
            ProductGateRole::Required,
            rust.clone(),
            "cargo clippy --workspace --all-targets -- -D warnings",
            "the workspace type-checks (the same unit set `cargo check --workspace --all-targets` builds) and the configured lint contract holds",
            "does not prove runtime behavior or tests observing changed behavior",
        ),
        gate(
            ProductGateId::WorkspaceTests,
            ProductGateRole::Required,
            rust.clone(),
            "cargo nextest run --workspace --profile ci",
            "the default-feature workspace test binaries selected by nextest's unfiltered `ci` profile pass, and a fresh JUnit report names at least one of them",
            "does not execute Rust doctests or non-default-feature tests such as `lang-perl`, and does not prove mutation resistance",
        ),
        gate(
            ProductGateId::WorkspaceDocTests,
            ProductGateRole::Required,
            rust.clone(),
            "cargo test --workspace --doc",
            "the default-feature workspace Rust doctests compile and pass under Cargo and rustdoc",
            "does not replace nextest coverage of compiled test binaries",
        ),
        gate(
            ProductGateId::Precommit,
            ProductGateRole::Required,
            policy.clone(),
            "cargo xtask ci-policy",
            "repository policy, complete change-set and whitespace invariants hold; selected Clippy remains owned by the explicit workspace lint producer",
            "does not replace product tests",
        ),
        gate(
            ProductGateId::EvidencePromotionHonesty,
            ProductGateRole::Required,
            evidence.clone(),
            "cargo xtask check-evidence-promotion-honesty",
            "evidence promotion claims obey their contract",
            "does not establish the underlying evidence itself",
        ),
        gate(
            ProductGateId::AgentSkills,
            ProductGateRole::Required,
            policy.clone(),
            "cargo policy preflight",
            "workflow policy and checked-in agent skills obey repository policy before the product build",
            "does not validate provider execution",
        ),
        gate(
            ProductGateId::Dependencies,
            ProductGateRole::Required,
            policy.clone(),
            "cargo xtask check-dependencies",
            "dependency policy holds",
            "does not prove dependency behavior",
        ),
        gate(
            ProductGateId::ProcessPolicy,
            ProductGateRole::Required,
            policy.clone(),
            "cargo xtask check-process-policy",
            "process policy holds",
            "does not authorize arbitrary processes",
        ),
        gate(
            ProductGateId::NetworkPolicy,
            ProductGateRole::Required,
            policy.clone(),
            "cargo xtask check-network-policy",
            "network policy holds",
            "does not prove network availability",
        ),
        gate(
            ProductGateId::Goldens,
            ProductGateRole::Required,
            evidence.clone(),
            "cargo xtask goldens check",
            "golden output contracts hold",
            "does not prove unrepresented behavior",
        ),
        gate(
            ProductGateId::Fixtures,
            ProductGateRole::Required,
            evidence.clone(),
            "cargo xtask fixtures",
            "fixture contracts hold",
            "does not prove live-repository behavior",
        ),
        gate(
            ProductGateId::CapabilityProducers,
            ProductGateRole::Required,
            evidence.clone(),
            "cargo xtask check-capability-producers",
            "the capability producer inventory matches the binary registry",
            "does not establish the documented registry block",
        ),
        gate(
            ProductGateId::CapabilityDocs,
            ProductGateRole::Required,
            evidence.clone(),
            "cargo xtask capability-docs --check",
            "the capability registry documentation matches the producer",
            "does not substitute for producer inventory or runtime tests",
        ),
        gate(
            ProductGateId::VerdictCorpora,
            ProductGateRole::Required,
            evidence,
            "cargo xtask verdict-corpus check-all",
            "all declared verdict corpora retain their expected scores",
            "does not prove behavior outside the declared corpora",
        ),
        composite_gate(
            ProductGateId::BaseUserText,
            CompositeProducer::BaseUserText,
            "cargo xtask check-user-text --tree \"$GITHUB_WORKSPACE\"",
            "the unchanged first-parent matcher judges the candidate tree when cross-tree execution is supported, after only the exact pinned compiler recovery if needed",
            "legacy matcher-pin-only success does not establish cross-tree execution",
        ),
        composite_gate(
            ProductGateId::PrEvidence,
            CompositeProducer::PrEvidence,
            composites::PR_COMMANDS[0],
            "all ten ordered PR-evidence commands complete for the admitted comparison with same-run producer inputs and one owning step result",
            "does not create ten independent receipts or authenticate supplied artifacts; merge-group comparison is B to G, retained PR comparison is B to H",
        ),
        composite_gate(
            ProductGateId::SchemaProducerSweep,
            CompositeProducer::SchemaSweep,
            "cargo xtask schema-producer-sweep \"${artifacts[@]}\"",
            "the schema sweep completes after fresh doctor generation and consumes this run's PR evidence and review comments on PR or merge-group events",
            "does not qualify omitted producer-only schema rows or turn event-dependent inputs into unconditional evidence",
        ),
    ]
}

fn gate(
    id: ProductGateId,
    role: ProductGateRole,
    surfaces: BTreeSet<ProductSurface>,
    command: &'static str,
    claim: &'static str,
    non_claim: &'static str,
) -> ProductGateDefinition {
    ProductGateDefinition {
        id,
        role,
        surfaces,
        command,
        claim,
        non_claim,
        composite: None,
    }
}

fn composite_gate(
    id: ProductGateId,
    composite: CompositeProducer,
    command: &'static str,
    claim: &'static str,
    non_claim: &'static str,
) -> ProductGateDefinition {
    // The old three-surface selector cannot establish narrower composite
    // applicability. Keep all declared surfaces; no workflow consumes NoOp.
    ProductGateDefinition {
        id,
        role: ProductGateRole::Required,
        surfaces: [
            ProductSurface::Rust,
            ProductSurface::RepositoryPolicy,
            ProductSurface::Evidence,
        ]
        .into_iter()
        .collect(),
        command,
        claim,
        non_claim,
        composite: Some(composite),
    }
}

#[cfg(test)]
mod independent_inventory_tests {
    use super::*;

    // This independent test inventory is anchored to actual workflow steps by
    // rust_gate_workflow_contract, which tests their commands and failure shape.
    // It is deliberately a partial inventory, not proof of composite steps or
    // of the other callable families.
    const WORKFLOW_CONTRACT: &str =
        include_str!("../../../xtask/tests/rust_gate_workflow_contract.rs");

    fn contract_commands() -> Result<Vec<&'static str>, &'static str> {
        let body = WORKFLOW_CONTRACT
            .split_once("const REQUIRED_GATES:")
            .ok_or("independent required gate inventory is missing")?
            .1
            .split_once("\n];")
            .ok_or("independent required gate inventory is incomplete")?
            .0;
        let commands: Vec<_> = body
            .split('"')
            .filter(|part| part.starts_with("cargo "))
            .collect();
        assert_eq!(
            commands.len(),
            15,
            "independent inventory parser/denominator changed"
        );
        assert_eq!(
            commands.iter().collect::<BTreeSet<_>>().len(),
            commands.len(),
            "duplicate commands in independent inventory"
        );
        Ok(commands)
    }

    fn missing_commands<'a>(producer: impl IntoIterator<Item = &'a str>) -> Vec<String> {
        let planned: BTreeSet<_> = product_gate_definitions()
            .into_iter()
            .map(|row| row.command)
            .collect();
        producer
            .into_iter()
            .filter(|command| !planned.contains(command))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn typed_plan_accounts_for_independent_required_gate_inventory() -> Result<(), &'static str> {
        let missing = missing_commands(contract_commands()?);
        assert!(
            missing.is_empty(),
            "required workflow commands absent from plan: {missing:?}"
        );
        Ok(())
    }

    #[test]
    fn unrepresented_command_is_rejected() {
        assert_eq!(
            missing_commands(["cargo hypothetical-new-required-producer"]),
            ["cargo hypothetical-new-required-producer"]
        );
    }

    #[test]
    fn parity_accepts_unknown_producer_ids_and_reports_them() {
        let producer: BTreeSet<_> = product_gate_definitions()
            .into_iter()
            .map(|gate| gate.id.as_str())
            .collect();
        let mut dynamic: Vec<String> = producer.iter().map(|id| (*id).to_owned()).collect();
        dynamic.push(format!("product.unknown.{}", "new_required_producer"));
        let parity = ProductGateParity::compare(dynamic.iter().map(String::as_str));
        assert!(!parity.aligned);
        assert_eq!(
            parity.unrepresented_producer_gates,
            BTreeSet::from(["product.unknown.new_required_producer".to_owned()])
        );
        assert!(parity.missing_from_producer.is_empty());
    }

    #[test]
    fn parity_reports_a_missing_producer_without_renaming_it() {
        let mut producer: BTreeSet<_> = product_gate_definitions()
            .into_iter()
            .map(|gate| gate.id.as_str())
            .collect();
        assert!(producer.remove("product.evidence.verdict_corpora"));
        let parity = ProductGateParity::compare(producer);
        assert!(!parity.aligned);
        assert_eq!(
            parity.missing_from_producer,
            BTreeSet::from(["product.evidence.verdict_corpora".to_owned()])
        );
        assert!(parity.unrepresented_producer_gates.is_empty());
    }

    #[test]
    fn parity_rejects_duplicate_producer_ids() {
        let mut producer: Vec<_> = product_gate_definitions()
            .into_iter()
            .map(|gate| gate.id.as_str())
            .collect();
        producer.push("product.evidence.verdict_corpora");
        let parity = ProductGateParity::compare(producer);
        assert!(!parity.aligned);
        assert_eq!(
            parity.duplicate_producer_gate_ids,
            BTreeSet::from(["product.evidence.verdict_corpora".to_owned()])
        );
        assert!(parity.missing_from_producer.is_empty());
        assert!(parity.unrepresented_producer_gates.is_empty());
    }
}
