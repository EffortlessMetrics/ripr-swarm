use super::*;
use crate::formal_evidence::{audit, kani_output, native_inventory, parse_json};

fn refusal<T>(result: AuditResult<T>, expected: &str) -> AuditResult<()> {
    let reason = result.err().ok_or("expected a diagnostic refusal")?;
    assert!(reason.contains(expected), "{reason}");
    Ok(())
}

fn complete_document() -> AuditResult<Run> {
    let mut value: serde_json::Value = parse_json(include_bytes!(
        "../../../../docs/formal-verification/examples/native-proof-run.example.json"
    ))?;
    let digest = sha256(b"synthetic document control; not native execution");
    let artifact = serde_json::json!({"path":"data.json","sha256":digest,"bytes":2});
    let subject = serde_json::json!({"package":"ripr-contract","file":"crates/ripr-contract/src/a.rs","name":"proof","qualified_name":"m::proof","source_digest":digest});
    let id = "ripr-contract:crates/ripr-contract/src/a.rs::m::proof";
    let snapshot = serde_json::json!({"revision":"1".repeat(40),"tree":"2".repeat(40),"dirty":false,"manifest":artifact,"file_count":1,"error":null});
    let invocations:Vec<_>=["preflight","metadata","inventory","verification"].into_iter().map(|phase|serde_json::json!({
        "phase":phase,"package":if matches!(phase,"inventory"|"verification"){Some("ripr-contract")}else{None},
        "program":"cargo","arguments":[],"working_directory":"/repo","timeout_ms":1000,"stream_limit_bytes":1024,
        "duration_ms":1,"exit_code":0,"native_status":"exit status: 0","timed_out":false,
        "capture_complete":true,"stdout":artifact,"stderr":artifact,"error":null
    })).collect();
    value["revision"] = serde_json::json!("1".repeat(40));
    value["tree"] = serde_json::json!("2".repeat(40));
    value["source_closure_digest"] = serde_json::json!(digest);
    value["inventory"] = serde_json::json!({"expected_claim_ids":[],"discovered_subjects":[id],"complete":true,"rejection_reasons":[]});
    value["process"] = serde_json::json!({"state":"completed","exit_code":0,"timed_out":false,"raw_output_digest":digest,"cleanup_state":"complete"});
    value["results"] = serde_json::json!([]);
    value["unregistered_results"] = serde_json::json!([{"subject":subject,"status":"VERIFIED","native_verdict":"verified","duration_ms":100,"detail":"within the harness bounds","kani_time":"0.1s","covers":{"total":1,"satisfied":1,"undetermined":0},"cover_properties":[{"name":"m::proof.cover.1","status":"SATISFIED","description":null,"location":null}],"properties":[{"name":"m::proof.cover.1","status":"SATISFIED","description":null,"location":null}],"raw_block_digest":digest}]);
    value["execution"] = serde_json::json!({"producer":"ripr-kani-custody-v1","producer_entrypoint":"formal-kani","producer_arguments":["formal-kani"],
        "artifact_directory":"kani-evidence/control","repository_directory":"/repo","native_inventory_artifacts":[{"package":"ripr-contract","artifact":artifact}],
        "source_before":snapshot,"source_after":snapshot,"source_current":true,
        "configuration_policy":"kani-0.68-defaults-v1","configuration_digest":digest,"configuration_before":artifact,"configuration_after":artifact,
        "cargo_metadata":artifact,"installed_toolchain":artifact,"installed_toolchain_after":artifact,"registration":null,"explicit_inputs":[],"package_inputs":{},"configuration_admitted":true,"configuration_rejections":[],
        "selected_packages":["ripr-contract"],"harness_filter":null,"default_unwind":null,"declared_subjects":[subject],"selected_subjects":[subject],"native_subjects":[subject],"observed_subjects":[id],
        "invocations":invocations,"inventory_rejections":[],"rejection_reasons":[],"allow_empty":false,"lane_success":true,
        "github_run_id":null,"github_run_attempt":null,"github_job":null,"non_claims":["synthetic document; no observed verification"]});
    serde_json::from_value(value).map_err(|error| error.to_string())
}

