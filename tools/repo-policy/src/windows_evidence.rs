//! Source-owned Windows evidence obligations, not a Windows qualification gate.
//!
//! The job captures advisory failures successfully. Its successful outcome is
//! neither a passing test/lint verdict nor authenticated artifact custody. This
//! model compares supplied observations only; no workflow calls it, no artifact
//! is downloaded, and even an empty finding set remains `NotEstablished`.

use crate::product_gate_plan::composites::{Event, StepOwner};
use crate::queue_workflow::reconciliation::{
    AdvisoryVerdict, Authority, ObservationOutcome, Subject,
};
use std::collections::{BTreeMap, BTreeSet};

pub const WORKFLOW: &str = ".github/workflows/windows-subset-qualification.yml";
pub const JOB: &str = "windows-advisory-subset";
pub const RUNNER_LABEL: &str = "windows-latest";
pub const SOURCE_PATH: &str = "target/windows-subset-source.json";
pub const SUMMARY_PATH: &str = "target/windows-subset-verdict.md";
pub const SOURCE_OWNER: &str = "Verify admitted integration source before product execution";
pub const GROUP_OWNER: &str = "Verify actual native merge-group subject";
pub const CARGO_EVIDENCE_OWNER: &str = "Fail on missing or empty subset evidence";
pub const SELECTOR_EVIDENCE_OWNER: &str =
    "Require selector consumer receipt, log and native status";
pub const SUMMARY_OWNER: &str = "Summarize subset verdicts";
pub const UPLOAD_OWNER: &str = "Upload subset evidence";
pub const EVIDENCE_OWNERS: [&str; 5] = [
    SOURCE_OWNER,
    GROUP_OWNER,
    CARGO_EVIDENCE_OWNER,
    SELECTOR_EVIDENCE_OWNER,
    SUMMARY_OWNER,
];

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Producer {
    GapArtifacts,
    CargoHome,
    CacheWarning,
    Clippy,
    SelectorRecovery,
}

