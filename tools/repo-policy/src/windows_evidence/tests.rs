use super::*;

fn subject() -> Subject {
    Subject {
        event: Event::MergeGroup,
        repository: "EffortlessMetrics/ripr-swarm".into(),
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
        run_attempt: 2,
    }
}

fn run() -> ExpectedRun {
    ExpectedRun {
        workflow_id: 456,
        workflow_ref: "EffortlessMetrics/ripr-swarm/.github/workflows/routed-rust.yml@refs/heads/gh-readonly-queue/main/pr-42".into(),
    }
}

fn observation() -> Observation {
    let subject = subject();
    let mut paths = vec![SOURCE_PATH, SUMMARY_PATH];
    for producer in PRODUCERS {
        let contract = producer.contract();
        paths.extend([contract.log, contract.status]);
        paths.extend(contract.receipt);
    }
    let files: Vec<_> = paths
        .into_iter()
        .map(|path| FileClaim {
            path: path.into(),
            bytes: 1,
            sha256: "4".repeat(64),
        })
        .collect();
    Observation {
        workflow_id: run().workflow_id,
        workflow_path: WORKFLOW.into(),
        actions_app_id: 15368,
        job_id: 789,
        job_key: JOB.into(),
        runner_os: "Windows".into(),
        runner_label: RUNNER_LABEL.into(),
        job_outcome: StepOutcome::Success,
        source: SourceReceipt {
            proof_profile: "merge-group".into(),
            request_id: "123-2".into(),
            head: subject.head_sha.clone(),
            base: subject.base_sha.clone(),
            subject: subject.group_sha.clone(),
            tree: subject.tree_sha.clone(),
            workflow_generation: subject.workflow_generation.clone(),
            producer_run: subject.run_id,
            producer_attempt: subject.run_attempt,
            workflow_ref: run().workflow_ref,
            workflow_sha: subject.workflow_generation.clone(),
        },
        producers: PRODUCERS
            .into_iter()
            .map(|producer| ProducerObservation {
                producer,
                owner: producer.contract().producer.name.into(),
                command: producer.contract().command.map(String::from),
                wrapper_outcome: StepOutcome::Success,
                native_exit: Some(0),
                outcome: ObservationOutcome::EvidenceCaptured(AdvisoryVerdict::Pass),
                executed_tests: producer.contract().requires_nonzero_test_count.then_some(1),
            })
            .collect(),
        evidence_steps: EVIDENCE_OWNERS
            .into_iter()
            .map(|owner| StepObservation {
                owner: owner.into(),
                outcome: StepOutcome::Success,
            })
            .collect(),
        files: files.clone(),
        upload: UploadObservation {
            owner: UPLOAD_OWNER.into(),
            outcome: StepOutcome::Success,
            artifact_id: 999,
            artifact_name: "ripr-windows-advisory-subset-123-2".into(),
            run_id: 123,
            run_attempt: 2,
            job_id: 789,
            files,
        },
        subject,
    }
}

fn report(observation: &Observation) -> Report {
    inspect(&subject(), &run(), observation)
}

fn refused(observation: &Observation) {
    let report = report(observation);
    assert_eq!(report.authority, Authority::NotEstablished);
    assert!(
        !report.findings.is_empty(),
        "unexpectedly accepted {observation:?}"
    );
}

#[test]
fn captured_advisory_failure_is_not_a_failed_evidence_contract_or_a_pass() {
    let mut observed = observation();
    assert_eq!(observed.producers.len(), 5);
    assert_eq!(observed.files.len(), 13);
    for row in &mut observed.producers {
        row.native_exit = Some(if row.producer == Producer::SelectorRecovery {
            1
        } else {
            101
        });
        row.outcome = ObservationOutcome::EvidenceCaptured(AdvisoryVerdict::Fail);
        row.executed_tests = None; // Compilation failure still has useful advisory evidence.
    }
    let result = report(&observed);
    assert!(result.findings.is_empty(), "{result:?}");
    assert_eq!(result.authority, Authority::NotEstablished);
    assert!(
        observed
            .producers
            .iter()
            .all(|row| row.outcome == ObservationOutcome::EvidenceCaptured(AdvisoryVerdict::Fail))
    );
}

#[test]
fn a_structurally_complete_pass_never_establishes_native_execution_or_authority() {
    let result = report(&observation());
    assert!(result.findings.is_empty(), "{result:?}");
    assert_eq!(result.authority, Authority::NotEstablished);
}

#[test]
fn all_five_producers_and_exact_commands_are_mandatory() {
    for index in 0..PRODUCERS.len() {
        let mut observed = observation();
        observed.producers.remove(index);
        refused(&observed);
        let mut observed = observation();
        observed.producers.push(observed.producers[index].clone());
        refused(&observed);
        let mut observed = observation();
        observed.producers[index].owner = "other step".into();
        refused(&observed);
        let mut observed = observation();
        observed.producers[index].command = Some("cargo test -- --list".into());
        refused(&observed);
    }
}

#[test]
fn successful_job_or_wrong_kind_cannot_replace_the_native_advisory_status() {
    for index in 0..PRODUCERS.len() {
        for outcome in [
            ObservationOutcome::Passed,
            ObservationOutcome::Failed,
            ObservationOutcome::Skipped,
            ObservationOutcome::Cancelled,
            ObservationOutcome::TimedOut,
            ObservationOutcome::EvidenceCaptured(AdvisoryVerdict::Fail),
        ] {
            let mut observed = observation();
            observed.producers[index].outcome = outcome;
            refused(&observed);
        }
        let mut observed = observation();
        observed.producers[index].native_exit = None;
        refused(&observed);
    }
    let mut observed = observation();
    observed.producers[4].native_exit = Some(101);
    observed.producers[4].outcome = ObservationOutcome::EvidenceCaptured(AdvisoryVerdict::Fail);
    refused(&observed);
}