fn audit_observation_roundtrip(
    original: &Run,
    invocation: &Invocation,
    observation: &(Vec<UnregisteredResult>, Vec<String>, Vec<String>),
) -> AuditResult<Run> {
    assert_eq!(observation.0.len(), 1, "expected one selected observation");
    let mut run = original.clone();
    run.unregistered_results = observation.0.clone();
    run.inventory.complete = observation.2.is_empty();
    run.inventory.rejection_reasons = observation.2.clone();
    let execution = run.execution.as_mut().ok_or("execution absent")?;
    *execution
        .invocations
        .last_mut()
        .ok_or("verification absent")? = invocation.clone();
    execution.observed_subjects = observation.1.clone();
    execution.inventory_rejections = observation.2.clone();
    execution.rejection_reasons = observation.2.clone();
    execution.lane_success = observation.2.is_empty()
        && invocation.succeeded()
        && observation
            .0
            .iter()
            .all(|row| row.status == ObservationStatus::Verified);
    run.process = aggregate_process(
        &execution.invocations,
        &sha256(b"synthetic invocation index"),
    );
    let bytes = serde_json::to_vec(&run).map_err(|error| error.to_string())?;
    let decoded: Run = parse_json(&bytes)?;
    assert_eq!(
        serde_json::to_value(&decoded).map_err(|error| error.to_string())?,
        serde_json::to_value(&run).map_err(|error| error.to_string())?
    );
    let summary = audit(&[], Some(&decoded))?;
    assert_eq!(summary.native_assurance, "not_assessed");
    assert_eq!(summary.declared_established_rows, 0);
    Ok(decoded)
}

#[test]
fn native_document_admission_refuses_false_currentness_and_incomplete_covers() -> AuditResult<()> {
    let valid = complete_document()?;
    audit(&[], Some(&valid))?;
    for control in 0..9 {
        let mut run = valid.clone();
        match control {
            0 => {
                run.execution
                    .as_mut()
                    .ok_or("execution absent")?
                    .source_after
                    .tree = Some("3".repeat(40))
            }
            1 => {
                run.unregistered_results[0].cover_properties[0].status = "UNSATISFIABLE".to_string()
            }
            2 => run.unregistered_results[0].cover_properties.clear(),
            3 => {
                run.execution
                    .as_mut()
                    .ok_or("execution absent")?
                    .cargo_metadata = None
            }
            4 => run
                .execution
                .as_mut()
                .ok_or("execution absent")?
                .observed_subjects
                .clear(),
            5 => run.process.exit_code = Some(7),
            6 => {
                run.execution
                    .as_mut()
                    .ok_or("execution absent")?
                    .invocations
                    .remove(0);
            }
            7 => {
                run.execution
                    .as_mut()
                    .ok_or("execution absent")?
                    .installed_toolchain_after = None
            }
            _ => run.process.cleanup_state = super::super::Cleanup::NotObserved,
        };
        assert!(
            audit(&[], Some(&run)).is_err(),
            "false admission control {control}"
        );
    }
    Ok(())
}

#[test]
fn native_document_audit_refuses_duplicate_observations_with_complete_inventory() -> AuditResult<()>
{
    let mut run = complete_document()?;
    audit(&[], Some(&run))?;
    assert_eq!(run.unregistered_results.len(), 1);
    assert!(run.inventory.complete);

    // Exact set equality still holds after repeating this otherwise valid row.
    // The consumer must reject the duplicate instead of silently deduplicating it.
    run.unregistered_results
        .push(run.unregistered_results[0].clone());
    refusal(audit(&[], Some(&run)), "duplicate unregistered observation")
}

