use super::super::python_eval_sweep_refresh::{materialized, test_binary, test_subject};
use super::super::*;

const EMPTY: &[u8] =
    include_bytes!("../../../../../fixtures/python_noop_docstring_change/expected/check.json");
const NONEMPTY: &[u8] =
    include_bytes!("../../../../../fixtures/python_boundary_gap/expected/check.json");

fn capture(stdout: &[u8]) -> AnalysisPass {
    analysis_pass_from_capture(
        false,
        true,
        10,
        stdout.to_vec(),
        b"retained stderr".to_vec(),
    )
}

fn document(findings: Value) -> Value {
    json!({
        "analysis_outcome": {
            "analysis_complete": true,
            "outcome": {
                "kind": if findings.as_array().is_some_and(|items| !items.is_empty()) {
                    "complete_with_findings"
                } else {
                    "complete_no_findings"
                },
                "limitations": [],
            },
        },
        "findings": findings,
    })
}

fn reports(first: &AnalysisPass, stability: Option<&StabilityResult>) -> (Value, Value) {
    let subject = test_subject("admission", &"c".repeat(40), "click_typer");
    let binary = test_binary();
    let digest = "a".repeat(64);
    let row = assemble_row(RowInputs {
        subject: &subject,
        binary: &binary,
        diff_portable: &subject.synthetic_diff,
        diff_digest: Some(digest.clone()),
        materialization: &materialized(),
        first: Some(first),
        stability,
        corpus: None,
        config_profile: CONFIG_PROFILE_DEFAULT,
    });
    let (stability_limitation, output_identity_stable) = match stability {
        Some(StabilityResult::Compared {
            output_identity_stable,
            ..
        }) => (None, Some(*output_identity_stable)),
        Some(StabilityResult::NotCompared { limitation }) => (Some(limitation.clone()), None),
        None => (None, None),
    };
    let executions = [SubjectExecution {
        subject_id: subject.id.clone(),
        row,
        materialization_state: "materialized",
        materialization_source: Some("local_seed_clone"),
        materialization_limitation: None,
        analysis_complete: Some(first.complete),
        analysis_limitations: analysis_limitations(first),
        stability_performed: stability.is_some(),
        stability_limitation,
        output_identity_stable,
        config_profile: CONFIG_PROFILE_DEFAULT.to_string(),
        subject_config_path: None,
        corpus_state: "absent",
        corpus_limitation: None,
        infrastructure_limitation: None,
    }];
    let manifest = AcceptedManifest {
        sha256: digest.clone(),
        subjects: vec![subject],
        incomplete: Vec::new(),
    };
    let args = RefreshArgs {
        manifest: "manifest.json".to_string(),
        ripr_bin: Some("bin/ripr".to_string()),
        out: Some("candidate".to_string()),
        checkout_root: "checkouts".to_string(),
        timeout: Duration::from_secs(1),
        clone_timeout: Duration::from_secs(1),
        allow_network: true,
    };
    (
        assemble_receipt(&digest, &binary, &executions),
        assemble_execution_receipt(
            &manifest,
            &digest,
            &args,
            &binary,
            Path::new("candidate"),
            &executions,
            10,
        ),
    )
}

