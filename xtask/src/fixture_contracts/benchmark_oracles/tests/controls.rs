//! Synthetic control-collection models; no execution receipts are manufactured for the corpus.

use super::*;

struct StaticModel {
    evidence: SyntheticEvidence,
    capture: Value,
    controls: Vec<Value>,
}

impl StaticModel {
    fn new() -> Result<Self, String> {
        let mut evidence = SyntheticEvidence::new()?;
        let executable =
            json!({"path": "/synthetic/bin/ripr", "bytes": 7, "sha256": "c".repeat(64)});
        let compiler =
            json!({"executable": executable["path"], "target": {"name": "ripr", "kind": ["bin"]}});
        let producer = json!({"head": "a".repeat(40), "tree": "d".repeat(40), "artifacts": {"ripr": {"head": "a".repeat(40), "tree": "d".repeat(40), "compiler_artifact": compiler, "files": [{"path": executable["path"], "sha256": executable["sha256"], "metadata": {"size": executable["bytes"]}}]}}});
        let producer_file = evidence.file(
            "static-producer.json",
            &serde_json::to_vec(&producer).map_err(|error| error.to_string())?,
        )?;
        let identity = json!({"executable": executable, "producer_receipt": producer_file});
        let diff = evidence.file(
            "historical.patch",
            b"Synthetic retained static diff model\n",
        )?;
        let mut runs = Vec::new();
        let mut observations = Vec::new();
        for (variant, native_index) in [("corrected", 0), ("original", 4), ("weak", 2)] {
            let inputs = json!([
                {"path": "src/production.rs", "bytes": evidence.key["sources"]["fixed"]["bytes"], "sha256": evidence.key["sources"]["fixed"]["sha256"]},
                {"path": "src/lib.rs", "bytes": evidence.key["tests"][variant]["bytes"], "sha256": evidence.key["tests"][variant]["sha256"]},
                {"path": "Cargo.lock", "bytes": evidence.pairing["lock"]["bytes"], "sha256": evidence.pairing["lock"]["sha256"]}
            ]);
            let inventory = evidence.file(
                &format!("static-{variant}-inputs.json"),
                &serde_json::to_vec(&inputs).map_err(|error| error.to_string())?,
            )?;
            let mut native_capture = evidence.capture(native_index)?;
            native_capture["full_workspace_inputs_before"] = inventory.clone();
            native_capture["full_workspace_inputs_after"] = inventory.clone();
            evidence.persist_capture(native_index, &native_capture)?;
            let report = json!({"root": "/synthetic/workspace", "summary": {"probes": 1}, "analysis_outcome": {"analysis_complete": true}, "findings": [{"classification": "exposed"}]});
            let stdout = evidence.file(
                &format!("static-{variant}.json"),
                &serde_json::to_vec(&report).map_err(|error| error.to_string())?,
            )?;
            let stderr = evidence.file(&format!("static-{variant}.stderr"), b"")?;
            runs.push(json!({"variant": variant, "root": "/synthetic/workspace", "argv": [executable["path"], "check", "--root", "/synthetic/workspace", "--diff", diff["path"], "--format", "json"], "timed_out": false, "resource_stop": null, "parsed_json": true, "exit_code": 0, "identity_before": identity, "identity_after": identity, "patch_before": diff, "patch_after": diff, "stdout": stdout, "stderr": stderr, "inputs_before": inventory, "inputs_after": inventory}));
            observations.push(json!({"variant": variant, "native_exit": 0, "json": stdout, "stderr": stderr, "inputs_before": inventory, "inputs_after": inventory}));
        }
        let execution = json!({"producer_head": producer["head"], "compiler_artifact": compiler, "identity_before": identity, "identity_after": identity, "status": "THREE_COMMANDS_TERMINAL", "observations": runs});
        let capture = json!({"kind": "historical_exact_ripr_observation", "status": "THREE_COMMANDS_COMPLETED", "case_id": evidence.case["id"], "normative_static_expectation": false, "test_source_path": "src/lib.rs", "production_source_path": "src/production.rs", "transforms": "Synthetic model only; no commands executed", "producer_head": producer["head"], "producer_tree": producer["tree"], "producer_receipt": producer_file, "execution_receipt": evidence.file("static-execution.json", &serde_json::to_vec(&execution).map_err(|error| error.to_string())?)?, "executable": executable, "diff": diff, "observations": observations});
        evidence.persist()?;
        evidence.accept_review()?;
        let mut corrected = evidence.case.clone();
        corrected["fixture_reference"] = json!("synthetic contract model");
        evidence.case["semantic_oracle"]["status"] = json!("invalid");
        evidence.case["semantic_oracle"]["variant"] = json!("original");
        // Keep the two independently reviewed verdict records as distinct bytes.
        corrected["semantic_oracle"]["independent_review"] = evidence.file(
            "corrected-review.json",
            &fs::read(evidence.root.join("review.json")).map_err(|error| error.to_string())?,
        )?;
        evidence.accept_review()?;
        let mut original = evidence.case.clone();
        original["fixture_reference"] = json!("synthetic contract model");
        let mut model = Self {
            evidence,
            capture,
            controls: vec![corrected, original],
        };
        model.persist_static()?;
        Ok(model)
    }

