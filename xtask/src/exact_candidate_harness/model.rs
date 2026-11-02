//! Generic exact-candidate proof DTO (#4510). JSON and Markdown both render from
//! [`CandidateProofPacket`]; neither projection may strengthen a non-passing state.

#![expect(
    dead_code,
    reason = "policy:4510-harness-api: shared DTO and render surface for successor qualification corpora"
)]

use serde::Serialize;

pub(crate) const SCHEMA: &str = "ripr-exact-candidate-harness-v1";

/// Harness-owned execution states. Child corpora may refine these but cannot
/// weaken a non-pass state in human output.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HarnessExecutionState {
    Passed,
    SemanticFailure,
    WrongSubject,
    WrongRootOrWrite,
    ProcessAbort,
    NonterminalDeadline,
    CleanupFailed,
    InstrumentFailure,
    NotRun,
}

impl HarnessExecutionState {
    pub(crate) fn is_pass(self) -> bool {
        matches!(self, Self::Passed)
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::SemanticFailure => "semantic_failure",
            Self::WrongSubject => "wrong_subject",
            Self::WrongRootOrWrite => "wrong_root_or_write",
            Self::ProcessAbort => "process_abort",
            Self::NonterminalDeadline => "nonterminal_deadline",
            Self::CleanupFailed => "cleanup_failed",
            Self::InstrumentFailure => "instrument_failure",
            Self::NotRun => "not_run",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct InputDigest {
    pub(crate) path: String,
    pub(crate) bytes: usize,
    pub(crate) sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct BinaryIdentity {
    pub(crate) path: String,
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
    pub(crate) reported_version: String,
    pub(crate) path_resolution: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct HostIdentity {
    pub(crate) os: String,
    pub(crate) arch: String,
    pub(crate) family: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SubjectCounts {
    pub(crate) selected: usize,
    pub(crate) executed: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CandidateProofRow {
    pub(crate) id: String,
    pub(crate) state: HarnessExecutionState,
    pub(crate) detail: String,
    pub(crate) argv: Vec<String>,
    pub(crate) cwd: String,
    pub(crate) inputs: Vec<InputDigest>,
    pub(crate) stdout_sha256: String,
    pub(crate) stderr_sha256: String,
    pub(crate) artifact_sha256: Option<String>,
    pub(crate) cleanup: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CandidateProofControl {
    pub(crate) id: String,
    pub(crate) expected: String,
    pub(crate) observed: Option<String>,
    pub(crate) discriminated: bool,
    pub(crate) detail: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Aggregate {
    Passed,
    Failed,
}

impl Aggregate {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CandidateProofPacket {
    pub(crate) schema: &'static str,
    pub(crate) candidate: BinaryIdentity,
    pub(crate) host: HostIdentity,
    pub(crate) environment_boundary: Vec<String>,
    pub(crate) counts: SubjectCounts,
    pub(crate) rows: Vec<CandidateProofRow>,
    pub(crate) controls: Vec<CandidateProofControl>,
    pub(crate) aggregate: Aggregate,
    pub(crate) aggregate_reasons: Vec<String>,
    pub(crate) non_claims: Vec<String>,
    pub(crate) digest: String,
}

pub(crate) fn render_json(packet: &CandidateProofPacket) -> Result<String, String> {
    serde_json::to_string_pretty(packet)
        .map(|mut text| {
            text.push('\n');
            text
        })
        .map_err(|error| format!("failed to render exact-candidate harness JSON: {error}"))
}

pub(crate) fn render_markdown(packet: &CandidateProofPacket) -> String {
    let mut lines = vec![
        "# Exact candidate harness".to_string(),
        String::new(),
        format!("Aggregate: **{}**", packet.aggregate.as_str()),
        format!("Packet digest: `{}`", packet.digest),
        format!(
            "Candidate: `{}` sha256 `{}` ({} bytes, `{}`)",
            packet.candidate.path,
            packet.candidate.sha256,
            packet.candidate.bytes,
            packet.candidate.reported_version
        ),
        format!("Resolution: {}", packet.candidate.path_resolution),
        format!(
            "Host: {} {} ({})",
            packet.host.os, packet.host.arch, packet.host.family
        ),
        String::new(),
        format!(
            "Subjects: selected {} executed {} passed {} failed {} skipped {}",
            packet.counts.selected,
            packet.counts.executed,
            packet.counts.passed,
            packet.counts.failed,
            packet.counts.skipped
        ),
        String::new(),
    ];
    if !packet.aggregate_reasons.is_empty() {
        lines.push("Aggregate reasons:".to_string());
        for reason in &packet.aggregate_reasons {
            lines.push(format!("- {reason}"));
        }
        lines.push(String::new());
    }
    lines.push("## Rows".to_string());
    lines.push(String::new());
    lines.push("| Row | State | Cleanup | Detail |".to_string());
    lines.push("|---|---|---|---|".to_string());
    for row in &packet.rows {
        lines.push(format!(
            "| `{}` | {} | {} | {} |",
            row.id,
            row.state.as_str(),
            row.cleanup,
            row.detail.replace('|', "\\|")
        ));
    }
    lines.push(String::new());
    lines.push("## Controls".to_string());
    lines.push(String::new());
    lines.push("| Control | Expected | Observed | Discriminated |".to_string());
    lines.push("|---|---|---|---|".to_string());
    for control in &packet.controls {
        lines.push(format!(
            "| `{}` | {} | {} | {} |",
            control.id,
            control.expected,
            control.observed.as_deref().unwrap_or("not_run"),
            if control.discriminated { "yes" } else { "no" }
        ));
    }
    lines.push(String::new());
    lines.push("## Environment boundary".to_string());
    lines.push(String::new());
    for item in &packet.environment_boundary {
        lines.push(format!("- {item}"));
    }
    lines.push(String::new());
    lines.push("## Non-claims".to_string());
    lines.push(String::new());
    for item in &packet.non_claims {
        lines.push(format!("- {item}"));
    }
    lines.push(String::new());
    lines.join("\n")
}
