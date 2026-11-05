use super::super::retention::{StabilityAttempt, finish_subject_analysis};
use super::*;

struct RetentionFixture {
    root: PathBuf,
    destinations: CandidateOut,
    binary: BinaryIdentity,
    manifest: AcceptedManifest,
}

impl Drop for RetentionFixture {
    fn drop(&mut self) {
        discard_partial_dir(&self.root);
    }
}

impl RetentionFixture {
    fn new(name: &str) -> Result<Self, String> {
        let root = temp_root(name);
        discard_partial_dir(&root);
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        let pins = [
            VALID_SHA_A,
            VALID_SHA_B,
            VALID_SHA_C,
            VALID_SHA_D,
            VALID_SHA_E,
            VALID_SHA_F,
            VALID_SHA_0,
            VALID_SHA_1,
        ]
        .map(str::to_string);
        let manifest_path = write_manifest_and_diffs(&root, &pins)?;
        let (value, digest) = load_strict_json(&manifest_path.to_string_lossy())?;
        let manifest = validate_accepted_manifest(&value, digest)?;
        assert_eq!(manifest.subjects.len(), 8);

        // Finalization re-reads the real supplied file. These bytes witness
        // identity only; the tests inject captured facts, not analyzer runs.
        let bytes = b"owned retention-test binary identity";
        let binary_path = root.join("binary-identity");
        std::fs::write(&binary_path, bytes).map_err(|error| error.to_string())?;
        let binary = BinaryIdentity {
            absolute: binary_path,
            binary_digest: sha256_hex(bytes),
            ..test_binary()
        };
        let out = root.join("candidate");
        let staging = out.join(STAGING_DIR);
        std::fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
        Ok(Self {
            root,
            destinations: CandidateOut { out, staging },
            binary,
            manifest,
        })
    }

    fn subject(&self, index: usize) -> Result<&AcceptedSubject, String> {
        self.manifest
            .subjects
            .get(index)
            .ok_or_else(|| format!("missing selected subject {index}"))
    }

    fn finish(
        &self,
        index: usize,
        first: AnalysisPass,
        stability: Option<StabilityAttempt>,
    ) -> Result<SubjectExecution, String> {
        let subject = self.subject(index)?;
        let dir = self.destinations.out.join(SUBJECTS_DIR).join(&subject.id);
        std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
        write_raw_output(
            &self.destinations.staging,
            &subject.id,
            "pass1",
            &first.stdout,
            &first.stderr,
        )?;
        Ok(finish_subject_analysis(
            ReadyContext {
                subject,
                binary: &self.binary,
                diff_portable: &subject.synthetic_diff,
                diff_digest: Some(DIGEST_ONE.to_string()),
                dir,
                source: "local_seed_clone",
                corpus: None,
                config_profile: CONFIG_PROFILE_DEFAULT.to_string(),
                subject_config_path: None,
            },
            first,
            stability,
            &self.destinations.staging,
        ))
    }

    fn finish_repeat(
        &self,
        index: usize,
        second: AnalysisPass,
    ) -> Result<SubjectExecution, String> {
        let first = analysis_pass(RunStatus::Complete, 10);
        let attempt = StabilityAttempt::captured(&first, second);
        self.finish(index, first, Some(attempt))
    }

    fn stable_siblings(&self) -> Result<Vec<SubjectExecution>, String> {
        (1..8)
            .map(|index| self.finish_repeat(index, analysis_pass(RunStatus::Complete, 20)))
            .collect()
    }

    fn publish(&self, executions: &[SubjectExecution]) -> Result<(Value, Value), String> {
        assert_eq!(
            executions.len(),
            8,
            "the full selected population is exercised"
        );
        let receipt = assemble_receipt(&self.manifest.sha256, &self.binary, executions);
        let validated = validate_run_receipt(
            &receipt,
            &self.manifest.sha256,
            &self.manifest,
            "retention-receipt.json",
        )?;
        assert_eq!(validated.denominator_selected, 8);
        let execution = assemble_execution_receipt(
            &self.manifest,
            &self.manifest.sha256,
            &parse_refresh_args(&[])?,
            &self.binary,
            &self.destinations.out,
            executions,
            0,
        );
        write_candidate_file(&self.destinations.staging, RECEIPT_FILE, &receipt)?;
        write_candidate_file(&self.destinations.staging, EXEC_RECEIPT_FILE, &execution)?;
        finalize_candidate_generation(&self.destinations.staging, &self.destinations.out)?;
        Ok((receipt, execution))
    }

