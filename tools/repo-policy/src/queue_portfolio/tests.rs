use super::*;
use crate::queue_workflow::reconciliation::{Disposition, ExecutionAxes, PropositionKind};

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
fn empty(s: &Subject) -> Input<'_> {
    Input {
        subject: s,
        rust_expected: &[],
        rust_observed: &[],
        windows: None,
        wheel_workflow_id: 456,
        wheel_jobs: &[],
        printed: None,
    }
}
fn lookup(report: &Report, obligation: Obligation) -> Observation {
    report
        .rows
        .iter()
        .find(|r| r.obligation == obligation)
        .expect("source obligation exists")
        .observation
}
fn rust_fixture(s: &Subject) -> (Vec<ExpectedRow>, Vec<ObservedRow>) {
    let expected: Vec<_> = product_gate_definitions()
        .into_iter()
        .map(|g| ExpectedRow {
            id: g.id.as_str().into(),
            family: Family::Rust,
            proposition: g.claim.into(),
            command: g.command.into(),
            workflow_path: ".github/workflows/rust-gates.yml".into(),
            workflow_id: 456,
            job_key: "rust-gates".into(),
            matrix_key: "".into(),
            axes: ExecutionAxes {
                platform: "linux".into(),
                target: "x64".into(),
                feature_profile: "default".into(),
                toolchain: "1.95.0".into(),
                configuration_sha256: "4".repeat(64),
            },
            kind: PropositionKind::Proof,
            disposition: Disposition::Execute,
            composite: g.composite,
        })
        .collect();
    let observed = expected
        .iter()
        .map(|r| ObservedRow {
            row: r.clone(),
            subject: s.clone(),
            actions_app_id: 15368,
            workflow_id: 456,
            job_id: 789,
            job_key: "rust-gates".into(),
            matrix_key: "".into(),
            outcome: ObservationOutcome::Passed,
            composite: None,
        })
        .collect();
    (expected, observed)
}
fn wheel_job(s: &Subject, cell: wheel::Cell) -> wheel::JobObservation {
    let def = cell.definition();
    wheel::JobObservation {
        cell,
        subject: s.clone(),
        actions_app_id: 15368,
        workflow_id: 456,
        workflow_path: crate::python_wheel::WORKFLOW.into(),
        job_id: 22,
        job_key: def.job_key.into(),
        matrix_key: def.matrix_key.into(),
        conclusion: wheel::State::Success,
        steps: vec![],
        produced: vec![],
        consumed: vec![],
    }
}
#[test]
fn source_denominator_and_missing_authority_do_not_depend_on_caller() {
    let s = subject();
    let report = inspect(&empty(&s));
    let inventory = feature_obligations::source_inventory();
    let commands: usize = inventory.children.iter().map(|c| c.commands.len()).sum();
    assert_eq!(
        report.rows.len(),
        product_gate_definitions().len() + 5 + 5 + commands + 3
    );
    assert_eq!(report.subject, Some(s.clone()));
    assert_eq!(report.authority, Authority::NotEstablished);
    assert!(
        report
            .rows
            .iter()
            .all(|r| r.authority == Authority::NotEstablished
                && r.observation == Observation::Missing)
    );
    assert!(
        report
            .missing_authority
            .contains(&MissingAuthority::WholeRouteDeadlineAndTerminalRelease)
    );
    assert!(
        report
            .missing_authority
            .contains(&MissingAuthority::CompleteArtifactCustodyAndBytes)
    );
}
#[test]
fn rust_reconciler_rejects_missing_duplicate_unknown_stale_and_wrong_owner() {
    let s = subject();
    let (expected, mut observed) = rust_fixture(&s);
    let mut input = empty(&s);
    input.rust_expected = &expected;
    input.rust_observed = &observed;
    let report = inspect(&input);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("missing composite observation"))
    );
    assert_eq!(
        lookup(&report, Obligation::Rust(ProductGateId::Formatting)),
        Observation::Conflicting
    );
    observed[0].subject.run_attempt += 1;
    observed.push(observed[0].clone());
    let mut extra = observed[1].clone();
    extra.row.id = "invented".into();
    observed.push(extra);
    observed[1].row.job_key = "different".into();
    let mut input = empty(&s);
    input.rust_expected = &expected;
    input.rust_observed = &observed;
    let report = inspect(&input);
    for text in [
        "duplicate observation",
        "unexpected observation",
        "subject or producer identity differs",
        "row identity differs",
    ] {
        assert!(
            report.findings.iter().any(|f| f.contains(text)),
            "missing {text}: {:?}",
            report.findings
        );
    }
    assert_eq!(
        lookup(&report, Obligation::Rust(ProductGateId::Formatting)),
        Observation::Conflicting
    );
}
#[test]
fn wheel_generic_success_and_failed_or_skipped_step_cannot_be_a_pass() {
    let s = subject();
    let mut job = wheel_job(&s, wheel::Cell::Build);
    let inspect_jobs = |jobs: &[wheel::JobObservation]| {
        let mut input = empty(&s);
        input.wheel_jobs = jobs;
        lookup(&inspect(&input), Obligation::Wheel(wheel::Cell::Build))
    };
    assert_eq!(
        inspect_jobs(std::slice::from_ref(&job)),
        Observation::Conflicting
    );
    job.steps.push(wheel::StepObservation {
        step: wheel::Step::BuildWheel,
        outcome: wheel::State::Failure,
        conclusion: wheel::State::Success,
    });
    assert_eq!(
        inspect_jobs(std::slice::from_ref(&job)),
        Observation::Failed
    );
    job.steps[0].outcome = wheel::State::Skipped;
    assert_eq!(
        inspect_jobs(std::slice::from_ref(&job)),
        Observation::Skipped
    );
    let jobs = vec![job.clone(), job];
    assert_eq!(inspect_jobs(&jobs), Observation::Conflicting);
}
#[test]
fn oversized_inputs_do_not_erase_the_source_denominator() {
    let s = subject();
    let jobs = vec![wheel_job(&s, wheel::Cell::Build); 6];
    let mut input = empty(&s);
    input.wheel_jobs = &jobs;
    let report = inspect(&input);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("exceeds source bounds"))
    );
    assert_eq!(
        lookup(&report, Obligation::Wheel(wheel::Cell::Build)),
        Observation::Incomplete
    );
    assert_eq!(
        lookup(&report, Obligation::Printed("windows-latest")),
        Observation::Incomplete
    );
    assert!(
        report
            .rows
            .iter()
            .all(|r| r.authority == Authority::NotEstablished)
    );
}
#[test]
fn printed_identity_binds_all_native_fields_including_attempt() {
    let s = subject();
    let mut expected = printed::Expected {
        subject: printed::Subject {
            repository: s.repository.clone(),
            head_sha: s.head_sha.clone(),
            base_sha: s.base_sha.clone(),
            group_sha: s.group_sha.clone(),
            tree_sha: s.tree_sha.clone(),
            workflow_generation: s.workflow_generation.clone(),
            run_id: s.run_id.to_string(),
            run_attempt: s.run_attempt.to_string(),
            request_id: "123-2".into(),
        },
        qualification_started_at_ms: 1000,
        deadline_at_ms: 1201000,
    };
    assert!(printed_subject_matches(&s, &expected));
    expected.subject.run_attempt = "1".into();
    let mut input = empty(&s);
    input.printed = Some((&expected, &[]));
    let report = inspect(&input);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("printed-command expected identity differs"))
    );
    assert_eq!(
        lookup(&report, Obligation::Printed("ubuntu-latest")),
        Observation::Missing
    );
}

