//! Source-derived, structural wheel result coverage for native merge groups.
//!
//! This module has no workflow/CLI consumer and accepts unauthenticated claims,
//! not provider receipts. It cannot qualify a candidate, authenticate artifacts,
//! select/skip/reuse work, or establish a deadline. Even an empty finding set has
//! `NotEstablished` authority. The fixed denominator is this wheel family only.

use crate::product_gate_plan::composites::Event;
use crate::queue_workflow::reconciliation::{Authority, Subject};
use std::collections::{BTreeMap, BTreeSet};

mod contract;
pub use contract::{Cell, Definition, Step, definitions};
#[cfg(test)]
mod tests;

const ACTIONS_APP_ID: u64 = 15368;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Success,
    Failure,
    Skipped,
    Cancelled,
    TimedOut,
    InProgress,
    Unknown,
}

/// Outcome and conclusion are both necessary: continue-on-error can rewrite
/// a failing outcome into a successful conclusion.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StepObservation {
    pub step: Step,
    pub outcome: State,
    pub conclusion: State,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ArtifactKind {
    Wheelhouse,
    OrdinaryRunner,
    InstalledOrdinaryEvidence,
}

impl ArtifactKind {
    fn producer(self) -> Cell {
        match self {
            Self::Wheelhouse => Cell::Build,
            Self::OrdinaryRunner => Cell::SourceContract,
            Self::InstalledOrdinaryEvidence => Cell::Pip,
        }
    }
}

/// Digests describe declared bytes, not authentication or observed execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Content {
    Wheelhouse {
        wheel_sha256: String,
        payload_sha256: String,
    },
    OrdinaryRunner {
        runner_sha256: String,
    },
    InstalledOrdinaryEvidence {
        wheel_sha256: String,
        payload_sha256: String,
        runner_sha256: String,
    },
}

impl Content {
    fn kind(&self) -> ArtifactKind {
        match self {
            Self::Wheelhouse { .. } => ArtifactKind::Wheelhouse,
            Self::OrdinaryRunner { .. } => ArtifactKind::OrdinaryRunner,
            Self::InstalledOrdinaryEvidence { .. } => ArtifactKind::InstalledOrdinaryEvidence,
        }
    }

    fn valid(&self) -> bool {
        match self {
            Self::Wheelhouse {
                wheel_sha256,
                payload_sha256,
            } => hex(wheel_sha256, 64) && hex(payload_sha256, 64),
            Self::OrdinaryRunner { runner_sha256 } => hex(runner_sha256, 64),
            Self::InstalledOrdinaryEvidence {
                wheel_sha256,
                payload_sha256,
                runner_sha256,
            } => hex(wheel_sha256, 64) && hex(payload_sha256, 64) && hex(runner_sha256, 64),
        }
    }
}