    fn staged_raw(&self, stream: &str) -> Result<PathBuf, String> {
        Ok(self
            .destinations
            .staging
            .join(RAW_DIR)
            .join(&self.subject(0)?.id)
            .join(format!("pass2-{stream}.txt")))
    }

    fn retained_path(&self, stream: &str) -> Result<PathBuf, String> {
        Ok(self
            .destinations
            .out
            .join(RAW_DIR)
            .join(&self.subject(0)?.id)
            .join(format!("pass2-{stream}.txt")))
    }

    fn retained_bytes(&self, stream: &str) -> Result<Vec<u8>, String> {
        std::fs::read(self.retained_path(stream)?).map_err(|error| error.to_string())
    }

    fn assert_not_retained(&self, stream: &str) -> Result<(), String> {
        let path = self.retained_path(stream)?;
        match std::fs::read(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("could not inspect {}: {error}", path.display())),
            Ok(bytes) => Err(format!(
                "unexpected retained output at {}: {} bytes",
                path.display(),
                bytes.len()
            )),
        }
    }
}

fn assert_unavailable(receipt: &Value, execution: &Value) {
    assert_eq!(receipt["repos"][0]["status"], "complete");
    assert!(receipt["repos"][0].get("repeat").is_none());
    assert_eq!(receipt["summary"]["repos_total"], 8);
    assert_eq!(receipt["summary"]["repos_run"], 8);
    assert_eq!(receipt["summary"]["gate_status"], "review");
    assert!(receipt["summary"].get("gap_id_stability_rate").is_none());
    assert!(receipt["summary"].get("gap_id_stable_count").is_none());
    assert!(execution["subjects"][0]["stability"]["gap_stable"].is_null());
    assert!(execution["subjects"][0]["stability"]["output_identity_stable"].is_null());
}

#[test]
fn non_complete_repeats_retain_exact_bytes_without_comparison_credit() -> Result<(), String> {
    for (name, status, stdout) in [
        ("partial", RunStatus::Partial, b"partial stdout".as_slice()),
        (
            "parse",
            RunStatus::ParseFailed,
            b"parse-failed stdout".as_slice(),
        ),
        ("timeout", RunStatus::TimedOut, b"timeout\0\xff".as_slice()),
        ("exit", RunStatus::Crashed, b"nonzero exit\0\xfe".as_slice()),
        (
            "unsupported",
            RunStatus::Unsupported,
            b"unsupported stdout".as_slice(),
        ),
        (
            "malformed",
            RunStatus::Crashed,
            b"{malformed\xff".as_slice(),
        ),
    ] {
        let fixture = RetentionFixture::new(&format!("repeat-retention-{name}"))?;
        let stderr = b"repeat stderr\0\xff\xfe";
        // Most cases inject captured phase facts at the retention boundary.
        // The malformed case also crosses the real admission path so its
        // named rejection must survive retention and receipt assembly.
        let second = if name == "malformed" {
            analysis_pass_from_capture(false, true, 20, stdout.to_vec(), stderr.to_vec())
        } else {
            let mut second = analysis_pass(status, 20);
            second.stdout = stdout.to_vec();
            second.stderr = stderr.to_vec();
            second
        };
        assert_eq!(second.status.as_str(), status.as_str());
        // Both passes carry empty gap sets. Retaining this repeat must not
        // turn equality of failure results into a stable comparison.
        assert!(second.gap_ids.is_empty());
        let first = fixture.finish_repeat(0, second)?;
        assert_eq!(first.row.stability, None);
        assert!(first.stability_performed);
        let mut executions = fixture.stable_siblings()?;
        executions.insert(0, first);
        let (receipt, execution) = fixture.publish(&executions)?;
        assert_unavailable(&receipt, &execution);
        assert_eq!(fixture.retained_bytes("stdout")?, stdout);
        assert_eq!(fixture.retained_bytes("stderr")?, stderr);
        let retained = &execution["subjects"][0]["stability"]["retained_output"];
        assert_eq!(retained["status"], status.as_str());
        assert_eq!(retained["stdout"]["sha256"], sha256_hex(stdout));
        assert_eq!(retained["stderr"]["sha256"], sha256_hex(stderr));
        for stream in ["stdout", "stderr"] {
            assert_eq!(
                retained[stream]["path"],
                format!("raw/{}/pass2-{stream}.txt", fixture.subject(0)?.id)
            );
        }
        assert!(
            execution["subjects"][0]["stability"]["limitation"]
                .as_str()
                .is_some_and(|reason| reason.contains(status.as_str()))
        );
        if name == "malformed" {
            assert!(
                execution["subjects"][0]["stability"]["limitation"]
                    .as_str()
                    .is_some_and(|reason| reason.contains("stdout is not valid UTF-8"))
            );
        }
        assert!(execution["subjects"][0]["infrastructure_limitation"].is_null());
    }
    Ok(())
}