pub const PRODUCERS: [Producer; 5] = [
    Producer::GapArtifacts,
    Producer::CargoHome,
    Producer::CacheWarning,
    Producer::Clippy,
    Producer::SelectorRecovery,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contract {
    pub label: &'static str,
    pub producer: StepOwner,
    /// The selector has an inline program, not a single Cargo command.
    pub command: Option<&'static str>,
    pub log: &'static str,
    pub status: &'static str,
    pub receipt: Option<&'static str>,
    pub evidence_owner: StepOwner,
    pub requires_nonzero_test_count: bool,
}

impl Producer {
    pub const fn contract(self) -> Contract {
        let (label, name, command, log, status, requires_nonzero_test_count) = match self {
            Self::GapArtifacts => (
                "subset-gap-artifacts",
                "Run lsp::gap_artifacts subset (advisory, #4921)",
                Some("cargo test -p ripr --lib lsp::gap_artifacts"),
                "target/subset-gap-artifacts.log",
                "target/subset-gap-artifacts.status",
                true,
            ),
            Self::CargoHome => (
                "subset-cargo-home",
                "Run Cargo-home identity subset (advisory, #7748)",
                Some("cargo test -p ripr --lib analysis::workspace::cargo_input_identity::tests"),
                "target/subset-cargo-home.log",
                "target/subset-cargo-home.status",
                true,
            ),
            Self::CacheWarning => (
                "subset-cache-warning",
                "Run cache-warning smoke subset (advisory, #4918)",
                Some(
                    "cargo test -p ripr --test cli_smoke unusable_cache_directory_is_reported_once_per_build",
                ),
                "target/subset-cache-warning.log",
                "target/subset-cache-warning.status",
                true,
            ),
            Self::Clippy => (
                "subset-clippy",
                "Run clippy subset (advisory, #4337)",
                Some("cargo clippy -p ripr --all-targets -- -D warnings"),
                "target/subset-clippy.log",
                "target/subset-clippy.status",
                false,
            ),
            Self::SelectorRecovery => (
                "subset-selector-literal",
                "Produce and replay exact-head selector recovery (advisory)",
                None,
                "target/subset-selector-literal.log",
                "target/subset-selector-literal.status",
                false,
            ),
        };
        let selector = matches!(self, Self::SelectorRecovery);
        Contract {
            label,
            producer: StepOwner { name, id: None },
            command,
            log,
            status,
            receipt: if selector {
                Some("target/selector-literal/receipt.json")
            } else {
                None
            },
            evidence_owner: StepOwner {
                name: if selector {
                    SELECTOR_EVIDENCE_OWNER
                } else {
                    CARGO_EVIDENCE_OWNER
                },
                id: None,
            },
            requires_nonzero_test_count,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StepOutcome {
    Success,
    Failure,
    Skipped,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepObservation {
    pub owner: String,
    pub outcome: StepOutcome,
}

/// Caller-supplied file metadata, not inspected bytes or an authenticated receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileClaim {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducerObservation {
    pub producer: Producer,
    pub owner: String,
    pub command: Option<String>,
    /// Wrapper success means capture completed, including an advisory failure.
    pub wrapper_outcome: StepOutcome,
    pub native_exit: Option<u8>,
    pub outcome: ObservationOutcome,
    /// Supplied Cargo terminal-summary count, not an independently parsed log.
    pub executed_tests: Option<u64>,
}

/// Fields actually written by the source step. It does not write runner OS,
/// job id, artifact id or artifact digest; those require separate observations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceReceipt {
    pub proof_profile: String,
    pub request_id: String,
    pub head: String,
    pub base: String,
    pub subject: String,
    pub tree: String,
    pub workflow_generation: String,
    pub producer_run: u64,
    pub producer_attempt: u64,
    pub workflow_ref: String,
    pub workflow_sha: String,
}

/// Supplied upload claims. The current workflow has no upload step id or
/// artifact-id output; these fields are not manufactured from job success.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UploadObservation {
    pub owner: String,
    pub outcome: StepOutcome,
    pub artifact_id: u64,
    pub artifact_name: String,
    pub run_id: u64,
    pub run_attempt: u64,
    pub job_id: u64,
    pub files: Vec<FileClaim>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Observation {
    pub subject: Subject,
    pub workflow_id: u64,
    pub workflow_path: String,
    pub actions_app_id: u64,
    pub job_id: u64,
    pub job_key: String,
    pub runner_os: String,
    pub runner_label: String,
    pub job_outcome: StepOutcome,
    pub source: SourceReceipt,
    pub producers: Vec<ProducerObservation>,
    pub evidence_steps: Vec<StepObservation>,
    pub files: Vec<FileClaim>,
    pub upload: UploadObservation,
}

/// Caller workflow identity resolved separately from these supplied observations.
/// A reusable job's GITHUB_WORKFLOW_REF identifies its caller, not WORKFLOW.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedRun {
    pub workflow_id: u64,
    pub workflow_ref: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub authority: Authority,
    pub findings: Vec<String>,
}

fn lower_hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_subject(subject: &Subject) -> bool {
    subject.event == Event::MergeGroup
        && subject.repository == "EffortlessMetrics/ripr-swarm"
        && subject.pull_number > 0
        && subject.run_id > 0
        && subject.run_attempt > 0
        && [
            &subject.head_sha,
            &subject.base_sha,
            &subject.group_sha,
            &subject.tree_sha,
            &subject.workflow_generation,
        ]
        .into_iter()
        .all(|sha| lower_hex(sha, 40))
        && [
            &subject.producer_generation_sha256,
            &subject.plan_generation_sha256,
            &subject.changed_inventory_sha256,
        ]
        .into_iter()
        .all(|sha| lower_hex(sha, 64))
}

fn files<'a>(
    claims: &'a [FileClaim],
    findings: &mut Vec<String>,
) -> BTreeMap<&'a str, &'a FileClaim> {
    let mut files = BTreeMap::new();
    let mut windows_paths = BTreeSet::new();
    for claim in claims {
        if claim.path.len() > 512
            || !claim.path.starts_with("target/")
            || !claim.path.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-')
            })
            || claim
                .path
                .split('/')
                .any(|part| part.is_empty() || part.ends_with('.'))
            || claim.bytes == 0
            || !lower_hex(&claim.sha256, 64)
        {
            findings.push("invalid file identity, digest or empty evidence".into());
        }
        if !windows_paths.insert(claim.path.to_ascii_lowercase()) {
            findings.push("duplicate or case-aliased Windows evidence path".into());
        }
        files.insert(claim.path.as_str(), claim);
    }
    files
}

