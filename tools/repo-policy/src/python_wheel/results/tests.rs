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

fn artifact(content: Content, id: u64, job_id: u64) -> ArtifactObservation {
    ArtifactObservation {
        artifact_id: id,
        archive_sha256: "4".repeat(64),
        producer_job_id: job_id,
        run_id: subject().run_id,
        run_attempt: subject().run_attempt,
        subject_sha: subject().group_sha,
        source_tree: subject().tree_sha,
        content,
    }
}

/// Synthetic claims only. No actual workflow or wheel execution is implied.
fn fixture() -> Vec<JobObservation> {
    let runner = artifact(
        Content::OrdinaryRunner {
            runner_sha256: "5".repeat(64),
        },
        10,
        2,
    );
    let wheel = artifact(
        Content::Wheelhouse {
            wheel_sha256: "6".repeat(64),
            payload_sha256: "7".repeat(64),
        },
        20,
        3,
    );
    let installed = artifact(
        Content::InstalledOrdinaryEvidence {
            wheel_sha256: "6".repeat(64),
            payload_sha256: "7".repeat(64),
            runner_sha256: "5".repeat(64),
        },
        30,
        4,
    );
    definitions()
        .into_iter()
        .zip(1..)
        .map(|(definition, job_id)| {
            let produced = match definition.cell {
                Cell::SourceContract => vec![runner.clone()],
                Cell::Build => vec![wheel.clone()],
                Cell::Pip => vec![installed.clone()],
                _ => vec![],
            };
            let consumed = match definition.cell {
                Cell::Pip => vec![wheel.clone(), runner.clone()],
                Cell::Uv => vec![wheel.clone()],
                _ => vec![],
            };
            JobObservation {
                cell: definition.cell,
                subject: subject(),
                actions_app_id: ACTIONS_APP_ID,
                workflow_id: 987,
                workflow_path: super::super::WORKFLOW.into(),
                job_id,
                job_key: definition.job_key.into(),
                matrix_key: definition.matrix_key.into(),
                conclusion: State::Success,
                steps: definition
                    .steps
                    .iter()
                    .map(|step| StepObservation {
                        step: *step,
                        outcome: State::Success,
                        conclusion: State::Success,
                    })
                    .collect(),
                produced,
                consumed,
            }
        })
        .collect()
}

fn refuses(jobs: &[JobObservation]) {
    let report = inspect(&subject(), 987, jobs);
    assert_eq!(report.authority, Authority::NotEstablished);
    assert!(
        !report.findings.is_empty(),
        "counterexample escaped inspection"
    );
}

#[test]
fn perfect_claims_cannot_authenticate_or_qualify_anything() {
    let report = inspect(&subject(), 987, &fixture());
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.authority, Authority::NotEstablished);
    assert_eq!(
        report.missing_authority,
        [
            MissingAuthority::AuthenticatedSubjectAndGeneration,
            MissingAuthority::AuthenticatedJobAndStepExecution,
            MissingAuthority::AuthenticatedArtifactCustodyAndBytes,
            MissingAuthority::NativeRunDeadlineAndTerminalRelease,
            MissingAuthority::FullPortfolioAndCurrentEligibility,
            MissingAuthority::InstalledCompatibilityAndPublication,
        ]
    );
    let mut jobs = fixture();
    jobs.reverse();
    assert!(inspect(&subject(), 987, &jobs).findings.is_empty());
}

#[test]
fn wheel_denominator_requires_both_producers_and_both_distinct_consumers() {
    assert_eq!(
        definitions().map(|row| row.id),
        [
            "wheel.applicability",
            "wheel.source_contract",
            "wheel.build",
            "wheel.consume.pip",
            "wheel.consume.uv",
        ]
    );
    refuses(&[]);
    for index in 0..5 {
        let mut jobs = fixture();
        jobs.remove(index);
        refuses(&jobs);
    }
    let mut jobs = fixture();
    jobs[4] = jobs[3].clone();
    refuses(&jobs);
    let mut jobs = fixture();
    jobs[4].job_id = jobs[3].job_id;
    refuses(&jobs);
    let mut jobs = fixture();
    jobs[4].matrix_key = "client=pip".into();
    refuses(&jobs);
    let mut jobs = fixture();
    jobs[4].job_key = "mcp-evidence-pair".into();
    refuses(&jobs);
}