/// Exercise the actual bytes-to-pass function, then row/report assembly. A
/// parser-only rejection would miss a caller that discarded its failure.
fn rejects_before_summary(stdout: &[u8], diagnostic: &str) {
    let first = capture(stdout);
    assert_eq!(first.status.as_str(), "crashed", "{diagnostic}");
    assert!(!first.complete, "{diagnostic}");
    assert_eq!(
        first.stdout.as_slice(),
        stdout,
        "rejected stdout bytes stay exact"
    );
    assert_eq!(first.stderr.as_slice(), b"retained stderr");
    let (receipt, execution) = reports(&first, None);
    assert_eq!(receipt["repos"][0]["status"], "crashed");
    assert_eq!(receipt["repos"][0]["digests"]["raw"], sha256_hex(stdout));
    assert_eq!(receipt["summary"]["repos_total"], 1);
    assert_eq!(receipt["summary"]["repos_run"], 1);
    assert_eq!(receipt["summary"]["crash_count"], 1);
    assert_eq!(receipt["summary"]["gate_status"], "review");
    assert!(
        execution["subjects"][0]["analysis"]["limitations"]
            .as_array()
            .is_some_and(|items| items
                .iter()
                .any(|item| { item.as_str().is_some_and(|text| text.contains(diagnostic)) })),
        "the admission reason must reach the execution receipt: {execution}"
    );
    // A missing binary makes an accidental repeat fail with a spawn error;
    // the asserted first-status refusal establishes that no repeat was tried.
    let binary = BinaryIdentity {
        absolute: PathBuf::from("definitely-missing-admission-binary"),
        ..test_binary()
    };
    let decision = run_stability_pass(
        &binary,
        Path::new("unused"),
        Path::new("unused"),
        Path::new("unused"),
        Duration::from_secs(1),
        "admission",
        &first,
    );
    assert!(
        matches!(decision, StabilityResult::NotCompared { limitation }
        if limitation.contains("runs only where the first result is complete"))
    );

    // The same malformed bytes on the repeat side cannot compare or pass,
    // and the second-pass diagnosis survives in the stability limitation.
    let control = capture(EMPTY);
    assert_eq!(control.status.as_str(), "complete");
    let decision = compare_passes(&control, capture(stdout));
    assert!(
        matches!(&decision, StabilityResult::NotCompared { limitation }
        if limitation.contains(diagnostic))
    );
    let (receipt, execution) = reports(&control, Some(&decision));
    assert_eq!(receipt["summary"]["gate_status"], "review");
    assert!(
        execution["subjects"][0]["stability"]["limitation"]
            .as_str()
            .is_some_and(|reason| reason.contains(diagnostic)),
        "the repeat admission reason must reach the execution receipt: {execution}"
    );
}

#[test]
fn malformed_findings_never_become_complete_or_stable() {
    rejects_before_summary(
        br#"{"analysis_outcome":{"analysis_complete":true}}"#,
        "findings",
    );
    for findings in [
        json!(null),
        json!({}),
        json!(false),
        json!([null]),
        json!([3]),
    ] {
        rejects_before_summary(document(findings).to_string().as_bytes(), "findings");
    }
    for classification in [json!(null), json!("not_an_exposure_class"), json!(5)] {
        rejects_before_summary(
            document(json!([{ "classification": classification }]))
                .to_string()
                .as_bytes(),
            "findings[0].classification",
        );
    }
    rejects_before_summary(
        document(json!([{}])).to_string().as_bytes(),
        "findings[0].classification",
    );
    for (field, malformed) in [
        ("canonical_gap_id", json!(null)),
        ("canonical_gap_id", json!("")),
        ("canonical_gap", json!({})),
        ("canonical_gap", json!(false)),
        ("oracle_alignment", json!(null)),
        ("static_limit_kind", json!([])),
        ("repair_placement", json!(null)),
        ("repair_placement", json!({"verify_command": false})),
        ("python_repair_card", json!([])),
    ] {
        let mut finding = json!({"classification": "weakly_exposed"});
        finding[field] = malformed;
        rejects_before_summary(
            document(json!([finding])).to_string().as_bytes(),
            &format!("findings[0].{field}"),
        );
    }
    rejects_before_summary(
        document(json!([{
            "classification": "weakly_exposed",
            "canonical_gap_id": "first",
            "canonical_gap": {"id": "different"},
        }]))
        .to_string()
        .as_bytes(),
        "canonical_gap_id disagrees",
    );
}

