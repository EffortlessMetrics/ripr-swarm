//! Governed formal-claim registry, deterministic planner projection, and
//! registry-only assurance scoreboard (#14473).
//!
//! This module never executes a verifier, reads Git state, authenticates
//! artifacts, or grants CI authority. Claim-document semantics stay owned by
//! `formal_evidence`; this module owns registry membership, decision
//! generations, projection, retirement, and transition validation.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::formal_evidence;

const MAX_INPUT_BYTES: u64 = 1_048_576;
const MAX_TOTAL_INPUT_BYTES: u64 = 16 * MAX_INPUT_BYTES;
const MAX_CLAIMS: usize = 256;

pub const USAGE: &str = concat!(
    "usage:\n",
    "  formal-claim-registry check --registry FILE [--format json|markdown]\n",
    "  formal-claim-registry index --registry FILE [--format json|markdown]\n",
    "  formal-claim-registry scoreboard --registry FILE [--format json|markdown]\n",
    "  formal-claim-registry transition --base-registry FILE --head-registry FILE ",
    "[--format json|markdown]\n",
    "Exit 0: deterministic registry result; 2: invalid input. ",
    "No command grants native assurance or execution authority.",
);

type RegistryResult<T> = Result<T, String>;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
enum ProviderRoute {
    Native,
    Manual,
    Unsupported,
}

impl ProviderRoute {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Manual => "manual",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestEntry {
    document: String,
    claim_id: String,
    claim_generation: u64,
    route: ProviderRoute,
    authority_paths: Vec<String>,
    expected_decision_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Ord, PartialOrd)]
#[serde(deny_unknown_fields)]
struct Retirement {
    claim_id: String,
    final_generation: u64,
    replacement_claim_id: Option<String>,
    reason: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: String,
    repository: String,
    registry_generation: u64,
    global_authority_paths: Vec<String>,
    claims: Vec<ManifestEntry>,
    retirements: Vec<Retirement>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ClaimDocument {
    schema: String,
    claim_id: String,
    claim_generation: u64,
    family: String,
    provider: String,
    criticality: String,
    phase: String,
    proposition: String,
    non_claims: Vec<String>,
    owners: Vec<String>,
    source_closure: Vec<String>,
    domain: String,
    assumptions: Vec<String>,
    trust_dependencies: Vec<String>,
    observable: String,
    bounds: BTreeMap<String, serde_json::Value>,
    subjects: Vec<String>,
    required_covers: Vec<String>,
    negative_controls: Vec<String>,
    result_satisfaction: String,
    currentness_inputs: Vec<String>,
    transition_policy: String,
    owner_spec: String,
    owner_issue: u64,
}

#[derive(Clone, Debug)]
struct LoadedClaim {
    document: String,
    route: ProviderRoute,
    authority_paths: Vec<String>,
    decision_digest: String,
    claim: ClaimDocument,
}

#[derive(Clone, Debug)]
struct LoadedRegistry {
    manifest: Manifest,
    claims: Vec<LoadedClaim>,
    registry_digest: String,
    generation: String,
}

mod load;
mod project;
mod transition;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Markdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Check,
    Index,
    Scoreboard,
    Transition,
}

fn require(condition: bool, message: impl Into<String>) -> RegistryResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn text(value: &str, field: &str) -> RegistryResult<()> {
    require(!value.trim().is_empty(), format!("{field}: blank value"))
}

fn stable_id(value: &str, field: &str) -> RegistryResult<()> {
    let mut bytes = value.bytes();
    let first = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    require(
        value.len() >= 2
            && first
            && bytes.all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
            }),
        format!("{field}: expected stable lowercase identifier"),
    )
}

fn repository_path(value: &str, field: &str) -> RegistryResult<()> {
    text(value, field)?;
    require(
        !value.starts_with('/')
            && !value.ends_with('/')
            && !value.contains('\\')
            && !value.contains(':')
            && !value.chars().any(char::is_control)
            && value
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        format!("{field}: expected repository-relative path without traversal"),
    )
}

fn digest_field(value: &str, field: &str) -> RegistryResult<()> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| format!("{field}: expected sha256 digest"))?;
    require(
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
        format!("{field}: expected 64 lowercase hex digits"),
    )
}

fn unique_strings(values: &[String], field: &str, nonempty: bool) -> RegistryResult<()> {
    require(
        !nonempty || !values.is_empty(),
        format!("{field}: empty list"),
    )?;
    let mut seen = BTreeSet::new();
    for value in values {
        text(value, field)?;
        require(
            seen.insert(value.as_str()),
            format!("{field}: duplicate value {value}"),
        )?;
    }
    Ok(())
}

