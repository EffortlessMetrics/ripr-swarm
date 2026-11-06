//! Source-only portfolio inspection. No caller consumes this as a gate.
//!
//! A reported success is an untrusted assertion, never an established result.
//! This module does not authenticate GitHub, artifacts, the native group, or a
//! deadline, and does not select, skip, reuse, or authorize work.

use crate::feature_obligations;
use crate::printed_command_receipt as printed;
use crate::product_gate_plan::composites::Event;
use crate::product_gate_plan::{ProductGateId, product_gate_definitions};
use crate::python_wheel::results as wheel;
use crate::queue_workflow::reconciliation::{
    self as recon, Authority, ExpectedRow, Family, ObservationOutcome, ObservedRow, Subject,
};
use crate::windows_evidence as windows;
use std::collections::{BTreeMap, BTreeSet};

const PRINTED_OS: [&str; 3] = ["ubuntu-latest", "macos-latest", "windows-latest"];
const MAX_ROWS: usize = 4096;

/// The identity is supplied, not authenticated. All observations must refer to
/// this same H/B/G/tree, generations and native run/attempt.
pub struct Input<'a> {
    pub subject: &'a Subject,
    pub rust_expected: &'a [ExpectedRow],
    pub rust_observed: &'a [ObservedRow],
    pub windows: Option<(&'a windows::ExpectedRun, &'a windows::Observation)>,
    pub wheel_workflow_id: u64,
    pub wheel_jobs: &'a [wheel::JobObservation],
    pub printed: Option<(&'a printed::Expected, &'a [printed::CellInput<'a>])>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Obligation {
    Rust(ProductGateId),
    Windows(windows::Producer),
    Wheel(wheel::Cell),
    /// The indexes point to the source inventory, retaining its actual owner,
    /// command, condition, role and order without inventing a step ID.
    Feature {
        child: usize,
        command: usize,
    },
    Printed(&'static str),
}

/// Describes supplied claims only. In particular `ReportedSuccess` cannot be
/// used as a pass, and captured advisory failure stays distinct from success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Observation {
    Missing,
    ReportedSuccess,
    Failed,
    Skipped,
    Cancelled,
    TimedOut,
    Incomplete,
    Conflicting,
    AdvisoryPassCaptured,
    AdvisoryFailureCaptured,
    ReplayWithKnownGaps,
    ReplayWithExclusions,
    ReplayWithGapsAndExclusions,
    ReplayWithoutKnownGapsOrExclusions,
}

/// Supplied provider identifiers, not authenticated custody. `None` means no
/// unique identity was available; in particular the printed inspector does not
/// expose its raw job identity and no identity is invented for feature commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProducerIdentity {
    pub workflow_id: u64,
    pub job_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Row {
    pub obligation: Obligation,
    pub authority: Authority,
    pub observation: Observation,
    pub producer: Option<ProducerIdentity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissingAuthority {
    AuthenticatedGithubTransportAndSubject,
    CompleteArtifactCustodyAndBytes,
    FullPortfolioExecutionAndCurrentEligibility,
    RunnerCapacity,
    WholeRouteDeadlineAndTerminalRelease,
    FeatureCommandExecution,
    /// The printed receipt inspector does not expose job/workflow IDs for
    /// cross-family comparison. Even a clean printed report leaves this open.
    PrintedCrossFamilyProducerIdentity,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    /// The exact supplied identity, absent only when its strings exceed bounds.
    /// Retaining it does not authenticate it.
    pub subject: Option<Subject>,
    pub authority: Authority,
    pub rows: Vec<Row>,
    pub findings: Vec<String>,
    /// Disclosures retain shell-specific exclusions and known gaps verbatim.
    pub printed_disclosures: Vec<printed::CellDisclosure>,
    pub missing_authority: [MissingAuthority; 7],
}

fn row(obligation: Obligation, observation: Observation) -> Row {
    Row {
        obligation,
        authority: Authority::NotEstablished,
        observation,
        producer: None,
    }
}

fn printed_subject_matches(subject: &Subject, expected: &printed::Expected) -> bool {
    let s = &expected.subject;
    s.repository == subject.repository
        && s.head_sha == subject.head_sha
        && s.base_sha == subject.base_sha
        && s.group_sha == subject.group_sha
        && s.tree_sha == subject.tree_sha
        && s.workflow_generation == subject.workflow_generation
        && s.run_id == subject.run_id.to_string()
        && s.run_attempt == subject.run_attempt.to_string()
        && s.request_id == format!("{}-{}", subject.run_id, subject.run_attempt)
}

/// Inspect fixed source inventories and keep all outcomes NotEstablished. The
/// Rust declaration is checked by the existing shared reconciler, including
/// composite command order and artifact edges; it is never the denominator.
/// Feature results are intentionally not accepted: the inventory describes
/// source commands but does not own execution receipts or applicability proof.
pub fn inspect(input: &Input<'_>) -> Report {
    let subject_bounded = input.subject.repository.len() <= 256
        && [
            &input.subject.head_sha,
            &input.subject.base_sha,
            &input.subject.group_sha,
            &input.subject.tree_sha,
            &input.subject.workflow_generation,
            &input.subject.producer_generation_sha256,
            &input.subject.plan_generation_sha256,
            &input.subject.changed_inventory_sha256,
        ]
        .into_iter()
        .all(|s| s.len() <= 64);
    let mut report = Report {
        subject: subject_bounded.then(|| input.subject.clone()),
        authority: Authority::NotEstablished,
        rows: Vec::new(),
        findings: Vec::new(),
        printed_disclosures: Vec::new(),
        missing_authority: [
            MissingAuthority::AuthenticatedGithubTransportAndSubject,
            MissingAuthority::CompleteArtifactCustodyAndBytes,
            MissingAuthority::FullPortfolioExecutionAndCurrentEligibility,
            MissingAuthority::RunnerCapacity,
            MissingAuthority::WholeRouteDeadlineAndTerminalRelease,
            MissingAuthority::FeatureCommandExecution,
            MissingAuthority::PrintedCrossFamilyProducerIdentity,
        ],
    };
    if !subject_bounded {
        report
            .findings
            .push("subject identity exceeds bounds".into());
    }
    if input.subject.event != Event::MergeGroup {
        report
            .findings
            .push("not a native merge-group subject".into());
    }
    // Reject excessive input before retaining or constructing any derived rows.
    if input.rust_expected.len() > MAX_ROWS
        || input.rust_observed.len() > MAX_ROWS
        || !subject_bounded
        || input.wheel_jobs.len() > 5
        || input
            .wheel_jobs
            .iter()
            .any(|j| j.steps.len() > 16 || j.produced.len() > 3 || j.consumed.len() > 3)
        || input.printed.is_some_and(|(_, cells)| cells.len() > 3)
        || input.windows.is_some_and(|(_, o)| {
            o.producers.len() > 16
                || o.evidence_steps.len() > 16
                || o.files.len() > 256
                || o.upload.files.len() > 256
        })
    {
        if !subject_bounded {
            // Never copy or parse an unbounded identity. Preserve the fixed inventory
            // using a bounded sentinel and explicitly omit identity from the report.
            let sentinel = Subject {
                event: Event::MergeGroup,
                repository: String::new(),
                pull_number: 0,
                head_sha: String::new(),
                base_sha: String::new(),
                group_sha: String::new(),
                tree_sha: String::new(),
                workflow_generation: String::new(),
                producer_generation_sha256: String::new(),
                plan_generation_sha256: String::new(),
                changed_inventory_sha256: String::new(),
                run_id: 0,
                run_attempt: 0,
            };
            let empty = Input {
                subject: &sentinel,
                rust_expected: &[],
                rust_observed: &[],
                windows: None,
                wheel_workflow_id: 0,
                wheel_jobs: &[],
                printed: None,
            };
            let mut bounded = inspect(&empty);
            bounded.subject = None;
            bounded
                .findings
                .push("subject identity exceeds bounds".into());
            for row in &mut bounded.rows {
                if !matches!(row.obligation, Obligation::Feature { .. }) {
                    row.observation = Observation::Incomplete;
                }
            }
            return bounded;
        }
        let empty = Input {
            subject: input.subject,
            rust_expected: &[],
            rust_observed: &[],
            windows: None,
            wheel_workflow_id: 0,
            wheel_jobs: &[],
            printed: None,
        };
        let mut bounded = inspect(&empty);
        bounded
            .findings
            .push("portfolio input exceeds source bounds".into());
        for row in &mut bounded.rows {
            if !matches!(row.obligation, Obligation::Feature { .. }) {
                row.observation = Observation::Incomplete;
            }
        }
        return bounded;
    }
    let rust = recon::inspect(input.subject, input.rust_expected, input.rust_observed);
    if rust
        .findings
        .iter()
        .any(|f| f == "declared or observed input exceeds inspection bounds")
    {
        let empty = Input {
            subject: input.subject,
            rust_expected: &[],
            rust_observed: &[],
            windows: None,
            wheel_workflow_id: 0,
            wheel_jobs: &[],
            printed: None,
        };
        let mut bounded = inspect(&empty);
        bounded
            .findings
            .push("Rust input exceeds inspection bounds".into());
        for row in &mut bounded.rows {
            if !matches!(row.obligation, Obligation::Feature { .. }) {
                row.observation = Observation::Incomplete;
            }
        }
        return bounded;
    }
    #[derive(Clone, Copy, Eq, PartialEq)]
    enum JobOwner {
        Rust,
        Windows,
        Wheel(wheel::Cell),
    }
    let mut owners = BTreeMap::new();
    let mut collision = false;
    let mut record = |id, owner| {
        if id != 0
            && owners
                .insert(id, owner)
                .is_some_and(|previous| previous != owner)
        {
            collision = true;
        }
    };
    for job in input.rust_observed {
        record(job.job_id, JobOwner::Rust);
    }
    if let Some((_, job)) = input.windows {
        record(job.job_id, JobOwner::Windows);
    }
    for job in input.wheel_jobs {
        record(job.job_id, JobOwner::Wheel(job.cell));
    }
    let mut workflow_ids = BTreeSet::new();
    workflow_ids.extend(
        input
            .rust_expected
            .iter()
            .map(|row| row.workflow_id)
            .filter(|id| *id != 0),
    );
    if let Some((run, _)) = input.windows
        && run.workflow_id != 0
    {
        workflow_ids.insert(run.workflow_id);
    }
    if !input.wheel_jobs.is_empty() && input.wheel_workflow_id != 0 {
        workflow_ids.insert(input.wheel_workflow_id);
    }
    let cross_inconsistent = collision || workflow_ids.len() > 1;
    if collision {
        report
            .findings
            .push("native job identity crosses portfolio owners".into());
    }
    if workflow_ids.len() > 1 {
        report
            .findings
            .push("caller workflow identity differs across families".into());
    }
    // The inspector's other-family checks are handled by their source owners below.
    report
        .findings
        .extend(rust.findings.into_iter().filter(|finding| {
            ![
                "missing family: WindowsEvidence",
                "missing family: PythonWheel",
                "missing family: Features",
                "missing family: PrintedCommand",
            ]
            .contains(&finding.as_str())
        }));
    if input.rust_expected.iter().any(|r| r.family != Family::Rust)
        || input
            .rust_observed
            .iter()
            .any(|r| r.row.family != Family::Rust)
    {
        report
            .findings
            .push("non-Rust row supplied to Rust owner".into());
    }
    let canonical_rust = product_gate_definitions();
    for r in input.rust_expected {
        if !canonical_rust.iter().any(|gate| gate.id.as_str() == r.id) {
            report
                .findings
                .push(format!("unknown Rust obligation: {}", r.id));
        }
        if r.workflow_path != ".github/workflows/rust-gates.yml"
            || r.job_key != "rust-gates"
            || !r.matrix_key.is_empty()
        {
            report
                .findings
                .push(format!("Rust job owner differs: {}", r.id));
        }
    }
    let rust_inconsistent = !report.findings.is_empty();
    for gate in product_gate_definitions() {
        let matching: Vec<_> = input
            .rust_observed
            .iter()
            .filter(|r| r.row.id == gate.id.as_str())
            .collect();
        let observation = match matching.as_slice() {
            [] => Observation::Missing,
            [r] => match r.outcome {
                ObservationOutcome::Passed if !rust_inconsistent => Observation::ReportedSuccess,
                ObservationOutcome::Passed => Observation::Conflicting,
                ObservationOutcome::Failed => Observation::Failed,
                ObservationOutcome::Skipped => Observation::Skipped,
                ObservationOutcome::Cancelled => Observation::Cancelled,
                ObservationOutcome::TimedOut => Observation::TimedOut,
                ObservationOutcome::EvidenceCaptured(_) => Observation::Conflicting,
            },
            _ => Observation::Conflicting,
        };
        report
            .rows
            .push(row(Obligation::Rust(gate.id), observation));
    }
    let mut windows_inconsistent = true;
    if let Some((run, observed)) = input.windows {
        let inspected = windows::inspect(input.subject, run, observed);
        windows_inconsistent =
            !inspected.findings.is_empty() || input.subject.event != Event::MergeGroup;
        report.findings.extend(inspected.findings);
    } else {
        report.findings.push("missing Windows observation".into());
    }
    for producer in windows::PRODUCERS {
        let matches: Vec<_> = input
            .windows
            .into_iter()
            .flat_map(|(_, o)| &o.producers)
            .filter(|o| o.producer == producer)
            .collect();
        let observation = match matches.as_slice() {
            [] => Observation::Missing,
            [o] => match (o.wrapper_outcome, o.native_exit, o.outcome) {
                (
                    windows::StepOutcome::Success,
                    Some(0),
                    ObservationOutcome::EvidenceCaptured(recon::AdvisoryVerdict::Pass),
                ) if !windows_inconsistent => Observation::AdvisoryPassCaptured,
                (
                    windows::StepOutcome::Success,
                    Some(n),
                    ObservationOutcome::EvidenceCaptured(recon::AdvisoryVerdict::Fail),
                ) if n != 0 => Observation::AdvisoryFailureCaptured,
                (windows::StepOutcome::Failure, _, _) => Observation::Failed,
                (windows::StepOutcome::Skipped, _, _) => Observation::Skipped,
                (windows::StepOutcome::Cancelled, _, _) => Observation::Cancelled,
                (windows::StepOutcome::TimedOut, _, _) => Observation::TimedOut,
                _ => Observation::Conflicting,
            },
            _ => Observation::Conflicting,
        };
        report
            .rows
            .push(row(Obligation::Windows(producer), observation));
    }
    let wheel_report = wheel::inspect(input.subject, input.wheel_workflow_id, input.wheel_jobs);
    let wheel_inconsistent =
        !wheel_report.findings.is_empty() || input.subject.event != Event::MergeGroup;
    report.findings.extend(wheel_report.findings);
    for definition in wheel::definitions() {
        let matches: Vec<_> = input
            .wheel_jobs
            .iter()
            .filter(|j| j.cell == definition.cell)
            .collect();
        let observation = match matches.as_slice() {
            [] => Observation::Missing,
            [j] => match j.conclusion {
                wheel::State::Success
                    if j.steps.iter().any(|s| {
                        s.outcome == wheel::State::Failure || s.conclusion == wheel::State::Failure
                    }) =>
                {
                    Observation::Failed
                }
                wheel::State::Success
                    if j.steps.iter().any(|s| {
                        s.outcome == wheel::State::Skipped || s.conclusion == wheel::State::Skipped
                    }) =>
                {
                    Observation::Skipped
                }
                wheel::State::Success
                    if j.steps.iter().any(|s| {
                        s.outcome == wheel::State::Cancelled
                            || s.conclusion == wheel::State::Cancelled
                    }) =>
                {
                    Observation::Cancelled
                }
                wheel::State::Success
                    if j.steps.iter().any(|s| {
                        s.outcome == wheel::State::TimedOut
                            || s.conclusion == wheel::State::TimedOut
                    }) =>
                {
                    Observation::TimedOut
                }
                wheel::State::Success if !wheel_inconsistent => Observation::ReportedSuccess,
                wheel::State::Success => Observation::Conflicting,
                wheel::State::Failure => Observation::Failed,
                wheel::State::Skipped => Observation::Skipped,
                wheel::State::Cancelled => Observation::Cancelled,
                wheel::State::TimedOut => Observation::TimedOut,
                _ => Observation::Incomplete,
            },
            _ => Observation::Conflicting,
        };
        report
            .rows
            .push(row(Obligation::Wheel(definition.cell), observation));
    }
    for (child, owner) in feature_obligations::source_inventory()
        .children
        .iter()
        .enumerate()
    {
        for (command, _) in owner.commands.iter().enumerate() {
            // Prerequisites are retained alongside proof commands; neither is a result.
            report.rows.push(row(
                Obligation::Feature { child, command },
                Observation::Missing,
            ));
        }
    }
    let mut printed_inconsistent = true;
    if let Some((expected, inputs)) = input.printed {
        if !printed_subject_matches(input.subject, expected) {
            report
                .findings
                .push("printed-command expected identity differs from portfolio".into());
        }
        let inspected = printed::inspect(expected, inputs);
        printed_inconsistent = !inspected.findings.is_empty()
            || !printed_subject_matches(input.subject, expected)
            || input.subject.event != Event::MergeGroup;
        report.findings.extend(inspected.findings);
        report.printed_disclosures = inspected.cells;
    } else {
        report
            .findings
            .push("missing printed-command observations".into());
    }
    for os in PRINTED_OS {
        let matches: Vec<_> = report
            .printed_disclosures
            .iter()
            .filter(|c| c.matrix_os == os)
            .collect();
        let observation = match matches.as_slice() {
            [] if input
                .printed
                .is_some_and(|(_, cells)| cells.iter().any(|c| c.matrix_os == os)) =>
            {
                Observation::Incomplete
            }
            [] => Observation::Missing,
            [_] if printed_inconsistent => Observation::Conflicting,
            [cell] => {
                let exclusions = cell.shells.iter().any(|shell| !shell.excluded.is_empty());
                match (cell.replay_observation, exclusions) {
                    (printed::ReplayObservation::CompletedWithDisclosedGaps, true) => {
                        Observation::ReplayWithGapsAndExclusions
                    }
                    (printed::ReplayObservation::CompletedWithDisclosedGaps, false) => {
                        Observation::ReplayWithKnownGaps
                    }
                    (printed::ReplayObservation::CompletedWithoutDisclosedGaps, true) => {
                        Observation::ReplayWithExclusions
                    }
                    (printed::ReplayObservation::CompletedWithoutDisclosedGaps, false) => {
                        Observation::ReplayWithoutKnownGapsOrExclusions
                    }
                }
            }
            _ => Observation::Conflicting,
        };
        report.rows.push(row(Obligation::Printed(os), observation));
    }
    for row in &mut report.rows {
        row.producer = match row.obligation {
            Obligation::Rust(id) => {
                let mut matches = input
                    .rust_observed
                    .iter()
                    .filter(|value| value.row.id == id.as_str());
                let first = matches.next();
                if matches.next().is_none() {
                    first.map(|value| ProducerIdentity {
                        workflow_id: value.workflow_id,
                        job_id: value.job_id,
                    })
                } else {
                    None
                }
            }
            Obligation::Windows(producer) => input.windows.and_then(|(_, value)| {
                (value
                    .producers
                    .iter()
                    .filter(|item| item.producer == producer)
                    .count()
                    == 1)
                    .then_some(ProducerIdentity {
                        workflow_id: value.workflow_id,
                        job_id: value.job_id,
                    })
            }),
            Obligation::Wheel(cell) => {
                let mut matches = input.wheel_jobs.iter().filter(|value| value.cell == cell);
                let first = matches.next();
                if matches.next().is_none() {
                    first.map(|value| ProducerIdentity {
                        workflow_id: value.workflow_id,
                        job_id: value.job_id,
                    })
                } else {
                    None
                }
            }
            Obligation::Feature { .. } | Obligation::Printed(_) => None,
        };
    }
    if cross_inconsistent {
        for row in &mut report.rows {
            if matches!(
                row.observation,
                Observation::ReportedSuccess
                    | Observation::AdvisoryPassCaptured
                    | Observation::ReplayWithKnownGaps
                    | Observation::ReplayWithExclusions
                    | Observation::ReplayWithGapsAndExclusions
                    | Observation::ReplayWithoutKnownGapsOrExclusions
            ) {
                row.observation = Observation::Conflicting;
            }
        }
    }
    report
}

#[cfg(test)]
mod tests;
