//! Structural diagnostics for a declared queue obligation set.
//!
//! This is not an authorization or final-result producer. The caller-supplied
//! expected set and observations are not authenticated here, and the full
//! five-family obligation denominator is not yet represented. Even a report
//! with no structural findings remains `NotEstablished`. No workflow invokes
//! this helper; it cannot select, skip, reuse, or authorize a job.

use crate::product_gate_plan::composites::{CompositeObservation, CompositeProducer, Event};
use crate::product_gate_plan::{ProductGateRole, product_gate_definitions};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
mod composite_tests;
mod composites;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Family {
    Rust,
    WindowsEvidence,
    PythonWheel,
    Features,
    PrintedCommand,
}

const MAX_ROWS: usize = 4096;
// This is the native Actions producer identity, not the separate H publisher.
// Matching a supplied value does not authenticate the observation.
const ACTIONS_APP_ID: u64 = 15368;

const FAMILIES: [Family; 5] = [
    Family::Rust,
    Family::WindowsEvidence,
    Family::PythonWheel,
    Family::Features,
    Family::PrintedCommand,
];

impl Family {
    fn permits_workflow(self, path: &str) -> bool {
        match self {
            Self::Rust => path == ".github/workflows/rust-gates.yml",
            Self::WindowsEvidence => path == ".github/workflows/windows-subset-qualification.yml",
            Self::PythonWheel => path == ".github/workflows/python-wheel-qualification.yml",
            Self::Features => matches!(
                path,
                ".github/workflows/ci.yml" | ".github/workflows/vscode-e2e.yml"
            ),
            Self::PrintedCommand => path == ".github/workflows/printed-command-paste.yml",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subject {
    pub event: Event,
    pub repository: String,
    pub pull_number: u64,
    pub head_sha: String,
    pub base_sha: String,
    pub group_sha: String,
    pub tree_sha: String,
    pub workflow_generation: String,
    pub producer_generation_sha256: String,
    pub plan_generation_sha256: String,
    pub changed_inventory_sha256: String,
    pub run_id: u64,
    pub run_attempt: u64,
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Subject {
    fn valid(&self) -> bool {
        self.repository == "EffortlessMetrics/ripr-swarm"
            && self.pull_number > 0
            && self.run_id > 0
            && self.run_attempt > 0
            && [
                &self.head_sha,
                &self.base_sha,
                &self.group_sha,
                &self.tree_sha,
                &self.workflow_generation,
            ]
            .into_iter()
            .all(|value| lower_hex(value, 40))
            && [
                &self.producer_generation_sha256,
                &self.plan_generation_sha256,
                &self.changed_inventory_sha256,
            ]
            .into_iter()
            .all(|value| lower_hex(value, 64))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Disposition {
    Execute,
    /// Explicitly retained as non-success. Reuse and not-applicable decisions
    /// are deliberately unsupported until their independent proof exists.
    Blocked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PropositionKind {
    Proof,
    /// Evidence was retained; the underlying advisory verdict is separate.
    AdvisoryEvidence,
}

/// Declared execution axes. Matching these strings is not compatibility proof;
/// an authenticated producer must bind them to its actual execution later.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionAxes {
    pub platform: String,
    pub target: String,
    pub feature_profile: String,
    pub toolchain: String,
    pub configuration_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExpectedRow {
    pub id: String,
    pub family: Family,
    pub proposition: String,
    pub command: String,
    pub workflow_path: String,
    pub workflow_id: u64,
    pub job_key: String,
    /// Empty for a non-matrix job; otherwise identifies one selected cell.
    pub matrix_key: String,
    pub axes: ExecutionAxes,
    pub kind: PropositionKind,
    pub disposition: Disposition,
    pub composite: Option<CompositeProducer>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdvisoryVerdict {
    Pass,
    Fail,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObservationOutcome {
    Passed,
    EvidenceCaptured(AdvisoryVerdict),
    Failed,
    Skipped,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedRow {
    pub row: ExpectedRow,
    pub subject: Subject,
    pub actions_app_id: u64,
    pub workflow_id: u64,
    pub job_id: u64,
    pub job_key: String,
    pub matrix_key: String,
    pub outcome: ObservationOutcome,
    pub composite: Option<CompositeObservation>,
}

/// No variant can express qualification, authentication, or merge success.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Authority {
    NotEstablished,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Report {
    pub authority: Authority,
    pub findings: Vec<String>,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b':'))
}

fn valid_matrix_key(value: &str) -> bool {
    value.len() <= 200
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b':' | b'=' | b',')
        })
}

fn bounded_subject(subject: &Subject) -> bool {
    subject.repository.len() <= 256
        && [
            &subject.head_sha,
            &subject.base_sha,
            &subject.group_sha,
            &subject.tree_sha,
            &subject.workflow_generation,
            &subject.producer_generation_sha256,
            &subject.plan_generation_sha256,
            &subject.changed_inventory_sha256,
        ]
        .into_iter()
        .all(|value| value.len() <= 64)
}

fn bounded_row(row: &ExpectedRow) -> bool {
    row.id.len() <= 200
        && row.proposition.len() <= 4096
        && row.command.len() <= 4096
        && row.workflow_path.len() <= 256
        && row.job_key.len() <= 200
        && row.matrix_key.len() <= 200
        && row.axes.platform.len() <= 200
        && row.axes.target.len() <= 200
        && row.axes.feature_profile.len() <= 200
        && row.axes.toolchain.len() <= 200
        && row.axes.configuration_sha256.len() <= 64
}

/// Compare supplied rows without treating either side as an authority.
///
/// This checks only a necessary structural subset. It does not establish a
/// complete expected denominator, authenticated GitHub job/run custody, an
/// actual G tree, current native eligibility, or any reusable evidence. The
/// result remains `NotEstablished` even when `findings` is empty.
pub fn inspect(subject: &Subject, expected: &[ExpectedRow], observed: &[ObservedRow]) -> Report {
    let mut findings = Vec::new();
    if expected.len() > MAX_ROWS
        || observed.len() > MAX_ROWS
        || !bounded_subject(subject)
        || expected.iter().any(|row| !bounded_row(row))
        || observed.iter().any(|row| {
            !bounded_row(&row.row)
                || !bounded_subject(&row.subject)
                || row.job_key.len() > 200
                || row.matrix_key.len() > 200
                || row
                    .composite
                    .as_ref()
                    .is_some_and(|observation| !composites::bounded(observation))
        })
    {
        return Report {
            authority: Authority::NotEstablished,
            findings: vec!["declared or observed input exceeds inspection bounds".to_owned()],
        };
    }
    if !subject.valid() {
        findings.push("invalid subject identity".to_owned());
    }
    if expected.is_empty() {
        findings.push("empty expected set".to_owned());
    }
    let gates = product_gate_definitions();
    let mut rows = BTreeMap::new();
    let mut families = BTreeSet::new();
    for row in expected {
        if !valid_id(&row.id)
            || row.proposition.trim().is_empty()
            || row.proposition.len() > 4096
            || row.command.trim().is_empty()
            || row.command.len() > 4096
            || !row.family.permits_workflow(&row.workflow_path)
            || row.workflow_id == 0
            || !valid_id(&row.job_key)
            || (!row.matrix_key.is_empty() && !valid_matrix_key(&row.matrix_key))
            || !valid_id(&row.axes.platform)
            || !valid_id(&row.axes.target)
            || !valid_id(&row.axes.feature_profile)
            || !valid_id(&row.axes.toolchain)
            || !lower_hex(&row.axes.configuration_sha256, 64)
            || (row.family == Family::WindowsEvidence)
                != (row.kind == PropositionKind::AdvisoryEvidence)
        {
            findings.push(format!("malformed expected row: {}", row.id));
        }
        if let Some(producer) = row.composite
            && !gates
                .iter()
                .any(|gate| gate.composite == Some(producer) && gate.id.as_str() == row.id)
        {
            findings.push(format!("noncanonical composite owner: {}", row.id));
        }
        if row.disposition == Disposition::Blocked {
            findings.push(format!("blocked expected row: {}", row.id));
        }
        families.insert(row.family);
        if rows.insert(row.id.as_str(), row).is_some() {
            findings.push(format!("duplicate expected row: {}", row.id));
        }
    }
    for family in FAMILIES {
        if !families.contains(&family) {
            findings.push(format!("missing family: {family:?}"));
        }
    }
    // Reuse the existing Rust product propositions; this is a minimum, not a
    // second list. Composite command/edge semantics stay with that authority.
    for gate in gates {
        let id = gate.id.as_str();
        match rows.get(id) {
            Some(row)
                if row.family == Family::Rust
                    && row.kind == PropositionKind::Proof
                    && row.disposition == Disposition::Execute
                    && gate.role == ProductGateRole::Required
                    && row.proposition == gate.claim
                    && row.command == gate.command
                    && row.composite == gate.composite => {}
            Some(_) => findings.push(format!("altered canonical Rust row: {id}")),
            None => findings.push(format!("missing canonical Rust row: {id}")),
        }
    }
    let mut seen = BTreeSet::new();
    let mut job_families = BTreeMap::new();
    let mut cell_jobs = BTreeMap::new();
    for result in observed {
        let id = result.row.id.as_str();
        if !seen.insert(id) {
            findings.push(format!("duplicate observation: {id}"));
        }
        match rows.get(id) {
            None => findings.push(format!("unexpected observation: {id}")),
            Some(row) if **row != result.row => {
                findings.push(format!("row identity differs: {id}"))
            }
            Some(_) => {}
        }
        if result.subject != *subject
            || result.actions_app_id != ACTIONS_APP_ID
            || result.workflow_id == 0
            || result.workflow_id != result.row.workflow_id
            || result.job_key != result.row.job_key
            || result.matrix_key != result.row.matrix_key
            || result.job_id == 0
        {
            findings.push(format!("subject or producer identity differs: {id}"));
        }
        let cell = (
            result.row.family,
            result.row.workflow_path.as_str(),
            result.workflow_id,
            result.job_key.as_str(),
            result.matrix_key.as_str(),
        );
        if let Some(previous) = job_families.insert(result.job_id, cell)
            && previous != cell
        {
            findings.push(format!(
                "job identity crosses producer cells: {}",
                result.job_id
            ));
        }
        if let Some(previous) = cell_jobs.insert(cell, result.job_id)
            && previous != result.job_id
        {
            findings.push(format!("producer cell maps to multiple jobs: {id}"));
        }
        composites::inspect(subject, result, &mut findings);
        match (result.row.family, result.row.kind, result.outcome) {
            (
                Family::WindowsEvidence,
                PropositionKind::AdvisoryEvidence,
                ObservationOutcome::EvidenceCaptured(_),
            ) => {}
            (family, PropositionKind::Proof, ObservationOutcome::Passed)
                if family != Family::WindowsEvidence => {}
            _ => findings.push(format!("non-success or wrong outcome kind: {id}")),
        }
    }
    for id in rows.keys() {
        if !seen.contains(id) {
            findings.push(format!("missing observation: {id}"));
        }
    }
    Report {
        authority: Authority::NotEstablished,
        findings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn subject() -> Subject {
        Subject {
            event: Event::MergeGroup,
            repository: "EffortlessMetrics/ripr-swarm".to_owned(),
            pull_number: 42,
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
            group_sha: "c".repeat(40),
            tree_sha: "d".repeat(40),
            workflow_generation: "e".repeat(40),
            producer_generation_sha256: "1".repeat(64),
            plan_generation_sha256: "2".repeat(64),
            changed_inventory_sha256: "3".repeat(64),
            run_id: 123,
            run_attempt: 1,
        }
    }

    fn axes() -> ExecutionAxes {
        ExecutionAxes {
            platform: "linux-x64".to_owned(),
            target: "x86_64-unknown-linux-gnu".to_owned(),
            feature_profile: "default".to_owned(),
            toolchain: "1.95.0".to_owned(),
            configuration_sha256: "4".repeat(64),
        }
    }

    pub(super) fn expected() -> Vec<ExpectedRow> {
        let mut rows: Vec<_> = product_gate_definitions()
            .into_iter()
            .map(|gate| ExpectedRow {
                id: gate.id.as_str().to_owned(),
                family: Family::Rust,
                proposition: gate.claim.to_owned(),
                command: gate.command.to_owned(),
                workflow_path: ".github/workflows/rust-gates.yml".to_owned(),
                workflow_id: 100,
                job_key: "rust-gates".to_owned(),
                matrix_key: String::new(),
                axes: axes(),
                kind: PropositionKind::Proof,
                disposition: Disposition::Execute,
                composite: gate.composite,
            })
            .collect();
        for (family, id, workflow, kind) in [
            (
                Family::WindowsEvidence,
                "windows.evidence",
                ".github/workflows/windows-subset-qualification.yml",
                PropositionKind::AdvisoryEvidence,
            ),
            (
                Family::PythonWheel,
                "wheel.build",
                ".github/workflows/python-wheel-qualification.yml",
                PropositionKind::Proof,
            ),
            (
                Family::Features,
                "features.example",
                ".github/workflows/ci.yml",
                PropositionKind::Proof,
            ),
            (
                Family::PrintedCommand,
                "printed.example",
                ".github/workflows/printed-command-paste.yml",
                PropositionKind::Proof,
            ),
        ] {
            rows.push(ExpectedRow {
                id: id.to_owned(),
                family,
                proposition: "example structural proposition".to_owned(),
                command: "example command".to_owned(),
                workflow_path: workflow.to_owned(),
                workflow_id: 100 + family as u64,
                job_key: "fixture-job".to_owned(),
                matrix_key: String::new(),
                axes: axes(),
                kind,
                disposition: Disposition::Execute,
                composite: None,
            });
        }
        rows
    }

    fn composite_observation(
        producer: CompositeProducer,
        subject: &Subject,
    ) -> CompositeObservation {
        use crate::product_gate_plan::composites::{ArtifactBinding, CompositeResult, StepOwner};
        let contract = producer.contract(subject.event);
        CompositeObservation {
            owner: contract.result_owner().unwrap_or(StepOwner {
                name: "missing fixture owner",
                id: None,
            }),
            completed_steps: contract.steps,
            commands: contract.commands.into_iter().map(str::to_owned).collect(),
            base_sha: subject.base_sha.clone(),
            target_sha: subject.group_sha.clone(),
            matcher_sha: (producer == CompositeProducer::BaseUserText)
                .then(|| subject.base_sha.clone()),
            supports_cross_tree: (producer == CompositeProducer::BaseUserText).then_some(true),
            recovery: None,
            artifacts: contract
                .artifacts
                .into_iter()
                .map(|edge| ArtifactBinding {
                    edge,
                    run_id: subject.run_id,
                    run_attempt: subject.run_attempt,
                    subject_sha: subject.group_sha.clone(),
                })
                .collect(),
            result: CompositeResult::Executed,
        }
    }

    pub(super) fn observed(rows: &[ExpectedRow], subject: &Subject) -> Vec<ObservedRow> {
        rows.iter()
            .map(|row| ObservedRow {
                row: row.clone(),
                subject: subject.clone(),
                actions_app_id: 15368,
                workflow_id: row.workflow_id,
                job_id: 200 + row.family as u64,
                job_key: row.job_key.clone(),
                matrix_key: row.matrix_key.clone(),
                composite: row
                    .composite
                    .map(|producer| composite_observation(producer, subject)),
                outcome: if row.family == Family::WindowsEvidence {
                    ObservationOutcome::EvidenceCaptured(AdvisoryVerdict::Fail)
                } else {
                    ObservationOutcome::Passed
                },
            })
            .collect()
    }

    #[test]
    fn structural_agreement_never_authorizes_a_result() {
        let s = subject();
        let rows = expected();
        assert!(!rows.iter().any(|row| row.id == "wheel.uv"));
        assert!(!rows.iter().any(|row| row.id == "printed.macos"));
        let report = inspect(&s, &rows, &observed(&rows, &s));
        assert!(report.findings.is_empty(), "{:?}", report.findings);
        assert_eq!(report.authority, Authority::NotEstablished);
        // The single synthetic row for each non-Rust family is intentionally
        // incomplete; even its invented Linux axes for Windows evidence do not
        // become a compatibility claim. Matching cannot establish the portfolio.
    }

    #[test]
    fn empty_missing_family_and_canonical_rust_rows_refuse() {
        let report = inspect(&subject(), &[], &[]);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding == "empty expected set")
        );
        for family in FAMILIES {
            assert!(
                report
                    .findings
                    .iter()
                    .any(|finding| finding == &format!("missing family: {family:?}"))
            );
        }
        assert!(
            report.findings.iter().any(
                |finding| finding == "missing canonical Rust row: product.rust.workspace_tests"
            )
        );
    }

    #[test]
    fn missing_extra_duplicate_and_relabelled_rows_are_visible() -> Result<(), String> {
        let s = subject();
        let rows = expected();
        let mut results = observed(&rows, &s);
        let removed = results.pop().ok_or("fixture lacks a result")?;
        let report = inspect(&s, &rows, &results);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding == &format!("missing observation: {}", removed.row.id))
        );
        results.push(removed.clone());
        results.push(removed.clone());
        let report = inspect(&s, &rows, &results);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding == &format!("duplicate observation: {}", removed.row.id))
        );
        let mut extra = removed.clone();
        extra.row.id = "invented.row".to_owned();
        results.push(extra);
        let report = inspect(&s, &rows, &results);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding == "unexpected observation: invented.row")
        );
        let mut altered = observed(&rows, &s);
        altered
            .first_mut()
            .ok_or("fixture lacks a first result")?
            .row
            .proposition = "weaker proposition".to_owned();
        assert!(
            inspect(&s, &rows, &altered)
                .findings
                .iter()
                .any(|finding| finding.starts_with("row identity differs:"))
        );
        Ok(())
    }

    #[test]
    fn wrong_subject_producer_and_non_success_are_visible() -> Result<(), String> {
        let s = subject();
        let rows = expected();
        for mutation in 0..18 {
            let mut results = observed(&rows, &s);
            let row = results.first_mut().ok_or("fixture lacks a first result")?;
            match mutation {
                0 => row.subject.head_sha = "f".repeat(40),
                1 => row.subject.group_sha = "f".repeat(40),
                2 => row.subject.run_attempt = 2,
                3 => row.actions_app_id = 1,
                4 => row.job_id = 0,
                5 => row.outcome = ObservationOutcome::Skipped,
                6 => row.outcome = ObservationOutcome::Cancelled,
                7 => row.workflow_id += 1,
                8 => row.job_key = "different-job".to_owned(),
                9 => row.matrix_key = "client=uv".to_owned(),
                10 => row.subject.base_sha = "f".repeat(40),
                11 => row.subject.tree_sha = "f".repeat(40),
                12 => row.subject.workflow_generation = "f".repeat(40),
                13 => row.subject.producer_generation_sha256 = "f".repeat(64),
                14 => row.subject.plan_generation_sha256 = "f".repeat(64),
                15 => row.subject.changed_inventory_sha256 = "f".repeat(64),
                16 => row.subject.run_id += 1,
                _ => row.outcome = ObservationOutcome::TimedOut,
            }
            assert!(
                !inspect(&s, &rows, &results).findings.is_empty(),
                "mutation {mutation}"
            );
        }
        Ok(())
    }

    #[test]
    fn changed_execution_axes_do_not_match_declared_rows() -> Result<(), String> {
        let s = subject();
        let rows = expected();
        for mutation in 0..5 {
            let mut results = observed(&rows, &s);
            let axes = &mut results
                .first_mut()
                .ok_or("fixture lacks a result")?
                .row
                .axes;
            match mutation {
                0 => axes.platform = "windows-x64".to_owned(),
                1 => axes.target = "x86_64-pc-windows-msvc".to_owned(),
                2 => axes.feature_profile = "all-features".to_owned(),
                3 => axes.toolchain = "1.94.0".to_owned(),
                _ => axes.configuration_sha256 = "5".repeat(64),
            }
            assert!(
                inspect(&s, &rows, &results)
                    .findings
                    .iter()
                    .any(|finding| finding.starts_with("row identity differs:")),
                "axis {mutation}"
            );
        }
        Ok(())
    }

    #[test]
    fn one_job_identity_cannot_stand_in_for_two_matrix_cells() -> Result<(), String> {
        let s = subject();
        let mut rows = expected();
        let template = rows
            .iter()
            .find(|row| row.family == Family::PythonWheel)
            .ok_or("fixture lacks wheel producer")?
            .clone();
        let mut pip = template.clone();
        pip.id = "wheel.consume.pip".to_owned();
        pip.job_key = "consume".to_owned();
        pip.matrix_key = "client=pip".to_owned();
        let mut uv = pip.clone();
        uv.id = "wheel.consume.uv".to_owned();
        uv.matrix_key = "client=uv".to_owned();
        rows.push(pip);
        rows.push(uv);
        let mut results = observed(&rows, &s);
        // Only pip and uv share an ID; build has a distinct one. This makes
        // the matrix cell, rather than a different job key, the discriminator.
        for row in results
            .iter_mut()
            .filter(|row| row.row.family == Family::PythonWheel)
        {
            row.job_id = if row.row.id == "wheel.build" {
                700
            } else {
                701
            };
        }
        assert!(
            inspect(&s, &rows, &results)
                .findings
                .iter()
                .any(|finding| finding.starts_with("job identity crosses producer cells:"))
        );
        results
            .iter_mut()
            .find(|row| row.row.id == "wheel.consume.uv")
            .ok_or("fixture lacks uv cell")?
            .job_id = 702;
        assert!(inspect(&s, &rows, &results).findings.is_empty());
        Ok(())
    }

    #[test]
    fn one_producer_cell_cannot_claim_multiple_job_identities() -> Result<(), String> {
        let s = subject();
        let rows = expected();
        let mut results = observed(&rows, &s);
        let row = results.first_mut().ok_or("fixture lacks a result")?;
        row.job_id = 999_999;
        assert!(
            inspect(&s, &rows, &results)
                .findings
                .iter()
                .any(|finding| finding.starts_with("producer cell maps to multiple jobs:"))
        );
        Ok(())
    }

    #[test]
    fn one_job_identity_cannot_cross_workflow_owners() -> Result<(), String> {
        let s = subject();
        for mutation in 0..2 {
            let mut rows = expected();
            let mut nested = rows
                .iter()
                .find(|row| row.family == Family::Features)
                .ok_or("fixture lacks feature producer")?
                .clone();
            nested.id = "features.nested".to_owned();
            if mutation == 0 {
                nested.workflow_path = ".github/workflows/vscode-e2e.yml".to_owned();
            } else {
                nested.workflow_id += 1;
            }
            rows.push(nested);
            let mut results = observed(&rows, &s);
            assert!(
                inspect(&s, &rows, &results)
                    .findings
                    .iter()
                    .any(|finding| finding.starts_with("job identity crosses producer cells:")),
                "mutation {mutation}"
            );
            results
                .iter_mut()
                .find(|row| row.row.id == "features.nested")
                .ok_or("fixture lacks nested producer")?
                .job_id += 1000;
            assert!(inspect(&s, &rows, &results).findings.is_empty());
        }
        Ok(())
    }

    #[test]
    fn windows_advisory_failure_is_evidence_not_a_test_pass() -> Result<(), String> {
        let s = subject();
        let rows = expected();
        let mut results = observed(&rows, &s);
        let windows = results
            .iter_mut()
            .find(|row| row.row.family == Family::WindowsEvidence)
            .ok_or("fixture lacks Windows evidence")?;
        assert_eq!(
            windows.outcome,
            ObservationOutcome::EvidenceCaptured(AdvisoryVerdict::Fail)
        );
        assert!(inspect(&s, &rows, &results).findings.is_empty());
        let windows = results
            .iter_mut()
            .find(|row| row.row.family == Family::WindowsEvidence)
            .ok_or("fixture lacks Windows evidence")?;
        windows.outcome = ObservationOutcome::Passed;
        assert!(
            inspect(&s, &rows, &results)
                .findings
                .iter()
                .any(|finding| finding.starts_with("non-success or wrong outcome kind:"))
        );
        Ok(())
    }

    #[test]
    fn oversized_declared_or_observed_sets_remain_not_established() -> Result<(), String> {
        let s = subject();
        let seed = expected().into_iter().next().ok_or("fixture lacks a row")?;
        let excessive = vec![seed; MAX_ROWS + 1];
        let report = inspect(&s, &excessive, &[]);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert_eq!(
            report.findings,
            ["declared or observed input exceeds inspection bounds"]
        );
        let observed = observed(&excessive, &s);
        let report = inspect(&s, &[], &observed);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert_eq!(
            report.findings,
            ["declared or observed input exceeds inspection bounds"]
        );
        Ok(())
    }

    #[test]
    fn oversized_row_or_subject_data_is_not_echoed_into_diagnostics() -> Result<(), String> {
        let s = subject();
        let mut rows = expected();
        rows.first_mut().ok_or("fixture lacks row")?.id = "x".repeat(10_000);
        let report = inspect(&s, &rows, &[]);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert_eq!(
            report.findings,
            ["declared or observed input exceeds inspection bounds"]
        );
        let rows = expected();
        let mut results = observed(&rows, &s);
        results
            .first_mut()
            .ok_or("fixture lacks result")?
            .subject
            .repository = "x".repeat(10_000);
        let report = inspect(&s, &rows, &results);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert_eq!(
            report.findings,
            ["declared or observed input exceeds inspection bounds"]
        );
        Ok(())
    }

    #[test]
    fn blocked_duplicate_plan_and_altered_canonical_proposition_refuse() -> Result<(), String> {
        let s = subject();
        let mut rows = expected();
        rows.first_mut()
            .ok_or("fixture lacks a canonical row")?
            .disposition = Disposition::Blocked;
        let report = inspect(&s, &rows, &[]);
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.starts_with("blocked expected row:"))
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.starts_with("altered canonical Rust row:"))
        );
        let mut duplicate = expected();
        let first = duplicate
            .first()
            .ok_or("fixture lacks a first row")?
            .clone();
        duplicate.push(first);
        assert!(
            inspect(&s, &duplicate, &[])
                .findings
                .iter()
                .any(|finding| finding.starts_with("duplicate expected row:"))
        );
        Ok(())
    }
}