#[test]
fn native_document_status_and_complete_subject_cannot_borrow_success() -> AuditResult<()> {
    let valid = complete_document()?;
    for verdict in [
        super::super::Verdict::Counterexample,
        super::super::Verdict::UnreachableCover,
        super::super::Verdict::Timeout,
        super::super::Verdict::Missing,
        super::super::Verdict::Ambiguous,
        super::super::Verdict::AbnormalProcess,
        super::super::Verdict::Malformed,
        super::super::Verdict::Unsupported,
        super::super::Verdict::Unknown,
    ] {
        let mut run = valid.clone();
        run.unregistered_results[0].native_verdict = verdict;
        refusal(audit(&[], Some(&run)), "native observation status")?;
    }
    let mut run = valid.clone();
    run.unregistered_results[0].subject.source_digest =
        sha256(b"another source with the same native name");
    refusal(audit(&[], Some(&run)), "observation identity or source")?;
    let mut run = valid.clone();
    run.unregistered_results[0].properties.clear();
    refusal(audit(&[], Some(&run)), "native cover projection differs")?;
    let mut run = valid.clone();
    run.execution
        .as_mut()
        .ok_or("execution absent")?
        .lane_success = false;
    run.unregistered_results[0].status = ObservationStatus::Failed;
    refusal(audit(&[], Some(&run)), "native observation status")?;
    for verdict in [
        super::super::Verdict::Counterexample,
        super::super::Verdict::UnreachableCover,
    ] {
        let mut run = valid.clone();
        run.execution
            .as_mut()
            .ok_or("execution absent")?
            .lane_success = false;
        run.unregistered_results[0].status = ObservationStatus::Failed;
        run.unregistered_results[0].native_verdict = verdict.clone();
        refusal(audit(&[], Some(&run)), "native observation status")?;
        if verdict == super::super::Verdict::Counterexample {
            run.unregistered_results[0].properties.push(NativeProperty {
                name: "m::proof.assertion.1".into(),
                status: "FAILURE".into(),
                description: None,
                location: None,
            });
            audit(&[], Some(&run))?;
            run.unregistered_results[0]
                .properties
                .last_mut()
                .ok_or("failure absent")?
                .name = "m::proof.unwind.1".into();
            refusal(audit(&[], Some(&run)), "native observation status")?;
        } else {
            run.unregistered_results[0].cover_properties[0].status = "UNREACHABLE".into();
            run.unregistered_results[0].properties[0].status = "UNREACHABLE".into();
            run.unregistered_results[0]
                .covers
                .as_mut()
                .ok_or("covers absent")?
                .satisfied = 0;
            audit(&[], Some(&run))?;
        }
    }
    Ok(())
}

#[test]
fn aggregate_process_preserves_nonzero_timeout_and_unsettled_capture() -> AuditResult<()> {
    use super::super::{Cleanup, ProcessState};
    let run = complete_document()?;
    let invocations = &run
        .execution
        .as_ref()
        .ok_or("execution absent")?
        .invocations;
    let commitment = sha256(b"the exact retained invocation index");
    let positive = aggregate_process(invocations, &commitment);
    assert_eq!(positive.state, ProcessState::Completed);
    assert_eq!(positive.exit_code, Some(0));
    assert_eq!(
        positive.raw_output_digest.as_deref(),
        Some(commitment.as_str())
    );
    let mut calls = invocations.clone();
    calls.last_mut().ok_or("verification absent")?.exit_code = Some(17);
    let failed = aggregate_process(&calls, &commitment);
    assert_eq!(failed.state, ProcessState::Failed);
    assert_eq!(failed.exit_code, Some(17));
    let mut errored = invocations.clone();
    errored.last_mut().ok_or("verification absent")?.error = Some("capture failed".into());
    let refused = aggregate_process(&errored, &commitment);
    assert_eq!(refused.state, ProcessState::Failed);
    assert!(refused.exit_code.is_none());
    calls.last_mut().ok_or("verification absent")?.timed_out = true;
    assert_eq!(
        aggregate_process(&calls, &commitment).state,
        ProcessState::TimedOut
    );
    let mut calls = invocations.clone();
    calls[0].capture_complete = false;
    assert_eq!(
        aggregate_process(&calls, &commitment).cleanup_state,
        Cleanup::NotObserved
    );
    let absent = aggregate_process(&invocations[..invocations.len() - 1], &commitment);
    assert_eq!(absent.state, ProcessState::NotStarted);
    assert_eq!(absent.cleanup_state, Cleanup::NotApplicable);
    assert!(absent.exit_code.is_none() && absent.raw_output_digest.is_none());
    Ok(())
}