fn windows_fixture(s: &Subject) -> (windows::ExpectedRun, windows::Observation) {
    let run = windows::ExpectedRun { workflow_id: 456,
        workflow_ref: "EffortlessMetrics/ripr-swarm/.github/workflows/routed-rust.yml@refs/heads/gh-readonly-queue/main/pr-42".into() };
    let mut paths = vec![windows::SOURCE_PATH, windows::SUMMARY_PATH];
    for producer in windows::PRODUCERS {
        let c = producer.contract();
        paths.extend([c.log, c.status]);
        paths.extend(c.receipt);
    }
    let files: Vec<_> = paths
        .into_iter()
        .map(|path| windows::FileClaim {
            path: path.into(),
            bytes: 1,
            sha256: "4".repeat(64),
        })
        .collect();
    let observation = windows::Observation {
        subject: s.clone(),
        workflow_id: 456,
        workflow_path: windows::WORKFLOW.into(),
        actions_app_id: 15368,
        job_id: 789,
        job_key: windows::JOB.into(),
        runner_os: "Windows".into(),
        runner_label: windows::RUNNER_LABEL.into(),
        job_outcome: windows::StepOutcome::Success,
        source: windows::SourceReceipt {
            proof_profile: "merge-group".into(),
            request_id: "123-2".into(),
            head: s.head_sha.clone(),
            base: s.base_sha.clone(),
            subject: s.group_sha.clone(),
            tree: s.tree_sha.clone(),
            workflow_generation: s.workflow_generation.clone(),
            producer_run: s.run_id,
            producer_attempt: s.run_attempt,
            workflow_ref: run.workflow_ref.clone(),
            workflow_sha: s.workflow_generation.clone(),
        },
        producers: windows::PRODUCERS
            .into_iter()
            .map(|producer| windows::ProducerObservation {
                producer,
                owner: producer.contract().producer.name.into(),
                command: producer.contract().command.map(String::from),
                wrapper_outcome: windows::StepOutcome::Success,
                native_exit: Some(0),
                outcome: ObservationOutcome::EvidenceCaptured(recon::AdvisoryVerdict::Pass),
                executed_tests: producer.contract().requires_nonzero_test_count.then_some(1),
            })
            .collect(),
        evidence_steps: windows::EVIDENCE_OWNERS
            .iter()
            .map(|name| windows::StepObservation {
                owner: (*name).into(),
                outcome: windows::StepOutcome::Success,
            })
            .collect(),
        files: files.clone(),
        upload: windows::UploadObservation {
            owner: windows::UPLOAD_OWNER.into(),
            outcome: windows::StepOutcome::Success,
            artifact_id: 999,
            artifact_name: "ripr-windows-advisory-subset-123-2".into(),
            run_id: 123,
            run_attempt: 2,
            job_id: 789,
            files,
        },
    };
    (run, observation)
}
#[test]
fn captured_windows_failure_is_advisory_failure_even_if_wrapper_job_succeeds() {
    let s = subject();
    let (run, mut observed) = windows_fixture(&s);
    {
        let mut input = empty(&s);
        input.windows = Some((&run, &observed));
        let report = inspect(&input);
        assert_eq!(
            lookup(
                &report,
                Obligation::Windows(windows::Producer::GapArtifacts)
            ),
            Observation::AdvisoryPassCaptured
        );
        assert_eq!(report.authority, Authority::NotEstablished);
        let row = report
            .rows
            .iter()
            .find(|r| r.obligation == Obligation::Windows(windows::Producer::GapArtifacts))
            .expect("Windows producer");
        assert_eq!(
            row.producer,
            Some(ProducerIdentity {
                workflow_id: 456,
                job_id: 789
            })
        );
        assert!(
            report
                .rows
                .iter()
                .filter(|r| matches!(
                    r.obligation,
                    Obligation::Feature { .. } | Obligation::Printed(_)
                ))
                .all(|r| r.producer.is_none())
        );
    }
    observed.producers[0].native_exit = Some(1);
    observed.producers[0].outcome =
        ObservationOutcome::EvidenceCaptured(recon::AdvisoryVerdict::Fail);
    let run_with = |o: &windows::Observation| {
        let mut input = empty(&s);
        input.windows = Some((&run, o));
        inspect(&input)
    };
    let report = run_with(&observed);
    assert_eq!(
        lookup(
            &report,
            Obligation::Windows(windows::Producer::GapArtifacts)
        ),
        Observation::AdvisoryFailureCaptured
    );
    assert!(
        report
            .rows
            .iter()
            .all(|r| r.authority == Authority::NotEstablished)
    );
    observed.producers[0].outcome = ObservationOutcome::Passed;
    let report = run_with(&observed);
    assert_eq!(
        lookup(
            &report,
            Obligation::Windows(windows::Producer::GapArtifacts)
        ),
        Observation::Conflicting
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("promoted to proof"))
    );
    observed.producers[0].outcome =
        ObservationOutcome::EvidenceCaptured(recon::AdvisoryVerdict::Fail);
    observed.source.producer_attempt = 1;
    let report = run_with(&observed);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("source receipt differs"))
    );
    assert_eq!(
        lookup(&report, Obligation::Windows(windows::Producer::CargoHome)),
        Observation::Conflicting
    );
}