fn unique_paths(values: &[String], field: &str, nonempty: bool) -> RegistryResult<()> {
    unique_strings(values, field, nonempty)?;
    for value in values {
        repository_path(value, field)?;
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn digest_json<T: Serialize>(value: &T) -> RegistryResult<String> {
    serde_json::to_vec(value)
        .map(|bytes| sha256(&bytes))
        .map_err(|error| error.to_string())
}

fn sorted(values: &[String]) -> Vec<String> {
    let mut values = values.to_vec();
    values.sort();
    values
}

fn canonical_decision(
    claim: &ClaimDocument,
    document: &str,
    route: ProviderRoute,
    authority_paths: &[String],
) -> RegistryResult<BTreeMap<&'static str, serde_json::Value>> {
    let mut value = BTreeMap::new();
    value.insert(
        "authority_paths",
        serde_json::to_value(sorted(authority_paths)).map_err(|error| error.to_string())?,
    );
    value.insert("claim_id", serde_json::to_value(&claim.claim_id).map_err(|error| error.to_string())?);
    value.insert(
        "document",
        serde_json::to_value(document).map_err(|error| error.to_string())?,
    );
    value.insert("family", serde_json::to_value(&claim.family).map_err(|e| e.to_string())?);
    value.insert("provider", serde_json::to_value(&claim.provider).map_err(|e| e.to_string())?);
    value.insert(
        "criticality",
        serde_json::to_value(&claim.criticality).map_err(|e| e.to_string())?,
    );
    value.insert("phase", serde_json::to_value(&claim.phase).map_err(|e| e.to_string())?);
    value.insert(
        "proposition",
        serde_json::to_value(&claim.proposition).map_err(|e| e.to_string())?,
    );
    value.insert(
        "non_claims",
        serde_json::to_value(sorted(&claim.non_claims)).map_err(|e| e.to_string())?,
    );
    value.insert(
        "owners",
        serde_json::to_value(sorted(&claim.owners)).map_err(|e| e.to_string())?,
    );
    value.insert(
        "source_closure",
        serde_json::to_value(sorted(&claim.source_closure)).map_err(|e| e.to_string())?,
    );
    value.insert("domain", serde_json::to_value(&claim.domain).map_err(|e| e.to_string())?);
    // Assumption order is decision-bearing and matches the native row digest law.
    value.insert(
        "assumptions",
        serde_json::to_value(&claim.assumptions).map_err(|e| e.to_string())?,
    );
    value.insert(
        "trust_dependencies",
        serde_json::to_value(sorted(&claim.trust_dependencies)).map_err(|e| e.to_string())?,
    );
    value.insert(
        "observable",
        serde_json::to_value(&claim.observable).map_err(|e| e.to_string())?,
    );
    value.insert(
        "bounds",
        serde_json::to_value(&claim.bounds).map_err(|e| e.to_string())?,
    );
    value.insert(
        "subjects",
        serde_json::to_value(sorted(&claim.subjects)).map_err(|e| e.to_string())?,
    );
    value.insert(
        "required_covers",
        serde_json::to_value(sorted(&claim.required_covers)).map_err(|e| e.to_string())?,
    );
    value.insert(
        "negative_controls",
        serde_json::to_value(sorted(&claim.negative_controls)).map_err(|e| e.to_string())?,
    );
    value.insert(
        "result_satisfaction",
        serde_json::to_value(&claim.result_satisfaction).map_err(|e| e.to_string())?,
    );
    value.insert(
        "currentness_inputs",
        serde_json::to_value(sorted(&claim.currentness_inputs)).map_err(|e| e.to_string())?,
    );
    value.insert(
        "transition_policy",
        serde_json::to_value(&claim.transition_policy).map_err(|e| e.to_string())?,
    );
    value.insert(
        "owner_spec",
        serde_json::to_value(&claim.owner_spec).map_err(|e| e.to_string())?,
    );
    value.insert(
        "owner_issue",
        serde_json::to_value(claim.owner_issue).map_err(|error| error.to_string())?,
    );
    value.insert(
        "route",
        serde_json::to_value(route).map_err(|error| error.to_string())?,
    );
    Ok(value)
}

fn decision_digest(
    claim: &ClaimDocument,
    document: &str,
    route: ProviderRoute,
    authority_paths: &[String],
) -> RegistryResult<String> {
    digest_json(&canonical_decision(
        claim,
        document,
        route,
        authority_paths,
    )?)
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CanonicalRegistryClaim<'a> {
    document: &'a str,
    claim_id: &'a str,
    claim_generation: u64,
    route: ProviderRoute,
    authority_paths: Vec<String>,
    decision_digest: &'a str,
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CanonicalRegistry<'a> {
    repository: &'a str,
    global_authority_paths: Vec<String>,
    claims: Vec<CanonicalRegistryClaim<'a>>,
    retirements: Vec<Retirement>,
}

fn registry_digest(manifest: &Manifest, claims: &[LoadedClaim]) -> RegistryResult<String> {
    let mut canonical_claims: Vec<_> = claims
        .iter()
        .map(|loaded| CanonicalRegistryClaim {
            document: &loaded.document,
            claim_id: &loaded.claim.claim_id,
            claim_generation: loaded.claim.claim_generation,
            route: loaded.route,
            authority_paths: sorted(&loaded.authority_paths),
            decision_digest: &loaded.decision_digest,
        })
        .collect();
    canonical_claims.sort_by(|left, right| left.claim_id.cmp(right.claim_id));
    let mut retirements = manifest.retirements.clone();
    retirements.sort();
    digest_json(&CanonicalRegistry {
        repository: &manifest.repository,
        global_authority_paths: sorted(&manifest.global_authority_paths),
        claims: canonical_claims,
        retirements,
    })
}

fn json<T: Serialize>(value: &T) -> RegistryResult<String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

fn render<T: Serialize>(
    value: &T,
    format: OutputFormat,
    markdown: impl FnOnce() -> String,
) -> RegistryResult<String> {
    match format {
        OutputFormat::Json => json(value),
        OutputFormat::Markdown => Ok(markdown()),
    }
}

fn parse_command(value: &str) -> RegistryResult<Command> {
    match value {
        "check" => Ok(Command::Check),
        "index" => Ok(Command::Index),
        "scoreboard" => Ok(Command::Scoreboard),
        "transition" => Ok(Command::Transition),
        _ => Err(format!("unknown command {value}; {USAGE}")),
    }
}

pub fn execute(args: &[String]) -> RegistryResult<String> {
    let command = args
        .first()
        .ok_or_else(|| format!("command is required; {USAGE}"))
        .and_then(|value| parse_command(value))?;
    let mut registry = None;
    let mut base_registry = None;
    let mut head_registry = None;
    let mut format = OutputFormat::Json;

    let mut iterator = args.iter().skip(1);
    while let Some(flag) = iterator.next() {
        let value = iterator
            .next()
            .ok_or_else(|| format!("{flag} requires a value; {USAGE}"))?;
        match flag.as_str() {
            "--registry" => {
                require(registry.is_none(), "--registry may be supplied only once")?;
                registry = Some(value);
            }
            "--base-registry" => {
                require(
                    base_registry.is_none(),
                    "--base-registry may be supplied only once",
                )?;
                base_registry = Some(value);
            }
            "--head-registry" => {
                require(
                    head_registry.is_none(),
                    "--head-registry may be supplied only once",
                )?;
                head_registry = Some(value);
            }
            "--format" => {
                format = match value.as_str() {
                    "json" => OutputFormat::Json,
                    "markdown" => OutputFormat::Markdown,
                    _ => return Err(format!("unknown format {value}; {USAGE}")),
                };
            }
            _ => return Err(format!("unknown argument {flag}; {USAGE}")),
        }
    }

    match command {
        Command::Check | Command::Index | Command::Scoreboard => {
            require(
                base_registry.is_none() && head_registry.is_none(),
                "base/head registry flags apply only to transition",
            )?;
            let path = registry.ok_or_else(|| format!("--registry is required; {USAGE}"))?;
            let loaded = load::load_registry(Path::new(path))?;
            match command {
                Command::Check => project::render_check(&loaded, format),
                Command::Index => project::render_index(&loaded, format),
                Command::Scoreboard => project::render_scoreboard(&loaded, format),
                Command::Transition => return Err("transition command reached the single-registry route".to_string()),
            }
        }
        Command::Transition => {
            require(
                registry.is_none(),
                "--registry does not apply to transition",
            )?;
            let base = base_registry
                .ok_or_else(|| format!("--base-registry is required; {USAGE}"))?;
            let head = head_registry
                .ok_or_else(|| format!("--head-registry is required; {USAGE}"))?;
            let base = load::load_registry(Path::new(base))?;
            let head = load::load_registry(Path::new(head))?;
            transition::render_transition(&base, &head, format)
        }
    }
}

#[cfg(test)]
mod tests;