#[test]
fn repeat_digests_track_each_stream_without_comparison_credit() -> Result<(), String> {
    let mut rows = Vec::new();
    let mut retained = Vec::new();
    for (index, stdout, stderr) in [
        (0, b"stdout-a".as_slice(), b"stderr-a".as_slice()),
        (1, b"stdout-b".as_slice(), b"stderr-a".as_slice()),
        (2, b"stdout-a".as_slice(), b"stderr-b".as_slice()),
    ] {
        let fixture = RetentionFixture::new(&format!("repeat-digests-{index}"))?;
        let mut second = analysis_pass(RunStatus::Partial, 20);
        second.stdout = stdout.to_vec();
        second.stderr = stderr.to_vec();
        let mut executions = fixture.stable_siblings()?;
        executions.insert(0, fixture.finish_repeat(0, second)?);
        let (receipt, execution) = fixture.publish(&executions)?;
        assert_unavailable(&receipt, &execution);
        assert_eq!(fixture.retained_bytes("stdout")?, stdout);
        assert_eq!(fixture.retained_bytes("stderr")?, stderr);
        rows.push(receipt["repos"][0].clone());
        retained.push(execution["subjects"][0]["stability"]["retained_output"].clone());
    }
    assert_eq!(rows[0], rows[1]);
    assert_eq!(rows[0], rows[2]);
    assert_ne!(
        retained[0]["stdout"]["sha256"],
        retained[1]["stdout"]["sha256"]
    );
    assert_eq!(
        retained[0]["stderr"]["sha256"],
        retained[1]["stderr"]["sha256"]
    );
    assert_eq!(
        retained[0]["stdout"]["sha256"],
        retained[2]["stdout"]["sha256"]
    );
    assert_ne!(
        retained[0]["stderr"]["sha256"],
        retained[2]["stderr"]["sha256"]
    );
    Ok(())
}