#[test]
fn native_failed_assertion_is_distinct_from_unwind_expected_panic_and_false_success()
-> AuditResult<()> {
    let subject = complete_document()?
        .execution
        .ok_or("execution absent")?
        .selected_subjects;
    let failed = "Checking harness m::proof...\nCheck 1: m::proof.assertion.1\n - Status: FAILURE\n - Description: \"intended assertion\"\n - Location: source.rs:1:1 in function m::proof\nCheck 2: m::proof.cover.1\n - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- FAILED\nVerification Time: 0.012345678s\n";
    let row = native_inventory::observe(&subject, failed).0.remove(0);
    assert_eq!(row.status, ObservationStatus::Failed);
    assert_eq!(row.native_verdict, super::super::Verdict::Counterexample);
    assert_eq!(row.properties.len(), 2);
    assert_eq!(
        row.properties[0].description.as_deref(),
        Some("\"intended assertion\"")
    );
    assert_eq!(row.duration_ms, Some(12));
    for bad in [
        failed.replace(".assertion.1", ".unwind.1"),
        failed.replace("VERIFICATION:- FAILED", "VERIFICATION:- SUCCESSFUL"),
        failed.replace(
            "VERIFICATION:- FAILED",
            "VERIFICATION:- SUCCESSFUL (encountered one or more panics as expected)",
        ),
        failed.replace(" - Status: FAILURE", " - Status: ERROR"),
    ] {
        assert_eq!(
            native_inventory::observe(&subject, &bad).0[0].status,
            ObservationStatus::NotEstablished
        );
    }
    Ok(())
}

#[test]
fn measured_multiline_assertion_description_is_retained_without_metadata_borrowing()
-> AuditResult<()> {
    let bytes =
        include_bytes!("../../../tests/fixtures/kani-native-output/currentness-mutant.stdout");
    assert_eq!(
        sha256(bytes),
        "sha256:f14b79a7045346579a5592bd1fbee69ef947f9922dbd641ce99acd406907e147"
    );
    let output = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let properties = kani_output::native_properties(output)?;
    assert_eq!(properties.len(), 40);
    let failures: Vec<_> = properties
        .iter()
        .filter(|property| property.status == "FAILURE")
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(
        failures[0].name,
        "domain::probe::kani_proofs::currentness_action_and_base_side_are_disjoint.assertion.1"
    );
    assert_eq!(
        failures[0].description.as_deref(),
        Some(
            "\"assertion failed: value.permits_candidate_action() ==\nvalue == SourceCurrentness::CandidateCurrent\""
        )
    );
    assert_eq!(
        failures[0].location.as_deref(),
        Some(
            "crates/ripr-contract/src/domain/probe_kani.rs:32:5 in function domain::probe::kani_proofs::currentness_action_and_base_side_are_disjoint"
        )
    );
    assert!(has_counterexample_properties(&properties));
    assert_eq!(
        kani_output::cover_properties(output)?
            .iter()
            .filter(|property| property.status == "SATISFIED")
            .count(),
        6
    );
    for malformed in [
        output.replace(
            "value == SourceCurrentness::CandidateCurrent\"",
            "value == SourceCurrentness::CandidateCurrent",
        ),
        output.replace(
            "value == SourceCurrentness::CandidateCurrent\"",
            "- Status: SUCCESS\"",
        ),
        output.replace(
            "value == SourceCurrentness::CandidateCurrent\"",
            "Check 99: hidden.cover.1\"",
        ),
    ] {
        refusal(
            kani_output::native_properties(&malformed),
            "native description is unterminated",
        )?;
    }
    Ok(())
}

#[test]
fn native_property_identity_and_summary_must_both_establish_success() -> AuditResult<()> {
    let run = complete_document()?;
    let subjects = &run
        .execution
        .as_ref()
        .ok_or("execution absent")?
        .selected_subjects;
    let output = "Checking harness m::proof...\nCheck 1: m::proof.cover.1\n - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
    assert_eq!(
        native_inventory::observe(subjects, output).0[0].status,
        ObservationStatus::Verified
    );
    for bad in [
        output.replace("Check 1: m::proof.cover.1\n - Status: SATISFIED\n", ""),
        output.replace("SATISFIED", "UNSATISFIABLE"),
        output.replace(
            "VERIFICATION:- SUCCESSFUL",
            " - Description: VERIFICATION:- SUCCESSFUL",
        ),
    ] {
        assert_ne!(
            native_inventory::observe(subjects, &bad).0[0].status,
            ObservationStatus::Verified
        );
    }
    refusal(
        kani_output::cover_properties(
            "Check 1: h.cover.1\n - Status: SATISFIED\nCheck 2: h.cover.1\n - Status: SATISFIED\n",
        ),
        "identity/status is duplicate",
    )?;
    Ok(())
}

