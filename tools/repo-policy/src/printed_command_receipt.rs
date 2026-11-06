//! Bounded structural inspection of printed-command terminal receipts.
//!
//! No network, filesystem reads, workflow invocation or result authorization.
//! All records, hashes, job outcomes and custody labels are supplied observations,
//! not authenticated evidence. The caller must obtain them independently from a
//! trusted producer for the exact subject. Even a clean report is NotEstablished:
//! terminal replay completion is not unconditional command correctness, full
//! Cargo/job success, authenticated artifact custody or native queue authority.

use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const MAX_DOCUMENT: usize = 16 * 1024 * 1024;
const MAX_OBSERVATION: usize = 64 * 1024;
const MAX_CASES: usize = 100_000;
const WORKFLOW: &str = ".github/workflows/printed-command-paste.yml";
const PLATFORMS: [(&str, &str, &str, &[&str]); 3] = [
    (
        "ubuntu-latest",
        "Linux",
        "linux",
        &["bash", "sh", "zsh", "pwsh"],
    ),
    (
        "macos-latest",
        "macOS",
        "macos",
        &["bash", "sh", "zsh", "pwsh"],
    ),
    (
        "windows-latest",
        "Windows",
        "windows",
        &["bash", "pwsh", "powershell"],
    ),
];

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Subject {
    pub repository: String,
    pub head_sha: String,
    pub base_sha: String,
    pub group_sha: String,
    pub tree_sha: String,
    pub workflow_generation: String,
    pub run_id: String,
    pub run_attempt: String,
    pub request_id: String,
}

/// Independently selected caller configuration, never inferred from a receipt.
#[derive(Clone, Debug)]
pub struct Expected {
    pub subject: Subject,
    pub qualification_started_at_ms: u64,
    pub deadline_at_ms: u64,
}