struct PrintedFixture {
    os: &'static str,
    inventory: Vec<u8>,
    receipt: Vec<u8>,
    observation: serde_json::Value,
}
fn printed_fixtures(s: &Subject) -> (printed::Expected, Vec<PrintedFixture>) {
    use serde_json::json;
    use sha2::{Digest, Sha256};
    let expected = printed::Expected {
        subject: printed::Subject {
            repository: s.repository.clone(),
            head_sha: s.head_sha.clone(),
            base_sha: s.base_sha.clone(),
            group_sha: s.group_sha.clone(),
            tree_sha: s.tree_sha.clone(),
            workflow_generation: s.workflow_generation.clone(),
            run_id: s.run_id.to_string(),
            run_attempt: s.run_attempt.to_string(),
            request_id: format!("{}-{}", s.run_id, s.run_attempt),
        },
        qualification_started_at_ms: 1000,
        deadline_at_ms: 1201000,
    };
    let subject_json = json!({"repository":s.repository,"head_sha":s.head_sha,"base_sha":s.base_sha,"group_sha":s.group_sha,
        "tree_sha":s.tree_sha,"workflow_generation":s.workflow_generation,"run_id":s.run_id.to_string(),
        "run_attempt":s.run_attempt.to_string(),"request_id":format!("{}-{}", s.run_id, s.run_attempt)});
    let cells = [
        (
            "ubuntu-latest",
            "Linux",
            "linux",
            vec!["bash", "sh", "zsh", "pwsh"],
        ),
        (
            "macos-latest",
            "macOS",
            "macos",
            vec!["bash", "sh", "zsh", "pwsh"],
        ),
        (
            "windows-latest",
            "Windows",
            "windows",
            vec!["bash", "pwsh", "powershell"],
        ),
    ];
    let fixtures = cells.into_iter().enumerate().map(|(index, (os, runner, runtime, shells))| {
        let inventory = (0..15).map(|id| json!({"case_id":id,"source":"fixture","origin":"Foreign", "bash":"ripr check",
            "powershell":null,"bash_only":id==3,"powershell_unavailable":id==4}).to_string()).collect::<Vec<_>>().join("\n").into_bytes();
        let digest = format!("{:x}", Sha256::digest(&inventory));
        let shell_rows: Vec<_> = shells.iter().map(|shell| {
            let ps = matches!(*shell, "pwsh" | "powershell");
            json!({"shell":shell,"executed_case_ids":(0..15).filter(|id| !ps || (*id!=3 && *id!=4)).collect::<Vec<_>>(),
                "excluded":if ps {vec![json!({"case_id":3,"reason":"bash_only"}),json!({"case_id":4,"reason":"powershell_unavailable"})]} else {vec![]},
                "known_gaps":if *shell=="bash" {vec![json!({"case_id":2,"reason":"fixture known gap"})]} else {vec![]}})
        }).collect();
        let mut receipt = subject_json.clone();
        receipt.as_object_mut().expect("object").extend(json!({"schema_id":"ripr.printed-command-terminal.v1","schema_version":1,
            "claim":"main_replay_test_completed_with_disclosed_known_gaps","job_key":"paste","matrix_os":os,
            "runner_os":runner,"runtime_os":runtime,"inventory_sha256":digest,"inventory_case_count":15,
            "required_shells":shells,"shells":shell_rows}).as_object().expect("object").clone());
        let receipt = receipt.to_string().into_bytes();
        let observation = json!({"schema_id":"ripr.printed-command-observation.v1","schema_version":1,"subject":subject_json,
            "workflow_path":".github/workflows/printed-command-paste.yml","job_key":"paste","matrix_os":os,
            "job":{"id":index+10,"status":"completed","conclusion":"success","replay_conclusion":"success",
                "receipt_upload_conclusion":"success","started_at_ms":2000,"completed_at_ms":4000},
            "deadline":{"entry_conclusion":"success","completion_conclusion":"success","completed_at_ms":3500,"deadline_at_ms":1201000},
            "inventory_artifact":{"id":index*2+100,"name":format!("printed-commands-{os}"),"run_id":"123","run_attempt":"2","sha256":digest},
            "receipt_artifact":{"id":index*2+101,"name":format!("printed-command-receipt-{os}-123-2"),"run_id":"123","run_attempt":"2",
                "sha256":format!("{:x}",Sha256::digest(&receipt))}});
        PrintedFixture { os, inventory, receipt, observation }
    }).collect();
    (expected, fixtures)
}
fn printed_report(
    s: &Subject,
    expected: &printed::Expected,
    fixtures: &[PrintedFixture],
) -> Report {
    let observations: Vec<_> = fixtures
        .iter()
        .map(|f| f.observation.to_string().into_bytes())
        .collect();
    let cells: Vec<_> = fixtures
        .iter()
        .zip(&observations)
        .map(|(f, o)| printed::CellInput {
            matrix_os: f.os,
            receipt: &f.receipt,
            inventory: &f.inventory,
            observation: o,
        })
        .collect();
    let mut input = empty(s);
    input.printed = Some((expected, &cells));
    inspect(&input)
}
#[test]
fn printed_disclosures_remain_gaps_and_exclusions_not_passes() {
    let s = subject();
    let (expected, mut fixtures) = printed_fixtures(&s);
    let report = printed_report(&s, &expected, &fixtures);
    assert_eq!(report.printed_disclosures.len(), 3, "{:?}", report.findings);
    for os in PRINTED_OS {
        assert_eq!(
            lookup(&report, Obligation::Printed(os)),
            Observation::ReplayWithGapsAndExclusions
        );
    }
    let disclosure = &report.printed_disclosures[0];
    assert!(
        disclosure
            .shells
            .iter()
            .any(|shell| !shell.known_gaps.is_empty())
    );
    assert!(
        disclosure
            .shells
            .iter()
            .any(|shell| !shell.excluded.is_empty())
    );
    fixtures[0].observation["job"]["replay_conclusion"] = "failure".into();
    let failed = printed_report(&s, &expected, &fixtures);
    assert_eq!(
        lookup(&failed, Obligation::Printed("ubuntu-latest")),
        Observation::Incomplete
    );
    assert!(failed.findings.iter().any(|f| f.contains("unsuitable job")));
    assert_eq!(
        lookup(&failed, Obligation::Printed("macos-latest")),
        Observation::Conflicting
    );
    assert!(
        failed
            .rows
            .iter()
            .all(|r| r.authority == Authority::NotEstablished)
    );
}