#[test]
fn every_step_and_real_outcome_is_required_even_after_artifact_upload() {
    let non_success = [
        State::Failure,
        State::Skipped,
        State::Cancelled,
        State::TimedOut,
        State::InProgress,
        State::Unknown,
    ];
    for index in 0..5 {
        for outcome in non_success {
            let mut jobs = fixture();
            jobs[index].conclusion = outcome;
            refuses(&jobs);
        }
        for step in 0..fixture()[index].steps.len() {
            let mut jobs = fixture();
            jobs[index].steps.remove(step);
            refuses(&jobs);
            for outcome in non_success {
                for rewrite_conclusion in [false, true] {
                    let mut jobs = fixture();
                    if rewrite_conclusion {
                        jobs[index].steps[step].outcome = outcome;
                    } else {
                        jobs[index].steps[step].conclusion = outcome;
                    }
                    refuses(&jobs);
                }
            }
        }
        let mut jobs = fixture();
        jobs[index].steps.swap(0, 1);
        refuses(&jobs);
    }
}

#[test]
fn trailing_extra_step_is_rejected_independently_of_order() {
    for index in 0..5 {
        let mut jobs = fixture();
        assert!(inspect(&subject(), 987, &jobs).findings.is_empty());
        let duplicate = jobs[index].steps[0].clone();
        jobs[index].steps.push(duplicate);
        let report = inspect(&subject(), 987, &jobs);
        assert_eq!(report.authority, Authority::NotEstablished);
        assert_eq!(
            report.findings,
            [format!(
                "missing, extra, or reordered wheel steps: {:?}",
                jobs[index].cell
            )]
        );
    }
}

#[test]
fn pip_only_ordinary_journey_cannot_be_credited_to_uv_or_build() {
    let jobs = fixture();
    assert!(
        jobs[3]
            .steps
            .iter()
            .any(|step| step.step == Step::OrdinaryJourney)
    );
    assert!(
        !jobs[4]
            .steps
            .iter()
            .any(|step| step.step == Step::OrdinaryJourney)
    );
    let mut jobs = fixture();
    let ordinary: Vec<_> = jobs[3]
        .steps
        .iter()
        .filter(|step| {
            matches!(
                step.step,
                Step::OrdinaryJourney | Step::UploadOrdinaryEvidence
            )
        })
        .cloned()
        .collect();
    jobs[3].steps.retain(|step| !ordinary.contains(step));
    jobs[4].steps.extend(ordinary);
    refuses(&jobs);
    let mut jobs = fixture();
    jobs[4].produced = std::mem::take(&mut jobs[3].produced);
    refuses(&jobs);
}

#[test]
fn every_subject_dimension_and_current_run_attempt_is_bound() {
    let edits: &[fn(&mut Subject)] = &[
        |s| s.event = Event::PullRequest,
        |s| s.repository = "EffortlessMetrics/ripr".into(),
        |s| s.pull_number += 1,
        |s| s.head_sha = "f".repeat(40),
        |s| s.base_sha = "f".repeat(40),
        |s| s.group_sha = "f".repeat(40),
        |s| s.tree_sha = "f".repeat(40),
        |s| s.workflow_generation = "f".repeat(40),
        |s| s.producer_generation_sha256 = "f".repeat(64),
        |s| s.plan_generation_sha256 = "f".repeat(64),
        |s| s.changed_inventory_sha256 = "f".repeat(64),
        |s| s.run_id += 1,
        |s| s.run_attempt += 1,
    ];
    for edit in edits {
        for index in 0..5 {
            let mut jobs = fixture();
            edit(&mut jobs[index].subject);
            refuses(&jobs);
        }
    }
    for edit in [
        (|s: &mut Subject| s.head_sha.clear()) as fn(&mut Subject),
        |s| s.run_id = 0,
        |s| s.base_sha = "B".repeat(40),
        |s| s.event = Event::PullRequest,
    ] {
        let mut subject = subject();
        let mut jobs = fixture();
        edit(&mut subject);
        for job in &mut jobs {
            job.subject = subject.clone();
        }
        assert!(!inspect(&subject, 987, &jobs).findings.is_empty());
    }
    for index in 0..5 {
        let edits: &[fn(&mut JobObservation)] = &[
            |job| job.workflow_id += 1,
            |job| job.workflow_path = ".github/workflows/ci.yml".into(),
            |job| job.actions_app_id += 1,
            |job| job.job_id = 0,
        ];
        for edit in edits {
            let mut jobs = fixture();
            edit(&mut jobs[index]);
            refuses(&jobs);
        }
    }
}