/// Bytes supplied by a future authenticated, bounded retrieval adapter. The OS
/// label and JSON are untrusted here, including any artifact custody assertions.
pub struct CellInput<'a> {
    pub matrix_os: &'a str,
    pub receipt: &'a [u8],
    pub inventory: &'a [u8],
    pub observation: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Status {
    NotEstablished,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub status: Status,
    pub findings: Vec<String>,
    pub cells: Vec<CellDisclosure>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReplayObservation {
    CompletedWithDisclosedGaps,
    CompletedWithoutDisclosedGaps,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellDisclosure {
    pub matrix_os: String,
    pub inventory_sha256: String,
    pub inventory_case_count: usize,
    pub replay_observation: ReplayObservation,
    pub shells: Vec<ShellReplay>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CaseDisclosure {
    pub case_id: usize,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ShellReplay {
    pub shell: String,
    pub executed_case_ids: Vec<usize>,
    pub excluded: Vec<CaseDisclosure>,
    pub known_gaps: Vec<CaseDisclosure>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_id: String,
    schema_version: u64,
    claim: String,
    repository: String,
    head_sha: String,
    base_sha: String,
    group_sha: String,
    tree_sha: String,
    workflow_generation: String,
    request_id: String,
    run_id: String,
    run_attempt: String,
    job_key: String,
    matrix_os: String,
    runner_os: String,
    runtime_os: String,
    inventory_sha256: String,
    inventory_case_count: usize,
    required_shells: Vec<String>,
    shells: Vec<ShellReplay>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InventoryEntry {
    case_id: usize,
    source: String,
    origin: String,
    bash: String,
    #[serde(deserialize_with = "required_nullable")]
    powershell: Option<String>,
    bash_only: bool,
    powershell_unavailable: bool,
}

fn required_nullable<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactObservation {
    id: u64,
    name: String,
    run_id: String,
    run_attempt: String,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JobObservation {
    id: u64,
    status: String,
    conclusion: String,
    replay_conclusion: String,
    receipt_upload_conclusion: String,
    started_at_ms: u64,
    completed_at_ms: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeadlineObservation {
    entry_conclusion: String,
    completion_conclusion: String,
    completed_at_ms: u64,
    deadline_at_ms: u64,
}

/// Normalized observation schema, not a GitHub API response or attestation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Observation {
    schema_id: String,
    schema_version: u64,
    subject: Subject,
    workflow_path: String,
    job_key: String,
    matrix_os: String,
    job: JobObservation,
    deadline: DeadlineObservation,
    inventory_artifact: ArtifactObservation,
    receipt_artifact: ArtifactObservation,
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_owned())
}

fn hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn decimal(value: &str) -> bool {
    !value.starts_with('0')
        && value.len() <= 20
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u64>().is_ok_and(|n| n > 0)
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8], limit: usize) -> Result<T, String> {
    require(
        !bytes.is_empty() && bytes.len() <= limit,
        "missing or oversized JSON",
    )?;
    // Typed closed structs reject unknown and duplicate fields at every level;
    // serde_json rejects non-UTF-8, nonfinite values and trailing JSON documents.
    serde_json::from_slice(bytes).map_err(|error| format!("malformed JSON: {error}"))
}

fn validate_expected(expected: &Expected) -> Result<(), String> {
    let s = &expected.subject;
    require(
        s.repository == "EffortlessMetrics/ripr-swarm"
            && [
                &s.head_sha,
                &s.base_sha,
                &s.group_sha,
                &s.tree_sha,
                &s.workflow_generation,
            ]
            .into_iter()
            .all(|v| hex(v, 40))
            && decimal(&s.run_id)
            && decimal(&s.run_attempt)
            && s.request_id == format!("{}-{}", s.run_id, s.run_attempt),
        "invalid independently selected subject",
    )?;
    require(
        expected.qualification_started_at_ms > 0
            && expected.qualification_started_at_ms.checked_add(1_200_000)
                == Some(expected.deadline_at_ms),
        "invalid selected whole-route deadline",
    )
}

fn inventory(bytes: &[u8], count: usize) -> Result<Vec<InventoryEntry>, String> {
    require(
        bytes.len() <= MAX_DOCUMENT && (15..=MAX_CASES).contains(&count),
        "inventory denominator or bound invalid",
    )?;
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("inventory is not UTF-8: {error}"))?;
    let mut rows = Vec::new();
    for (id, line) in text.lines().enumerate() {
        require(id < count, "inventory has extra cases")?;
        let row: InventoryEntry = decode(line.as_bytes(), MAX_DOCUMENT)?;
        require(
            row.case_id == id
                && !row.source.is_empty()
                && !row.bash.is_empty()
                && matches!(row.origin.as_str(), "Foreign" | "Portable")
                && row.powershell.as_ref().is_none_or(|s| !s.is_empty()),
            "inventory identity, command or eligibility invalid",
        )?;
        rows.push(row);
    }
    require(rows.len() == count, "inventory has missing cases")?;
    Ok(rows)
}

fn artifact(a: &ArtifactObservation, name: &str, bytes: &[u8], s: &Subject) -> Result<(), String> {
    require(
        a.id > 0
            && a.name == name
            && a.run_id == s.run_id
            && a.run_attempt == s.run_attempt
            && hex(&a.sha256, 64)
            && a.sha256 == digest(bytes),
        "artifact identity, supplied custody or bytes differ",
    )
}

fn cell(expected: &Expected, input: &CellInput<'_>) -> Result<(CellDisclosure, [u64; 3]), String> {
    let (_, runner, runtime, required) = PLATFORMS
        .iter()
        .find(|p| p.0 == input.matrix_os)
        .ok_or("unknown OS cell")?;
    // Bound every raw input before decoding or hashing any of them.
    require(
        input.inventory.len() <= MAX_DOCUMENT
            && input.receipt.len() <= MAX_DOCUMENT
            && input.observation.len() <= MAX_OBSERVATION,
        "cell input exceeds bound",
    )?;
    let r: Receipt = decode(input.receipt, MAX_DOCUMENT)?;
    let o: Observation = decode(input.observation, MAX_OBSERVATION)?;
    let s = &expected.subject;
    require(
        r.schema_id == "ripr.printed-command-terminal.v1"
            && r.schema_version == 1
            && r.claim == "main_replay_test_completed_with_disclosed_known_gaps"
            && r.repository == s.repository
            && r.head_sha == s.head_sha
            && r.base_sha == s.base_sha
            && r.group_sha == s.group_sha
            && r.tree_sha == s.tree_sha
            && r.workflow_generation == s.workflow_generation
            && r.request_id == s.request_id
            && r.run_id == s.run_id
            && r.run_attempt == s.run_attempt
            && r.job_key == "paste"
            && r.matrix_os == input.matrix_os
            && r.runner_os == *runner
            && r.runtime_os == *runtime,
        "receipt subject, platform or claim differs",
    )?;
    require(
        o.schema_id == "ripr.printed-command-observation.v1"
            && o.schema_version == 1
            && o.subject == *s
            && o.workflow_path == WORKFLOW
            && o.job_key == "paste"
            && o.matrix_os == input.matrix_os,
        "job observation subject or producer differs",
    )?;
    let job = &o.job;
    let deadline = &o.deadline;
    require(
        job.id > 0
            && job.status == "completed"
            && job.conclusion == "success"
            && job.replay_conclusion == "success"
            && job.receipt_upload_conclusion == "success"
            && deadline.entry_conclusion == "success"
            && deadline.completion_conclusion == "success",
        "unsuitable job, replay, upload or deadline outcome",
    )?;
    require(
        deadline.deadline_at_ms == expected.deadline_at_ms
            && expected.qualification_started_at_ms <= job.started_at_ms
            && job.started_at_ms <= deadline.completed_at_ms
            && deadline.completed_at_ms <= job.completed_at_ms
            && job.completed_at_ms <= expected.deadline_at_ms,
        "job or completion outside selected whole-route deadline",
    )?;
    artifact(
        &o.inventory_artifact,
        &format!("printed-commands-{}", input.matrix_os),
        input.inventory,
        s,
    )?;
    artifact(
        &o.receipt_artifact,
        &format!(
            "printed-command-receipt-{}-{}-{}",
            input.matrix_os, s.run_id, s.run_attempt
        ),
        input.receipt,
        s,
    )?;
    require(
        r.inventory_sha256 == o.inventory_artifact.sha256,
        "receipt inventory digest differs",
    )?;
    let rows = inventory(input.inventory, r.inventory_case_count)?;
    let declared: BTreeSet<_> = r.required_shells.iter().map(String::as_str).collect();
    require(
        declared.len() == r.required_shells.len() && declared == required.iter().copied().collect(),
        "receipt cannot reduce or redefine the canonical required shells",
    )?;
    require(
        !r.shells.is_empty() && r.shells.len() <= 5,
        "missing or excessive shell records",
    )?;
    let mut seen = BTreeSet::new();
    for shell in &r.shells {
        require(
            matches!(
                shell.shell.as_str(),
                "bash" | "sh" | "zsh" | "pwsh" | "powershell"
            ) && seen.insert(shell.shell.as_str()),
            "unknown or duplicate replay shell",
        )?;
        let ps = matches!(shell.shell.as_str(), "pwsh" | "powershell");
        let mut executed = Vec::new();
        let mut excluded = Vec::new();
        for row in &rows {
            let reason = match (ps, row.bash_only, row.powershell_unavailable) {
                (true, true, true) => Some("bash_only_and_powershell_unavailable"),
                (true, true, false) => Some("bash_only"),
                (true, false, true) => Some("powershell_unavailable"),
                _ => None,
            };
            if let Some(reason) = reason {
                excluded.push(CaseDisclosure {
                    case_id: row.case_id,
                    reason: reason.to_owned(),
                });
            } else {
                executed.push(row.case_id);
            }
        }
        require(
            !executed.is_empty()
                && shell.executed_case_ids == executed
                && shell.excluded == excluded,
            "shell executed/excluded denominator differs from inventory",
        )?;
        let mut gaps = BTreeSet::new();
        require(
            shell.known_gaps.len() <= executed.len()
                && shell.known_gaps.iter().all(|gap| {
                    executed.binary_search(&gap.case_id).is_ok()
                        && gaps.insert(gap.case_id)
                        && !gap.reason.trim().is_empty()
                        && gap.reason.len() <= 4096
                }),
            "malformed, duplicate or unexecuted known gap",
        )?;
    }
    require(
        required.iter().all(|name| seen.contains(name)),
        "required shell replay is missing",
    )?;
    Ok((
        CellDisclosure {
            matrix_os: input.matrix_os.to_owned(),
            inventory_sha256: r.inventory_sha256,
            inventory_case_count: r.inventory_case_count,
            replay_observation: if r.shells.iter().any(|shell| !shell.known_gaps.is_empty()) {
                ReplayObservation::CompletedWithDisclosedGaps
            } else {
                ReplayObservation::CompletedWithoutDisclosedGaps
            },
            shells: r.shells,
        },
        [job.id, o.inventory_artifact.id, o.receipt_artifact.id],
    ))
}

/// Inspect all three reviewed matrix cells, retaining explicit exclusions and
/// gaps. Empty findings mean structural agreement only. There is deliberately
/// no successful authorization disposition or boolean pass/reuse output.
pub fn inspect(expected: &Expected, inputs: &[CellInput<'_>]) -> Report {
    let mut report = Report {
        status: Status::NotEstablished,
        findings: Vec::new(),
        cells: Vec::new(),
    };
    if let Err(error) = validate_expected(expected) {
        report.findings.push(error);
        return report;
    }
    if inputs.len() != PLATFORMS.len() {
        report
            .findings
            .push("exactly three OS cells are required".to_owned());
        return report;
    }
    let mut names = BTreeSet::new();
    let mut jobs = BTreeSet::new();
    let mut artifacts = BTreeSet::new();
    for (index, input) in inputs.iter().enumerate() {
        if !names.insert(input.matrix_os) {
            report.findings.push("duplicate OS cell".to_owned());
            continue;
        }
        match cell(expected, input) {
            Ok((disclosure, [job, inventory, receipt])) => {
                if !jobs.insert(job) || !artifacts.insert(inventory) || !artifacts.insert(receipt) {
                    report
                        .findings
                        .push("reused job or artifact identity across cells".to_owned());
                }
                report.cells.push(disclosure);
            }
            Err(error) => report.findings.push(format!("cell {index}: {error}")),
        }
    }
    if !PLATFORMS.iter().all(|p| names.contains(p.0)) {
        report.findings.push("canonical OS cell missing".to_owned());
    }
    report
}

#[cfg(test)]
mod tests;