#[test]
fn ambiguous_native_blocks_publish_a_consistent_refusal_without_borrowed_properties()
-> AuditResult<()> {
    let positive = "Checking harness m::proof...\nCheck 1: m::proof.cover.1\n - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\n";
    let failed = positive.replace("SATISFIED", "UNSATISFIABLE");
    for output in [
        format!("{positive}{positive}"),
        format!("{positive}{failed}"),
    ] {
        let mut run = complete_document()?;
        let subjects = &run
            .execution
            .as_ref()
            .ok_or("execution absent")?
            .selected_subjects;
        let (rows, observed, reasons) = native_inventory::observe(subjects, &output);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, ObservationStatus::NotEstablished);
        assert_eq!(rows[0].native_verdict, super::super::Verdict::Ambiguous);
        assert!(rows[0].detail.contains("duplicate native result blocks"));
        assert!(rows[0].properties.is_empty() && rows[0].cover_properties.is_empty());
        assert!(rows[0].covers.is_none() && rows[0].raw_block_digest.is_none());
        assert!(!reasons.is_empty());
        run.unregistered_results = rows;
        run.inventory.complete = false;
        run.inventory.rejection_reasons = reasons.clone();
        let execution = run.execution.as_mut().ok_or("execution absent")?;
        execution.observed_subjects = observed;
        execution.inventory_rejections = reasons.clone();
        execution.rejection_reasons = reasons;
        execution.lane_success = false;
        let bytes = serde_json::to_vec(&run).map_err(|error| error.to_string())?;
        let decoded: Run = parse_json(&bytes)?;
        assert_eq!(
            serde_json::to_value(&decoded).map_err(|error| error.to_string())?,
            serde_json::to_value(&run).map_err(|error| error.to_string())?
        );
        let summary = audit(&[], Some(&decoded))?;
        assert_eq!(summary.native_assurance, "not_assessed");
        assert_eq!(summary.declared_established_rows, 0);
    }
    Ok(())
}

#[test]
fn source_manifest_rejects_duplicate_and_escaping_paths() -> AuditResult<()> {
    let digest = sha256(b"source");
    for (text, expected) in [
        (
            format!("{{\"a.rs\":\"{digest}\",\"a.rs\":\"{digest}\"}}"),
            "duplicate JSON key",
        ),
        (
            format!("{{\"../a.rs\":\"{digest}\"}}"),
            "artifact path must remain relative",
        ),
        (
            format!("{{\"C:/a.rs\":\"{digest}\"}}"),
            "artifact path must remain relative",
        ),
    ] {
        refusal(parse_source_manifest(text.as_bytes()), expected)?;
    }
    let manifest = SourceManifest::from([("src/a.rs".to_string(), digest)]);
    assert_eq!(
        parse_source_manifest(&canonical_manifest_bytes(&manifest)?)?,
        manifest
    );
    Ok(())
}

