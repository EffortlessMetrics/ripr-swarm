use super::*;

fn fixture() -> Result<(Value, Binding), String> {
    let binding = Binding {
        producer_version: "test".into(),
        producer_commit: "a".repeat(40),
        producer_identity: "test+languages:rust=1,typescript=0,python=0,perl=0".into(),
        cargo_input_identity: "test-cargo".into(),
        environment_identity: crate::config::counted_environment_identity(),
        rust_sources: BTreeMap::from([("lib.rs".into(), digest(b"test bytes"))]),
        head_commit: "b".repeat(40),
        base_commit: "c".repeat(40),
        diff_base_commit: "c".repeat(40),
        diff_sha256: digest(b"first diff"),
        analysis_config_identity: "config".into(),
        count_policy_identity: "counts".into(),
        mode: "draft".into(),
        languages: vec!["rust".into()],
        include_unchanged_tests: true,
    };
    let findings = json!([
        {"id":"static-id","classification":"no_static_path","language":"rust",
         "source_currentness":"candidate_current","related_tests":[],
         "static_limit_kind":"cross_language_oracle_visibility_unresolved"},
        {"id":"actionable-id","classification":"no_static_path","language":"rust",
         "source_currentness":"candidate_current","related_tests":[]}
    ]);
    let contract = Contract {
        schema_version: 1,
        status: "complete".into(),
        basis: BASIS.into(),
        analysis_binding: binding.clone(),
        findings_sha256: value_digest(&findings),
        dispositions: dispositions(&findings).ok_or("fixture dispositions")?,
    };
    let outcome = crate::analysis_outcome::AnalysisOutcome::new(
        crate::analysis_outcome::AnalysisOutcomeKind::CompleteWithFindings,
        crate::analysis_outcome::AnalysisIdentity {
            input_identity: Some(binding.diff_sha256.clone()),
            ..Default::default()
        },
        crate::analysis_outcome::AnalysisOutcomeCounts {
            changed_file_count: 1,
            changed_line_count: 2,
            candidate_line_count: 2,
            probe_count: 2,
            finding_count: 2,
        },
        vec![],
    )?;
    Ok((
        json!({FIELD:contract,"findings":findings,"tool":"ripr","schema_version":"0.2",
        "head":{"source":"commit","commit":binding.head_commit},"base_commit":binding.base_commit,
        "mode":"draft", "analysis_outcome":{"analysis_complete":true,"outcome":outcome},
        "summary":{"findings":2,"no_static_path":2,"exposed":0,"weakly_exposed":0,
            "reachable_unrevealed":0,"infection_unknown":0,"propagation_unknown":0,"static_unknown":0}}),
        binding,
    ))
}

#[test]
fn finding_origin_static_disposition_preserves_unrelated_actionable_finding() -> Result<(), String>
{
    let (document, binding) = fixture()?;
    let contract = validate(&document, &binding).ok_or("positive binding")?;
    assert_eq!(contract.dispositions[0].finding_id, "static-id");
    assert_eq!(contract.dispositions[0].gap_state, "static_limitation");
    assert_eq!(contract.dispositions[1].finding_id, "actionable-id");
    assert_eq!(contract.dispositions[1].gap_state, "actionable");
    Ok(())
}