#[test]
fn malformed_or_inconsistent_outcomes_never_pass() {
    for envelope in [
        json!(null),
        json!({}),
        json!({"analysis_complete": "true"}),
        json!({"analysis_complete": true}),
        json!({"analysis_complete": true, "outcome": []}),
        json!({"analysis_complete": true, "outcome": {}}),
        json!({"analysis_complete": true, "outcome": {"kind": "future_kind"}}),
        json!({"analysis_complete": true, "outcome": {"kind": "partial_with_limitations"}}),
        json!({"analysis_complete": false, "outcome": {"kind": "complete_no_findings"}}),
        json!({"analysis_complete": true, "outcome": {"kind": "complete_no_findings"}}),
        json!({"analysis_complete": true, "outcome": {
            "kind": "complete_no_findings", "limitations": [null]}}),
        json!({"analysis_complete": true, "document_complete": "true", "outcome": {
            "kind": "complete_no_findings", "limitations": []}}),
    ] {
        let mut value = document(json!([]));
        value["analysis_outcome"] = envelope;
        rejects_before_summary(value.to_string().as_bytes(), "analysis_outcome");
    }
    let mut claimed_findings = document(json!([]));
    claimed_findings["analysis_outcome"]["outcome"]["kind"] = json!("complete_with_findings");
    rejects_before_summary(claimed_findings.to_string().as_bytes(), "kind disagrees");
    let mut claimed_empty = document(json!([{"classification": "exposed"}]));
    claimed_empty["analysis_outcome"]["outcome"]["kind"] = json!("complete_no_findings");
    rejects_before_summary(claimed_empty.to_string().as_bytes(), "kind disagrees");
    for limitations in [json!(null), json!([false]), json!([{"run_status": null}])] {
        let mut value = document(json!([]));
        value["run_limitations"] = limitations;
        rejects_before_summary(value.to_string().as_bytes(), "run_limitations");
    }
}

#[test]
fn duplicate_keys_and_invalid_utf8_keep_a_named_execution_limitation() {
    for stdout in [
        br#"{"analysis_outcome":{"analysis_complete":true,"outcome":{"kind":"complete_no_findings","limitations":[]}},"findings":[{"classification":"exposed"}],"findings":[]}"#.as_slice(),
        br#"{"analysis_outcome":{"analysis_complete":false,"analysis_complete":true,"outcome":{"kind":"complete_no_findings","limitations":[]}},"findings":[]}"#.as_slice(),
        br#"{"analysis_outcome":{"analysis_complete":true,"outcome":{"kind":"complete_with_findings","limitations":[]}},"findings":[{"classification":"static_unknown","classification":"exposed"}]}"#.as_slice(),
    ] {
        rejects_before_summary(stdout, "duplicate object key");
    }
    // Insert an invalid byte in an otherwise ignored string field. The old
    // lossy decoder repaired exactly this shape before admitting the output.
    let mut invalid_utf8 = b"{\"extra\":\"\xff\",".to_vec();
    invalid_utf8.extend_from_slice(
        document(json!([]))
            .to_string()
            .trim_start_matches('{')
            .as_bytes(),
    );
    rejects_before_summary(&invalid_utf8, "UTF-8");
    rejects_before_summary(b"{", "not well-formed JSON");
    rejects_before_summary(b"[]", "check document");
}

