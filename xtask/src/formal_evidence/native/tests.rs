use super::*;
use crate::formal_evidence::{audit, parse_json};

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
        assert!(audit(&[], Some(&run)).is_err());
    }
    let mut run = valid.clone();
    run.unregistered_results[0].subject.source_digest =
        sha256(b"another source with the same native name");
    assert!(audit(&[], Some(&run)).is_err());
    let mut run = valid.clone();
    run.unregistered_results[0].properties.clear();
    assert!(audit(&[], Some(&run)).is_err());
    let mut run = valid.clone();
    run.execution
        .as_mut()
        .ok_or("execution absent")?
        .lane_success = false;
    run.unregistered_results[0].status = ObservationStatus::Failed;
    assert!(audit(&[], Some(&run)).is_err());
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
        assert!(audit(&[], Some(&run)).is_err());
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
            assert!(audit(&[], Some(&run)).is_err());
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
fn source_manifest_rejects_duplicate_and_escaping_paths() -> AuditResult<()> {
    let digest = sha256(b"source");
    for text in [
        format!("{{\"a.rs\":\"{digest}\",\"a.rs\":\"{digest}\"}}"),
        format!("{{\"../a.rs\":\"{digest}\"}}"),
        format!("{{\"C:/a.rs\":\"{digest}\"}}"),
    ] {
        assert!(parse_source_manifest(text.as_bytes()).is_err());
    }
    let manifest = SourceManifest::from([("src/a.rs".to_string(), digest)]);
    assert_eq!(
        parse_source_manifest(&canonical_manifest_bytes(&manifest)?)?,
        manifest
    );
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