/// Inspect the closed source-owned producer set and supplied capture custody.
///
/// `run` must come from an independently resolved caller workflow identity.
/// Matching it does not authenticate it. This does not parse logs, evaluate the
/// selector receipt, verify hashes against bytes, establish native Windows
/// execution, or supply the shared queue's planner/final-result authority.
pub fn inspect(subject: &Subject, run: &ExpectedRun, observation: &Observation) -> Report {
    let mut findings = Vec::new();
    if observation.producers.len() > 16
        || observation.evidence_steps.len() > 16
        || observation.files.len() > 256
        || observation.upload.files.len() > 256
    {
        return Report {
            authority: Authority::NotEstablished,
            findings: vec!["Windows observation exceeds inspection bounds".into()],
        };
    }
    if !valid_subject(subject)
        || observation.subject != *subject
        || run.workflow_id == 0
        || observation.workflow_id != run.workflow_id
        || observation.workflow_path != WORKFLOW
        || observation.actions_app_id != 15368
        || observation.job_id == 0
        || observation.job_key != JOB
        || observation.runner_os != "Windows"
        || observation.runner_label != RUNNER_LABEL
        || observation.job_outcome != StepOutcome::Success
    {
        findings.push("Windows subject, native job or producer identity differs".into());
    }
    let source = &observation.source;
    if source.proof_profile != "merge-group"
        || source.request_id != format!("{}-{}", subject.run_id, subject.run_attempt)
        || source.head != subject.head_sha
        || source.base != subject.base_sha
        || source.subject != subject.group_sha
        || source.tree != subject.tree_sha
        || source.workflow_generation != subject.workflow_generation
        || source.workflow_sha != subject.workflow_generation
        || source.producer_run != subject.run_id
        || source.producer_attempt != subject.run_attempt
        || source.workflow_ref != run.workflow_ref
        || !run
            .workflow_ref
            .starts_with(&format!("{}/.github/workflows/", subject.repository))
        || run
            .workflow_ref
            .split_once('@')
            .is_none_or(|(_, reference)| reference.is_empty())
    {
        findings.push("Windows source receipt differs from the admitted group".into());
    }
    let mut producers = BTreeSet::new();
    for row in &observation.producers {
        let contract = row.producer.contract();
        if !producers.insert(row.producer) {
            findings.push("duplicate Windows producer".into());
        }
        if row.owner != contract.producer.name
            || row.command.as_deref() != contract.command
            || row.wrapper_outcome != StepOutcome::Success
        {
            findings.push(format!(
                "Windows producer/capture owner differs: {}",
                contract.label
            ));
        }
        let expected = row.native_exit.map(|status| {
            ObservationOutcome::EvidenceCaptured(if status == 0 {
                AdvisoryVerdict::Pass
            } else {
                AdvisoryVerdict::Fail
            })
        });
        if expected != Some(row.outcome) {
            findings.push(format!(
                "advisory verdict missing or promoted to proof: {}",
                contract.label
            ));
        }
        if row.producer == Producer::SelectorRecovery && !matches!(row.native_exit, Some(0 | 1)) {
            findings.push(
                "selector native status disagrees with its PASS/FAIL receipt contract".into(),
            );
        }
        if contract.requires_nonzero_test_count
            && row.native_exit == Some(0)
            && row.executed_tests.is_none_or(|count| count == 0)
        {
            findings.push(format!(
                "successful Cargo filter executed no test: {}",
                contract.label
            ));
        }
    }
    for producer in PRODUCERS {
        if !producers.contains(&producer) {
            findings.push(format!(
                "missing Windows producer: {}",
                producer.contract().label
            ));
        }
    }
    let mut seen_steps = BTreeSet::new();
    for step in &observation.evidence_steps {
        if !seen_steps.insert(step.owner.as_str())
            || !EVIDENCE_OWNERS.contains(&step.owner.as_str())
            || step.outcome != StepOutcome::Success
        {
            findings.push("missing, duplicate or non-successful Windows evidence owner".into());
        }
    }
    if EVIDENCE_OWNERS
        .iter()
        .any(|name| !seen_steps.contains(name))
    {
        findings.push("missing mandatory Windows evidence owner".into());
    }
    let captured = files(&observation.files, &mut findings);
    let uploaded = files(&observation.upload.files, &mut findings);
    let mut required = BTreeSet::from([SOURCE_PATH, SUMMARY_PATH]);
    for producer in PRODUCERS {
        let contract = producer.contract();
        required.extend([contract.log, contract.status]);
        required.extend(contract.receipt);
    }
    for path in &required {
        if !captured.contains_key(path) {
            findings.push(format!("missing Windows evidence file: {path}"));
        }
    }
    if captured
        .keys()
        .any(|path| !required.contains(path) && !path.starts_with("target/selector-literal/"))
    {
        findings.push("unexpected Windows evidence file".into());
    }
    let upload = &observation.upload;
    if upload.owner != UPLOAD_OWNER
        || upload.outcome != StepOutcome::Success
        || upload.artifact_id == 0
        || upload.artifact_name
            != format!(
                "ripr-windows-advisory-subset-{}-{}",
                subject.run_id, subject.run_attempt
            )
        || upload.run_id != subject.run_id
        || upload.run_attempt != subject.run_attempt
        || upload.job_id != observation.job_id
        || captured != uploaded
    {
        findings.push("Windows upload custody, completeness or identity differs".into());
    }
    Report {
        authority: Authority::NotEstablished,
        findings,
    }
}

#[cfg(test)]
mod tests;