#[test]
fn producer_receipts_and_names_do_not_substitute_for_exact_artifact_edges() {
    for index in [1, 2, 3] {
        let mut jobs = fixture();
        jobs[index].produced.clear();
        refuses(&jobs);
    }
    for (cell, input) in [(3, 0), (3, 1), (4, 0)] {
        let mut jobs = fixture();
        jobs[cell].consumed.remove(input);
        refuses(&jobs);
        let edits: &[fn(&mut ArtifactObservation)] = &[
            |a| a.artifact_id += 1,
            |a| a.archive_sha256 = "a".repeat(64),
            |a| a.producer_job_id += 1,
            |a| a.run_id += 1,
            |a| a.run_attempt -= 1,
            |a| a.subject_sha = subject().head_sha,
            |a| a.source_tree = "f".repeat(40),
        ];
        for edit in edits {
            let mut jobs = fixture();
            edit(&mut jobs[cell].consumed[input]);
            refuses(&jobs);
        }
    }
    for edit in [
        (|a: &mut ArtifactObservation| a.archive_sha256.clear()) as fn(&mut ArtifactObservation),
        |a| a.artifact_id = 0,
        |a| a.producer_job_id = 8,
        |a| a.run_attempt -= 1,
        |a| a.subject_sha = subject().head_sha,
        |a| a.source_tree = "f".repeat(40),
        |a| {
            a.content = Content::Wheelhouse {
                wheel_sha256: "wrong".into(),
                payload_sha256: "7".repeat(64),
            }
        },
    ] {
        let mut jobs = fixture();
        edit(&mut jobs[2].produced[0]);
        // Consistent untrusted consumer claims cannot sanitize a bad producer.
        jobs[3].consumed[0] = jobs[2].produced[0].clone();
        jobs[4].consumed[0] = jobs[2].produced[0].clone();
        refuses(&jobs);
    }
    let mut jobs = fixture();
    jobs[2].produced[0].artifact_id = jobs[1].produced[0].artifact_id;
    jobs[3].consumed[0] = jobs[2].produced[0].clone();
    jobs[4].consumed[0] = jobs[2].produced[0].clone();
    refuses(&jobs);
}

#[test]
fn retained_installed_journey_binds_all_three_actual_producer_digests() {
    for field in 0..3 {
        let mut jobs = fixture();
        if let Content::InstalledOrdinaryEvidence {
            wheel_sha256,
            payload_sha256,
            runner_sha256,
        } = &mut jobs[3].produced[0].content
        {
            [wheel_sha256, payload_sha256, runner_sha256][field].replace_range(.., &"a".repeat(64));
        }
        refuses(&jobs);
    }
    let mut jobs = fixture();
    jobs[3].produced[0].content = jobs[2].produced[0].content.clone();
    refuses(&jobs);
}

#[test]
fn oversized_and_duplicate_observations_fail_closed() {
    let mut jobs = fixture();
    jobs.push(jobs[0].clone());
    refuses(&jobs);
    let mut jobs = fixture();
    jobs[0].steps = vec![jobs[0].steps[0].clone(); 17];
    refuses(&jobs);
    let mut jobs = fixture();
    jobs[3].consumed = vec![jobs[3].consumed[0].clone(); 4];
    refuses(&jobs);
    let mut jobs = fixture();
    jobs[2].produced = vec![jobs[2].produced[0].clone(); 4];
    refuses(&jobs);
}

#[path = "source_tests.rs"]
mod source_tests;