#[test]
fn cross_family_job_and_workflow_identity_conflicts_are_rejected() {
    let s = subject();
    let (run, windows) = windows_fixture(&s);
    let (expected, observed) = rust_fixture(&s);
    let mut input = empty(&s);
    input.rust_expected = &expected;
    input.rust_observed = &observed;
    input.windows = Some((&run, &windows));
    let report = inspect(&input);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("job identity crosses portfolio owners"))
    );
    assert_eq!(
        lookup(
            &report,
            Obligation::Windows(windows::Producer::GapArtifacts)
        ),
        Observation::Conflicting
    );
    let mut other = run.clone();
    other.workflow_id = 12345;
    input.windows = Some((&other, &windows));
    let report = inspect(&input);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("caller workflow identity differs"))
    );
}
#[test]
fn overlong_obligation_is_not_echoed_and_denominator_survives() {
    let s = subject();
    let (mut expected, observed) = rust_fixture(&s);
    expected[0].id = "x".repeat(50_000);
    let mut input = empty(&s);
    input.rust_expected = &expected;
    input.rust_observed = &observed;
    let report = inspect(&input);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("Rust input exceeds inspection bounds"))
    );
    assert!(report.findings.iter().all(|f| f.len() < 1000));
    assert_eq!(
        lookup(&report, Obligation::Rust(ProductGateId::Formatting)),
        Observation::Incomplete
    );
}

