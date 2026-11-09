//! Inactive Rust producer graph. This extends the canonical portfolio; branch
//! placement is execution topology, never a second applicability selector.

use super::{
    ProductGateDefinition, ProductGateId, ProductGateRole, ProductSurface, gate,
    product_gate_definitions,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod command;
mod projection;
pub use command::run;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Branch {
    Preflight,
    Policy,
    BaseMatcher,
    AllFeatures,
    Clippy,
    Doctests,
    Tests,
    Corpus,
    PrSchema,
}

impl Branch {
    pub(crate) const ALL: [Self; 9] = [
        Self::Preflight,
        Self::Policy,
        Self::BaseMatcher,
        Self::AllFeatures,
        Self::Clippy,
        Self::Doctests,
        Self::Tests,
        Self::Corpus,
        Self::PrSchema,
    ];
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Preflight => "preflight",
            Self::Policy => "policy",
            Self::BaseMatcher => "base_matcher",
            Self::AllFeatures => "all_features",
            Self::Clippy => "clippy",
            Self::Doctests => "doctests",
            Self::Tests => "tests",
            Self::Corpus => "corpus",
            Self::PrSchema => "pr_schema",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        Self::ALL
            .into_iter()
            .find(|b| b.name() == value)
            .ok_or_else(|| format!("unknown Rust branch {value}"))
    }
}

pub(crate) fn definitions() -> Vec<ProductGateDefinition> {
    let mut rows = product_gate_definitions();
    let evidence = [ProductSurface::Evidence]
        .into_iter()
        .collect::<BTreeSet<_>>();
    for (id, command, claim) in [
        (
            ProductGateId::WorkspacePreparation,
            "cargo test --no-run --locked --workspace",
            "current full default workspace test targets are prepared before native policy inventory; no test success or reuse is inferred",
        ),
        (
            ProductGateId::BasePreparation,
            "git worktree add --detach \"$base_tree\" \"$base_sha\"",
            "exact first-parent matcher worktree exists before pinned recovery",
        ),
        (
            ProductGateId::BaseCompilerRecovery,
            "pinned base compiler-arity recovery with exact old/new blob checks",
            "base compiler recovery completes without changing matcher authority",
        ),
        (
            ProductGateId::BaseFormatRecovery,
            "pinned base MissedTxt recovery with exact old/new blob and changed-path checks",
            "base output match recovery completes after compiler recovery",
        ),
        (
            ProductGateId::BaseUserText,
            "cargo xtask check-user-text --tree <subject> (from exact base)",
            "candidate text is judged by the independent base matcher",
        ),
        (
            ProductGateId::CapabilityProducers,
            "cargo xtask check-capability-producers",
            "candidate capability producers agree with inventory",
        ),
        (
            ProductGateId::CapabilityDocs,
            "cargo xtask capability-docs --check",
            "candidate capability registry agrees with documentation",
        ),
        (
            ProductGateId::VerdictCorpora,
            "cargo xtask verdict-corpus check-all",
            "all declared verdict corpora satisfy their contract",
        ),
        (
            ProductGateId::PrEvidence,
            "ripr-pr, ripr-review-comments, impacted-evidence, ripr-pr-summary, ripr-annotations (produce and check)",
            "all five PR evidence producers and their paired checks pass",
        ),
        (
            ProductGateId::SchemaSweep,
            "fresh doctor plus cargo xtask schema-producer-sweep with PR/review artifacts",
            "published schemas validate committed and fresh required producer bytes",
        ),
        (
            ProductGateId::WildcardRetention,
            "upload actual wildcard calibration producer outputs",
            "required bounded calibration bytes are retained, not re-created",
        ),
    ] {
        rows.push(gate(
            id,
            ProductGateRole::Required,
            evidence.clone(),
            command,
            claim,
            "does not establish full caller acceptance or timing compliance",
        ));
    }
    rows
}

impl ProductGateId {
    pub(crate) fn branch(self) -> Branch {
        match self {
            Self::Dependencies | Self::ProcessPolicy | Self::NetworkPolicy => Branch::Policy,
            Self::BasePreparation
            | Self::BaseCompilerRecovery
            | Self::BaseFormatRecovery
            | Self::BaseUserText => Branch::BaseMatcher,
            Self::AllFeaturesCheck => Branch::AllFeatures,
            Self::Clippy => Branch::Clippy,
            Self::WorkspaceDocTests => Branch::Doctests,
            Self::WorkspaceTests | Self::WorkspacePreparation | Self::Precommit => Branch::Tests,
            Self::Formatting | Self::AgentSkills => Branch::Preflight,
            Self::EvidencePromotionHonesty
            | Self::Goldens
            | Self::Fixtures
            | Self::CapabilityProducers
            | Self::CapabilityDocs
            | Self::VerdictCorpora
            | Self::WildcardRetention => Branch::Corpus,
            Self::PrEvidence | Self::SchemaSweep => Branch::PrSchema,
        }
    }