/// A declaration of the provider-owned archive and its source/run/job origin.
/// Matching all these supplied fields still does not establish provider custody.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactObservation {
    pub artifact_id: u64,
    pub archive_sha256: String,
    pub producer_job_id: u64,
    pub run_id: u64,
    pub run_attempt: u64,
    pub subject_sha: String,
    pub source_tree: String,
    pub content: Content,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobObservation {
    pub cell: Cell,
    pub subject: Subject,
    pub actions_app_id: u64,
    pub workflow_id: u64,
    pub workflow_path: String,
    pub job_id: u64,
    pub job_key: String,
    pub matrix_key: String,
    pub conclusion: State,
    pub steps: Vec<StepObservation>,
    pub produced: Vec<ArtifactObservation>,
    pub consumed: Vec<ArtifactObservation>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissingAuthority {
    AuthenticatedSubjectAndGeneration,
    AuthenticatedJobAndStepExecution,
    AuthenticatedArtifactCustodyAndBytes,
    NativeRunDeadlineAndTerminalRelease,
    FullPortfolioAndCurrentEligibility,
    InstalledCompatibilityAndPublication,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub authority: Authority,
    pub findings: Vec<String>,
    /// These gaps remain even when the structural claims match perfectly.
    pub missing_authority: [MissingAuthority; 6],
}

fn hex(value: &str, length: usize) -> bool {
    value.len() == length
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
        .all(|value| hex(value, 40))
        && [
            &subject.producer_generation_sha256,
            &subject.plan_generation_sha256,
            &subject.changed_inventory_sha256,
        ]
        .into_iter()
        .all(|value| hex(value, 64))
}

/// Inspect the five source-owned cells, never a caller-pruned expected list.
/// Dispatch diagnostics and the advisory MCP pair are not merge-group proof.
/// Successful deadline-step claims are checked structurally but cannot prove
/// the native absolute deadline, queue delay, cancellation, or terminal release.
pub fn inspect(subject: &Subject, workflow_id: u64, jobs: &[JobObservation]) -> Report {
    let mut report = Report {
        authority: Authority::NotEstablished,
        findings: Vec::new(),
        missing_authority: [
            MissingAuthority::AuthenticatedSubjectAndGeneration,
            MissingAuthority::AuthenticatedJobAndStepExecution,
            MissingAuthority::AuthenticatedArtifactCustodyAndBytes,
            MissingAuthority::NativeRunDeadlineAndTerminalRelease,
            MissingAuthority::FullPortfolioAndCurrentEligibility,
            MissingAuthority::InstalledCompatibilityAndPublication,
        ],
    };
    if !valid_subject(subject) || workflow_id == 0 {
        report
            .findings
            .push("invalid native wheel subject or workflow identity".into());
    }
    if jobs.len() > 5
        || jobs
            .iter()
            .any(|job| job.steps.len() > 16 || job.produced.len() > 3 || job.consumed.len() > 3)
    {
        report
            .findings
            .push("wheel observation exceeds fixed source bounds".into());
        return report;
    }
    let findings = &mut report.findings;
    let mut cells = BTreeMap::new();
    let mut job_ids = BTreeSet::new();
    let mut artifact_ids = BTreeSet::new();
    for job in jobs {
        if cells.insert(job.cell, job).is_some() {
            findings.push(format!("duplicate wheel cell: {:?}", job.cell));
        }
        if job.job_id == 0 || !job_ids.insert(job.job_id) {
            findings.push(format!(
                "invalid or shared native job identity: {:?}",
                job.cell
            ));
        }
        let definition = job.cell.definition();
        if job.subject != *subject
            || !valid_subject(&job.subject)
            || job.actions_app_id != ACTIONS_APP_ID
            || job.workflow_id != workflow_id
            || job.workflow_path != super::WORKFLOW
            || job.job_key != definition.job_key
            || job.matrix_key != definition.matrix_key
        {
            findings.push(format!("wheel subject or producer differs: {:?}", job.cell));
        }
        if job.conclusion != State::Success {
            findings.push(format!("wheel job did not succeed: {:?}", job.cell));
        }
        if job.steps.iter().map(|step| step.step).collect::<Vec<_>>() != definition.steps {
            findings.push(format!(
                "missing, extra, or reordered wheel steps: {:?}",
                job.cell
            ));
        }
        for step in &job.steps {
            if step.outcome != State::Success || step.conclusion != State::Success {
                findings.push(format!(
                    "wheel step did not succeed: {:?}/{:?}",
                    job.cell, step.step
                ));
            }
        }
        for artifact in &job.produced {
            if artifact.artifact_id == 0
                || !artifact_ids.insert(artifact.artifact_id)
                || !hex(&artifact.archive_sha256, 64)
                || !artifact.content.valid()
                || artifact.content.kind().producer() != job.cell
                || artifact.producer_job_id != job.job_id
                || artifact.run_id != subject.run_id
                || artifact.run_attempt != subject.run_attempt
                || artifact.subject_sha != subject.group_sha
                || artifact.source_tree != subject.tree_sha
            {
                findings.push(format!(
                    "invalid wheel artifact origin or digest: {:?}",
                    job.cell
                ));
            }
        }
        if job
            .produced
            .iter()
            .map(|artifact| artifact.content.kind())
            .collect::<Vec<_>>()
            != definition.produces
        {
            findings.push(format!("wheel output inventory differs: {:?}", job.cell));
        }
        if job
            .consumed
            .iter()
            .map(|artifact| artifact.content.kind())
            .collect::<Vec<_>>()
            != definition.consumes
        {
            findings.push(format!("wheel input inventory differs: {:?}", job.cell));
        }
    }
    for definition in definitions() {
        if !cells.contains_key(&definition.cell) {
            findings.push(format!("missing wheel cell: {:?}", definition.cell));
        }
    }
    for job in jobs {
        for input in &job.consumed {
            let producer = cells.get(&input.content.kind().producer());
            let produced = producer.and_then(|producer| {
                producer
                    .produced
                    .iter()
                    .find(|artifact| artifact.content.kind() == input.content.kind())
            });
            if produced != Some(input) {
                findings.push(format!(
                    "wheel artifact lacks exact producer-to-consumer binding: {:?}",
                    job.cell
                ));
            }
        }
    }
    inspect_installed_binding(&cells, findings);
    report
}

fn inspect_installed_binding(cells: &BTreeMap<Cell, &JobObservation>, findings: &mut Vec<String>) {
    let content = |cell: Cell| {
        cells
            .get(&cell)
            .and_then(|job| job.produced.first())
            .map(|artifact| &artifact.content)
    };
    if let (
        Some(Content::Wheelhouse {
            wheel_sha256: wheel,
            payload_sha256: payload,
        }),
        Some(Content::OrdinaryRunner {
            runner_sha256: runner,
        }),
        Some(Content::InstalledOrdinaryEvidence {
            wheel_sha256,
            payload_sha256,
            runner_sha256,
        }),
    ) = (
        content(Cell::Build),
        content(Cell::SourceContract),
        content(Cell::Pip),
    ) && (wheel != wheel_sha256 || payload != payload_sha256 || runner != runner_sha256)
    {
        findings
            .push("installed pip journey does not bind the built wheel and source runner".into());
    }
}