#[test]
fn each_subject_axis_is_checked_against_the_canonical_input() {
    let s = subject();
    let (expected, base) = rust_fixture(&s);
    for mutation in 0..10 {
        let mut observed = base.clone();
        let subject = &mut observed[0].subject;
        match mutation {
            0 => subject.head_sha = "f".repeat(40),
            1 => subject.base_sha = "f".repeat(40),
            2 => subject.group_sha = "f".repeat(40),
            3 => subject.tree_sha = "f".repeat(40),
            4 => subject.workflow_generation = "f".repeat(40),
            5 => subject.producer_generation_sha256 = "f".repeat(64),
            6 => subject.plan_generation_sha256 = "f".repeat(64),
            7 => subject.changed_inventory_sha256 = "f".repeat(64),
            8 => subject.run_id += 1,
            _ => subject.run_attempt += 1,
        }
        let mut input = empty(&s);
        input.rust_expected = &expected;
        input.rust_observed = &observed;
        let report = inspect(&input);
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.contains("subject or producer identity differs")),
            "axis {mutation}"
        );
        assert_eq!(report.subject, Some(s.clone()));
        assert_eq!(
            lookup(&report, Obligation::Rust(ProductGateId::Formatting)),
            Observation::Conflicting
        );
    }
}

/// Synthetic observations of every canonical Rust composite, not provider proof.
fn complete_rust(s: &Subject) -> (Vec<ExpectedRow>, Vec<ObservedRow>) {
    use crate::product_gate_plan::composites::{
        ArtifactBinding, CompositeObservation, CompositeProducer, CompositeResult,
    };
    let (expected, mut observed) = rust_fixture(s);
    for row in &mut observed {
        if let Some(producer) = row.row.composite {
            let contract = producer.contract(s.event);
            row.composite = Some(CompositeObservation {
                owner: contract.result_owner().expect("source composite has owner"),
                completed_steps: contract.steps,
                commands: contract.commands.into_iter().map(str::to_owned).collect(),
                base_sha: s.base_sha.clone(),
                target_sha: s.group_sha.clone(),
                matcher_sha: (producer == CompositeProducer::BaseUserText)
                    .then(|| s.base_sha.clone()),
                supports_cross_tree: (producer == CompositeProducer::BaseUserText).then_some(true),
                recovery: None,
                artifacts: contract
                    .artifacts
                    .into_iter()
                    .map(|edge| ArtifactBinding {
                        edge,
                        run_id: s.run_id,
                        run_attempt: s.run_attempt,
                        subject_sha: s.group_sha.clone(),
                    })
                    .collect(),
                result: CompositeResult::Executed,
            });
        }
    }
    (expected, observed)
}