    pub(crate) fn step_name(self) -> &'static str {
        match self {
            Self::WorkspacePreparation => {
                "Prepare default workspace before native policy inventory"
            }
            Self::Formatting => "Formatting preflight",
            Self::AllFeaturesCheck => "Strict all-features ripr Clippy",
            Self::Clippy => "Strict workspace Clippy",
            Self::WorkspaceTests => "Required Rust tests",
            Self::WorkspaceDocTests => "Required Rust doctests",
            Self::Precommit => "Repository policy preflight",
            Self::EvidencePromotionHonesty => "Evidence promotion honesty",
            Self::AgentSkills => "Agent skill contracts",
            Self::Dependencies => "Dependency policy",
            Self::ProcessPolicy => "Process policy",
            Self::NetworkPolicy => "Network policy",
            Self::Goldens => "Golden output contracts",
            Self::Fixtures => "Fixture contracts",
            Self::BasePreparation => "Prepare independent user-text base",
            Self::BaseCompilerRecovery => "Recover pinned base compiler arity if needed",
            Self::BaseFormatRecovery => "Recover pinned base missed-txt match if needed",
            Self::BaseUserText => "User-text goldens judged by the merge-base matcher",
            Self::CapabilityProducers => "Capability producer inventory",
            Self::CapabilityDocs => "Capability registry docs block",
            Self::VerdictCorpora => "Verdict corpora",
            Self::PrEvidence => "PR evidence",
            Self::SchemaSweep => "Schema producer sweep",
            Self::WildcardRetention => "Retain wildcard calibration producer outputs",
        }
    }

    pub(crate) fn step_id(self) -> &'static str {
        match self {
            Self::WorkspacePreparation => "workspace_preparation",
            Self::Formatting => "formatting",
            Self::AllFeaturesCheck => "all_features_check",
            Self::Clippy => "clippy",
            Self::WorkspaceTests => "rust-tests",
            Self::WorkspaceDocTests => "doctests",
            Self::Precommit => "precommit",
            Self::EvidencePromotionHonesty => "promotion_honesty",
            Self::AgentSkills => "agent_skills",
            Self::Dependencies => "dependencies",
            Self::ProcessPolicy => "process_policy",
            Self::NetworkPolicy => "network_policy",
            Self::Goldens => "goldens",
            Self::Fixtures => "fixtures",
            Self::BasePreparation => "user_text_base",
            Self::BaseCompilerRecovery => "user_text_recovery",
            Self::BaseFormatRecovery => "base_format_recovery",
            Self::BaseUserText => "user_text_matcher",
            Self::CapabilityProducers => "capability_producers",
            Self::CapabilityDocs => "capability_docs",
            Self::VerdictCorpora => "verdict_corpus",
            Self::PrEvidence => "pr_evidence",
            Self::SchemaSweep => "schema_producer_sweep",
            Self::WildcardRetention => "wildcard_retention",
        }
    }

    pub(crate) fn predecessors(self) -> &'static [ProductGateId] {
        match self {
            Self::BaseCompilerRecovery => &[Self::BasePreparation],
            Self::BaseFormatRecovery => &[Self::BaseCompilerRecovery],
            Self::BaseUserText => &[Self::BaseFormatRecovery],
            Self::SchemaSweep => &[Self::PrEvidence],
            Self::WildcardRetention => &[Self::Goldens],
            Self::WorkspacePreparation => &[Self::Formatting, Self::AgentSkills],
            Self::Precommit => &[Self::AgentSkills, Self::WorkspacePreparation],
            Self::WorkspaceTests => &[Self::Precommit],
            _ => &[],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Row {
    id: String,
    branch: Branch,
    step: String,
    command: String,
    predecessors: Vec<String>,
}

fn rows() -> Vec<Row> {
    definitions()
        .into_iter()
        .map(|definition| Row {
            id: definition.id.as_str().into(),
            branch: definition.id.branch(),
            step: definition.id.step_id().into(),
            command: definition.command.into(),
            predecessors: definition
                .id
                .predecessors()
                .iter()
                .map(|id| id.as_str().into())
                .collect(),
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    version: u32,
    subject: serde_json::Value,
    inventory: String,
    configuration: String,
    producer: String,
    rows: Vec<Row>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ResultRow {
    id: String,
    step: String,
    outcome: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BranchResult {
    plan: Plan,
    branch: Branch,
    rows: Vec<ResultRow>,
    elapsed_seconds: u64,
}

fn reconcile(plan: &Plan, results: &[BranchResult]) -> Result<(), String> {
    if plan.version != 1 || plan.rows != rows() {
        return Err("noncanonical/empty expected denominator".into());
    }
    let mut seen = BTreeSet::new();
    for result in results {
        if &result.plan != plan || !seen.insert(result.branch) {
            return Err("stale/substituted/duplicate branch".into());
        }
        reconcile_branch(plan, result)?;
    }
    if seen != Branch::ALL.into_iter().collect() {
        return Err("missing required branch".into());
    }
    Ok(())
}

fn reconcile_branch(plan: &Plan, result: &BranchResult) -> Result<(), String> {
    if &result.plan != plan || plan.rows != rows() || plan.version != 1 {
        return Err("stale or noncanonical branch receipt".into());
    }
    let expected: Vec<_> = plan
        .rows
        .iter()
        .filter(|row| row.branch == result.branch)
        .map(|row| ResultRow {
            id: row.id.clone(),
            step: row.step.clone(),
            outcome: "success".into(),
        })
        .collect();
    if result.rows != expected {
        return Err(format!(
            "missing/duplicate/failed/incompatible rows in {}",
            result.branch.name()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