#[test]
fn complete_repeats_preserve_existing_comparison_and_digest_shapes() -> Result<(), String> {
    for stable in [true, false] {
        let fixture = RetentionFixture::new(&format!("repeat-complete-{stable}"))?;
        // Synthetic captured bytes use the real admission path; this is not
        // an analyzer run. Both complete controls carry valid documents.
        let first_stdout = json!({
            "analysis_outcome": {
                "analysis_complete": true,
                "outcome": { "kind": "complete_no_findings", "limitations": [] },
            },
            "findings": [],
        })
        .to_string()
        .into_bytes();
        let second_stdout = if stable {
            first_stdout.clone()
        } else {
            json!({
                "analysis_outcome": {
                    "analysis_complete": true,
                    "outcome": { "kind": "complete_with_findings", "limitations": [] },
                },
                "findings": [{
                    "classification": "exposed",
                    "canonical_gap_id": "gap:python:changed",
                }],
            })
            .to_string()
            .into_bytes()
        };
        let first = analysis_pass_from_capture(false, true, 10, first_stdout, Vec::new());
        let second = analysis_pass_from_capture(
            false,
            true,
            20,
            second_stdout,
            b"complete repeat stderr\0\xff".to_vec(),
        );
        assert_eq!(first.status.as_str(), "complete");
        assert_eq!(second.status.as_str(), "complete");
        let expected_stdout = second.stdout.clone();
        let expected_stderr = second.stderr.clone();
        let evidence_preimage = format!(
            "raw={}\nstderr={}\nrepeat_raw={}\n",
            sha256_hex(&first.stdout),
            sha256_hex(&first.stderr),
            sha256_hex(&second.stdout)
        );
        let attempt = StabilityAttempt::captured(&first, second);
        let mut executions = fixture.stable_siblings()?;
        executions.insert(0, fixture.finish(0, first, Some(attempt))?);
        let (receipt, execution) = fixture.publish(&executions)?;
        assert_eq!(fixture.retained_bytes("stdout")?, expected_stdout);
        assert_eq!(fixture.retained_bytes("stderr")?, expected_stderr);
        assert_eq!(
            receipt["repos"][0]["digests"]["evidence"],
            sha256_hex(evidence_preimage.as_bytes())
        );
        let unstable_gap_ids = if stable {
            Vec::<String>::new()
        } else {
            vec!["gap:python:changed".to_string()]
        };
        assert_eq!(
            receipt["repos"][0]["repeat"],
            json!({
                "comparable_with": "pass-1",
                "gap_ids_stable": stable,
                "unstable_gap_ids": unstable_gap_ids,
                "repeat_stderr": sha256_hex(&expected_stderr),
            })
        );
        assert_eq!(
            execution["subjects"][0]["stability"],
            json!({
                "performed": true,
                "gap_stable": stable,
                "output_identity_stable": stable,
                "limitation": null,
            })
        );
        let expected_gate = if stable { "pass" } else { "review" };
        let expected_stable_count = if stable { 8 } else { 7 };
        assert_eq!(receipt["summary"]["gate_status"], expected_gate);
        assert_eq!(
            receipt["summary"]["gap_id_stable_count"],
            expected_stable_count
        );
    }
    Ok(())
}

#[test]
fn unstarted_and_failed_capture_routes_never_invent_repeat_output() -> Result<(), String> {
    for (name, status, performed) in [
        ("unstarted", RunStatus::Partial, false),
        ("spawn-failed", RunStatus::Complete, true),
    ] {
        let fixture = RetentionFixture::new(&format!("repeat-{name}"))?;
        let first = analysis_pass(status, 10);
        // The real stability route refuses a non-complete first pass before
        // spawning. For the complete first pass this non-executable owned
        // file makes capture fail, while its identity remains readable.
        let attempt = run_stability_pass(
            &fixture.binary,
            &fixture.root,
            &fixture.root.join("unused.diff"),
            &fixture.root.join("cache"),
            Duration::from_secs(1),
            &fixture.subject(0)?.id,
            &first,
        );
        let mut executions = fixture.stable_siblings()?;
        executions.insert(0, fixture.finish(0, first, Some(attempt))?);
        let (receipt, execution) = fixture.publish(&executions)?;
        assert_eq!(receipt["summary"]["repos_total"], 8);
        assert_eq!(receipt["summary"]["repos_run"], 8);
        assert_eq!(receipt["summary"]["gate_status"], "review");
        assert!(receipt["repos"][0].get("repeat").is_none());
        let stability = &execution["subjects"][0]["stability"];
        assert_eq!(stability["performed"], performed);
        assert!(stability.get("retained_output").is_none());
        assert!(stability["gap_stable"].is_null());
        assert!(stability["output_identity_stable"].is_null());
        let expected = if performed {
            "stability pass could not run"
        } else {
            "runs only where the first result is complete"
        };
        assert!(
            stability["limitation"]
                .as_str()
                .is_some_and(|text| text.contains(expected))
        );
        for stream in ["stdout", "stderr"] {
            fixture.assert_not_retained(stream)?;
        }
    }
    Ok(())
}