/// Five cells with exact ordered steps and matching producer-to-consumer claims.
fn complete_wheel(s: &Subject) -> Vec<wheel::JobObservation> {
    use wheel::{ArtifactObservation, Cell, Content};
    let artifact = |content, artifact_id, producer_job_id| ArtifactObservation {
        artifact_id,
        archive_sha256: "4".repeat(64),
        producer_job_id,
        run_id: s.run_id,
        run_attempt: s.run_attempt,
        subject_sha: s.group_sha.clone(),
        source_tree: s.tree_sha.clone(),
        content,
    };
    let runner = artifact(
        Content::OrdinaryRunner {
            runner_sha256: "5".repeat(64),
        },
        10,
        102,
    );
    let wheelhouse = artifact(
        Content::Wheelhouse {
            wheel_sha256: "6".repeat(64),
            payload_sha256: "7".repeat(64),
        },
        20,
        103,
    );
    let installed = artifact(
        Content::InstalledOrdinaryEvidence {
            wheel_sha256: "6".repeat(64),
            payload_sha256: "7".repeat(64),
            runner_sha256: "5".repeat(64),
        },
        30,
        104,
    );
    wheel::definitions()
        .into_iter()
        .zip(101..)
        .map(|(definition, job_id)| {
            let (produced, consumed) = match definition.cell {
                Cell::Applicability => (vec![], vec![]),
                Cell::SourceContract => (vec![runner.clone()], vec![]),
                Cell::Build => (vec![wheelhouse.clone()], vec![]),
                Cell::Pip => (
                    vec![installed.clone()],
                    vec![wheelhouse.clone(), runner.clone()],
                ),
                Cell::Uv => (vec![], vec![wheelhouse.clone()]),
            };
            wheel::JobObservation {
                cell: definition.cell,
                subject: s.clone(),
                actions_app_id: 15368,
                workflow_id: 456,
                workflow_path: crate::python_wheel::WORKFLOW.into(),
                job_id,
                job_key: definition.job_key.into(),
                matrix_key: definition.matrix_key.into(),
                conclusion: wheel::State::Success,
                steps: definition
                    .steps
                    .iter()
                    .map(|step| wheel::StepObservation {
                        step: *step,
                        outcome: wheel::State::Success,
                        conclusion: wheel::State::Success,
                    })
                    .collect(),
                produced,
                consumed,
            }
        })
        .collect()
}