#[test]
fn pinned_native_inventory_requires_exact_version_count_and_captured_files() -> AuditResult<()> {
    let bytes = include_bytes!("../../../tests/fixtures/kani-native-inventory/kani-list.json");
    assert_eq!(
        sha256(bytes),
        "sha256:d023ab5a23a66e93b649312ef2f229af1659d04abd522330c26d587dc5fb7224"
    );
    let original: serde_json::Value = parse_json(bytes)?;
    let files = original["standard-harnesses"]
        .as_object()
        .ok_or("fixture inventory absent")?;
    let sources: SourceManifest = files
        .keys()
        .map(|path| (path.clone(), sha256(b"synthetic source binding")))
        .collect();
    let root = Path::new("/repo");
    let subjects = native_inventory::native_inventory(bytes, "ripr-contract", root, &sources)?;
    assert_eq!(subjects.len(), 21);
    assert_eq!(
        subjects
            .iter()
            .filter(|s| s.qualified_name.as_deref()
                == Some(
                    "domain::probe::kani_proofs::currentness_action_and_base_side_are_disjoint"
                ))
            .count(),
        1
    );
    assert!(native_inventory::compare_inventories(&subjects, &subjects).is_empty());
    for file in ["/outside/source.rs", "crates/ripr-contract/src/missing.rs"] {
        let mut changed = original.clone();
        changed["standard-harnesses"] = serde_json::json!({file: ["m::proof"]});
        changed["totals"]["standard-harnesses"] = serde_json::json!(1);
        refusal(
            native_inventory::native_inventory(
                &serde_json::to_vec(&changed).map_err(|e| e.to_string())?,
                "ripr-contract",
                root,
                &sources,
            ),
            "outside captured",
        )?;
    }
    let file = files.keys().next().ok_or("fixture files absent")?;
    let mut repeated = original.clone();
    repeated["standard-harnesses"] = serde_json::json!({file: ["m::proof", "m::proof"]});
    repeated["totals"]["standard-harnesses"] = serde_json::json!(2);
    refusal(
        native_inventory::native_inventory(
            &serde_json::to_vec(&repeated).map_err(|e| e.to_string())?,
            "ripr-contract",
            root,
            &sources,
        ),
        "duplicate qualified native harness",
    )?;
    let mut duplicate = subjects.clone();
    duplicate.push(subjects[0].clone());
    assert!(!native_inventory::compare_inventories(&duplicate, &subjects).is_empty());
    assert!(!native_inventory::compare_inventories(&subjects, &subjects[1..]).is_empty());
    assert!(!native_inventory::compare_inventories(&subjects[1..], &subjects).is_empty());
    for (pointer, value) in [
        ("/kani-version", serde_json::json!("0.69.0")),
        ("/file-version", serde_json::json!("0.2")),
        ("/totals/standard-harnesses", serde_json::json!(20)),
        ("/contracts", serde_json::json!([{}])),
        ("/totals/contract-harnesses", serde_json::json!(1)),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).ok_or("fixture field absent")? = value;
        assert!(
            native_inventory::native_inventory(
                &serde_json::to_vec(&changed).map_err(|e| e.to_string())?,
                "ripr-contract",
                root,
                &sources
            )
            .is_err(),
            "accepted {pointer}"
        );
    }
    refusal(
        native_inventory::native_inventory(bytes, "ripr-contract", root, &BTreeMap::new()),
        "outside captured source closure",
    )?;
    let duplicated_key = String::from_utf8(bytes.to_vec())
        .map_err(|e| e.to_string())?
        .replacen(
            "\"kani-version\": \"0.68.0\",",
            "\"kani-version\": \"0.68.0\",\"kani-version\": \"0.68.0\",",
            1,
        );
    refusal(
        native_inventory::native_inventory(
            duplicated_key.as_bytes(),
            "ripr-contract",
            root,
            &sources,
        ),
        "duplicate JSON key",
    )?;
    Ok(())
}

#[test]
fn observed_invocation_cannot_borrow_success_across_profile_or_process_refusals() -> AuditResult<()>
{
    let run = complete_document()?;
    let execution = run.execution.as_ref().ok_or("execution absent")?;
    let invocation = execution.invocations.last().ok_or("verification absent")?;
    let stdout = b"Kani Rust Verifier 0.68.0\nCBMC 6.11.0\nChecking harness m::proof...\nCheck 1: m::proof.cover.1\n - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n";
    let observe = |call: &Invocation, out: Option<&[u8]>, err: Option<&[u8]>| {
        let observation =
            native_inventory::observe_invocation(&execution.selected_subjects, call, out, err);
        audit_observation_roundtrip(&run, call, &observation)?;
        Ok::<_, String>(observation)
    };
    let positive = observe(invocation, Some(stdout), Some(b""))?;
    assert_eq!(positive.0[0].status, ObservationStatus::Verified);
    assert_eq!(positive.1.len(), 1);
    assert!(positive.2.is_empty());
    for control in 0..5 {
        let mut changed = invocation.clone();
        match control {
            0 => changed.timed_out = true,
            1 => changed.exit_code = Some(7),
            2 => changed.exit_code = None,
            3 => changed.capture_complete = false,
            _ => changed.error = Some("synthetic capture failure".into()),
        }
        assert_eq!(
            observe(&changed, Some(stdout), Some(b""))?.0[0].status,
            ObservationStatus::NotEstablished,
            "control {control}"
        );
    }
    for bytes in [None, Some(&b"\xff"[..])] {
        let refused = observe(invocation, bytes, Some(b""))?;
        assert_eq!(refused.0[0].status, ObservationStatus::NotEstablished);
        assert!(!refused.2.is_empty());
    }
    let text = std::str::from_utf8(stdout).map_err(|e| e.to_string())?;
    for changed in [
        text.replace("0.68.0", "0.69.0"),
        text.replace("6.11.0", "6.12.0"),
        text.replace("Kani Rust Verifier 0.68.0\n", ""),
    ] {
        let refused = observe(invocation, Some(changed.as_bytes()), Some(b""))?;
        assert_eq!(refused.0[0].status, ObservationStatus::NotEstablished);
        assert!(!refused.2.is_empty());
    }
    assert!(
        !observe(
            invocation,
            Some(stdout),
            Some(b"Checking harness foreign::proof...\n")
        )?
        .2
        .is_empty()
    );
    Ok(())
}