#[test]
fn repeat_write_failures_keep_the_run_and_cannot_improve_a_mixed_gate() -> Result<(), String> {
    for status in [RunStatus::Complete, RunStatus::Partial] {
        for blocked_stream in ["stdout", "stderr"] {
            let fixture = RetentionFixture::new(&format!(
                "repeat-write-failure-{}-{blocked_stream}",
                status.as_str()
            ))?;
            let mut second = analysis_pass(status, 20);
            second.stderr = b"repeat stderr that must not receive a retention claim".to_vec();
            let expected_stdout = second.stdout.clone();
            let blocked = fixture.staged_raw(blocked_stream)?;
            std::fs::create_dir_all(&blocked).map_err(|error| error.to_string())?;
            assert!(blocked.is_dir());
            let mut executions = fixture.stable_siblings()?;
            executions.insert(0, fixture.finish_repeat(0, second)?);
            assert!(blocked.is_dir(), "failure injection must remain present");
            if blocked_stream == "stderr" {
                assert_eq!(
                    std::fs::read(fixture.staged_raw("stdout")?)
                        .map_err(|error| error.to_string())?,
                    expected_stdout,
                    "the first stream was written before the second stream failed"
                );
            }
            let (receipt, execution) = fixture.publish(&executions)?;
            assert_unavailable(&receipt, &execution);
            assert_eq!(receipt["repos"][0]["runtime_ms"], 10);
            assert!(!receipt["repos"][0]["digests"]["raw"].is_null());
            let stability = &execution["subjects"][0]["stability"];
            assert_eq!(stability["performed"], true);
            assert!(stability.get("retained_output").is_none());
            let reason = stability["limitation"]
                .as_str()
                .ok_or_else(|| "retention failure limitation missing".to_string())?;
            assert!(reason.contains("raw evidence could not be retained"));
            assert!(reason.contains(&format!("pass2-{blocked_stream}.txt")));
            if matches!(status, RunStatus::Partial) {
                assert!(reason.contains("stability pass ended in status `partial`"));
            }
            assert!(
                execution["subjects"][0]["infrastructure_limitation"]
                    .as_str()
                    .is_some_and(|text| text.contains("raw evidence could not be retained"))
            );
        }
    }
    Ok(())
}

#[test]
fn repeat_retention_failure_still_rechecks_binary_identity() -> Result<(), String> {
    let fixture = RetentionFixture::new("repeat-retention-binary-drift")?;
    let mut executions = fixture.stable_siblings()?;
    std::fs::create_dir_all(fixture.staged_raw("stderr")?).map_err(|error| error.to_string())?;
    std::fs::write(&fixture.binary.absolute, b"changed binary identity")
        .map_err(|error| error.to_string())?;
    executions.insert(
        0,
        fixture.finish_repeat(0, analysis_pass(RunStatus::Partial, 20))?,
    );
    let (receipt, execution) = fixture.publish(&executions)?;
    assert_eq!(receipt["summary"]["repos_total"], 8);
    assert_eq!(receipt["summary"]["repos_run"], 7);
    assert_eq!(receipt["repos"][0]["status"], "tempfail");
    assert!(receipt["repos"][0]["digests"].is_null());
    assert!(receipt["repos"][0].get("repeat").is_none());
    assert!(
        execution["subjects"][0]["stability"]
            .get("retained_output")
            .is_none()
    );
    assert!(execution["subjects"][0]["stability"]["gap_stable"].is_null());
    assert!(
        execution["subjects"][0]["infrastructure_limitation"]
            .as_str()
            .is_some_and(|text| text.contains("binary content identity drifted"))
    );
    Ok(())
}