#[test]
fn structurally_complete_rust_and_wheel_claims_are_reported_but_never_established() {
    let s = subject();
    let (expected, observed) = complete_rust(&s);
    let jobs = complete_wheel(&s);
    assert!(wheel::inspect(&s, 456, &jobs).findings.is_empty());
    let mut input = empty(&s);
    input.rust_expected = &expected;
    input.rust_observed = &observed;
    input.wheel_jobs = &jobs;
    let report = inspect(&input);
    for gate in product_gate_definitions() {
        assert_eq!(
            lookup(&report, Obligation::Rust(gate.id)),
            Observation::ReportedSuccess,
            "{:?}",
            report.findings
        );
    }
    for definition in wheel::definitions() {
        assert_eq!(
            lookup(&report, Obligation::Wheel(definition.cell)),
            Observation::ReportedSuccess,
            "{:?}",
            report.findings
        );
    }
    assert_eq!(report.authority, Authority::NotEstablished);
    assert!(
        report
            .rows
            .iter()
            .all(|row| row.authority == Authority::NotEstablished)
    );
    assert!(
        report
            .rows
            .iter()
            .filter(|row| matches!(row.obligation, Obligation::Feature { .. }))
            .all(|row| row.observation == Observation::Missing)
    );
    assert!(
        report
            .missing_authority
            .contains(&MissingAuthority::PrintedCrossFamilyProducerIdentity)
    );

    let mut bad_rust = observed.clone();
    let composite = bad_rust
        .iter_mut()
        .find_map(|r| r.composite.as_mut())
        .expect("composite");
    composite.commands.clear();
    let mut bad = empty(&s);
    bad.rust_expected = &expected;
    bad.rust_observed = &bad_rust;
    bad.wheel_jobs = &jobs;
    let report = inspect(&bad);
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("composite command order"))
    );
    assert_eq!(
        lookup(&report, Obligation::Rust(ProductGateId::Formatting)),
        Observation::Conflicting
    );
    assert_eq!(
        lookup(&report, Obligation::Wheel(wheel::Cell::Build)),
        Observation::ReportedSuccess
    );

    let mut bad_jobs = jobs.clone();
    bad_jobs[2].steps[0].outcome = wheel::State::Failure;
    let mut bad = empty(&s);
    bad.rust_expected = &expected;
    bad.rust_observed = &observed;
    bad.wheel_jobs = &bad_jobs;
    let report = inspect(&bad);
    assert_eq!(
        lookup(&report, Obligation::Wheel(wheel::Cell::Build)),
        Observation::Failed
    );
    assert_eq!(
        lookup(&report, Obligation::Rust(ProductGateId::Formatting)),
        Observation::ReportedSuccess
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.contains("wheel step did not succeed"))
    );
}

#[test]
fn printed_job_collision_is_an_explicit_unverified_identity_gap() {
    let s = subject();
    let (run, windows) = windows_fixture(&s);
    let (expected, mut fixtures) = printed_fixtures(&s);
    let run_with = |fixtures: &[PrintedFixture]| {
        let observations: Vec<_> = fixtures
            .iter()
            .map(|f| f.observation.to_string().into_bytes())
            .collect();
        let cells: Vec<_> = fixtures
            .iter()
            .zip(&observations)
            .map(|(f, observation)| printed::CellInput {
                matrix_os: f.os,
                receipt: &f.receipt,
                inventory: &f.inventory,
                observation,
            })
            .collect();
        let mut input = empty(&s);
        input.windows = Some((&run, &windows));
        input.printed = Some((&expected, &cells));
        inspect(&input)
    };
    let distinct = run_with(&fixtures);
    assert_eq!(
        lookup(
            &distinct,
            Obligation::Windows(windows::Producer::GapArtifacts)
        ),
        Observation::AdvisoryPassCaptured
    );
    assert_eq!(
        lookup(&distinct, Obligation::Printed("ubuntu-latest")),
        Observation::ReplayWithGapsAndExclusions
    );
    fixtures[0].observation["job"]["id"] = 789.into(); // same supplied native job ID as Windows
    let collision = run_with(&fixtures);
    assert_eq!(
        collision, distinct,
        "the existing printed parser does not expose its job ID to the portfolio"
    );
    assert!(
        collision
            .missing_authority
            .contains(&MissingAuthority::PrintedCrossFamilyProducerIdentity)
    );
    assert_eq!(collision.authority, Authority::NotEstablished);
    assert!(
        collision
            .rows
            .iter()
            .all(|r| r.authority == Authority::NotEstablished)
    );
    assert!(
        collision
            .rows
            .iter()
            .filter(|r| matches!(r.obligation, Obligation::Printed(_)))
            .all(|r| r.producer.is_none())
    );
}