#[test]
fn incomplete_streams_preserve_observations_and_known_process_failures() -> AuditResult<()> {
    let run = complete_document()?;
    let execution = run.execution.as_ref().ok_or("execution absent")?;
    let invocation = execution.invocations.last().ok_or("verification absent")?;
    let stdout = b"Kani Rust Verifier 0.68.0\nCBMC 6.11.0\nChecking harness m::proof...\nCheck 1: m::proof.cover.1\n - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n";
    let observe = |call: &Invocation, out: Option<&[u8]>, err: Option<&[u8]>| {
        let observation =
            native_inventory::observe_invocation(&execution.selected_subjects, call, out, err);
        audit_observation_roundtrip(&run, call, &observation)?;
        Ok::<_, String>(observation)
    };
    let positive = observe(invocation, Some(stdout), Some(b""))?;
    assert_eq!(positive.0[0].status, ObservationStatus::Verified);
    for control in 0..3 {
        let mut call = invocation.clone();
        let stderr = match control {
            0 => None,
            1 => Some(&b"\xff"[..]),
            _ => {
                call.capture_complete = false;
                Some(&b""[..])
            }
        };
        let refused = observe(&call, Some(stdout), stderr)?;
        assert_eq!(refused.0[0].status, ObservationStatus::NotEstablished);
        assert_eq!(refused.1, positive.1, "observed stdout must not disappear");
        assert_eq!(
            refused.0[0].raw_block_digest,
            positive.0[0].raw_block_digest
        );
        assert_eq!(refused.0[0].properties.len(), 1);
        assert!(
            refused
                .2
                .iter()
                .any(|r| r.contains("not completely captured"))
        );
        assert!(
            !refused
                .2
                .iter()
                .any(|r| r.contains("missing native result"))
        );
    }
    for (timed_out, verdict, detail) in [
        (true, crate::formal_evidence::Verdict::Timeout, "deadline"),
        (
            false,
            crate::formal_evidence::Verdict::AbnormalProcess,
            "normal process outcome",
        ),
    ] {
        let mut call = invocation.clone();
        call.capture_complete = false;
        call.timed_out = timed_out;
        call.exit_code = None;
        call.error = Some("synthetic process/capture error".into());
        let refused = observe(&call, None, None)?;
        assert_eq!(refused.0[0].status, ObservationStatus::NotEstablished);
        assert_eq!(refused.0[0].native_verdict, verdict);
        assert!(refused.0[0].detail.contains(detail));
        assert!(
            refused
                .2
                .iter()
                .any(|r| r.contains("not completely captured"))
        );
    }
    Ok(())
}

#[test]
fn unsupported_stderr_results_cannot_leave_verified_rows() -> AuditResult<()> {
    let run = complete_document()?;
    let execution = run.execution.as_ref().ok_or("execution absent")?;
    let invocation = execution.invocations.last().ok_or("verification absent")?;
    let stdout = b"Kani Rust Verifier 0.68.0\nCBMC 6.11.0\nChecking harness m::proof...\nCheck 1: m::proof.cover.1\n - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n";
    let observation = native_inventory::observe_invocation(
        &execution.selected_subjects,
        invocation,
        Some(stdout),
        Some(b"Checking harness foreign::proof...\n"),
    );
    audit_observation_roundtrip(&run, invocation, &observation)?;
    let (rows, observed, reasons) = observation;
    assert_eq!(rows[0].status, ObservationStatus::NotEstablished);
    assert!(rows[0].detail.contains("unsupported stderr"));
    assert_eq!(observed.len(), 1);
    assert!(rows[0].raw_block_digest.is_some());
    assert!(reasons.iter().any(|r| r.contains("unsupported stderr")));
    Ok(())
}