#[test]
fn real_empty_nonempty_and_grouped_reports_keep_stable_comparisons() {
    let grouped = include_bytes!(
        "../../../../../fixtures/error_variant_boxed_wrapper_downcast_witness/expected/check.json"
    );
    for (stdout, has_findings) in [(EMPTY, false), (NONEMPTY, true), (grouped.as_slice(), true)] {
        let first = capture(stdout);
        assert_eq!(first.status.as_str(), "complete");
        assert_eq!(!first.gap_ids.is_empty(), has_findings);
        let decision = compare_passes(&first, capture(stdout));
        assert!(matches!(
            &decision,
            StabilityResult::Compared {
                gap_stable: true,
                ..
            }
        ));
        let (receipt, _) = reports(&first, Some(&decision));
        assert_eq!(receipt["summary"]["gate_status"], "pass");
        assert_eq!(receipt["summary"]["gap_id_stable_count"], 1);
        assert_eq!(receipt["summary"]["repos_run"], 1);
        assert_eq!(first.stdout.as_slice(), stdout);
    }
    let first = capture(NONEMPTY);
    assert_eq!(first.classification.to_json()["weakly_exposed"], 1);
    assert_eq!(first.gap_ids.len(), 1);
    let grouped_pass = capture(grouped);
    assert!(
        grouped_pass
            .output_json
            .as_ref()
            .and_then(|value| value.get("findings"))
            .and_then(Value::as_array)
            .is_some_and(|findings| {
                findings
                    .iter()
                    .filter(|finding| finding.get("canonical_gap_id").is_some())
                    .count()
                    > grouped_pass.gap_ids.len()
            }),
        "the grouped control must actually repeat canonical IDs"
    );
}

#[test]
fn missing_envelope_and_terminal_states_stay_distinct() {
    let missing = capture(br#"{"findings":[]}"#);
    assert_eq!(missing.status.as_str(), "partial");
    assert!(
        analysis_limitations(&missing)
            .iter()
            .any(|text| text.contains("analysis_outcome"))
    );
    assert_eq!(
        reports(&missing, None).0["summary"]["gate_status"],
        "review"
    );
    for (kind, expected) in [
        ("unsupported_input", "unsupported"),
        ("analysis_failed", "parse-failed"),
        ("partial_with_limitations", "partial"),
    ] {
        let stdout = json!({
            "analysis_outcome": {"analysis_complete": false,
                "outcome": {"kind": kind, "limitations": [{"kind": "language_scope_unsupported"}]}},
            "findings": [],
        });
        let pass = capture(stdout.to_string().as_bytes());
        assert_eq!(pass.status.as_str(), expected);
        assert_eq!(
            pass.limitations,
            vec!["language_scope_unsupported".to_string()]
        );
    }
    for (stdout, expected) in [
        (
            include_bytes!("../../../../../fixtures/python_disabled/expected/check.json").as_slice(),
            "partial",
        ),
        (
            include_bytes!(
                "../../../../../fixtures/typescript_parse_error_unsupported_syntax/expected/check.json"
            )
            .as_slice(),
            "parse-failed",
        ),
    ] {
        let pass = capture(stdout);
        assert_eq!(pass.status.as_str(), expected);
        assert!(!pass.limitations.is_empty());
    }
    for (timed_out, exit_ok, expected) in [
        (true, true, "timed-out"),
        (true, false, "timed-out"),
        (false, false, "crashed"),
    ] {
        for stdout in [EMPTY, b"invalid JSON".as_slice()] {
            let pass =
                analysis_pass_from_capture(timed_out, exit_ok, 10, stdout.to_vec(), Vec::new());
            assert_eq!(pass.status.as_str(), expected);
        }
    }
}

#[test]
fn additive_fields_and_unknown_alignment_strings_remain_compatible() -> Result<(), String> {
    for alignment in ["future_alignment", ""] {
        let mut value: Value =
            serde_json::from_slice(NONEMPTY).map_err(|error| error.to_string())?;
        value["additional_producer_field"] = json!({"future": true});
        value["findings"][0]["oracle_alignment"] = json!(alignment);
        value["findings"][0]["static_limit_kind"] = json!("");
        value["findings"][0]["repair_placement"] = json!({"verify_command": ""});
        let pass = capture(value.to_string().as_bytes());
        assert_eq!(pass.status.as_str(), "complete");
        assert_eq!(pass.alignment.unknown, 1);
        assert_eq!(pass.alignment.verify_command_present, 0);
        assert_eq!(pass.classification.to_json()["weakly_exposed"], 1);
    }
    Ok(())
}