#[test]
fn zero_subject_success_is_not_captured_test_evidence_but_clippy_needs_no_test_count() {
    for index in 0..3 {
        for count in [None, Some(0)] {
            let mut observed = observation();
            observed.producers[index].executed_tests = count;
            refused(&observed);
        }
    }
    assert_eq!(observation().producers[3].executed_tests, None);
    assert!(report(&observation()).findings.is_empty());
}

#[test]
fn capture_guards_summary_and_upload_have_independent_nonadvisory_owners() {
    for status in [
        StepOutcome::Failure,
        StepOutcome::Skipped,
        StepOutcome::Cancelled,
        StepOutcome::TimedOut,
    ] {
        for index in 0..5 {
            let mut observed = observation();
            observed.producers[index].wrapper_outcome = status;
            refused(&observed);
        }
        for index in 0..EVIDENCE_OWNERS.len() {
            let mut observed = observation();
            observed.evidence_steps[index].outcome = status;
            refused(&observed);
        }
        let mut observed = observation();
        observed.upload.outcome = status;
        refused(&observed);
        let mut observed = observation();
        observed.job_outcome = status;
        refused(&observed);
    }
    for index in 0..EVIDENCE_OWNERS.len() {
        let mut observed = observation();
        observed.evidence_steps.remove(index);
        refused(&observed);
        let mut observed = observation();
        observed
            .evidence_steps
            .push(observed.evidence_steps[index].clone());
        refused(&observed);
    }
}

#[test]
fn source_receipt_and_job_identity_cannot_be_transplanted_between_subjects_or_attempts() {
    let changes: &[fn(&mut Observation)] = &[
        |o| o.runner_os = "Linux".into(),
        |o| o.runner_label = "ubuntu-latest".into(),
        |o| o.job_id = 0,
        |o| o.workflow_id += 1,
        |o| o.actions_app_id += 1,
        |o| o.job_key = "other-job".into(),
        |o| o.workflow_path = "other.yml".into(),
        |o| o.subject.run_attempt += 1,
        |o| o.source.producer_attempt += 1,
        |o| o.source.producer_run += 1,
        |o| o.source.proof_profile = "diagnostic".into(),
        |o| o.source.request_id = "123-1".into(),
        |o| o.source.head = "f".repeat(40),
        |o| o.source.base = "f".repeat(40),
        |o| o.source.subject = o.source.head.clone(),
        |o| o.source.tree = "f".repeat(40),
        |o| o.source.workflow_generation = "f".repeat(40),
        |o| o.source.workflow_sha = "f".repeat(40),
        |o| {
            o.source.workflow_ref =
                "EffortlessMetrics/ripr-swarm/.github/workflows/other.yml@refs/heads/main".into()
        },
    ];
    for change in changes {
        let mut observed = observation();
        change(&mut observed);
        refused(&observed);
    }
    let mut invalid = subject();
    invalid.event = Event::PullRequest;
    assert!(
        !inspect(&invalid, &run(), &observation())
            .findings
            .is_empty()
    );
    let mut invalid = run();
    invalid.workflow_id = 0;
    assert!(
        !inspect(&subject(), &invalid, &observation())
            .findings
            .is_empty()
    );
}

#[test]
fn every_required_file_must_be_nonempty_unique_and_present_in_uploaded_manifest() {
    for index in 0..13 {
        let mut observed = observation();
        observed.files.remove(index);
        observed.upload.files.remove(index);
        refused(&observed);
        let mut observed = observation();
        observed.files[index].bytes = 0;
        observed.upload.files = observed.files.clone();
        refused(&observed);
        let mut observed = observation();
        observed.files.push(observed.files[index].clone());
        observed.upload.files = observed.files.clone();
        refused(&observed);
        let mut observed = observation();
        observed.upload.files.remove(index);
        refused(&observed);
        let mut observed = observation();
        observed.upload.files[index].sha256 = "5".repeat(64);
        refused(&observed);
    }
}

#[test]
fn file_paths_and_artifact_custody_refuse_ambiguity() {
    for path in [
        "target/selector-literal/../foreign",
        "target/selector-literal//receipt.json",
        "target/selector-literal/./receipt.json",
        "target/selector-literal/receipt.JSON",
        "target/selector-literal/receipt.json.",
        "target/selector-literal/c:evil",
        "target/selector-literal\\receipt.json",
        "target/unrelated.json",
    ] {
        let mut observed = observation();
        observed.files.push(FileClaim {
            path: path.into(),
            bytes: 1,
            sha256: "4".repeat(64),
        });
        observed.upload.files = observed.files.clone();
        refused(&observed);
    }
    let changes: &[fn(&mut UploadObservation)] = &[
        |u| u.owner = "another upload".into(),
        |u| u.artifact_id = 0,
        |u| u.artifact_name = "ripr-windows-advisory-subset-123-1".into(),
        |u| u.run_id += 1,
        |u| u.run_attempt += 1,
        |u| u.job_id += 1,
        |u| u.files.push(u.files[0].clone()),
    ];
    for change in changes {
        let mut observed = observation();
        change(&mut observed.upload);
        refused(&observed);
    }
    let mut observed = observation();
    observed.producers.resize(17, observed.producers[0].clone());
    refused(&observed);
}