#[test]
fn changed_binding_or_incomplete_basis_never_receives_credit() -> Result<(), String> {
    let (document, binding) = fixture()?;
    for (pointer, replacement) in [
        ("/counted_finding_dispositions/schema_version", json!(2)),
        (
            "/counted_finding_dispositions/analysis_binding/environment_identity",
            json!("different-environment"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/cargo_input_identity",
            json!("different-cargo"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/producer_identity",
            json!("different-features"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/head_commit",
            json!("stale"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/base_commit",
            json!("stale"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/diff_base_commit",
            json!("other"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/diff_sha256",
            json!(digest(b"other diff")),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/producer_commit",
            json!("other build"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/analysis_config_identity",
            json!("other"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/count_policy_identity",
            json!("other"),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/include_unchanged_tests",
            json!(false),
        ),
        (
            "/counted_finding_dispositions/analysis_binding/languages",
            json!(["python"]),
        ),
        (
            "/counted_finding_dispositions/dispositions/1/gap_state",
            json!("static_limitation"),
        ),
        ("/counted_finding_dispositions/dispositions", json!([])),
        ("/analysis_outcome/analysis_complete", json!(false)),
        ("/summary/no_static_path", json!(1)),
        ("/findings/1/id", json!("static-id")),
        ("/findings/1/classification", json!("new_class")),
    ] {
        let mut invalid = document.clone();
        *invalid.pointer_mut(pointer).ok_or("test pointer")? = replacement;
        assert!(validate(&invalid, &binding).is_none(), "accepted {pointer}");
    }
    for (key, value) in [
        ("suppression_policy", json!({})),
        ("owner_opt_outs", json!({})),
        ("run_limitations", json!([])),
        ("document_complete", json!(false)),
    ] {
        let mut invalid = document.clone();
        invalid[key] = value;
        assert!(validate(&invalid, &binding).is_none(), "accepted {key}");
    }
    let mut missing = document.clone();
    missing.as_object_mut().ok_or("object")?.remove(FIELD);
    assert!(validate(&missing, &binding).is_none());
    Ok(())
}

#[test]
fn suppression_duplicate_and_conflicting_raw_rows_fail_closed() -> Result<(), String> {
    let (document, _) = fixture()?;
    let rows = document["findings"].clone();
    for key in ["suppressed", "suppressed_by", "withheld_by"] {
        let mut invalid = rows.clone();
        invalid[0][key] = json!(true);
        assert!(dispositions(&invalid).is_none());
    }
    let mut duplicate = rows.clone();
    duplicate[1]["id"] = duplicate[0]["id"].clone();
    assert!(dispositions(&duplicate).is_none());
    for (key, value) in [
        (
            "static_limit_kind",
            json!("rust_transitive_reach_unresolved"),
        ),
        ("classification", json!("weakly_exposed")),
        ("related_tests", json!([{}])),
    ] {
        let mut conflicting = rows.clone();
        conflicting[0][key] = value;
        let result = dispositions(&conflicting).ok_or("unsupported rows retained")?;
        assert_eq!(result[0].status, "unjoinable");
        assert_ne!(result[1].gap_state, "static_limitation");
    }
    let mut outside = rows;
    outside[0]["source_currentness"] = json!("base_deleted");
    assert_eq!(
        dispositions(&outside).ok_or("outside rows")?[0].status,
        "outside_basis"
    );
    Ok(())
}

#[test]
fn ignored_test_mutation_invalidates_an_otherwise_clean_git_subject() -> Result<(), String> {
    use crate::testing::fixture_git::{fixture_git_ok, fixture_git_stdout, remove_fixture_tree};
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let parent = std::env::temp_dir();
    std::fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let root = parent.join(format!(
        "ripr-counted-inputs-{}-{stamp}",
        std::process::id()
    ));
    // Exclusive creation establishes this test's ownership. Failure retains
    // the fixture for diagnosis; only this successfully created tree is removed.
    std::fs::create_dir(&root).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join(".gitignore"),
        "/target/\n/ignored-tests/\n/.cargo/\n",
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(root.join("lib.rs"), "pub fn value() -> bool { true }\n")
        .map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname=\"counted_fixture\"\nversion=\"0.1.0\"\n[workspace]\n",
    )
    .map_err(|e| e.to_string())?;
    fixture_git_ok(&root, &["init", "-q"])?;
    fixture_git_ok(&root, &["add", "."])?;
    fixture_git_ok(
        &root,
        &[
            "-c",
            "user.name=RIPR fixture",
            "-c",
            "user.email=ripr-fixture@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    )?;
    let head = fixture_git_stdout(&root, &["rev-parse", "HEAD"])?;
    let (_, mut binding) = fixture()?;
    binding.head_commit = head.trim().to_string();
    binding.base_commit = binding.head_commit.clone();
    binding.diff_base_commit = binding.head_commit.clone();
    binding.diff_sha256 = digest(b"");
    binding.cargo_input_identity =
        crate::analysis::cargo_input_identity(&root).ok_or("Cargo identity")?;
    binding.rust_sources = BTreeMap::from([(
        "lib.rs".into(),
        digest(b"pub fn value() -> bool { true }\n"),
    )]);
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_some());
    std::fs::create_dir(root.join(".cargo")).map_err(|e| e.to_string())?;
    std::fs::write(root.join(".cargo/config.toml"), "[build]\njobs = 2\n")
        .map_err(|e| e.to_string())?;
    assert!(bound_workspace_inputs(&root).is_none());
    assert_ne!(
        crate::analysis::cargo_input_identity(&root).as_ref(),
        Some(&binding.cargo_input_identity)
    );
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_none());
    std::fs::remove_file(root.join(".cargo/config.toml")).map_err(|e| e.to_string())?;
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_some());
    std::fs::create_dir(root.join("target")).map_err(|e| e.to_string())?;
    std::fs::write(root.join("target/generated.rs"), "ignored build state")
        .map_err(|e| e.to_string())?;
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_some());
    std::fs::create_dir(root.join("ignored-tests")).map_err(|e| e.to_string())?;
    std::fs::write(
        root.join("ignored-tests/observer.rs"),
        "#[test] fn observes() { assert!(true); }\n",
    )
    .map_err(|e| e.to_string())?;
    assert!(
        fixture_git_stdout(&root, &["status", "--porcelain"])?
            .trim()
            .is_empty()
    );
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_none());
    std::fs::remove_file(root.join("ignored-tests/observer.rs")).map_err(|e| e.to_string())?;
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_some());
    std::fs::write(
        root.join("ignored-tests/fragment.inc"),
        "pub fn auxiliary() {}",
    )
    .map_err(|e| e.to_string())?;
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_none());
    std::fs::remove_file(root.join("ignored-tests/fragment.inc")).map_err(|e| e.to_string())?;
    fixture_git_ok(&root, &["update-index", "--assume-unchanged", "lib.rs"])?;
    assert!(current_subject(&root, "HEAD", "HEAD", &binding).is_none());
    fixture_git_ok(&root, &["update-index", "--no-assume-unchanged", "lib.rs"])?;
    assert!(bound_workspace_inputs(&root).is_some());
    fixture_git_ok(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{},nested", binding.head_commit),
        ],
    )?;
    assert!(bound_workspace_inputs(&root).is_none());
    remove_fixture_tree(&root)
}

#[test]
fn consumed_source_manifest_requires_actual_complete_observations() {
    let mut source = crate::analysis::consumed_source::ConsumedRustSources::default();
    assert!(source.complete_manifest().is_none());
    source.record(Path::new("src/lib.rs"), Some(b"first"));
    assert_eq!(
        source.complete_manifest(),
        Some(BTreeMap::from([("src/lib.rs".into(), digest(b"first"))]))
    );
    source.record(Path::new("src/lib.rs"), Some(b"other"));
    assert!(source.complete_manifest().is_none());
    let mut missing = crate::analysis::consumed_source::ConsumedRustSources::default();
    missing.record(Path::new("src/lib.rs"), None);
    assert!(missing.complete_manifest().is_none());
}

#[test]
fn identity_git_probes_obey_the_active_deadline() -> Result<(), String> {
    let root = crate::output::test_support::repo_root()?;
    assert!(git(&root, &["--version"]).is_some());
    let token = crate::analysis::cancellation::AnalysisCancellationToken::with_budget(
        std::time::Instant::now(),
        std::time::Duration::ZERO,
        std::sync::Arc::new(std::time::Instant::now),
    );
    assert!(
        crate::analysis::cancellation::with_token(&token, || git(&root, &["--version"])).is_none()
    );
    Ok(())
}