    fn persist_static(&mut self) -> Result<(), String> {
        let descriptor = self.evidence.file(
            "static-capture.json",
            &serde_json::to_vec(&self.capture).map_err(|error| error.to_string())?,
        )?;
        for control in &mut self.controls {
            control["observed_static"] = descriptor.clone();
        }
        Ok(())
    }

    fn check(&self) -> (crate::PolicyDisclosure, Vec<String>) {
        let mut violations = Vec::new();
        let disclosure = append_controls_disclosure(
            &self.evidence.root,
            &json!({"cases": [], "semantic_oracle_controls": self.controls}),
            Summary::default().disclosure(),
            &mut violations,
        );
        (disclosure, violations)
    }
}

#[test]
fn benchmark_semantic_controls_require_explicit_complete_views() -> Result<(), String> {
    let model = StaticModel::new()?;
    assert!(model.check().1.is_empty());
    for field in [
        "id",
        "fixture_reference",
        "semantic_oracle",
        "observed_static",
    ] {
        let mut model = StaticModel::new()?;
        model.controls[0][field] = Value::Null;
        assert!(!model.check().1.is_empty(), "accepted missing {field}");
    }
    for controls in [
        Value::Null,
        json!({}),
        json!([null]),
        json!([{"id": "legacy"}]),
    ] {
        let mut violations = Vec::new();
        let _ = append_controls_disclosure(
            Path::new("."),
            &json!({"semantic_oracle_controls": controls}),
            Summary::default().disclosure(),
            &mut violations,
        );
        assert!(!violations.is_empty());
    }
    let before = Summary::default().disclosure();
    let mut violations = Vec::new();
    let after = append_controls_disclosure(
        Path::new("."),
        &json!({}),
        Summary::default().disclosure(),
        &mut violations,
    );
    assert!(violations.is_empty());
    assert_eq!(before.items, after.items);
    Ok(())
}

#[test]
fn benchmark_semantic_controls_reject_conflicting_views() -> Result<(), String> {
    for mode in [
        "duplicate",
        "wrong_variant",
        "unreviewed",
        "same_case_subject",
        "stale_review",
    ] {
        let mut model = StaticModel::new()?;
        assert!(model.check().1.is_empty());
        match mode {
            "duplicate" => model.controls.push(model.controls[0].clone()),
            "wrong_variant" => model.controls[0]["semantic_oracle"]["variant"] = json!("weak"),
            "unreviewed" => model.controls[0]["semantic_oracle"]["status"] = json!("unreviewed"),
            "same_case_subject" => {
                model.controls[1]["observed_static"]["path"] = json!("other-static.json")
            }
            _ => {
                model.controls[0]["semantic_oracle"]["independent_review"] =
                    model.controls[1]["semantic_oracle"]["independent_review"].clone()
            }
        }
        assert!(!model.check().1.is_empty(), "accepted {mode}");
    }
    Ok(())
}

