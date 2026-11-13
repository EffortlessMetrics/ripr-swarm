//! Offline consistency audit for the draft `FormalClaimV1` / `NativeProofRunV1`
//! contracts (#14471/#14472/#14473). This does NOT run a verifier, authenticate
//! artifacts, recompute source commitments, or establish current applicability.
//! Exit zero means the supplied documents are consistent, never proof success.

pub mod kani_output;
pub mod native;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MAX_INPUT_BYTES: u64 = 1_048_576;
const MAX_CLAIMS: usize = 256;
const MAX_TOTAL_INPUT_BYTES: u64 = 16 * MAX_INPUT_BYTES;
pub const USAGE: &str = "usage: formal-evidence-audit --claim FILE [--claim FILE ...] [--run FILE]\nExit 0: documents consistent; 2: invalid input. Native assurance is never assessed.";

pub type AuditResult<T> = Result<T, String>;

/// Exact claim-array commitment: compact UTF-8 JSON, preserving assumption
/// order. An empty assumption set is the committed bytes `[]`, not absence.
pub fn assumptions_digest(assumptions: &[String]) -> AuditResult<String> {
    serde_json::to_vec(assumptions)
        .map(|bytes| native::sha256(&bytes))
        .map_err(|error| error.to_string())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    BoundedModelCheck,
    DeductiveProof,
    StateMachineModel,
    ConcurrencyModel,
    UndefinedBehaviorExecution,
    PropertyTest,
    FuzzCampaign,
    MutationCalibration,
    ManualOrExternal,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Criticality {
    Critical,
    Important,
    Advisory,
    Scheduled,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    AdmittedMergeGroup,
    // Retained for historical document inspection, not trigger authority.
    OrdinaryPrCritical,
    MergeReady,
    Scheduled,
    Release,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum BoundValue {
    Text(String),
    Number(serde_json::Number),
    Flag(bool),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Claim {
    pub schema: String,
    pub claim_id: String,
    pub claim_generation: u64,
    pub family: Family,
    pub provider: String,
    pub criticality: Criticality,
    pub phase: Phase,
    pub proposition: String,
    pub non_claims: Vec<String>,
    pub owners: Vec<String>,
    pub source_closure: Vec<String>,
    pub domain: String,
    pub assumptions: Vec<String>,
    pub trust_dependencies: Vec<String>,
    pub observable: String,
    pub bounds: BTreeMap<String, BoundValue>,
    pub subjects: Vec<String>,
    pub required_covers: Vec<String>,
    pub negative_controls: Vec<String>,
    pub result_satisfaction: String,
    pub currentness_inputs: Vec<String>,
    pub transition_policy: String,
    pub owner_spec: String,
    pub owner_issue: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    pub family: Family,
    pub name: String,
    pub version: String,
    pub solver_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Toolchain {
    pub language: String,
    pub target: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Deadlines {
    pub preflight_seconds: u64,
    pub whole_run_seconds: u64,
    pub per_subject_seconds: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Runner {
    pub generation: String,
    pub command_digest: String,
    pub deadline_policy: Deadlines,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Inventory {
    pub expected_claim_ids: Vec<String>,
    pub discovered_subjects: Vec<String>,
    pub complete: bool,
    #[serde(default)]
    pub rejection_reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    Completed,
    Failed,
    TimedOut,
    Killed, // ripr-allow: static-language: OS process termination, not a mutation outcome.
    NotStarted,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Cleanup {
    Complete,
    Residue,
    Failed,
    NotApplicable,
    NotObserved,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Process {
    pub state: ProcessState,
    #[serde(deserialize_with = "required_nullable")]
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    #[serde(deserialize_with = "required_nullable")]
    pub raw_output_digest: Option<String>,
    pub cleanup_state: Cleanup,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Status {
    Established,
    Failed,
    NotEstablished,
    Stale,
    Unsupported,
    NotProven,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Verified,
    Counterexample,
    UnreachableCover,
    Timeout,
    Missing,
    Ambiguous,
    AbnormalProcess,
    Malformed,
    Unsupported,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Covers {
    pub required: u64,
    pub satisfied: u64,
    pub unreachable: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub claim_id: String,
    pub claim_generation: u64,
    pub subject: String,
    pub source_digest: String,
    pub status: Status,
    pub native_verdict: Verdict,
    pub covers: Covers,
    pub assumptions_digest: Option<String>,
    pub bounds_digest: String,
    #[serde(deserialize_with = "required_nullable")]
    pub duration_ms: Option<u64>,
    pub peak_memory_bytes: Option<u64>,
    pub raw_block_digest: Option<String>,
    pub current: bool,
    pub reasons: Vec<String>,
    pub non_claims: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub schema: String,
    pub run_id: String,
    pub repository: String,
    #[serde(deserialize_with = "required_nullable")]
    pub revision: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub tree: Option<String>,
    pub worktree_digest: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub source_closure_digest: Option<String>,
    pub claim_registry_generation: String,
    pub provider: Provider,
    pub toolchain: Toolchain,
    pub runner: Runner,
    pub inventory: Inventory,
    pub process: Process,
    pub results: Vec<Row>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unregistered_results: Vec<native::UnregisteredResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution: Option<native::Execution>,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub schema: &'static str,
    pub document_consistency: &'static str,
    pub native_assurance: &'static str,
    pub registered_claims: usize,
    pub selected_claims: usize,
    pub observed_rows: usize,
    pub declared_established_rows: usize,
    pub missing_subjects: Vec<String>,
    pub undiscovered_selected_subjects: Vec<String>,
    pub unmapped_discovered_subjects: Vec<String>,
    pub process_completed_successfully: Option<bool>,
    pub inventory_declared_complete: Option<bool>,
    pub cleanup_complete: Option<bool>,
    pub limitations: Vec<&'static str>,
}

pub fn require(condition: bool, message: &str) -> AuditResult<()> {
    if condition {
        Ok(())
    } else {
        Err(message.to_string())
    }
}

pub fn text(value: &str, field: &str) -> AuditResult<()> {
    require(!value.trim().is_empty(), &format!("{field}: blank value"))
}

pub fn strings(values: &[String], field: &str, nonempty: bool) -> AuditResult<()> {
    require(
        !nonempty || !values.is_empty(),
        &format!("{field}: empty list"),
    )?;
    let mut seen = BTreeSet::new();
    for value in values {
        text(value, field)?;
        require(
            seen.insert(value),
            &format!("{field}: duplicate value {value}"),
        )?;
    }
    Ok(())
}

fn claim_id(value: &str) -> AuditResult<()> {
    let mut bytes = value.bytes();
    let first_valid = bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    require(
        value.len() >= 2
            && first_valid
            && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b)),
        "claim_id: expected stable lowercase identifier",
    )
}

pub fn digest(value: &str, field: &str) -> AuditResult<()> {
    let hex = value
        .strip_prefix("sha256:")
        .ok_or_else(|| format!("{field}: expected sha256 digest"))?;
    require(
        hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        &format!("{field}: expected 64 hex digits"),
    )
}

fn optional_digest(value: &Option<String>, field: &str) -> AuditResult<()> {
    match value {
        Some(value) => digest(value, field),
        None => Ok(()),
    }
}

pub fn git_object(value: &str, field: &str) -> AuditResult<()> {
    require(
        matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit()),
        &format!("{field}: expected full Git object ID"),
    )
}

fn validate_claim(claim: &Claim) -> AuditResult<()> {
    require(
        claim.schema == "ripr.formal-claim.v1",
        "unsupported claim schema",
    )?;
    claim_id(&claim.claim_id)?;
    if claim.provider == "kani" {
        require(
            claim.family == Family::BoundedModelCheck,
            "Kani claim has the wrong proof family",
        )?;
    }
    require(
        claim.claim_generation > 0 && claim.owner_issue > 0,
        "claim generation and owner issue must be positive",
    )?;
    for (field, value) in [
        ("provider", &claim.provider),
        ("proposition", &claim.proposition),
        ("domain", &claim.domain),
        ("observable", &claim.observable),
        ("result_satisfaction", &claim.result_satisfaction),
        ("transition_policy", &claim.transition_policy),
        ("owner_spec", &claim.owner_spec),
    ] {
        text(value, field)?;
    }
    for (field, values, nonempty) in [
        ("non_claims", &claim.non_claims, true),
        ("owners", &claim.owners, true),
        ("source_closure", &claim.source_closure, true),
        ("assumptions", &claim.assumptions, false),
        ("trust_dependencies", &claim.trust_dependencies, true),
        ("subjects", &claim.subjects, true),
        (
            "required_covers",
            &claim.required_covers,
            claim.criticality == Criticality::Critical,
        ),
        ("negative_controls", &claim.negative_controls, true),
        ("currentness_inputs", &claim.currentness_inputs, true),
    ] {
        strings(values, field, nonempty)?;
    }
    require(
        !claim.bounds.is_empty(),
        "bounds: explicit disposition required",
    )?;
    for (key, value) in &claim.bounds {
        text(key, "bound key")?;
        if let BoundValue::Text(value) = value {
            text(value, "bound value")?;
        }
        if key == "unwind" {
            require(
                matches!(value, BoundValue::Number(n) if n.as_u64().is_some_and(|v| v > 0)),
                "unwind: positive integer required",
            )?;
        }
    }
    for path in &claim.source_closure {
        require(
            !path.contains('\\')
                && !path.contains(':')
                && !path.chars().any(char::is_control)
                && path
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != ".."),
            "source_closure: repository-relative paths only; no glob execution or traversal",
        )?;
    }
    Ok(())
}

fn validate_row(row: &Row, claim: &Claim, run: &Run) -> AuditResult<()> {
    if matches!(row.status, Status::Established | Status::Failed) {
        require(
            run.revision.is_some() && run.tree.is_some() && run.source_closure_digest.is_some(),
            "observed registered result requires source commitments",
        )?;
    }
    require(
        row.claim_generation > 0,
        "result claim_generation must be positive",
    )?;
    require(
        claim.subjects.contains(&row.subject),
        "result subject is not registered for its claim",
    )?;
    strings(
        &row.reasons,
        "result reasons",
        row.status != Status::Established,
    )?;
    strings(&row.non_claims, "result non_claims", true)?;
    digest(&row.source_digest, "result source_digest")?;
    digest(&row.bounds_digest, "result bounds_digest")?;
    optional_digest(&row.assumptions_digest, "assumptions_digest")?;
    optional_digest(&row.raw_block_digest, "raw_block_digest")?;
    let counted = row
        .covers
        .satisfied
        .checked_add(row.covers.unreachable)
        .ok_or("cover count overflow")?;
    require(
        counted <= row.covers.required,
        "cover counts exceed declared requirement",
    )?;
    if row.status == Status::Stale {
        require(!row.current, "STALE result cannot declare current=true")?;
    }
    if row.claim_generation != claim.claim_generation {
        require(
            row.status == Status::Stale && !row.current,
            "wrong claim generation must remain STALE",
        )?;
    }
    if row.native_verdict == Verdict::Timeout {
        require(
            matches!(row.status, Status::NotEstablished | Status::Stale),
            "timeout is not a counterexample or established result",
        )?;
    }
    if run.process.state == ProcessState::NotStarted {
        require(
            row.status == Status::NotEstablished
                && row.native_verdict == Verdict::Missing
                && row.raw_block_digest.is_none(),
            "not-started run cannot contain native observations",
        )?;
        require(
            row.covers.satisfied == 0
                && row.covers.unreachable == 0
                && row.duration_ms.is_none()
                && row.peak_memory_bytes.is_none(),
            "not-started row cannot contain observed covers, duration or memory",
        )?;
    }
    // A behavioral failure is also an evidence-bearing claim. Tool absence,
    // stale observations and success-looking output cannot become a failed
    // proposition merely because the input labels them FAILED.
    if matches!(row.status, Status::Established | Status::Failed) {
        require(
            row.assumptions_digest.as_ref() == Some(&assumptions_digest(&claim.assumptions)?),
            "ESTABLISHED/FAILED requires the exact claim assumption commitment",
        )?;
        require(row.current, "ESTABLISHED/FAILED requires current evidence")?;
        require(
            run.inventory.discovered_subjects.contains(&row.subject),
            "ESTABLISHED/FAILED subject was not discovered",
        )?;
        require(
            row.raw_block_digest.is_some() && run.process.raw_output_digest.is_some(),
            "ESTABLISHED/FAILED requires retained raw-output and block commitments",
        )?;
        require(
            row.duration_ms.is_some(),
            "ESTABLISHED/FAILED requires observed duration",
        )?;
        require(
            row.covers.required
                == u64::try_from(claim.required_covers.len()).map_err(|e| e.to_string())?,
            "required-cover count disagrees with claim",
        )?;
    }
    if row.status == Status::Failed {
        require(
            matches!(
                row.native_verdict,
                Verdict::Counterexample | Verdict::UnreachableCover
            ),
            "FAILED requires a behavioral counterexample or unreachable required cover",
        )?;
        if row.native_verdict == Verdict::UnreachableCover {
            require(
                row.covers.unreachable > 0,
                "unreachable-cover failure requires an observed unreachable cover",
            )?;
        }
    }
    if row.status == Status::Established {
        require(
            row.native_verdict == Verdict::Verified,
            "ESTABLISHED requires verified evidence",
        )?;
        require(
            row.covers.satisfied == row.covers.required && row.covers.unreachable == 0,
            "ESTABLISHED requires all declared covers satisfied",
        )?;
        if claim.provider == "kani" && claim.criticality == Criticality::Critical {
            require(
                row.covers.required > 0,
                "critical Kani claim requires positive cover evidence",
            )?;
        }
    }
    Ok(())
}

pub fn audit(claims: &[Claim], run: Option<&Run>) -> AuditResult<Summary> {
    require(
        claims.len() <= MAX_CLAIMS
            && (!claims.is_empty() || run.is_some_and(|r| r.execution.is_some())),
        "supply between 1 and 256 claims",
    )?;
    let mut registry = BTreeMap::new();
    for claim in claims {
        validate_claim(claim).map_err(|e| format!("{}: {e}", claim.claim_id))?;
        require(
            registry.insert(claim.claim_id.as_str(), claim).is_none(),
            "duplicate claim ID",
        )?;
    }
    let mut summary = Summary {
        schema: "ripr.formal-evidence-audit.v1",
        document_consistency: "consistent",
        native_assurance: "not_assessed",
        registered_claims: claims.len(),
        selected_claims: 0,
        observed_rows: 0,
        declared_established_rows: 0,
        missing_subjects: Vec::new(),
        undiscovered_selected_subjects: Vec::new(),
        unmapped_discovered_subjects: Vec::new(),
        process_completed_successfully: None,
        inventory_declared_complete: None,
        cleanup_complete: None,
        limitations: vec![
            "Input declarations are not authenticated evidence.",
            "Source/configuration/registry commitments and currentness are not recomputed.",
            "Cover identities, proposition adequacy, negative-control execution and proof-authority transitions are not established.",
            "This audit cannot issue a proof receipt or authorize CI admission.",
        ],
    };
    let Some(run) = run else {
        return Ok(summary);
    };
    require(
        run.schema == "ripr.native-proof-run.v1",
        "unsupported run schema",
    )?;
    // Only this provider profile has a semantic admission policy in this tool.
    require(
        run.provider.family == Family::BoundedModelCheck && run.provider.name == "kani",
        "unsupported native provider profile; no assurance inferred",
    )?;
    for (field, value) in [
        ("run_id", &run.run_id),
        ("repository", &run.repository),
        ("claim_registry_generation", &run.claim_registry_generation),
        ("provider version", &run.provider.version),
        ("toolchain language", &run.toolchain.language),
        ("runner generation", &run.runner.generation),
    ] {
        text(value, field)?;
    }
    if let Some(value) = &run.provider.solver_version {
        text(value, "solver_version")?;
    }
    if let Some(value) = &run.toolchain.target {
        text(value, "target")?;
    }
    if let Some(value) = &run.revision {
        git_object(value, "revision")?;
    }
    if let Some(value) = &run.tree {
        git_object(value, "tree")?;
    }
    optional_digest(&run.source_closure_digest, "source_closure_digest")?;
    native::validate(run)?;
    digest(&run.runner.command_digest, "command_digest")?;
    optional_digest(&run.worktree_digest, "worktree_digest")?;
    optional_digest(&run.process.raw_output_digest, "raw_output_digest")?;
    let deadline = &run.runner.deadline_policy;
    require(
        deadline.preflight_seconds > 0
            && deadline.whole_run_seconds > 0
            && deadline.per_subject_seconds > 0,
        "deadlines must be positive",
    )?;
    strings(
        &run.inventory.expected_claim_ids,
        "expected_claim_ids",
        false,
    )?;
    strings(
        &run.inventory.discovered_subjects,
        "discovered_subjects",
        false,
    )?;
    strings(
        &run.inventory.rejection_reasons,
        "inventory rejection_reasons",
        !run.inventory.complete,
    )?;
    require(
        run.process.timed_out == (run.process.state == ProcessState::TimedOut),
        "process timeout fields contradict",
    )?;
    if run.process.state == ProcessState::Completed {
        require(
            run.process.exit_code.is_some(),
            "completed process is missing exit status",
        )?;
    }
    if matches!(
        run.process.state,
        ProcessState::Failed | ProcessState::Killed // ripr-allow: static-language: OS process termination, not a mutation outcome.
    ) {
        require(
            run.process.exit_code != Some(0),
            "failed or terminated process cannot have successful exit",
        )?;
    }
    if run.process.state == ProcessState::NotStarted {
        require(
            run.process.exit_code.is_none() && run.process.raw_output_digest.is_none(),
            "not-started run cannot borrow process output or exit status",
        )?;
    }
    if run.inventory.complete {
        require(
            (!run.inventory.expected_claim_ids.is_empty() || !run.unregistered_results.is_empty())
                && !run.inventory.discovered_subjects.is_empty(),
            "empty inventory cannot be complete",
        )?;
        require(
            run.inventory.rejection_reasons.is_empty(),
            "complete inventory cannot carry rejection reasons",
        )?;
    }
    let mut expected = BTreeSet::new();
    for id in &run.inventory.expected_claim_ids {
        let claim = registry
            .get(id.as_str())
            .ok_or_else(|| format!("unregistered selected claim: {id}"))?;
        require(
            claim.family == run.provider.family && claim.provider == run.provider.name,
            "selected claim/provider mismatch",
        )?;
        for subject in &claim.subjects {
            expected.insert((id.as_str(), subject.as_str()));
        }
    }
    let mut observed = BTreeSet::new();
    for row in &run.results {
        let key = (row.claim_id.as_str(), row.subject.as_str());
        require(
            expected.contains(&key),
            "result is outside the selected registered subjects",
        )?;
        require(
            observed.insert(key),
            "duplicate result subject; never first/last match wins",
        )?;
        let claim = registry
            .get(row.claim_id.as_str())
            .ok_or("result claim is not registered")?;
        validate_row(row, claim, run)?;
    }
    summary.missing_subjects = expected
        .difference(&observed)
        .map(|(id, subject)| format!("{id}::{subject}"))
        .collect();
    let unregistered: Vec<String> = run
        .unregistered_results
        .iter()
        .map(|r| r.subject.id())
        .collect();
    let selected_subjects: BTreeSet<&str> = expected
        .iter()
        .map(|(_, subject)| *subject)
        .chain(unregistered.iter().map(String::as_str))
        .collect();
    let discovered_subjects: BTreeSet<&str> = run
        .inventory
        .discovered_subjects
        .iter()
        .map(String::as_str)
        .collect();
    summary.undiscovered_selected_subjects = selected_subjects
        .difference(&discovered_subjects)
        .map(|subject| (*subject).to_string())
        .collect();
    summary.unmapped_discovered_subjects = run
        .inventory
        .discovered_subjects
        .iter()
        .filter(|subject| !selected_subjects.contains(subject.as_str()))
        .cloned()
        .collect();
    summary.unmapped_discovered_subjects.sort();
    if run.inventory.complete {
        require(
            summary.undiscovered_selected_subjects.is_empty(),
            "complete inventory has selected subjects absent from native discovery",
        )?;
        require(
            summary.unmapped_discovered_subjects.is_empty(),
            "complete inventory has unmapped discovered subjects",
        )?;
        require(
            summary.missing_subjects.is_empty(),
            "complete inventory has missing result subjects",
        )?;
    }
    summary.selected_claims = run.inventory.expected_claim_ids.len();
    summary.observed_rows = run.results.len();
    summary.declared_established_rows = run
        .results
        .iter()
        .filter(|r| r.status == Status::Established)
        .count();
    summary.inventory_declared_complete = Some(run.inventory.complete);
    summary.process_completed_successfully = Some(
        run.process.state == ProcessState::Completed
            && run.process.exit_code == Some(0)
            && !run.process.timed_out,
    );
    summary.cleanup_complete = Some(run.process.cleanup_state == Cleanup::Complete);
    Ok(summary)
}

// A required nullable key must be present, even when its value is null.
pub fn required_nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

// serde_json::Value normally keeps the last duplicate map key. Reject duplicates
// before typed decoding, including extensible bound maps, not just DTO fields.
pub struct UniqueJson(serde_json::Value);

impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueJson(n.into()))
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(UniqueJson(value)) = seq.next_element()? {
                    values.push(value);
                }
                Ok(UniqueJson(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, UniqueJson(value))) = map.next_entry::<String, UniqueJson>()? {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key: {key}"
                        )));
                    }
                }
                Ok(UniqueJson(values.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

pub fn parse_json<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> AuditResult<T> {
    let UniqueJson(value) = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, remaining: &mut u64) -> AuditResult<T> {
    let metadata =
        std::fs::metadata(path).map_err(|e| format!("inspect {}: {e}", path.display()))?;
    require(metadata.is_file(), "input must be a regular file")?;
    let file = File::open(path).map_err(|e| format!("open {}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    let limit = MAX_INPUT_BYTES.min(*remaining);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    let length = u64::try_from(bytes.len()).map_err(|e| e.to_string())?;
    require(
        length <= limit,
        "input exceeds 1 MiB per file or 16 MiB total budget",
    )?;
    *remaining -= length;
    parse_json(&bytes).map_err(|e| format!("parse {}: {e}", path.display()))
}

pub fn execute(args: &[String]) -> AuditResult<Summary> {
    let mut claim_paths = Vec::new();
    let mut run_path = None;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a path; {USAGE}"))?;
        match flag.as_str() {
            "--claim" => {
                require(claim_paths.len() < MAX_CLAIMS, "too many claim files")?;
                claim_paths.push(value);
            }
            "--run" => {
                require(run_path.is_none(), "--run may be supplied only once")?;
                run_path = Some(value);
            }
            _ => return Err(format!("unknown argument {flag}; {USAGE}")),
        }
    }
    let mut remaining = MAX_TOTAL_INPUT_BYTES;
    let claims = claim_paths
        .iter()
        .map(|p| read_json::<Claim>(Path::new(p), &mut remaining))
        .collect::<AuditResult<Vec<_>>>()?;
    let run = run_path
        .map(|p| read_json::<Run>(Path::new(p), &mut remaining))
        .transpose()?;
    audit(&claims, run.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Synthetic contract inputs only; none of these fixtures is native evidence.
    fn fixtures() -> AuditResult<(Claim, Run)> {
        let hash = format!("sha256:{}", "1".repeat(64));
        let claim: Claim = serde_json::from_value(json!({
            "schema":"ripr.formal-claim.v1", "claim_id":"example.actionability", "claim_generation":1,
            "family":"bounded_model_check", "provider":"kani", "criticality":"critical", "phase":"manual",
            "proposition":"Only the Actionable variant is actionable.", "non_claims":["No upstream admission claim."],
            "owners":["ripr_contract::domain::EvidenceState::is_actionable"],
            "source_closure":["crates/ripr-contract/src/domain/evidence_state.rs", "crates/ripr/src/kani_smoke.rs"],
            "domain":"Five explicitly selected constructors", "assumptions":["index < 5"],
            "trust_dependencies":["Kani and symbolic constructor selection"], "observable":"boolean return",
            "bounds":{"selection":"index < 5"}, "subjects":["ripr::kani_smoke::evidence_state_actionable_only_for_actionable"],
            "required_covers":["actionable", "non-actionable"], "negative_controls":["Forbidden state becomes actionable"],
            "result_satisfaction":"Complete current native rows and covers", "currentness_inputs":["source and configuration"],
            "transition_policy":"base_and_head_protected_until_replacement_or_retirement", "owner_spec":"RIPR-SPEC-0371", "owner_issue":12109
        })).map_err(|e| e.to_string())?;
        let run: Run = serde_json::from_value(json!({
            "schema":"ripr.native-proof-run.v1", "run_id":"synthetic-test-only", "repository":"example/repo",
            "revision":"1".repeat(40), "tree":"2".repeat(40), "source_closure_digest":hash,
            "claim_registry_generation":"test-generation", "provider":{"family":"bounded_model_check","name":"kani","version":"0.68.0"},
            "toolchain":{"language":"rust-1.95.0"},
            "runner":{"generation":"test-only","command_digest":hash,"deadline_policy":{"preflight_seconds":10,"whole_run_seconds":60,"per_subject_seconds":30}},
            "inventory":{"expected_claim_ids":[claim.claim_id],"discovered_subjects":claim.subjects,"complete":true,"rejection_reasons":[]},
            "process":{"state":"completed","exit_code":0,"timed_out":false,"raw_output_digest":hash,"cleanup_state":"complete"},
            "results":[{"claim_id":claim.claim_id,"claim_generation":1,"subject":claim.subjects.first().ok_or("missing test subject")?,
                "source_digest":hash,"status":"ESTABLISHED","native_verdict":"verified","covers":{"required":2,"satisfied":2,"unreachable":0},
                "bounds_digest":hash,"assumptions_digest":assumptions_digest(&claim.assumptions)?,"duration_ms":1,"raw_block_digest":hash,"current":true,"reasons":["synthetic test input"],"non_claims":["not a native receipt"]}]
        })).map_err(|e| e.to_string())?;
        Ok((claim, run))
    }

    fn row(run: &mut Run) -> AuditResult<&mut Row> {
        run.results
            .first_mut()
            .ok_or_else(|| "missing test row".to_string())
    }

    fn rejection<T>(result: AuditResult<T>) -> AuditResult<String> {
        result.err().ok_or_else(|| {
            "expected a diagnostic refusal, but supplied evidence was accepted".to_string()
        })
    }

    #[test]
    fn consistent_documents_never_become_native_assurance() -> AuditResult<()> {
        let (claim, run) = fixtures()?;
        let summary = audit(&[claim], Some(&run))?;
        assert_eq!(summary.native_assurance, "not_assessed");
        assert_eq!(summary.declared_established_rows, 1);
        assert_eq!(summary.process_completed_successfully, Some(true));
        Ok(())
    }

    #[test]
    fn unknown_duplicate_fields_and_unknown_vocabulary_reject() -> AuditResult<()> {
        let (claim, _) = fixtures()?;
        let encoded = serde_json::to_string(&claim).map_err(|e| e.to_string())?;
        let duplicate = encoded.replacen("{", "{\"claim_generation\":1,", 1);
        assert!(!rejection(parse_json::<Claim>(duplicate.as_bytes()))?.is_empty());
        let unknown = encoded.replacen("{", "{\"native_status\":\"VERIFIED\",", 1);
        assert!(!rejection(parse_json::<Claim>(unknown.as_bytes()))?.is_empty());
        let bad_family = encoded.replace("bounded_model_check", "optimistic_proof");
        assert!(!rejection(parse_json::<Claim>(bad_family.as_bytes()))?.is_empty());
        Ok(())
    }

    #[test]
    fn admitted_merge_group_phase_is_passive_metadata_not_admission() -> AuditResult<()> {
        let (mut claim, _) = fixtures()?;
        claim.phase = Phase::AdmittedMergeGroup;
        let encoded = serde_json::to_vec(&claim).map_err(|e| e.to_string())?;
        let decoded: Claim = parse_json(&encoded)?;
        let summary = audit(&[decoded], None)?;
        assert_eq!(summary.native_assurance, "not_assessed");
        assert_eq!(summary.selected_claims, 0);
        assert_eq!(summary.process_completed_successfully, None);
        let unknown = String::from_utf8(encoded)
            .map_err(|e| e.to_string())?
            .replace("admitted_merge_group", "implicit_push_qualification");
        assert!(!rejection(parse_json::<Claim>(unknown.as_bytes()))?.is_empty());
        Ok(())
    }

    #[test]
    fn incomplete_critical_claim_metadata_rejects() -> AuditResult<()> {
        let (claim, _) = fixtures()?;
        let mut mutations = Vec::new();
        let mut c = claim.clone();
        c.claim_generation = 0;
        mutations.push(c);
        let mut c = claim.clone();
        c.proposition = " ".to_string();
        mutations.push(c);
        let mut c = claim.clone();
        c.required_covers.clear();
        mutations.push(c);
        let mut c = claim.clone();
        c.negative_controls.clear();
        mutations.push(c);
        let mut c = claim.clone();
        c.non_claims.clear();
        mutations.push(c);
        let mut c = claim.clone();
        c.bounds.clear();
        mutations.push(c);
        let mut c = claim.clone();
        c.source_closure = vec!["../escape.rs".to_string()];
        mutations.push(c);
        let mut c = claim.clone();
        c.bounds
            .insert("unwind".to_string(), BoundValue::Number(0.into()));
        mutations.push(c);
        for c in mutations {
            assert!(
                !rejection(validate_claim(&c))?.is_empty(),
                "accepted {}",
                c.claim_id
            );
        }
        assert!(!rejection(audit(&[], None))?.is_empty());
        assert!(!rejection(audit(&[claim.clone(), claim], None))?.is_empty());
        Ok(())
    }

    #[test]
    fn success_looking_incomplete_evidence_rejects() -> AuditResult<()> {
        let (claim, run) = fixtures()?;
        let mut mutations = Vec::new();
        let mut r = run.clone();
        row(&mut r)?.current = false;
        mutations.push(r);
        let mut r = run.clone();
        row(&mut r)?.native_verdict = Verdict::Missing;
        mutations.push(r);
        let mut r = run.clone();
        row(&mut r)?.covers.satisfied = 1;
        mutations.push(r);
        let mut r = run.clone();
        row(&mut r)?.covers.unreachable = 1;
        mutations.push(r);
        let mut r = run.clone();
        row(&mut r)?.covers.required = 3;
        mutations.push(r);
        let mut r = run.clone();
        row(&mut r)?.raw_block_digest = None;
        mutations.push(r);
        let mut r = run.clone();
        r.process.raw_output_digest = None;
        mutations.push(r);
        let mut r = run.clone();
        r.inventory.discovered_subjects.clear();
        mutations.push(r);
        let mut r = run.clone();
        row(&mut r)?.duration_ms = None;
        mutations.push(r);
        for r in mutations {
            assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&r)))?.is_empty());
        }
        Ok(())
    }

    #[test]
    fn wrong_identity_duplicate_rows_and_missing_subjects_reject() -> AuditResult<()> {
        let (claim, run) = fixtures()?;
        let mut r = run.clone();
        row(&mut r)?.claim_generation = 2;
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&r)))?.is_empty());
        let mut r = run.clone();
        row(&mut r)?.subject = "other::same_name".to_string();
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&r)))?.is_empty());
        let mut r = run.clone();
        r.results.extend(run.results.clone());
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&r)))?.is_empty());
        let mut r = run.clone();
        r.results.clear();
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&r)))?.is_empty());
        let mut r = run;
        r.provider.name = "lookalike".to_string();
        assert!(!rejection(audit(&[claim], Some(&r)))?.is_empty());
        Ok(())
    }

    #[test]
    fn partial_timeout_preserves_observed_rows_without_run_success() -> AuditResult<()> {
        let (mut claim, mut run) = fixtures()?;
        claim.subjects.push("ripr::kani_smoke::second".to_string());
        run.inventory.complete = false;
        run.inventory
            .rejection_reasons
            .push("second subject did not finish".to_string());
        run.process.state = ProcessState::TimedOut;
        run.process.exit_code = None;
        run.process.timed_out = true;
        let summary = audit(&[claim], Some(&run))?;
        assert_eq!(summary.declared_established_rows, 1);
        assert_eq!(summary.missing_subjects.len(), 1);
        assert_eq!(summary.process_completed_successfully, Some(false));
        assert_eq!(summary.native_assurance, "not_assessed");
        Ok(())
    }

    #[test]
    fn no_subject_or_not_started_run_needs_no_fabricated_result() -> AuditResult<()> {
        let (claim, mut run) = fixtures()?;
        run.inventory.complete = false;
        run.inventory
            .rejection_reasons
            .push("tool unavailable".to_string());
        run.inventory.discovered_subjects.clear();
        run.process.state = ProcessState::NotStarted;
        run.process.exit_code = None;
        run.process.raw_output_digest = None;
        run.process.cleanup_state = Cleanup::NotApplicable;
        run.results.clear();
        let summary = audit(&[claim], Some(&run))?;
        assert_eq!(summary.observed_rows, 0);
        assert_eq!(summary.missing_subjects.len(), 1);
        assert_eq!(summary.process_completed_successfully, Some(false));
        Ok(())
    }

    #[test]
    fn not_started_rows_cannot_borrow_cover_or_resource_observations() -> AuditResult<()> {
        let (claim, mut run) = fixtures()?;
        run.process.state = ProcessState::NotStarted;
        run.process.exit_code = None;
        run.process.raw_output_digest = None;
        run.process.cleanup_state = Cleanup::NotApplicable;
        run.inventory.complete = false;
        run.inventory
            .rejection_reasons
            .push("tool unavailable".into());
        run.inventory.discovered_subjects.clear();
        let result = row(&mut run)?;
        result.status = Status::NotEstablished;
        result.native_verdict = Verdict::Missing;
        result.raw_block_digest = None;
        result.duration_ms = None;
        result.peak_memory_bytes = None;
        result.covers.satisfied = 0;
        audit(std::slice::from_ref(&claim), Some(&run))?;
        let mut mutations = Vec::new();
        let mut candidate = run.clone();
        row(&mut candidate)?.covers.satisfied = 1;
        mutations.push(candidate);
        let mut candidate = run.clone();
        row(&mut candidate)?.covers.unreachable = 1;
        mutations.push(candidate);
        let mut candidate = run.clone();
        row(&mut candidate)?.duration_ms = Some(0);
        mutations.push(candidate);
        let mut candidate = run;
        row(&mut candidate)?.peak_memory_bytes = Some(0);
        mutations.push(candidate);
        for candidate in mutations {
            assert!(
                !rejection(audit(std::slice::from_ref(&claim), Some(&candidate)))?.is_empty(),
                "not-started row accepted native observations"
            );
        }
        Ok(())
    }

    #[test]
    fn stale_generation_remains_visible_without_promotion() -> AuditResult<()> {
        let (claim, mut run) = fixtures()?;
        let r = row(&mut run)?;
        r.claim_generation = 2;
        r.current = false;
        r.status = Status::Stale;
        let summary = audit(&[claim], Some(&run))?;
        assert_eq!(summary.declared_established_rows, 0);
        Ok(())
    }

    #[test]
    fn counterexample_timeout_and_process_consistency_are_distinct() -> AuditResult<()> {
        let (claim, mut run) = fixtures()?;
        row(&mut run)?.native_verdict = Verdict::Timeout;
        row(&mut run)?.status = Status::Failed;
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&run)))?.is_empty());
        row(&mut run)?.status = Status::NotEstablished;
        audit(std::slice::from_ref(&claim), Some(&run))?;
        run.process.timed_out = true;
        assert!(!rejection(audit(&[claim], Some(&run)))?.is_empty());
        Ok(())
    }

    #[test]
    fn failed_claim_requires_current_observed_behavioral_failure() -> AuditResult<()> {
        let (claim, run) = fixtures()?;
        let mut failed = run.clone();
        row(&mut failed)?.status = Status::Failed;
        row(&mut failed)?.native_verdict = Verdict::Counterexample;
        failed.process.exit_code = Some(1);
        audit(std::slice::from_ref(&claim), Some(&failed))?;
        let mut mutations = Vec::new();
        for verdict in [
            Verdict::Verified,
            Verdict::Missing,
            Verdict::AbnormalProcess,
            Verdict::Malformed,
            Verdict::Unknown,
        ] {
            let mut candidate = failed.clone();
            row(&mut candidate)?.native_verdict = verdict;
            mutations.push(candidate);
        }
        let mut candidate = failed.clone();
        row(&mut candidate)?.current = false;
        mutations.push(candidate);
        let mut candidate = failed.clone();
        row(&mut candidate)?.raw_block_digest = None;
        mutations.push(candidate);
        let mut candidate = failed.clone();
        candidate.process.raw_output_digest = None;
        mutations.push(candidate);
        let mut candidate = failed.clone();
        row(&mut candidate)?.duration_ms = None;
        mutations.push(candidate);
        let mut candidate = failed.clone();
        candidate.inventory.complete = false;
        candidate
            .inventory
            .rejection_reasons
            .push("subject was never discovered".into());
        candidate.inventory.discovered_subjects.clear();
        mutations.push(candidate);
        for candidate in mutations {
            let verdict = &candidate
                .results
                .first()
                .ok_or("missing row")?
                .native_verdict;
            assert!(
                !rejection(audit(std::slice::from_ref(&claim), Some(&candidate)))?.is_empty(),
                "accepted contradictory failed row: {verdict:?}"
            );
        }
        row(&mut failed)?.native_verdict = Verdict::UnreachableCover;
        assert!(
            !rejection(audit(std::slice::from_ref(&claim), Some(&failed)))?.is_empty(),
            "accepted unreachable verdict with zero unreachable covers"
        );
        row(&mut failed)?.covers.satisfied = 1;
        row(&mut failed)?.covers.unreachable = 1;
        audit(std::slice::from_ref(&claim), Some(&failed))?;
        row(&mut failed)?.covers.required = 3;
        assert!(
            !rejection(audit(&[claim], Some(&failed)))?.is_empty(),
            "accepted different required-cover inventory"
        );
        Ok(())
    }

    #[test]
    fn registry_order_does_not_change_summary() -> AuditResult<()> {
        let (claim, run) = fixtures()?;
        let mut other = claim.clone();
        other.claim_id = "example.other".to_string();
        let left = audit(&[claim.clone(), other.clone()], Some(&run))?;
        let right = audit(&[other, claim], Some(&run))?;
        assert_eq!(
            serde_json::to_value(left).map_err(|e| e.to_string())?,
            serde_json::to_value(right).map_err(|e| e.to_string())?
        );
        Ok(())
    }

    #[test]
    fn duplicate_bound_keys_missing_nullable_keys_and_trailing_input_reject() -> AuditResult<()> {
        let (claim, run) = fixtures()?;
        let encoded = serde_json::to_string(&claim).map_err(|e| e.to_string())?;
        let duplicate = encoded.replace("\"bounds\":{", "\"bounds\":{\"selection\":\"shadow\",");
        assert!(!rejection(parse_json::<Claim>(duplicate.as_bytes()))?.is_empty());
        let mut value = serde_json::to_value(run).map_err(|e| e.to_string())?;
        value
            .get_mut("process")
            .and_then(serde_json::Value::as_object_mut)
            .ok_or("missing process")?
            .remove("exit_code");
        let missing = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        assert!(!rejection(parse_json::<Run>(&missing))?.is_empty());
        let trailing = format!("{encoded} {{}}");
        assert!(!rejection(parse_json::<Claim>(trailing.as_bytes()))?.is_empty());
        Ok(())
    }

    #[test]
    fn extra_discovered_subject_cannot_hide_behind_complete_inventory() -> AuditResult<()> {
        let (claim, mut run) = fixtures()?;
        run.inventory
            .discovered_subjects
            .push("unmapped::proof".to_string());
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&run)))?.is_empty());
        run.inventory.complete = false;
        run.inventory
            .rejection_reasons
            .push("unmapped subject".to_string());
        let summary = audit(&[claim], Some(&run))?;
        assert_eq!(summary.unmapped_discovered_subjects, ["unmapped::proof"]);
        Ok(())
    }

    #[test]
    fn missing_placeholder_cannot_complete_an_undiscovered_subject() -> AuditResult<()> {
        let (mut claim, mut run) = fixtures()?;
        let missing_subject = "ripr::kani_smoke::undiscovered";
        claim.subjects.push(missing_subject.into());
        let mut missing = run.results.first().ok_or("missing fixture row")?.clone();
        missing.subject = missing_subject.into();
        missing.status = Status::NotEstablished;
        missing.native_verdict = Verdict::Missing;
        missing.raw_block_digest = None;
        missing.duration_ms = None;
        missing.covers.satisfied = 0;
        run.results.push(missing);
        assert!(
            !rejection(audit(std::slice::from_ref(&claim), Some(&run)))?.is_empty(),
            "placeholder row hid incomplete native discovery"
        );
        run.inventory.complete = false;
        run.inventory
            .rejection_reasons
            .push("selected subject not discovered".into());
        let summary = audit(&[claim], Some(&run))?;
        assert!(summary.missing_subjects.is_empty());
        assert_eq!(summary.undiscovered_selected_subjects, [missing_subject]);
        assert_eq!(summary.observed_rows, 2);
        assert_eq!(summary.native_assurance, "not_assessed");
        Ok(())
    }

    #[test]
    fn checked_in_examples_do_not_claim_execution() -> AuditResult<()> {
        let claim: Claim = parse_json(include_bytes!(
            "../../docs/formal-verification/examples/formal-claim.example.json"
        ))?;
        let run: Run = parse_json(include_bytes!(
            "../../docs/formal-verification/examples/native-proof-run.example.json"
        ))?;
        let summary = audit(&[claim], Some(&run))?;
        assert_eq!(summary.native_assurance, "not_assessed");
        assert_eq!(summary.observed_rows, 0);
        assert_eq!(summary.process_completed_successfully, Some(false));
        assert_eq!(summary.missing_subjects.len(), 1);
        Ok(())
    }

    #[test]
    fn retained_admission_controls_have_the_expected_semantic_disposition() -> AuditResult<()> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Case {
            name: String,
            #[serde(rename = "schema_valid")]
            _schema_valid: bool,
            semantic_valid: bool,
            document: Run,
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Corpus {
            #[serde(rename = "description")]
            _description: String,
            #[serde(rename = "schema_source_base")]
            _schema_source_base: String,
            claim: Claim,
            cases: Vec<Case>,
        }
        let corpus: Corpus = parse_json(include_bytes!(
            "../../docs/formal-verification/examples/admission-controls.json"
        ))?;
        assert!(!corpus.cases.is_empty());
        for case in corpus.cases {
            let result = audit(std::slice::from_ref(&corpus.claim), Some(&case.document));
            assert_eq!(
                result.is_ok(),
                case.semantic_valid,
                "{}: {result:?}",
                case.name
            );
        }
        Ok(())
    }

    #[test]
    fn cleanup_and_process_completion_remain_separate() -> AuditResult<()> {
        let (claim, mut run) = fixtures()?;
        run.process.cleanup_state = Cleanup::Residue;
        let summary = audit(&[claim], Some(&run))?;
        assert_eq!(summary.process_completed_successfully, Some(true));
        assert_eq!(summary.cleanup_complete, Some(false));
        assert_eq!(summary.native_assurance, "not_assessed");
        Ok(())
    }
    #[test]
    fn observed_assumptions_are_exact_and_empty_set_is_committed() -> AuditResult<()> {
        let (mut claim, mut run) = fixtures()?;
        run.results[0].reasons.clear();
        audit(std::slice::from_ref(&claim), Some(&run))?;
        for invalid in [None, Some(native::sha256(b"wrong"))] {
            let mut changed = run.clone();
            changed.results[0].assumptions_digest = invalid;
            assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&changed)))?.is_empty());
        }
        claim.assumptions = vec!["A".to_string(), "B".to_string()];
        // Independent SHA-256 of the exact bytes ["A","B"], not the helper
        // under test. Reordering the declaration must invalidate that result.
        run.results[0].assumptions_digest = Some(
            "sha256:b64e3448a83a5b86466465080361c1a7e1157a27ddccd4b68069cb18caffb74a".to_string(),
        );
        audit(std::slice::from_ref(&claim), Some(&run))?;
        claim.assumptions.reverse();
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&run)))?.is_empty());
        claim.assumptions.clear();
        run.results[0].assumptions_digest = Some(assumptions_digest(&claim.assumptions)?);
        assert_eq!(
            run.results[0].assumptions_digest.as_deref(),
            Some(native::sha256(b"[]").as_str())
        );
        audit(std::slice::from_ref(&claim), Some(&run))?;
        run.results[0].status = Status::Failed;
        run.results[0].native_verdict = Verdict::Counterexample;
        run.process.state = ProcessState::Failed;
        run.process.exit_code = Some(10);
        assert!(!rejection(audit(std::slice::from_ref(&claim), Some(&run)))?.is_empty());
        run.results[0]
            .reasons
            .push("observed synthetic counterexample".to_string());
        audit(&[claim], Some(&run))?;
        Ok(())
    }

    #[test]
    fn every_critical_provider_needs_reachability_obligations() -> AuditResult<()> {
        let (mut claim, _) = fixtures()?;
        claim.provider = "tlc".to_string();
        claim.family = Family::StateMachineModel;
        claim.required_covers.clear();
        assert!(!rejection(validate_claim(&claim))?.is_empty());
        claim
            .required_covers
            .push("finite terminal-state reachability".to_string());
        validate_claim(&claim)
    }
}