#[test]
fn observer_roundtrip_refuses_lane_and_capture_promotion() -> AuditResult<()> {
    let original = complete_document()?;
    let execution = original.execution.as_ref().ok_or("execution absent")?;
    let mut invocation = execution
        .invocations
        .last()
        .ok_or("verification absent")?
        .clone();
    let stdout = b"Kani Rust Verifier 0.68.0\nCBMC 6.11.0\nChecking harness m::proof...\nCheck 1: m::proof.cover.1\n - Status: SATISFIED\n ** 1 of 1 cover properties satisfied\nVERIFICATION:- SUCCESSFUL\nVerification Time: 0.1s\n";
    let positive_observation = native_inventory::observe_invocation(
        &execution.selected_subjects,
        &invocation,
        Some(stdout),
        Some(b""),
    );
    let positive = audit_observation_roundtrip(&original, &invocation, &positive_observation)?;
    assert!(
        positive
            .execution
            .as_ref()
            .ok_or("execution absent")?
            .lane_success
    );
    assert_eq!(
        positive.unregistered_results[0].status,
        ObservationStatus::Verified
    );

    // A missing stderr stream is represented honestly in both the invocation
    // receipt and interpreter inputs. Available stdout remains diagnostic only.
    invocation.capture_complete = false;
    invocation.stderr = None;
    let observation = native_inventory::observe_invocation(
        &execution.selected_subjects,
        &invocation,
        Some(stdout),
        None,
    );
    let incomplete = audit_observation_roundtrip(&original, &invocation, &observation)?;
    let captured = incomplete.execution.as_ref().ok_or("execution absent")?;
    assert!(!captured.lane_success);
    assert!(!incomplete.inventory.complete);
    assert!(!captured.inventory_rejections.is_empty());
    assert!(
        !captured
            .invocations
            .last()
            .ok_or("verification absent")?
            .capture_complete
    );
    assert_eq!(
        incomplete.unregistered_results[0].status,
        ObservationStatus::NotEstablished
    );
    assert_eq!(
        captured.observed_subjects,
        positive
            .execution
            .as_ref()
            .ok_or("execution absent")?
            .observed_subjects
    );
    assert_eq!(
        incomplete.unregistered_results[0].raw_block_digest,
        positive.unregistered_results[0].raw_block_digest
    );

    for (promote_lane, expected) in [
        (
            true,
            "successful lane lacks complete current admitted native evidence",
        ),
        (
            false,
            "complete capture lacks raw streams or native process observation",
        ),
    ] {
        let mut changed = incomplete.clone();
        let captured = changed.execution.as_mut().ok_or("execution absent")?;
        if promote_lane {
            captured.lane_success = true;
        } else {
            captured
                .invocations
                .last_mut()
                .ok_or("verification absent")?
                .capture_complete = true;
        }
        // Only the selected Boolean changes. Do not regenerate process or
        // inventory fields to make the contradictory document consistent.
        let bytes = serde_json::to_vec(&changed).map_err(|error| error.to_string())?;
        let decoded: Run = parse_json(&bytes)?;
        assert_eq!(audit(&[], Some(&decoded)).err().as_deref(), Some(expected));
    }
    Ok(())
}

#[test]
fn required_schema_fields_cannot_be_omitted() -> AuditResult<()> {
    let schema: serde_json::Value = parse_json(include_bytes!(
        "../../../../docs/formal-verification/schema/native-proof-run-v1.schema.json"
    ))?;
    let original = serde_json::to_value(complete_document()?).map_err(|e| e.to_string())?;
    for (document_path, schema_path) in [
        ("", ""),
        ("/execution", "/$defs/native_execution"),
        (
            "/execution/source_before",
            "/$defs/native_execution/properties/source_before",
        ),
        (
            "/execution/invocations/0",
            "/$defs/native_execution/properties/invocations/items",
        ),
        ("/unregistered_results/0", "/$defs/unregistered_result"),
        ("/unregistered_results/0/subject", "/$defs/native_subject"),
        (
            "/unregistered_results/0/cover_properties/0",
            "/$defs/unregistered_result/properties/cover_properties/items",
        ),
    ] {
        let required = schema
            .pointer(&format!("{schema_path}/required"))
            .and_then(serde_json::Value::as_array)
            .ok_or("schema required-field set is absent")?;
        for field in required {
            let field = field.as_str().ok_or("schema field is not a string")?;
            let mut document = original.clone();
            let object = document
                .pointer_mut(document_path)
                .and_then(serde_json::Value::as_object_mut)
                .ok_or("synthetic object is absent")?;
            assert!(
                object.remove(field).is_some(),
                "fixture lacks {document_path}/{field}"
            );
            let bytes = serde_json::to_vec(&document).map_err(|e| e.to_string())?;
            assert!(
                parse_json::<Run>(&bytes).is_err(),
                "omitted {document_path}/{field} was accepted"
            );
        }
    }
    Ok(())
}