#[test]
fn benchmark_semantic_controls_reject_stale_static_capture() -> Result<(), String> {
    for mode in [
        "variant",
        "input",
        "producer",
        "report_corrupt",
        "report_missing",
        "normative",
    ] {
        let mut model = StaticModel::new()?;
        assert!(model.check().1.is_empty());
        match mode {
            "variant" => model.capture["observations"][0]["variant"] = json!("wrong"),
            "input" => {
                model.capture["observations"][0]["inputs_before"] =
                    model.capture["observations"][1]["inputs_before"].clone()
            }
            "producer" => model.capture["producer_head"] = json!("f".repeat(40)),
            "report_corrupt" => fs::write(
                model.evidence.root.join("static-corrected.json"),
                b"changed report",
            )
            .map_err(|error| error.to_string())?,
            "report_missing" => fs::remove_file(model.evidence.root.join("static-corrected.json"))
                .map_err(|error| error.to_string())?,
            _ => model.capture["normative_static_expectation"] = json!(true),
        }
        model.persist_static()?;
        assert!(!model.check().1.is_empty(), "accepted {mode}");
    }
    Ok(())
}

#[test]
fn benchmark_semantic_controls_reject_wrong_selected_producer() -> Result<(), String> {
    for mode in [
        "artifact_head",
        "artifact_tree",
        "target_name",
        "target_kind",
    ] {
        let mut model = StaticModel::new()?;
        assert!(model.check().1.is_empty());
        let mut producer = retained_json(&model.evidence.root, &model.capture["producer_receipt"])?;
        let mut execution =
            retained_json(&model.evidence.root, &model.capture["execution_receipt"])?;
        match mode {
            "artifact_head" => producer["artifacts"]["ripr"]["head"] = json!("f".repeat(40)),
            "artifact_tree" => producer["artifacts"]["ripr"]["tree"] = json!("f".repeat(40)),
            "target_name" => {
                producer["artifacts"]["ripr"]["compiler_artifact"]["target"]["name"] =
                    json!("xtask")
            }
            _ => {
                producer["artifacts"]["ripr"]["compiler_artifact"]["target"]["kind"] =
                    json!(["lib"])
            }
        }
        // Refresh every enclosing descriptor and repeated identity. Only the
        // selected artifact's own source or target identity remains wrong.
        let descriptor = model.evidence.file(
            "static-producer.json",
            &serde_json::to_vec(&producer).map_err(|error| error.to_string())?,
        )?;
        model.capture["producer_receipt"] = descriptor.clone();
        execution["compiler_artifact"] = producer["artifacts"]["ripr"]["compiler_artifact"].clone();
        for field in ["identity_before", "identity_after"] {
            execution[field]["producer_receipt"] = descriptor.clone();
            for run in execution["observations"]
                .as_array_mut()
                .ok_or("missing observations")?
            {
                run[field]["producer_receipt"] = descriptor.clone();
            }
        }
        model.capture["execution_receipt"] = model.evidence.file(
            "static-execution.json",
            &serde_json::to_vec(&execution).map_err(|error| error.to_string())?,
        )?;
        model.persist_static()?;
        let violations = model.check().1;
        assert_eq!(
            violations.len(),
            2,
            "unexpected rejection count for {mode}: {violations:?}"
        );
        assert!(
            violations
                .iter()
                .all(|message| message.ends_with("historical static producer identity differs")),
            "wrong refusal for {mode}: {violations:?}"
        );
    }
    Ok(())
}

