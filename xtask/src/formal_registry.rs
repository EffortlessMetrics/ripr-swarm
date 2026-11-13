//! Governed critical claims and a passive assurance projection (#14473).
//!
//! This module launches no command. Source bytes, supplied native observations,
//! local byte custody, and provider authentication remain separate authorities.

use crate::formal_evidence::native::{self, SourceManifest};
use crate::formal_evidence::{self as formal, AuditResult, Claim, Run};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod control_pair;
mod prepare;
mod scoreboard;
mod transition;

pub use control_pair::{ControlObservation, NativeControlPair, RetainedRun, validate_control_pair};
pub use prepare::{prepare, validate_prepared, validate_registry};
pub use scoreboard::{
    Scoreboard, claim_index, render_markdown, scoreboard, scoreboard_with_control_pairs,
};
pub use transition::{Transition, compare_predecessor};

pub const REGISTRY_PATH: &str = ".ripr/formal-claims.json";
pub const USAGE: &str = "formal-assurance --root DIR [--registry FILE] [--run FILE --artifacts DIR] [--control-pair FILE] [--previous-root DIR --previous-registry FILE] [--changed-owner PATH] [--format json|markdown|claim-index] [--check-output FILE]";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CriticalFamily {
    ClassificationActionability,
    GateExitAggregation,
    CurrentnessIdentityCausality,
    ResourceBudgetSafety,
    EditRepairDisposition,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClosureRule {
    /// Entire package source trees, package/workspace manifests, Cargo config,
    /// and the shared provider/result/registry implementation. No file glob.
    KaniWorkspaceSourcesV1,
    /// Exact listed files for an explicitly manual or unsupported obligation.
    ExplicitManualV1,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Locator {
    pub file: String,
    pub symbol: String,
    /// Reviewed source anchor, checked for unique presence, not parsed as code.
    pub anchor: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CoverRequirement {
    pub id: String,
    pub file: String,
    pub anchor: String,
    /// Exact pinned native presentation, qualified by a retained format sample.
    /// An absent mapping is an explicit registration gap, never a count match.
    #[serde(default)]
    pub native: Option<NativeCoverExpectation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct NativeCoverExpectation {
    /// Exact shared-parser value, including native surrounding quotes.
    pub description: String,
    pub file: String,
    pub function: String,
    pub line: u64,
    pub column: u64,
}

impl NativeCoverExpectation {
    fn location(&self) -> String {
        format!(
            "{}:{}:{} in function {}",
            self.file, self.line, self.column, self.function
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NegativeControl {
    pub id: String,
    pub file: String,
    pub before: String,
    pub after: String,
    pub expected_failed_claim: String,
    pub expected_subject: String,
    pub effect: String,
    /// Exact failed native assertions, qualified by a retained format sample.
    /// Empty means no paired native admission is supported for this control.
    #[serde(default)]
    pub expected_failures: Vec<NativeAssertionExpectation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct NativeAssertionExpectation {
    pub property: String,
    pub presentation: NativeCoverExpectation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CorrespondenceReview {
    pub production: Vec<Locator>,
    pub verification_only_paths: Vec<String>,
    /// A reviewed mapping is a trust declaration, not authentication by text.
    pub rule: String,
    pub limitations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct KaniProfile {
    pub version: String,
    pub solver_version: String,
    pub configuration_policy: String,
    pub default_unwind: u64,
    pub target: String,
    pub compiler_trust: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registration {
    pub semantic_family: CriticalFamily,
    pub claim: Claim,
    pub closure_rule: ClosureRule,
    pub authority_paths: Vec<String>,
    pub subject: Option<Locator>,
    pub package: Option<String>,
    pub provider_profile: Option<KaniProfile>,
    pub cover_requirements: Vec<CoverRequirement>,
    pub negative_controls: Vec<NegativeControl>,
    pub correspondence_review: CorrespondenceReview,
    pub alternative_evidence_owner: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompatibleRelocation {
    pub claim_id: String,
    pub from_file: String,
    pub to_file: String,
    pub from_subject: String,
    pub to_subject: String,
    /// Both predecessor and candidate files must actually have these bytes.
    pub source_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub schema: String,
    pub generation: u64,
    pub claims: Vec<Registration>,
    pub compatible_relocations: Vec<CompatibleRelocation>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedClaim {
    pub registration: Registration,
    pub claim: Claim,
    pub semantic_family: CriticalFamily,
    pub source_manifest: SourceManifest,
    /// The shared passive Rust scanner's exact literal include/path closure.
    pub explicit_inputs: Vec<String>,
    pub source_digest: String,
    pub authority_manifest: SourceManifest,
    pub subject: Option<native::Subject>,
    pub assumptions_digest: String,
    pub bounds_digest: String,
    pub provider_profile: Option<KaniProfile>,
    pub required_cover_count: u64,
    pub correspondence_review: CorrespondenceReview,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedRegistry {
    pub schema: String,
    pub registry_generation: String,
    pub registry_path: String,
    pub registry_digest: String,
    pub registry: Registry,
    pub claims: Vec<PreparedClaim>,
}

pub fn json_bytes<T: Serialize>(value: &T) -> AuditResult<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|e| e.to_string())
}

pub fn digest_value<T: Serialize>(value: &T) -> AuditResult<String> {
    Ok(native::sha256(
        &serde_json::to_vec(value).map_err(|e| e.to_string())?,
    ))
}

fn bind_captured_registry(
    prepared: &PreparedRegistry,
    captured: &SourceManifest,
) -> AuditResult<()> {
    formal::require(
        captured.get(&prepared.registry_path) == Some(&prepared.registry_digest),
        "prepared registry bytes are absent from or differ from native source capture",
    )
}

/// Match semantic covers to exact retained native properties. The native owner
/// normalizes absolute locations using its captured repository root first.
/// Property identities and required mappings are one-to-one; no cover can be
/// borrowed from another function or replaced by an aggregate success count.
pub fn match_native_covers(
    binding: &PreparedClaim,
    properties: &[native::CoverProperty],
) -> AuditResult<formal::Covers> {
    let required = &binding.registration.cover_requirements;
    formal::require(
        !required.is_empty()
            && properties.len() == required.len()
            && binding.required_cover_count == required.len() as u64,
        "native cover inventory differs from registered obligations",
    )?;
    let mut names = BTreeSet::new();
    for property in properties {
        formal::text(&property.name, "native cover property identity")?;
        formal::require(
            names.insert(&property.name),
            "duplicate native cover property identity",
        )?;
    }
    let mut mappings = BTreeSet::new();
    let mut matched = BTreeSet::new();
    let mut covers = formal::Covers {
        required: binding.required_cover_count,
        satisfied: 0,
        unreachable: 0,
    };
    for cover in required {
        let expected = cover
            .native
            .as_ref()
            .ok_or_else(|| format!("{}: native cover mapping is not qualified", cover.id))?;
        formal::require(
            mappings.insert(expected),
            "duplicate registered native cover mapping",
        )?;
        let location = expected.location();
        let candidates: Vec<_> = properties
            .iter()
            .filter(|property| {
                property.description.as_ref() == Some(&expected.description)
                    && property.location.as_ref() == Some(&location)
            })
            .collect();
        formal::require(
            candidates.len() == 1,
            &format!(
                "{}: exact native cover description/location is missing or ambiguous",
                cover.id
            ),
        )?;
        let property = candidates[0];
        formal::require(
            matched.insert(&property.name),
            "native cover property was reused for two semantic obligations",
        )?;
        match property.status.as_str() {
            "SATISFIED" => covers.satisfied += 1,
            "UNSATISFIABLE" | "UNREACHABLE" => covers.unreachable += 1,
            _ => {
                return Err(format!(
                    "{}: native cover status is not determinate",
                    cover.id
                ));
            }
        }
    }
    Ok(covers)
}

/// Bind registered rows to a prepared registry. Captured bytes are verified by
/// the shared native custody owner before its manifest is supplied here.
/// A successful join is not an authenticated provider or transition decision.
pub fn validate_binding(
    prepared: &PreparedRegistry,
    run: &Run,
    captured: &SourceManifest,
) -> AuditResult<()> {
    validate_prepared(prepared)?;
    bind_captured_registry(prepared, captured)?;
    formal::audit(
        &prepared
            .claims
            .iter()
            .map(|p| p.claim.clone())
            .collect::<Vec<_>>(),
        Some(run),
    )?;
    formal::require(
        run.claim_registry_generation == prepared.registry_generation,
        "stale registry generation",
    )?;
    let execution = run
        .execution
        .as_ref()
        .ok_or("missing native execution custody")?;
    formal::require(
        execution.source_current && execution.configuration_admitted,
        "source/configuration not admitted",
    )?;
    for id in &run.inventory.expected_claim_ids {
        let binding = prepared
            .claims
            .iter()
            .find(|p| &p.claim.claim_id == id)
            .ok_or("unregistered native claim")?;
        let profile = binding
            .provider_profile
            .as_ref()
            .ok_or("claim has no native profile")?;
        formal::require(
            run.provider.name == "kani"
                && run.provider.version == profile.version
                && run.provider.solver_version.as_ref() == Some(&profile.solver_version)
                && run.toolchain.target.as_ref() == Some(&profile.target)
                && execution.configuration_policy == profile.configuration_policy
                && execution.default_unwind.map(u64::from) == Some(profile.default_unwind),
            "stale provider/configuration profile",
        )?;
        formal::require(
            binding.source_manifest
                == prepare::project_source(
                    &binding.registration,
                    captured,
                    &binding.explicit_inputs,
                )?,
            "stale, omitted or substituted source closure member",
        )?;
        let package = binding
            .registration
            .package
            .as_ref()
            .ok_or("registered native package absent")?;
        formal::require(
            execution.package_inputs.get(package) == Some(&binding.explicit_inputs),
            "registered explicit input closure differs from independent native capture",
        )?;
        let subject = binding
            .subject
            .as_ref()
            .ok_or("registered native subject absent")?;
        formal::require(
            execution.selected_subjects.contains(subject),
            "wrong native subject or substituted harness bytes",
        )?;
        for row in run.results.iter().filter(|row| &row.claim_id == id) {
            formal::require(
                row.claim_generation == binding.claim.claim_generation
                    && row.subject == subject.id()
                    && row.source_digest == binding.source_digest
                    && row.bounds_digest == binding.bounds_digest
                    && row.assumptions_digest.as_ref() == Some(&binding.assumptions_digest),
                "stale registered claim/source/bounds/assumptions binding",
            )?;
        }
    }
    Ok(())
}

/// Bounded regular-file reads only. No provider, shell, Git or network routes.
pub fn read_input(path: &Path) -> AuditResult<Vec<u8>> {
    prepare::read_bounded(path, 4 * 1_048_576)
}

#[cfg(test)]
mod tests;