#[test]
fn benchmark_semantic_controls_real_corpus_identity_and_disclosure() -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let path = repo.join("fixtures/evidence-quality-benchmark/corpus.json");
    let corpus = read_json(&path)?;
    let cases = corpus["cases"]
        .as_array()
        .ok_or_else(|| "missing cases".to_string())?;
    let controls = corpus["semantic_oracle_controls"]
        .as_array()
        .ok_or_else(|| "locked semantic controls missing".to_string())?;
    assert_eq!(cases.len(), 82);
    assert_eq!(controls.len(), 2);
    let variants = controls
        .iter()
        .map(|control| {
            (
                control["id"].as_str(),
                control["semantic_oracle"]["variant"].as_str(),
                control["semantic_oracle"]["status"].as_str(),
            )
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        variants,
        BTreeSet::from([
            (
                Some("regex-word-boundary-empty"),
                Some("corrected"),
                Some("valid")
            ),
            (
                Some("regex-word-boundary-empty"),
                Some("original"),
                Some("invalid")
            )
        ])
    );
    let root = path
        .parent()
        .ok_or_else(|| "corpus has no parent".to_string())?;
    let key = retained_json(root, &controls[0]["semantic_oracle"]["answer_key"])?;
    let pairing = retained_json(root, &controls[0]["semantic_oracle"]["native_pairing"])?;
    assert_eq!(
        key["test_id"],
        "hir::translate::tests::analysis_is_match_empty"
    );
    assert_eq!(
        pairing["original_workspace"]["parent_commit"],
        "72f09f1aeb0ff3f703b1afdbdd21f5ff63162fb4"
    );
    assert_eq!(
        pairing["original_workspace"]["fixed_commit"],
        "88a2a62d861d189faae539990f63cb9cf195bd8c"
    );
    assert_eq!(
        key["sources"]["fixed"]["sha256"],
        "51f1642b75e298b0847855d7f490eca2e8f445c9039dd3aa91491f533ec83f15"
    );
    assert_eq!(
        key["sources"]["broken"]["sha256"],
        "325dc1e42eb8fb9daeb7a8a5e7f967fdee745a7a7c5e26c20dec0b6c66109ad7"
    );
    assert_eq!(
        key["tests"]["corrected"]["sha256"],
        "de10ee2928001567f80c6ab602de0e280a8e3a1e615cf73def91d2dcc4f9f199"
    );
    assert_eq!(
        key["tests"]["original"]["sha256"],
        "cee557e068927ef028fdd0c8b673b948aa52397ed7bd5500f1a38a51bc29ec55"
    );
    assert_eq!(
        key["tests"]["weak"]["sha256"],
        "9150ff57bf007f523ef1dc8f4c9680ea2edfd3fe55fca47f83b36b1b5834e169"
    );
    let mut violations = Vec::new();
    let disclosure = super::super::super::general_validators::validate_evidence_quality_benchmark_fixture_corpus_at(&path, &mut violations)?;
    assert!(violations.is_empty(), "{violations:?}");
    assert!(disclosure.items.iter().any(|line| line.contains("Benchmark cases: 82. Historical semantic controls: views=2, historical_cases=1, valid=1, invalid=1, rejected=0")));
    assert!(
        disclosure
            .items
            .iter()
            .any(|line| line.contains("unreviewed=82 (legacy absent=82)"))
    );
    Ok(())
}

#[test]
fn benchmark_semantic_controls_production_command_keeps_separate_counts() -> Result<(), String> {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    crate::tests::with_temp_cwd("historical-controls-production-report", |root| {
        crate::tests::copy_dir_recursive(&repo.join("fixtures"), &root.join("fixtures"))?;
        crate::fixture_contracts::check_fixture_contracts()?;
        let report_path = root.join("target/ripr/reports/fixture-contracts.md");
        let report = fs::read_to_string(&report_path).map_err(|error| error.to_string())?;
        assert!(report.contains("Benchmark cases: 82. Historical semantic controls: views=2, historical_cases=1, valid=1, invalid=1, rejected=0"));
        assert!(report.contains("Semantic review accepts only its exact answer-key/native-pairing subject; it does not accept the attached static analysis"));
        assert!(report.contains("NOT_REVERIFIED"));
        let path = root.join("fixtures/evidence-quality-benchmark/corpus.json");
        let mut corpus = read_json(&path)?;
        corpus["semantic_oracle_controls"][0]["semantic_oracle"] = Value::Null;
        fs::write(
            path,
            serde_json::to_vec(&corpus).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        assert!(crate::fixture_contracts::check_fixture_contracts().is_err());
        let report = fs::read_to_string(&report_path).map_err(|error| error.to_string())?;
        assert!(report.contains("Historical semantic controls: views=2, historical_cases=1, valid=0, invalid=1, rejected=1"));
        Ok(())
    })
}
