use super::*;

#[test]
fn gap_decision_ledger_static_unknown_only_stays_report_only() -> Result<(), String> {
    let mut corpus = gap_decision_ledger_corpus_value()?;
    let case = gap_decision_ledger_case_mut(&mut corpus, "static_unknown_report_only")?;
    let record = case
        .get_mut("expected_gap_record")
        .ok_or_else(|| "case must have expected_gap_record".to_string())?;
    record["repairability"] = serde_json::json!("repairable");
    record["repair_route"] = serde_json::json!({
        "route_kind": "AddBoundaryAssertion",
        "target_file": "tests/macros.rs",
        "assertion_shape": "assert boundary"
    });
    record["projection_eligibility"]["pr_comment"]["eligible"] = serde_json::json!(true);

    let report = gap_decision_ledger_violations(&corpus).join("\n");

    assert!(report.contains(
            "static-unknown-only case static_unknown_report_only must stay report-only unless a repair route exists"
        ));
    Ok(())
}

#[test]
fn generated_clean_allows_badge_endpoint_diff_in_refresh_context() {
    let changes = vec![
        changed_path("badges/ripr.json", &["M"]),
        changed_path("badges/ripr-plus.json", &["M"]),
    ];
    let violations = generated_clean_violations(&changes, true);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    assert!(is_badge_refresh_context("automation/badge-endpoints"));
    assert!(is_badge_refresh_context("badge: refresh public endpoints"));
    assert!(is_badge_refresh_context("refs/heads/badge-refresh-counts"));
}

#[test]
fn generated_clean_allows_deleting_committed_target_residue() {
    let changes = vec![changed_path("target/ripr/reports/old.md", &["D"])];
    let violations = generated_clean_violations(&changes, false);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn generated_clean_rejects_badge_endpoint_diff_outside_refresh_context() {
    let changes = vec![changed_path("badges/ripr.json", &["M"])];
    let violations = generated_clean_violations(&changes, false);
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("generated badge endpoint changed"));
    assert!(violations[0].contains("do not manually edit RIPR badge numbers"));
}

#[test]
fn generated_clean_rejects_target_residue() {
    let changes = vec![
        changed_path("target/ripr/reports/check-pr.md", &["A"]),
        changed_path(
            "crates/ripr/examples/sample/target/debug/.fingerprint",
            &["??"],
        ),
    ];
    let violations = generated_clean_violations(&changes, false);
    assert_eq!(violations.len(), 2);
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("generated RIPR target artifact"))
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("sample workspace build output"))
    );
}

#[test]
fn generated_policy_detects_lockfiles_and_release_artifacts() {
    assert!(is_generated_candidate("Cargo.lock"));
    assert!(is_generated_candidate("editors/vscode/package-lock.json"));
    assert!(is_generated_candidate("editors/vscode/dist/ripr.vsix"));
    assert!(is_generated_candidate(".ripr/release/ripr.zip"));
    assert!(!is_generated_candidate("assets/logo/ripr-icon-dark.png"));
}

#[test]
fn gh_pr_status_args_parse_pr_number() -> Result<(), String> {
    assert_eq!(
        parse_gh_pr_status_args(&["--pr".to_string(), "905".to_string()])?,
        905
    );
    assert_eq!(parse_gh_pr_status_args(&["905".to_string()])?, 905);
    assert_gh_pr_status_arg_error(&[])?;
    assert_gh_pr_status_arg_error(&["--pr".to_string()])?;
    assert_gh_pr_status_arg_error(&["--pr".to_string(), "0".to_string()])?;
    Ok(())
}

#[test]
fn gh_pr_status_behind_does_not_hide_a_conflict_or_grant_merge() {
    let contexts = vec!["rust".to_string()];
    for (state, expected) in [
        ("CLEAN", "merge"),
        ("BEHIND", "inspect integration"),
        ("DIRTY", "inspect failure"),
        ("UNKNOWN", "wait"),
    ] {
        let mut pr = gh_pr_status_pr(vec![gh_pr_status_check("rust", "COMPLETED", "SUCCESS")]);
        pr.merge_state_status = state.to_string();
        let readiness = gh_pr_status_readiness(&pr, &contexts, true, Vec::new());
        assert_eq!(readiness.behind_main, state == "BEHIND");
        assert_eq!(readiness.safe_next_action, expected, "{state}");
    }
}

#[test]
fn gh_pr_status_behind_packet_reports_integration_judgment() -> Result<(), String> {
    let pr = parse_gh_pr_status_pull_request(
        r#"{
            "number": 42,
            "title": "behind-only observation",
            "isDraft": false,
            "mergeStateStatus": "BEHIND",
            "headRefName": "candidate",
            "baseRefName": "main",
            "reviewDecision": "APPROVED",
            "latestReviews": [],
            "statusCheckRollup": [
                {"__typename": "CheckRun", "name": "rust", "status": "COMPLETED", "conclusion": "SUCCESS"}
            ]
        }"#,
    )?;
    assert_eq!(
        pr.checks.len(),
        1,
        "setup must contain the successful check"
    );
    let contexts = vec!["rust".to_string()];
    let readiness = gh_pr_status_readiness(&pr, &contexts, true, Vec::new());
    let markdown = gh_pr_status_markdown(&pr, &contexts, &readiness);
    assert!(markdown.contains("- behind main: `true`"));
    assert!(markdown.contains("- safe next action: `inspect integration`"));
    let json: Value = serde_json::from_str(&gh_pr_status_json(&pr, &contexts, &readiness))
        .map_err(|err| format!("behind PR packet should parse: {err}"))?;
    assert_eq!(json["schema_version"], "0.1");
    assert_eq!(json["mode"], "advisory");
    assert_eq!(json["merge_state"], "BEHIND");
    assert_eq!(json["behind_main"], true);
    assert_eq!(json["safe_next_action"], "inspect integration");
    Ok(())
}

#[test]
fn gh_pr_status_behind_preserves_failure_and_wait_routes() {
    let contexts = vec!["rust".to_string()];
    for (case, draft, review, status, conclusion, expected) in [
        (
            "behind only",
            false,
            "APPROVED",
            "COMPLETED",
            "SUCCESS",
            "inspect integration",
        ),
        ("draft", true, "APPROVED", "COMPLETED", "SUCCESS", "wait"),
        (
            "review required",
            false,
            "REVIEW_REQUIRED",
            "COMPLETED",
            "SUCCESS",
            "wait",
        ),
        (
            "running check",
            false,
            "APPROVED",
            "IN_PROGRESS",
            "",
            "wait",
        ),
        ("queued check", false, "APPROVED", "QUEUED", "", "wait"),
        (
            "failed check",
            false,
            "APPROVED",
            "COMPLETED",
            "FAILURE",
            "inspect failure",
        ),
        (
            "changes requested",
            false,
            "CHANGES_REQUESTED",
            "COMPLETED",
            "SUCCESS",
            "inspect failure",
        ),
    ] {
        let mut pr = gh_pr_status_pr(vec![gh_pr_status_check("rust", status, conclusion)]);
        pr.is_draft = draft;
        pr.review_decision = review.to_string();
        pr.merge_state_status = "bEhInD".to_string();
        let readiness = gh_pr_status_readiness(&pr, &contexts, true, Vec::new());
        assert!(readiness.behind_main, "{case}: setup must exercise BEHIND");
        assert_eq!(readiness.safe_next_action, expected, "{case}");
    }
}

#[test]
fn gh_pr_status_behind_waits_for_a_running_advisory_check() {
    let mut pr = gh_pr_status_pr(vec![
        gh_pr_status_check("rust", "COMPLETED", "SUCCESS"),
        gh_pr_status_check("droid/review", "IN_PROGRESS", ""),
    ]);
    pr.merge_state_status = "BEHIND".to_string();
    let readiness = gh_pr_status_readiness(&pr, &["rust".to_string()], true, Vec::new());
    assert!(readiness.behind_main);
    assert!(readiness.required_checks_outstanding.is_empty());
    assert_eq!(readiness.pending_checks.len(), 1);
    assert_eq!(readiness.safe_next_action, "wait");
}

#[test]
fn gh_pr_status_final_snapshot_drives_the_real_readiness_and_renderers() -> Result<(), String> {
    for (status, conclusion, review, expected) in [
        ("COMPLETED", "SUCCESS", "APPROVED", "merge"),
        ("IN_PROGRESS", "", "APPROVED", "wait"),
        ("COMPLETED", "FAILURE", "APPROVED", "inspect failure"),
        (
            "COMPLETED",
            "SUCCESS",
            "CHANGES_REQUESTED",
            "inspect failure",
        ),
    ] {
        let first = serde_json::json!({
            "number":42,"title":"final snapshot","headRefOid":"1111111111111111111111111111111111111111",
            "baseRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","baseRefName":"main",
            "headRefName":"candidate","isDraft":false,"mergeStateStatus":"CLEAN","reviewDecision":"APPROVED",
            "latestReviews":[{"author":{"login":"fixture-reviewer"},"state":"APPROVED","submittedAt":"2026-10-15T01:00:00Z"}],
            "reviews":[{"author":{"login":"fixture-reviewer"},"state":"APPROVED","submittedAt":"2026-10-15T01:00:00Z","commit":{"oid":"1111111111111111111111111111111111111111"}}],"comments":[],
            "statusCheckRollup":[{"__typename":"CheckRun","name":"rust","status":"COMPLETED","conclusion":"SUCCESS"}]
        });
        let mut last = first.clone();
        last["reviewDecision"] = serde_json::json!(review);
        last["statusCheckRollup"][0]["status"] = serde_json::json!(status);
        last["statusCheckRollup"][0]["conclusion"] = serde_json::json!(conclusion);
        let mut reads = 0;
        let observation = super::super::gh_pr_status_snapshot::collect_current(
            42,
            || {
                reads += 1;
                super::super::collect_gh_pr_status_json_with(42, |program, args| {
                    assert_eq!(program, "gh");
                    if args.first().is_some_and(|arg| arg == "api") {
                        return Ok(gh_pr_status_identity_response(if reads == 1 {
                            &first
                        } else {
                            &last
                        }));
                    }
                    assert_eq!(&args[..4], ["pr", "view", "42", "--json"]);
                    let fields = args[4].split(',').collect::<Vec<_>>();
                    assert!(fields.contains(&"headRefOid") && !fields.contains(&"baseRefOid"));
                    Ok(if reads == 1 {
                        first.to_string()
                    } else {
                        last.to_string()
                    })
                })
            },
            |target| {
                assert_eq!(target, "main");
                Ok(vec!["rust".to_string()])
            },
        )?;
        assert_eq!(reads, 2);
        let pr = parse_gh_pr_status_pull_request(&observation.pr.to_string())?;
        let readiness = gh_pr_status_readiness(
            &pr,
            &observation.contexts,
            observation.contexts_available,
            observation.warnings,
        );
        assert_eq!(readiness.safe_next_action, expected);
        let json: Value =
            serde_json::from_str(&gh_pr_status_json(&pr, &observation.contexts, &readiness))
                .map_err(|error| error.to_string())?;
        assert_eq!(json["safe_next_action"], expected);
        let markdown = gh_pr_status_markdown(&pr, &observation.contexts, &readiness);
        assert!(markdown.contains(&format!("safe next action: `{expected}`")));
        assert!(markdown.contains("1111111111111111111111111111111111111111"));
        assert!(markdown.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(markdown.contains("not atomic"));
    }
    Ok(())
}

#[test]
fn gh_pr_status_json_reports_merge_readiness_packet() -> Result<(), String> {
    let pr = gh_pr_status_pr(vec![
        gh_pr_status_check("rust", "COMPLETED", "SUCCESS"),
        gh_pr_status_check("droid/review", "COMPLETED", "SUCCESS"),
    ]);
    let contexts = vec!["rust".to_string()];
    let readiness = gh_pr_status_readiness(&pr, &contexts, true, Vec::new());
    let json: Value = serde_json::from_str(&gh_pr_status_json(&pr, &contexts, &readiness))
        .map_err(|err| format!("gh-pr-status JSON should parse: {err}"))?;

    assert_eq!(json["schema_version"], "0.1");
    assert_eq!(json["mode"], "advisory");
    assert_eq!(json["pr_number"], pr.number);
    assert_eq!(json["merge_state"], "CLEAN");
    assert_eq!(json["behind_main"], false);
    assert_eq!(json["review_decision"], "APPROVED");
    assert_eq!(json["safe_next_action"], "merge");
    assert_eq!(
        json["advisory_checks"][0],
        "droid/review (status=COMPLETED, conclusion=SUCCESS)"
    );
    Ok(())
}

#[test]
fn gh_pr_status_markdown_reports_reviews_checks_and_action() {
    let pr = gh_pr_status_pr(vec![
        gh_pr_status_check("rust", "COMPLETED", "SUCCESS"),
        gh_pr_status_check("droid/review", "COMPLETED", "SUCCESS"),
    ]);
    let contexts = vec!["rust".to_string()];
    let readiness = gh_pr_status_readiness(&pr, &contexts, true, Vec::new());
    let markdown = gh_pr_status_markdown(&pr, &contexts, &readiness);
    assert!(markdown.contains("# GitHub PR Status"));
    assert!(markdown.contains("safe next action: `merge`"));
    assert!(markdown.contains("## Required Checks Outstanding"));
    assert!(markdown.contains("## Reviews"));
    assert!(markdown.contains("droid/review"));
}

#[test]
fn gh_pr_status_provider_quota_observations_preserve_evidence() -> Result<(), String> {
    let sourcery = "Sorry @EffortlessSteven, you have reached your weekly rate limit of 500000 diff characters.\n\nRetry later.";
    let gemini = "> [!WARNING]\r\n> You have reached your daily quota limit. Please wait up to 24 hours and I will start processing your requests again!";
    let codex = "You have reached your Codex usage limits for code reviews.\nSee `settings` — no review ran.";
    let rabbit = "<!-- This is an auto-generated comment: summarize by coderabbit.ai -->\n<!-- This is an auto-generated comment: rate limited by coderabbit.ai -->\n\n> ## Review details\n> Commits: aaa to bbb\n> Files selected: 6";
    let fixture = serde_json::json!({
        "number": 42, "title": "provider notices", "isDraft": false,
        "headRefOid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "headRefName": "candidate", "baseRefName": "main",
        "mergeStateStatus": "CLEAN", "reviewDecision": "APPROVED",
        "latestReviews": [{"author": {"login": "sourcery-ai[bot]"}, "state": "COMMENTED", "body": sourcery,
            "submittedAt": "2026-10-14T01:00:00Z"},
            {"author": {"login": "independent-reviewer"}, "state": "APPROVED", "submittedAt": "2026-10-14T02:00:00Z", "body": "Reviewed exact candidate."}],
        "reviews": [{"author": {"login": "independent-reviewer"}, "state": "APPROVED", "submittedAt": "2026-10-14T02:00:00Z", "commit": {"oid": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}}],
        "comments": [
            {"author": {"login": "gemini-code-assist[bot]"}, "body": gemini, "url": "https://github.com/example/repo/pull/42#issuecomment-2", "createdAt": "2026-10-13T01:00:00Z"},
            {"author": {"login": "chatgpt-codex-connector[bot]"}, "body": codex, "url": "https://github.com/example/repo/pull/42#issuecomment-3", "createdAt": "2026-10-14T01:00:00Z"},
            {"author": {"login": "coderabbitai[bot]"}, "body": rabbit, "url": "https://github.com/example/repo/pull/42#issuecomment-4", "createdAt": "2026-10-14T01:00:00Z"},
            {"author": {"login": "human"}, "body": "The provider hit quota. I reviewed the code."}
        ],
        "statusCheckRollup": [{"__typename":"CheckRun", "name":"rust", "status":"COMPLETED", "conclusion":"SUCCESS"}]
    });
    let pr = super::super::collect_gh_pr_status_with(42, |program, args| {
        assert_eq!(program, "gh");
        if args.first().is_some_and(|arg| arg == "api") {
            return Ok(gh_pr_status_identity_response(&fixture));
        }
        assert_eq!(args.len(), 5);
        assert_eq!(&args[..4], &["pr", "view", "42", "--json"]);
        assert!(!args[4].split(',').any(|field| field == "baseRefOid"));
        let fields: std::collections::BTreeSet<_> = args[4].split(',').collect();
        for required in ["comments", "headRefOid", "latestReviews"] {
            assert!(fields.contains(required), "collector omitted {required}");
        }
        Ok(fixture.to_string())
    })?;
    assert_eq!(pr.reviews.len(), 2);
    let readiness = gh_pr_status_readiness(&pr, &["rust".to_string()], true, Vec::new());
    let report: Value = serde_json::from_str(&gh_pr_status_json(&pr, &[], &readiness))
        .map_err(|e| e.to_string())?;
    let observations = &report["provider_review_observations"];
    assert_eq!(
        observations["snapshot_head"],
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    );
    assert_eq!(observations["quota_notice_count"], 4);
    let responses = observations["responses"]
        .as_array()
        .ok_or("missing response observations")?;
    assert_eq!(
        responses.len(),
        4,
        "three issue comments and one review must survive collection"
    );
    for (response, original) in responses.iter().zip([sourcery, gemini, codex, rabbit]) {
        assert_eq!(response["disposition"], "quota_exhausted");
        assert_eq!(response["body"], original);
        assert_eq!(response["subject_attribution"], "not_established");
    }
    assert_eq!(responses[0]["source_kind"], "review");
    assert_eq!(
        responses[0]["url"], "",
        "latestReviews does not supply a review URL"
    );
    assert_eq!(responses[0]["timestamp"], "2026-10-14T01:00:00Z");
    assert_eq!(responses[1]["source_kind"], "issue_comment");
    assert_eq!(responses[1]["timestamp"], "2026-10-13T01:00:00Z");
    assert_eq!(
        responses[1]["url"],
        "https://github.com/example/repo/pull/42#issuecomment-2"
    );
    assert!(observations.get("completed_review_count").is_none());
    let markdown = gh_pr_status_markdown(&pr, &[], &readiness);
    assert!(markdown.contains("Quota notices: 4 of 4 observed provider responses"));
    assert!(markdown.contains("not evidence of review completion"));
    assert!(markdown.contains("not attributed to the current head"));
    assert_eq!(
        readiness.safe_next_action, "merge",
        "advisory provider disclosure must not silently become a new gate"
    );
    Ok(())
}

#[test]
fn gh_pr_status_readiness_selects_safe_next_action() {
    let contexts = vec!["rust".to_string()];
    let clean = gh_pr_status_pr(vec![gh_pr_status_check("rust", "COMPLETED", "SUCCESS")]);
    let clean_readiness = gh_pr_status_readiness(&clean, &contexts, true, Vec::new());
    assert_eq!(clean_readiness.safe_next_action, "merge");

    let pending = gh_pr_status_pr(vec![gh_pr_status_check("rust", "IN_PROGRESS", "")]);
    let pending_readiness = gh_pr_status_readiness(&pending, &contexts, true, Vec::new());
    assert_eq!(pending_readiness.safe_next_action, "wait");
    assert!(
        pending_readiness
            .required_checks_outstanding
            .iter()
            .any(|check| check.contains("rust"))
    );

    let failed = gh_pr_status_pr(vec![gh_pr_status_check("rust", "COMPLETED", "FAILURE")]);
    let failed_readiness = gh_pr_status_readiness(&failed, &contexts, true, Vec::new());
    assert_eq!(failed_readiness.safe_next_action, "inspect failure");

    let mut behind = gh_pr_status_pr(vec![gh_pr_status_check("rust", "COMPLETED", "SUCCESS")]);
    behind.merge_state_status = "BEHIND".to_string();
    let behind_readiness = gh_pr_status_readiness(&behind, &contexts, true, Vec::new());
    assert_eq!(
        gh_pr_safe_next_action(&behind, &behind_readiness),
        "inspect integration"
    );
}

/// Use the existing approved runner to inspect real gh's offline field registry.
#[test]
#[ignore = "requires real gh; set RIPR_TEST_GH to the supported gh 2.46 binary and run --ignored"]
fn gh_pr_status_real_cli_schema() -> Result<(), String> {
    let gh = std::env::var_os("RIPR_TEST_GH").unwrap_or_else(|| "gh".into());
    let Err(text) = super::super::run_output_owned(
        &gh.to_string_lossy(),
        &[
            "pr".to_string(),
            "view".to_string(),
            "--json".to_string(),
            "riprUnsupportedSchemaControl".to_string(),
        ],
    ) else {
        return Err("unknown-field control unexpectedly succeeded".to_string());
    };
    assert!(
        text.contains("Unknown JSON field: \"riprUnsupportedSchemaControl\""),
        "{text}"
    );
    let (_, registry) = text
        .split_once("Available fields:")
        .ok_or("real gh did not expose its field registry")?;
    let available: std::collections::BTreeSet<_> = registry.lines().map(str::trim).collect();
    for field in super::super::gh_pr_status_snapshot::VIEW_FIELDS.split(',') {
        assert!(
            available.contains(field),
            "real gh rejects production field {field}: {text}"
        );
    }
    Ok(())
}

#[test]
fn github_event_title_reader_accepts_pull_request_title() {
    let title = github_event_pull_request_title_from_text(
        r#"{"pull_request":{"title":"badge: refresh public endpoints"}}"#,
    );
    assert_eq!(title.as_deref(), Some("badge: refresh public endpoints"));
}

#[test]
fn glob_match_supports_recursive_segments_and_star_suffixes() {
    assert!(glob_matches(
        "editors/vscode/**/*.ts",
        "editors/vscode/src/client.ts"
    ));
    assert!(glob_matches("*.md", "README.md"));
    assert!(glob_matches(
        "fixtures/**",
        "fixtures/boundary_gap/input/src/lib.rs"
    ));
    assert!(!glob_matches(
        "editors/vscode/**/*.ts",
        "docs/examples/client.ts"
    ));
}

#[test]
fn glob_matches_distinguishes_single_star_from_double_star_segments() {
    // docs/*.md must NOT match a nested path: a single `*` cannot cross `/`.
    assert!(!glob_matches(
        "docs/*.md",
        "docs/specs/RIPR-SPEC-0004-test-efficiency.md"
    ));

    // docs/**/*.md must match a top-level docs file because ** consumes zero segments.
    assert!(glob_matches("docs/**/*.md", "docs/BADGE_POLICY.md"));

    // docs/**/*.md must match deeply nested paths.
    assert!(glob_matches("docs/**/*.md", "docs/adr/sub/0001-x.md"));

    // Empty path against a non-empty pattern → false.
    assert!(!glob_matches("docs/*.md", ""));

    // A pattern with no wildcards behaves as exact match (case-sensitive).
    assert!(glob_matches("AGENTS.md", "AGENTS.md"));
    assert!(!glob_matches("AGENTS.md", "agents.md"));

    // A `*` segment cannot cross `/`, so it matches one path component only.
    assert!(glob_matches(
        "crates/*/src/lib.rs",
        "crates/ripr/src/lib.rs"
    ));
    assert!(!glob_matches(
        "crates/*/src/lib.rs",
        "crates/ripr/src/main.rs"
    ));
}

/// Bind the #3054 claim one level above `run_fixture_outputs`: the
/// golden-comparing entry point must also consume the cache the runner cleared.
///
/// `goldens_check`, `goldens_bless`, and `golden_drift` all reach a fixture only
/// through `collect_golden_runs -> run_fixture -> run_fixture_outputs`, and the
/// rebuild lives in `ripr_fixture_binary`, the single function every spawn
/// reaches the binary through. Asserting here covers the aggregate wrappers
/// without running all 222 fixtures: a stale entry seeded into the pinned cache
/// must be gone after a golden-comparing run, and that run must still agree with
/// its committed goldens.
#[test]
fn golden_comparison_runs_consume_the_cache_the_runner_cleared() -> Result<(), String> {
    with_repo_cwd(|| {
        let owned = fixture_cache::Case::new("golden")?;
        let fixture = owned.fixture.clone();
        let leaked = fixture.join("input").join("target");
        ignore_remove_dir_all(&leaked);

        let cache_dir = &owned.cache;
        let stale = cache_dir
            .join("repo-file-facts")
            .join("0.2")
            .join("stale-3054-golden.json");
        let stale_parent = stale
            .parent()
            .ok_or_else(|| "seeded cache entry should have a parent".to_string())?;
        fs::create_dir_all(stale_parent)
            .map_err(|err| format!("create {}: {err}", stale_parent.display()))?;
        fs::write(&stale, b"{\"stale\":true}")
            .map_err(|err| format!("write {}: {err}", stale.display()))?;

        let run = super::super::run_fixture(&fixture)?;

        assert!(
            !stale.exists(),
            "a golden-comparing run must discard cached facts computed by a previous binary: {}",
            stale.display()
        );
        assert!(
            !leaked.exists(),
            "a golden-comparing run must not write its cache into the tracked corpus: {}",
            leaked.display()
        );
        assert!(
            run.comparisons_all_match(),
            "the fixture must still agree with its committed goldens after the cache clear"
        );
        Ok(())
    })
}

#[test]
fn golden_drift_semantics_summarize_json_changes() {
    let expected = r#"{
  "findings": [
    {"id":"probe:src_lib.rs:1:predicate","classification":"infection_unknown","probe":{"family":"predicate"},"related_tests":[{"oracle_kind":"exact_value","oracle_strength":"strong"}],"recommended_next_step":"Add a boundary test"}
  ]
}"#;
    let actual = r#"{
  "findings": [
    {"id":"probe:src_lib.rs:1:predicate","classification":"weakly_exposed","probe":{"family":"predicate"},"related_tests":[{"oracle_kind":"smoke_only","oracle_strength":"smoke"}],"stop_reasons":["opaque fixture"],"recommended_next_step":"Assert the exact variant"},
    {"id":"probe:src_lib.rs:2:error_path","classification":"weakly_exposed","probe":{"family":"error_path"},"related_tests":[{"oracle_kind":"broad_error","oracle_strength":"smoke"}],"recommended_next_step":"Assert the exact variant"}
  ]
}"#;

    let semantics = golden_drift_semantics("check.json", expected, actual);

    assert!(
        semantics
            .added_finding_ids
            .contains(&"probe:src_lib.rs:2:error_path".to_string())
    );
    assert!(
        semantics.changed_exposure_classes.iter().any(|value| {
            value.contains("infection_unknown") && value.contains("weakly_exposed")
        })
    );
    assert!(
        semantics
            .changed_probe_families
            .iter()
            .any(|value| value.contains("error_path"))
    );
    assert!(
        semantics
            .changed_oracle_strengths
            .iter()
            .any(|value| value.contains("smoke"))
    );
    assert!(
        semantics
            .changed_oracle_kinds
            .iter()
            .any(|value| value.contains("broad_error"))
    );
    assert!(
        semantics
            .changed_stop_reasons
            .iter()
            .any(|value| value.contains("opaque fixture"))
    );
    assert!(
        semantics
            .changed_recommendations
            .iter()
            .any(|value| value.contains("Assert the exact variant"))
    );
}

#[test]
fn golden_drift_type_falls_back_to_formatting_only() {
    // A whitespace/format-only drift must not masquerade as a semantic change.
    let semantics = super::super::GoldenDriftSemantics {
        changed_line_count: 3,
        ..super::super::GoldenDriftSemantics::default()
    };
    let summary = super::super::golden_drift_type(&semantics);
    assert!(summary.contains("formatting_only"), "{summary}");
    assert!(summary.contains('3'), "{summary}");
}

#[test]
fn golden_drift_type_names_classification_flip() {
    // Gate-feedback contract: a goldens-check failure must say WHAT KIND of
    // drift happened. A classification flip is the highest-stakes case and
    // must be named explicitly with its before/after.
    let semantics = super::super::GoldenDriftSemantics {
        changed_exposure_classes: vec!["expected [exposed] -> actual [weakly_exposed]".to_string()],
        ..super::super::GoldenDriftSemantics::default()
    };
    let summary = super::super::golden_drift_type(&semantics);
    assert!(summary.contains("classification_changed"), "{summary}");
    assert!(summary.contains("exposed"), "{summary}");
    assert!(summary.contains("weakly_exposed"), "{summary}");
}

#[test]
fn golden_text_comparison_normalizes_line_endings_and_final_newline() {
    assert_eq!(
        normalize_golden_text("one\r\ntwo\r\n"),
        normalize_golden_text("one\ntwo\n")
    );
    assert_eq!(
        normalize_golden_text("one\ntwo\n"),
        normalize_golden_text("one\ntwo")
    );
    assert_ne!(
        normalize_golden_text("one\ntwo\n\n"),
        normalize_golden_text("one\ntwo\n")
    );
    assert_ne!(
        normalize_golden_text("one\ntwo\n\n"),
        normalize_golden_text("one\ntwo")
    );
}

#[test]
fn goldens_check_failure_message_is_a_repair_card() {
    // The failure message must point at the semantic report, carry the
    // reproduce command, and tell the reader how to re-bless an intended flip.
    let entries = vec![super::super::GoldenDriftEntry {
        fixture: "python_field_assignment_shape".to_string(),
        surface: "check.json".to_string(),
        expected: "fixtures/x/expected/check.json".to_string(),
        actual: "target/x/actual/check.json".to_string(),
        blessing_reason_required: true,
        blessing_reason_present: false,
        semantics: super::super::GoldenDriftSemantics {
            changed_exposure_classes: vec![
                "expected [exposed] -> actual [weakly_exposed]".to_string(),
            ],
            ..super::super::GoldenDriftSemantics::default()
        },
    }];
    let violations = vec!["drift for fixture `python_field_assignment_shape`".to_string()];
    let provenance = super::super::GoldenDriftProvenance {
        binary: "/work/target/debug/ripr".to_string(),
        ripr_version: "ripr 0.11.0 (abc123)".to_string(),
        revision: "abc123".to_string(),
    };
    let message = super::super::goldens_check_failure_message(&entries, &violations, &provenance);
    assert!(message.contains("golden-drift.md"), "{message}");
    assert!(
        message.contains("python_field_assignment_shape"),
        "{message}"
    );
    assert!(message.contains("classification_changed"), "{message}");
    assert!(message.contains("re-bless"), "{message}");
    assert!(
        message.contains("reproduce: cargo xtask goldens check"),
        "{message}"
    );
    // #7401: the card names the build that produced the actual side.
    assert!(message.contains("ripr 0.11.0 (abc123)"), "{message}");
    assert!(message.contains("/work/target/debug/ripr"), "{message}");
}

#[test]
fn goldens_check_includes_assistant_loop_health_contract_drift() -> Result<(), String> {
    let root = temp_dir("goldens-assistant-loop-health-invalid");
    let base = root.join("assistant-loop-health");
    let report = root.join("assistant-loop-health.json");
    let markdown = root.join("assistant-loop-health.md");
    let proof = root.join("proof.json");
    write_assistant_loop_health_corpus(&base, &report, &markdown, &proof, "advisory", 0);
    write(
        &report,
        r#"{
  "kind": "assistant_loop_health",
  "status": "blocked",
  "summary": {
    "proofs": 1,
    "complete": 1,
    "partial": 0,
    "missing_required_input": 0,
    "missing_optional_input": 0,
    "improved": 1,
    "unchanged": 0,
    "regressed": 0,
    "unknown_movement": 0,
    "warnings": 0,
    "repair_queue": 0
  },
  "repair_queue": [],
  "limits": ["Static RIPR evidence only."]
}
"#,
    );
    write(
        &markdown,
        "# RIPR Assistant Loop Health\n\nStatus: advisory\n",
    );

    let violations = super::super::golden_assistant_loop_health_contract_violations_at(&base)?;
    let report = violations.join("\n");

    assert!(report.contains("expected status advisory"), "{report}");
    Ok(())
}

#[test]
fn has_active_line_finds_active_content() {
    let lines = active_yaml_lines("review_model: \"custom:MiniMax-M3-0\"");
    assert!(has_active_line(
        &lines,
        "review_model: \"custom:MiniMax-M3-0\""
    ));
}

#[test]
fn has_active_line_ignores_commented_content() {
    let lines = active_yaml_lines("# review_model: \"custom:MiniMax-M3-0\"");
    assert!(!has_active_line(
        &lines,
        "review_model: \"custom:MiniMax-M3-0\""
    ));
}

#[test]
fn heading_slug_drops_punctuation_without_collapsing_the_spaces_around_it() -> Result<(), String> {
    let mut failures = Vec::new();
    // GitHub's own anchors for headings this repository links into. The doubled
    // dashes are not a typo: the dropped em-dash and slash leave the spaces on
    // both sides behind.
    let cases = [
        (
            "Coverage / Grip Frontier Report",
            "coverage--grip-frontier-report",
        ),
        (
            "2026-07-25: A green check is not evidence \u{2014} five ways",
            "2026-07-25-a-green-check-is-not-evidence--five-ways",
        ),
        ("`ripr+ 0`", "ripr-0"),
        (
            "Historical Operating Sequence: 0.9.0 Release",
            "historical-operating-sequence-090-release",
        ),
        ("Non-Goals", "non-goals"),
        ("agent_context_v2", "agent_context_v2"),
    ];
    for (heading, expected) in cases {
        let slug = heading_slug(heading);
        if slug != expected {
            failures.push(format!(
                "`{heading}` slugified to `{slug}`, expected `{expected}`"
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn heading_slugs_number_repeated_headings_the_way_github_does() -> Result<(), String> {
    let text = "# Added\n\n## Added\n\n### Added\n";
    let slugs = heading_slugs(text);
    let expected = ["added", "added-1", "added-2"];
    let missing: Vec<&str> = expected
        .into_iter()
        .filter(|slug| !slugs.contains(*slug))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("heading_slugs did not offer {missing:?}"))
    }
}

#[test]
fn heading_slugs_read_only_real_headings() -> Result<(), String> {
    let text = "# Title\n\n```md\n# Fenced Heading\n```\n\n#hashtag not a heading\n\n## Closing Hashes ##\n\n####### Seven Hashes\n";
    let slugs = heading_slugs(text);
    let mut failures = Vec::new();
    for offered in ["title", "closing-hashes"] {
        if !slugs.contains(offered) {
            failures.push(format!("expected the anchor `{offered}`"));
        }
    }
    for withheld in ["fenced-heading", "hashtag-not-a-heading", "seven-hashes"] {
        if slugs.contains(withheld) {
            failures.push(format!("`{withheld}` is not a heading in this document"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[test]
fn help_message_for_unknown_command_reuses_suggestion_text() {
    let err = help_message(&["chek-pr".to_string()]).expect_err("unknown help target");
    assert!(err.contains("Did you mean `check-pr`?"));
}

#[test]
fn help_message_highlights_common_workflows_and_command_lookup() -> Result<(), String> {
    let overview = help_message(&[])?;
    assert!(overview.contains("Common starting points:"));
    assert!(overview.contains("cargo xtask help <command>"));

    let check_pr = help_message(&["check-pr".to_string()])?;
    assert!(check_pr.contains("Usage: cargo xtask check-pr"));
    assert!(check_pr.contains("Mutability: non_mutating_check"));
    assert!(check_pr.contains("Writes: target/ripr/reports"));

    let goldens = help_message(&["goldens".to_string()])?;
    assert!(goldens.contains("Usage: cargo xtask goldens check"));
    assert!(goldens.contains("Usage: cargo xtask goldens bless <name> --reason <reason>"));

    Ok(())
}

#[test]
fn implementation_slices_validate_and_coexist() -> Result<(), String> {
    fn validate_slice(text: &str) -> Result<(), String> {
        for required in [
            "schema_version = \"2.0\"",
            "id = \"",
            "generation = ",
            "source_issue = \"",
            "design_reference = \"",
            "change_class = \"",
            "claim_boundary",
            "non_goals",
            "return_conditions",
            "owned_seams",
            "shared_seams",
            "forbidden_seams",
            "[[requirement_delta]]",
            "requirement_id = \"",
            "[implementation_claim]",
            "[evidence]",
            "[support_claim]",
        ] {
            if !text.contains(required) {
                return Err(format!("slice is missing {required}"));
            }
        }
        for line in text.lines() {
            let key = line.split('=').next().unwrap_or_default().trim();
            for forbidden in [
                "branch",
                "worktree",
                "pull_request",
                "pr_number",
                "ci_status",
                "reviewer",
                "worker",
                "assignee",
                "priority",
                "timing",
                "schedule",
                "progress",
                "percent_complete",
                "session",
                "model",
            ] {
                if key == forbidden {
                    return Err(format!("slice carries live state field `{forbidden}`"));
                }
            }
        }
        Ok(())
    }

    let slice_dir = slice_test_repo_root().join(".allow/spec-system/slices");
    let mut validated = 0;
    let entries =
        std::fs::read_dir(&slice_dir).map_err(|err| format!("read slices directory: {err}"))?;
    for entry in entries {
        let path = entry.map_err(|err| format!("slice entry: {err}"))?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|err| format!("read slice {}: {err}", path.display()))?;
        validate_slice(&text).map_err(|err| format!("{}: {err}", path.display()))?;
        validated += 1;
    }
    if validated == 0 {
        return Err("expected at least one checked-in slice".to_string());
    }

    // Coexistence: a second, unrelated slice validates alongside the
    // checked-in one; no slice acts as a current or active pointer.
    let second = r#"schema_version = "2.0"
id = "ripr.slice.unrelated-change.v1"
generation = 1
source_issue = "issue:9999"
design_reference = "RIPR-SPEC-9999#example"
change_class = "behavior_change"
claim_boundary = "One bounded change only."
non_goals = ["unrelated refactoring"]
return_conditions = ["Return to root if the requirement is stale."]
owned_seams = ["seam:ripr:xtask:example"]
shared_seams = []
forbidden_seams = []
[[requirement_delta]]
requirement_id = "RIPR-SPEC-9999#example"
requirement_generation = 1
[implementation_claim]
status = "implemented"
[evidence]
state = "outstanding"
[support_claim]
state = "unchanged"
"#;
    validate_slice(second)?;
    let with_live_state = second.replace(
        "change_class = \"behavior_change\"",
        "change_class = \"behavior_change\"\nbranch = \"codex/unrelated\"",
    );
    if validate_slice(&with_live_state).is_ok() {
        return Err("slice carrying branch state must be rejected".to_string());
    }
    Ok(())
}

#[test]
fn install_hooks_creates_missing_hook() -> Result<(), String> {
    let root = temp_dir("install-hooks-create");
    fs::create_dir(root.join(".git")).map_err(|err| err.to_string())?;

    let hook = install_hooks_in(&root)?;
    let text = fs::read_to_string(&hook).map_err(|err| err.to_string())?;

    assert_eq!(hook, root.join(".git").join("hooks").join("pre-commit"));
    assert!(is_ripr_managed_hook(&text));
    assert!(text.contains("cargo xtask precommit"));
    ignore_remove_dir_all(root);
    Ok(())
}

#[test]
fn install_hooks_errors_outside_git_worktree() -> Result<(), String> {
    let root = temp_dir("install-hooks-outside-git");

    let error = install_hooks_in(&root)
        .err()
        .ok_or_else(|| "expected missing git worktree error".to_string())?;

    assert!(error.contains("missing .git directory"));
    ignore_remove_dir_all(root);
    Ok(())
}

#[test]
fn install_hooks_is_idempotent_for_managed_hook() -> Result<(), String> {
    let root = temp_dir("install-hooks-idempotent");
    let hook = root.join(".git").join("hooks").join("pre-commit");
    let stale_managed_hook = ripr_pre_commit_hook().replace("cargo xtask precommit", "echo old");
    write(&hook, &stale_managed_hook);

    let first = install_hooks_in(&root)?;
    let second = install_hooks_in(&root)?;
    let text = fs::read_to_string(&hook).map_err(|err| err.to_string())?;

    assert_eq!(first, hook);
    assert_eq!(second, hook);
    assert_eq!(text, ripr_pre_commit_hook());
    ignore_remove_dir_all(root);
    Ok(())
}

#[test]
fn install_hooks_refuses_unmanaged_existing_hook() -> Result<(), String> {
    let root = temp_dir("install-hooks-unmanaged");
    let hook = root.join(".git").join("hooks").join("pre-commit");
    let user_hook = "#!/usr/bin/env sh\necho user hook\n";
    write(&hook, user_hook);

    let error = install_hooks_in(&root)
        .err()
        .ok_or_else(|| "expected unmanaged hook refusal".to_string())?;
    let text = fs::read_to_string(&hook).map_err(|err| err.to_string())?;

    assert!(error.contains("refusing to overwrite unmanaged hook"));
    assert_eq!(text, user_hook);
    ignore_remove_dir_all(root);
    Ok(())
}

#[test]
fn is_bdd_test_name_matches_given_when_then_pattern() {
    assert!(is_bdd_test_name(
        "given_a_user_when_logged_in_then_access_granted"
    ));
    assert!(is_bdd_test_name(
        "Given_Some_Context_When_Something_Happens_Then_Result"
    ));
    assert!(is_bdd_test_name("given_x_when_y_then_z"));

    assert!(!is_bdd_test_name("when_no_given")); // missing given
    assert!(!is_bdd_test_name("given_no_when_then")); // missing when
    assert!(!is_bdd_test_name("given_when_no_then")); // missing then
    assert!(!is_bdd_test_name("given__when__then")); // missing content between parts
    assert!(!is_bdd_test_name("regular_test_name")); // not BDD format
    assert!(!is_bdd_test_name(""));
}

#[test]
fn is_snake_case_id_validates_naming_rules() {
    assert!(is_snake_case_id("valid_id"));
    assert!(is_snake_case_id("also_valid_123"));
    assert!(is_snake_case_id("a"));
    assert!(is_snake_case_id("test123test"));

    assert!(!is_snake_case_id("")); // empty
    assert!(!is_snake_case_id("_starts_with")); // starts with underscore
    assert!(!is_snake_case_id("ends_with_")); // ends with underscore
    assert!(!is_snake_case_id("double__underscore")); // double underscore
    assert!(!is_snake_case_id("CamelCase")); // uppercase
    assert!(!is_snake_case_id("with-dash")); // non-alphanumeric
}

// ============================================================================
// Identifier validation tests
// ============================================================================

#[test]
fn is_spec_id_matches_expected_format() {
    assert!(is_spec_id("RIPR-SPEC-0001"));
    assert!(is_spec_id("RIPR-SPEC-9999"));
    assert!(is_spec_id("RIPR-SPEC-0000"));

    assert!(!is_spec_id("RIPR-SPEC-001")); // too short
    assert!(!is_spec_id("RIPR-SPEC-00001")); // too long
    assert!(!is_spec_id("RIPR-SPEC-abc1")); // non-digits
    assert!(!is_spec_id("ripr-spec-0001")); // lowercase
    assert!(!is_spec_id("RIPR-0001")); // missing SPEC
    assert!(!is_spec_id(""));
}

#[test]
fn json_string_values_for_key_reads_multiline_arrays() {
    let text = r#"{
  "stop_reasons": [
    "opaque fixture",
    "missing owner"
  ]
}"#;

    let values = json_string_values_for_key(text, "stop_reasons");

    assert_eq!(
        values,
        BTreeSet::from(["missing owner".to_string(), "opaque fixture".to_string()])
    );
}

#[test]
fn known_commands_has_no_duplicate_entries() {
    let commands = known_commands();
    let unique = commands.iter().collect::<BTreeSet<_>>();
    assert_eq!(commands.len(), unique.len());
}

#[test]
fn known_commands_include_current_report_and_policy_commands() {
    let commands = known_commands();
    assert!(commands.contains(&"commands"));
    assert!(commands.contains(&"install-hooks"));
    assert!(commands.contains(&"repo-seam-inventory"));
    assert!(commands.contains(&"repo-exposure-report"));
    assert!(commands.contains(&"repo-exposure-latency-report"));
    assert!(commands.contains(&"lsp-performance-report"));
    assert!(commands.contains(&"lane1-evidence-audit"));
    assert!(commands.contains(&"evidence-quality-audit"));
    assert!(commands.contains(&"evidence-quality-scorecard"));
    assert!(commands.contains(&"evidence-quality-trend [--current <path>] [--previous <path>]"));
    assert!(commands.contains(&"agent-seam-packets [root]"));
    assert!(commands.contains(&"lsp-cockpit-report"));
    assert!(commands.contains(&"operator-cockpit"));
    assert!(commands.contains(&"operator-cockpit-report"));
    assert!(commands.contains(&"pr-ready"));
    assert!(commands.contains(&"cockpit"));
    assert!(commands.contains(&"release-readiness --version <version>"));
    assert!(commands.contains(&"targeted-test-outcome --before <path> --after <path>"));
    assert!(commands.contains(&"mutation-calibration [root] --mutants-json <path>"));
    assert!(
        commands.contains(&"bun-ub-calibration [--corpus <path>] [--out <path>] [--out-md <path>]")
    );
    assert!(commands.contains(&"bun-ub-preview-summary [--calibration-corpus <path>] [--graph-corpus <path>] [--dogfood-corpus <path>] [--out <path>] [--out-md <path>]"));
    assert!(commands.contains(
        &"configured-bridge-inventory [--graph-corpus <path>] [--out <path>] [--out-md <path>]"
    ));
    assert!(commands.contains(&"sarif-policy --current <path> [--baseline <path>]"));
    assert!(commands.contains(&"badges [--check] [--gap-ledger <path>]"));
    assert!(commands.contains(&"pr-triage-report"));
    assert!(commands.contains(&"gh-pr-status --pr <number>"));
    assert!(commands.contains(
        &"merge-queue capture [--repo <owner/name>] [--out <dir>] [--input <path>] [--prior <path>]"
    ));
    assert!(commands.contains(&"check-badge-diff-policy"));
    assert!(commands.contains(&"check-command-catalog"));
    assert!(commands.contains(&"worktree doctor"));
    assert!(commands.contains(&"check-droid-review-config"));
    assert!(commands.contains(&"check-ci-lane-whitelist"));
    assert!(commands.contains(&"check-proof-packs"));
    assert!(commands.contains(&"vscode-compile"));
    assert!(commands.contains(&"vscode-package"));
    assert!(commands.contains(&"vscode-verify-vsix -- <vsix>"));
    assert!(commands.contains(&"vscode-test"));
    assert!(commands.contains(&"vscode-test-e2e"));
}

#[test]
fn known_xtask_command_accepts_every_help_catalog_root() {
    let commands = known_commands();
    assert!(
        commands
            .iter()
            .map(|command| command.split_once(' ').map_or(*command, |(root, _)| root))
            .all(known_xtask_command)
    );
}

#[test]
fn ledger_name_to_lookup_strips_clippy_prefix() {
    assert_eq!(
        super::super::ledger_name_to_lookup("clippy::panic"),
        ("panic", "clippy")
    );
    assert_eq!(
        super::super::ledger_name_to_lookup("clippy::cast_sign_loss"),
        ("cast_sign_loss", "clippy")
    );
    assert_eq!(
        super::super::ledger_name_to_lookup("unsafe_code"),
        ("unsafe_code", "rust")
    );
}

#[test]
fn line_has_static_language_inline_allow_requires_a_reason() {
    // Marker with a reason suppresses (input is lowercased by the caller).
    assert!(line_has_static_language_inline_allow(
        "# the proven approach  # ripr-allow: static-language: established ci wording"
    ));
    // Bare marker with no reason does NOT suppress — an allow must say why.
    assert!(!line_has_static_language_inline_allow(
        "# the proven approach  # ripr-allow: static-language:"
    ));
    assert!(!line_has_static_language_inline_allow(
        "# the proven approach  # ripr-allow: static-language:   "
    ));
    // No marker at all.
    assert!(!line_has_static_language_inline_allow(
        "# the proven approach"
    ));
}

#[test]
fn live_bun_stable_byte_dogfood_receipts_are_checked() -> Result<(), String> {
    with_repo_cwd(|| {
        let scenarios = dogfood_bun_ub_cross_language_scenarios();
        let runs = scenarios
            .iter()
            .map(super::super::dogfood_bun_ub_cross_language_run)
            .collect::<Vec<_>>();

        for run in &runs {
            assert!(
                run.errors.is_empty(),
                "{} live Bun stable-byte receipt should validate: {:?}",
                run.name,
                run.errors
            );
            assert_eq!(
                run.manual_verdict, "agrees",
                "{} should record a manual verdict",
                run.name
            );
            assert_eq!(
                run.authority_boundary, "preview_advisory_only",
                "{} should stay preview/advisory",
                run.name
            );
            assert!(
                !run.repair_packet_ready,
                "{} should not become repair-packet-ready",
                run.name
            );
            assert_ne!(
                run.review_before, run.review_after,
                "{} should record what changed in the review loop",
                run.name
            );
            assert!(
                !run.suggested_test_file.ends_with(".rs"),
                "{} should not suggest Rust test placement",
                run.name
            );
        }

        assert!(
            runs.iter().any(|run| {
                run.name == "bun_array_buffer_copy_to_unshared_live_receipt"
                    && run.observed_state == "rust_ungripped_ts_discriminated"
                    && run.bridge_verdict.contains("credited")
                    && run.suggested_test_file == "not_applicable"
            }),
            "live dogfood should credit the configured copy_to_unshared witness"
        );
        assert!(
            runs.iter().any(|run| {
                run.name == "bun_markdown_resizable_array_buffer_live_receipt"
                    && run.observed_state == "rust_ungripped_ts_discriminated"
                    && run.ts_test_file == super::super::BUN_MARKDOWN_TS_TEST_FILE
                    && run.bridge_verdict.contains("credited")
            }),
            "live dogfood should credit the configured MarkdownObject witness"
        );
        assert!(
            runs.iter().any(|run| {
                run.observed_state == "rust_ungripped_ts_missing_discriminator"
                    && run
                        .missing_discriminators
                        .contains(&"resizable_array_buffer".to_string())
                    && run.suggested_test_file == super::super::BUN_BLOB_ARRAY_BUFFER_TS_TEST_FILE
            }),
            "live dogfood should retain the missing resizable placement receipt"
        );
        assert!(
            runs.iter().any(|run| {
                run.observed_state == "ts_mention_not_observer"
                    && run.operator_action == "reject_token_mention"
                    && run.suggested_test_file == "not_applicable"
            }),
            "live dogfood should reject mention-only token evidence"
        );
        assert!(
            runs.iter().any(|run| {
                run.name == "bun_blob_bridge_unknown_live_receipt"
                    && run.observed_state == "bridge_unknown"
                    && run
                        .missing_graph_legs
                        .contains(&"binding_or_ffi_edge".to_string())
                    && run.operator_action == "inspect_or_add_bridge_evidence"
                    && run.suggested_test_file == "not_applicable"
            }),
            "live dogfood should stop at bridge inspection when the bridge is unknown"
        );
        assert!(
            runs.iter().any(|run| {
                run.name == "bun_node_fs_scalar_write_manifest_only_receipt"
                    && run.observed_state == "named_static_limitation"
                    && run.proof_mode == "observable_red_green"
                    && run.suggested_test_file == super::super::BUN_NODE_FS_TS_TEST_FILE
                    && run.placement_verdict.contains("not_actionable")
            }),
            "live dogfood should keep node:fs scalar write manifest-only and non-actionable"
        );
        assert!(
            runs.iter().any(|run| {
                run.name == "bun_write_helper_gated_manifest_only_receipt"
                    && run.observed_state == "named_static_limitation"
                    && run.proof_mode == "helper_gated"
                    && run
                        .missing_graph_legs
                        .contains(&"helper:bun_write_fixture_helper".to_string())
                    && run.suggested_test_file == "not_applicable"
            }),
            "live dogfood should keep Bun.write helper-gated"
        );
        assert!(
            runs.iter().any(|run| {
                run.observed_state == "public_reachable_panic_boundary_unrevealed"
                    && run.operator_action == "keep_panic_boundary_limitation"
                    && run.suggested_test_file == "not_applicable"
            }),
            "live dogfood should retain the FFI panic-boundary named limitation"
        );

        Ok(())
    })
}

#[test]
fn load_repository_license_notices_accepts_repo_root_files() -> Result<(), String> {
    with_repo_cwd(|| {
        let notices = super::super::load_repository_license_notices(&super::super::repo_root()?)?;
        if notices.mit.len() < super::super::COMPLETE_LICENSE_NOTICE_MIN_BYTES {
            return Err("repository LICENSE-MIT is shorter than a complete notice".to_string());
        }
        if notices.apache.len() < super::super::COMPLETE_LICENSE_NOTICE_MIN_BYTES {
            return Err("repository LICENSE-APACHE is shorter than a complete notice".to_string());
        }
        if notices.mit == notices.apache {
            return Err("repository MIT and Apache notices must stay distinct".to_string());
        }
        Ok(())
    })
}

#[test]
fn load_repository_license_notices_rejects_pointer_and_empty_files() -> Result<(), String> {
    with_temp_cwd("load-license-notices", |root| {
        write_license_fixture(root, "LICENSE-MIT", POINTER_LICENSE)?;
        write_license_fixture(root, "LICENSE-APACHE", COMPLETE_APACHE_NOTICE)?;
        let Err(pointer) = super::super::load_repository_license_notices(root) else {
            return Err(
                "the short VS Code LICENSE pointer must not load as LICENSE-MIT".to_string(),
            );
        };
        assert!(pointer.contains("LICENSE-MIT"), "{pointer}");
        assert!(
            pointer.contains("complete repository notices must be at least"),
            "{pointer}"
        );

        write_license_fixture(root, "LICENSE-MIT", COMPLETE_MIT_NOTICE)?;
        write_license_fixture(root, "LICENSE-APACHE", "")?;
        let Err(empty) = super::super::load_repository_license_notices(root) else {
            return Err("an empty LICENSE-APACHE must not load as a complete notice".to_string());
        };
        assert!(empty.contains("LICENSE-APACHE"), "{empty}");

        write_license_fixture(root, "LICENSE-APACHE", COMPLETE_APACHE_NOTICE)?;
        let loaded = super::super::load_repository_license_notices(root)?;
        assert_eq!(loaded, complete_license_notices());
        Ok(())
    })
}

#[test]
fn local_context_allow_entries_track_path_pattern_and_max_count() {
    let entry = LocalContextAllow {
        path: "crates/ripr/src/analysis/classifier.rs".to_string(),
        pattern: "unwrap()".to_string(),
        max_count: 3,
        line: 42,
    };
    assert_eq!(entry.path, "crates/ripr/src/analysis/classifier.rs");
    assert_eq!(entry.pattern, "unwrap()");
    assert_eq!(entry.max_count, 3);
}

#[test]
fn local_context_allowlist_rejects_real_machine_paths() {
    let allowlist = vec![LocalContextAllow {
        path: "docs/example.md".to_string(),
        pattern: concat!("H:", "\\Code\\Rust\\ripr").to_string(),
        max_count: 1,
        line: 7,
    }];

    let violations = validate_local_context_allowlist(&allowlist);

    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("real machine or session state"));
}

#[test]
fn local_context_detection_flags_codex_memory_and_chat_artifacts() {
    let memory_path = format!(
        "{}{}",
        concat!("C:", "\\Users\\steven\\"),
        concat!(".", "codex\\memories\\", "MEMORY", ".md")
    );
    let line = format!(
        "See {memory_path}, {}, and {}.",
        concat!("turn", "110", "file", "4"),
        concat!("file_", "00000000abcdef")
    );

    let findings = local_context_line_findings(&line);

    assert!(findings.iter().any(|(pattern, _)| pattern == &memory_path));
    assert!(
        findings
            .iter()
            .any(|(pattern, _)| pattern == concat!(".", "codex"))
    );
    assert!(
        findings
            .iter()
            .any(|(pattern, _)| pattern == concat!("MEMORY", ".md"))
    );
    assert!(
        findings
            .iter()
            .any(|(pattern, _)| pattern == concat!("turn", "110", "file", "4"))
    );
    assert!(
        findings
            .iter()
            .any(|(pattern, _)| pattern == concat!("file_", "00000000abcdef"))
    );
}

#[test]
fn local_context_detection_flags_machine_and_session_artifacts() {
    let machine_path = concat!("H:", "\\Code\\Rust\\ripr");
    let line = format!(
        "{}2026-05-01 from `{machine_path}`.",
        concat!("Recorded", " on ")
    );

    let findings = local_context_line_findings(&line);

    assert!(findings.iter().any(|(pattern, _)| pattern == machine_path));
    assert!(
        findings
            .iter()
            .any(|(pattern, _)| pattern == concat!("Recorded", " on <date>"))
    );
}

#[test]
fn local_context_findings_are_sorted_deterministically() {
    let mut findings = [
        LocalContextFinding {
            path: "b.rs".to_string(),
            line: Some(2),
            pattern: "pat".to_string(),
            problem: "prob".to_string(),
        },
        LocalContextFinding {
            path: "a.rs".to_string(),
            line: Some(1),
            pattern: "pat".to_string(),
            problem: "prob".to_string(),
        },
    ];
    findings.sort();
    assert_eq!(findings[0].path, "a.rs");
    assert_eq!(findings[1].path, "b.rs");
}

#[test]
fn local_markdown_target_filters_absolute_urls() {
    assert_eq!(local_markdown_target("https://example.com"), None);
    assert_eq!(local_markdown_target("http://example.com"), None);
    assert_eq!(local_markdown_target("mailto:test@example.com"), None);
}

#[test]
fn local_markdown_target_keeps_a_same_document_anchor() {
    assert_eq!(
        local_markdown_target("#anchor"),
        Some(LocalMarkdownTarget {
            path: None,
            fragment: Some("anchor".to_string()),
        })
    );
    // A bare `#` names no heading, so there is nothing to resolve.
    assert_eq!(local_markdown_target("#"), None);
}

#[test]
fn local_markdown_target_returns_relative_local_paths() {
    let target = local_markdown_target("relative/path.md");
    assert_eq!(
        target,
        Some(LocalMarkdownTarget {
            path: Some("relative/path.md".to_string()),
            fragment: None,
        })
    );

    let target = local_markdown_target("../sibling.md");
    assert_eq!(
        target,
        Some(LocalMarkdownTarget {
            path: Some("../sibling.md".to_string()),
            fragment: None,
        })
    );
}

#[test]
fn lsp_cockpit_fixture_report_preserves_nested_fixture_name() -> Result<(), String> {
    let root = temp_dir("lsp-cockpit-nested-fixture");
    let fixture = root.join("editor_gap_cockpit").join("rust_actionable");
    write(
        &fixture.join("expected/lsp-diagnostics.json"),
        r#"{
  "diagnostics": [
    {
      "data": {
        "seam_id": "seam:pricing",
        "grip_class": "weakly_gripped"
      }
    },
    {
      "data": {
        "finding_id": "finding:pricing"
      }
    }
  ]
}
"#,
    );
    write(
        &fixture.join("expected/lsp-code-actions.json"),
        r#"{
  "actions": [
    {"title": "Inspect Test Gap - Copy Context", "command": "ripr.copyContext"},
    {"title": "Copy agent packet", "command": "ripr.copyAgentPacketCommand", "arguments": [{"command": "ripr agent packet"}]},
    {"title": "Open related test", "command": "ripr.openRelatedTest", "arguments": [{"uri": "file:///repo/tests/pricing.rs"}]},
    {"title": "Refresh", "command": "ripr.refresh"}
  ]
}
"#,
    );

    let report =
        super::super::lsp_cockpit_fixture_report("editor_gap_cockpit/rust_actionable", &fixture)?
            .ok_or_else(|| "expected nested fixture report".to_string())?;

    assert_eq!(report.fixture, "editor_gap_cockpit/rust_actionable");
    assert_eq!(report.diagnostic_count, 2);
    assert_eq!(report.seam_diagnostic_count, 1);
    assert_eq!(report.finding_diagnostic_count, 1);
    assert!(report.seam_ids.iter().any(|id| id == "seam:pricing"));
    assert!(
        report
            .grip_classes
            .iter()
            .any(|class| class == "weakly_gripped")
    );
    assert!(report.context.seam_packet_available);
    assert!(report.context.agent_packet_command_available);
    assert!(report.context.related_test_available);
    assert!(report.context.refresh_available);
    Ok(())
}

#[test]
fn lsp_cockpit_report_command_writes_markdown_and_json() -> Result<(), String> {
    with_repo_cwd(|| {
        lsp_cockpit_report()?;
        let markdown = fs::read_to_string("target/ripr/reports/lsp-cockpit.md")
            .map_err(|err| format!("failed to read lsp cockpit markdown: {err}"))?;
        let json = fs::read_to_string("target/ripr/reports/lsp-cockpit.json")
            .map_err(|err| format!("failed to read lsp cockpit JSON: {err}"))?;
        assert!(markdown.contains("# ripr LSP cockpit report"));
        assert!(json.contains("\"schema_version\": \"0.1\""));
        Ok(())
    })
}

#[test]
fn lsp_cockpit_report_json_and_markdown_are_structured() -> Result<(), String> {
    let report = with_repo_cwd(build_lsp_cockpit_report)?;
    let json = lsp_cockpit_report_json(&report)?;
    let value: Value =
        serde_json::from_str(&json).map_err(|err| format!("cockpit JSON should parse: {err}"))?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["tool"], "ripr");
    assert!(json.contains("Write targeted test: copy brief"));

    let markdown = lsp_cockpit_report_markdown(&report);
    assert!(markdown.contains("# ripr LSP cockpit report"));
    assert!(markdown.contains("## Fixture: boundary_gap"));
    assert!(markdown.contains("## Fixture: editor_lsp_workflow"));
    assert!(markdown.contains("## Fixture: python_missing_import_graph_limit"));
    assert!(markdown.contains("## Fixture: typescript_mocked_module_limit"));
    assert!(markdown.contains("seam packet available: yes"));
    assert!(markdown.contains("finding diagnostics: 1"));
    assert!(markdown.contains("agent verify command available: yes"));
    assert!(markdown.contains("agent receipt command available: yes"));
    assert!(markdown.contains("ripr.collectContext"));
    Ok(())
}

#[test]
fn lsp_cockpit_report_reads_boundary_gap_fixture_expectations() -> Result<(), String> {
    let report = with_repo_cwd(build_lsp_cockpit_report)?;
    let Some(boundary_gap) = report
        .fixtures
        .iter()
        .find(|fixture| fixture.fixture == "boundary_gap")
    else {
        return Err("expected boundary_gap LSP cockpit fixture".to_string());
    };

    assert_eq!(boundary_gap.diagnostic_count, 1);
    assert_eq!(boundary_gap.seam_diagnostic_count, 1);
    assert!(
        boundary_gap
            .action_titles
            .contains(&"Inspect Test Gap - Copy Context".to_string())
    );
    assert!(
        boundary_gap
            .action_titles
            .contains(&"Write targeted test: copy brief".to_string())
    );
    assert!(
        boundary_gap
            .action_titles
            .contains(&"Write targeted test: open best related test".to_string())
    );
    assert!(boundary_gap.context.seam_packet_available);
    assert!(boundary_gap.context.targeted_test_brief_available);
    assert!(boundary_gap.context.agent_repair_command_available);
    assert!(boundary_gap.context.agent_packet_command_available);
    assert!(boundary_gap.context.agent_brief_command_available);
    assert!(boundary_gap.context.after_snapshot_command_available);
    assert!(boundary_gap.context.agent_verify_command_available);
    assert!(boundary_gap.context.agent_receipt_command_available);
    assert!(boundary_gap.context.assertion_available);
    assert!(boundary_gap.context.related_test_available);
    assert!(boundary_gap.context.refresh_available);
    assert!(
        boundary_gap
            .action_argument_fields
            .contains(&"seam_id".to_string())
    );
    Ok(())
}

#[test]
fn markdown_link_helpers_skip_fences_and_external_targets() {
    let text = "[Docs](docs/README.md#top)\n```md\n[Ignored](missing.md)\n```\n[External](https://example.com)\n[Anchor](#section)\n";

    let links = markdown_links_in_text(text);

    assert_eq!(
        links,
        vec![
            MarkdownLink {
                line: 1,
                target: "docs/README.md#top".to_string()
            },
            MarkdownLink {
                line: 5,
                target: "https://example.com".to_string()
            },
            MarkdownLink {
                line: 6,
                target: "#section".to_string()
            }
        ]
    );
    assert_eq!(
        local_markdown_target("docs/README.md#top"),
        Some(LocalMarkdownTarget {
            path: Some("docs/README.md".to_string()),
            fragment: Some("top".to_string()),
        })
    );
    assert_eq!(
        local_markdown_target("<docs/My File.md>"),
        Some(LocalMarkdownTarget {
            path: Some("docs/My File.md".to_string()),
            fragment: None,
        })
    );
    assert_eq!(local_markdown_target("https://example.com"), None);
    assert_eq!(
        local_markdown_target("#section"),
        Some(LocalMarkdownTarget {
            path: None,
            fragment: Some("section".to_string()),
        })
    );
}

#[test]
fn markdown_links_extracts_all_link_targets_including_urls() {
    let text = "[http](https://example.com) [anchor](#section) [mail](mailto:test@example.com)";
    let links = markdown_links_in_text(text);
    assert!(links.iter().any(|l| l.target == "https://example.com"));
    assert!(links.iter().any(|l| l.target == "#section"));
    assert!(links.iter().any(|l| l.target == "mailto:test@example.com"));
}

// ============================================================================
// Markdown link and local context tests
// ============================================================================

#[test]
fn markdown_links_extracts_relative_targets() {
    let text = "[link](relative/path.md) and [another](../sibling.md)";
    let links = markdown_links_in_text(text);
    assert!(links.iter().any(|l| l.target == "relative/path.md"));
    assert!(links.iter().any(|l| l.target == "../sibling.md"));
}

#[test]
fn markdown_links_includes_line_numbers() {
    let text = "Line 1\n[link](target.md)\nLine 3";
    let links = markdown_links_in_text(text);
    assert!(links.iter().any(|l| l.line == 2));
}

#[test]
fn metrics_helper_counts_all_seven_classes_with_zero_default() {
    let entries = vec![
        duplicate_entry(
            "strong",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "broad",
            20,
            "useful_but_broad",
            "broad predicate",
            "weak",
            &["pricing::p"],
            &[("function_argument", "2")],
        ),
        duplicate_entry("opaque", 30, "opaque", "smoke only", "smoke", &[], &[]),
    ];
    let metrics = test_efficiency_metrics(&entries, &[]);

    assert_eq!(metrics.tests_scanned, 3);
    // Every class is present with a zero default, even unused ones.
    for class in [
        "strong_discriminator",
        "useful_but_broad",
        "smoke_only",
        "likely_vacuous",
        "possibly_circular",
        "duplicative",
        "opaque",
    ] {
        assert!(
            metrics.class_counts.contains_key(class),
            "metrics.class_counts is missing `{class}`"
        );
    }
    assert_eq!(metrics.class_counts["strong_discriminator"], 1);
    assert_eq!(metrics.class_counts["useful_but_broad"], 1);
    assert_eq!(metrics.class_counts["opaque"], 1);
    assert_eq!(metrics.class_counts["smoke_only"], 0);
    assert_eq!(metrics.class_counts["duplicative"], 0);
}

#[test]
fn metrics_helper_counts_reason_strings_from_entries() {
    let mut a = duplicate_entry(
        "a",
        10,
        "useful_but_broad",
        "broad predicate",
        "weak",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    a.reasons.push("broad_oracle".to_string());
    a.reasons
        .push("assertion_may_not_match_detected_owner".to_string());
    let mut b = duplicate_entry(
        "b",
        20,
        "useful_but_broad",
        "broad predicate",
        "weak",
        &["pricing::p"],
        &[("function_argument", "2")],
    );
    b.reasons.push("broad_oracle".to_string());

    let metrics = test_efficiency_metrics(&[a, b], &[]);

    assert_eq!(metrics.reason_counts["broad_oracle"], 2);
    assert_eq!(
        metrics.reason_counts["assertion_may_not_match_detected_owner"],
        1
    );
    assert!(!metrics.reason_counts.contains_key("nonexistent_reason"));
}

#[test]
fn metrics_helper_distinguishes_duplicative_test_count_from_group_count() {
    // Two disjoint groups, each with two members + one with three members.
    // Total duplicative tests = 2 + 2 + 3 = 7. Total groups = 3.
    let mut entries = Vec::new();
    for owner in &["pricing::a", "pricing::b"] {
        for index in 0..2 {
            entries.push(duplicate_entry(
                owner,
                10 + index,
                "strong_discriminator",
                "exact assertion",
                "strong",
                &[owner],
                &[("function_argument", "1")],
            ));
        }
    }
    for index in 0..3 {
        entries.push(duplicate_entry(
            "pricing::c",
            100 + index,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::c"],
            &[("function_argument", "9")],
        ));
    }

    let groups = apply_duplicate_discriminator_groups(&mut entries);
    let metrics = test_efficiency_metrics(&entries, &groups);

    assert_eq!(groups.len(), 3, "three distinct duplicate groups");
    assert_eq!(
        metrics.duplicate_discriminator_group_count, 3,
        "duplicate_discriminator_group_count must count groups"
    );
    assert_eq!(
        metrics.class_counts["duplicative"], 7,
        "class_counts.duplicative must count tests, not groups"
    );
}

#[test]
fn metrics_json_contains_metrics_object_with_exact_keys() {
    let entries = vec![duplicate_entry(
        "alone",
        10,
        "strong_discriminator",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    )];
    let json = test_efficiency_report_json(&entries, &[], &TestIntentReportSummary::default());

    assert!(json.contains("\"metrics\": {"));
    assert!(json.contains("\"tests_scanned\": 1"));
    assert!(json.contains("\"class_counts\": {"));
    // Every required class key is present in the metrics.class_counts.
    for class in [
        "strong_discriminator",
        "useful_but_broad",
        "smoke_only",
        "likely_vacuous",
        "possibly_circular",
        "duplicative",
        "opaque",
    ] {
        assert!(
            json.contains(&format!("\"{class}\":")),
            "metrics JSON is missing class `{class}`"
        );
    }
    assert!(json.contains("\"duplicate_discriminator_group_count\": 0"));
}

#[test]
fn metrics_json_keeps_existing_top_level_counts_and_reason_counts() {
    let mut entry = duplicate_entry(
        "x",
        10,
        "useful_but_broad",
        "broad predicate",
        "weak",
        &["pricing::p"],
        &[("function_argument", "1")],
    );
    entry.reasons.push("broad_oracle".to_string());
    let json = test_efficiency_report_json(&[entry], &[], &TestIntentReportSummary::default());

    // Existing top-level surfaces must remain for backward compatibility.
    assert!(json.contains("\"counts\": {"));
    assert!(json.contains("\"reason_counts\": {"));
    // The new metrics object lives alongside, not instead.
    assert!(json.contains("\"metrics\": {"));
    // Both surfaces report the same broad_oracle count.
    let broad_oracle_count = json.matches("\"broad_oracle\": 1").count();
    assert!(
        broad_oracle_count >= 2,
        "expected `broad_oracle: 1` to appear in both top-level reason_counts and metrics.reason_counts; saw {broad_oracle_count}"
    );
}

#[test]
fn metrics_markdown_contains_metrics_table_with_group_count_separate_from_test_count() {
    let mut entries = vec![
        duplicate_entry(
            "a",
            10,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
        duplicate_entry(
            "b",
            30,
            "strong_discriminator",
            "exact assertion",
            "strong",
            &["pricing::p"],
            &[("function_argument", "1")],
        ),
    ];
    let groups = apply_duplicate_discriminator_groups(&mut entries);
    let markdown =
        test_efficiency_report_markdown(&entries, &groups, &TestIntentReportSummary::default());

    assert!(markdown.contains("## Metrics"));
    assert!(markdown.contains("| Metric | Value |"));
    assert!(markdown.contains("| Tests scanned | 2 |"));
    assert!(markdown.contains("| Duplicative | 2 |"));
    assert!(
        markdown.contains("| Duplicate discriminator groups | 1 |"),
        "metrics table must distinguish group count from duplicative test count"
    );
}

#[test]
fn metrics_report_has_no_coverage_or_denominator_language() {
    let entries = vec![duplicate_entry(
        "x",
        10,
        "strong_discriminator",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    )];
    let markdown =
        test_efficiency_report_markdown(&entries, &[], &TestIntentReportSummary::default());
    let json = test_efficiency_report_json(&entries, &[], &TestIntentReportSummary::default());

    for body in [&markdown, &json] {
        let lower = body.to_ascii_lowercase();
        assert!(
            !lower.contains("coverage"),
            "test-efficiency report must not use coverage framing"
        );
        // The badge contract is inbox-zero; no denominators in this report.
        assert!(
            !lower.contains("uncovered"),
            "test-efficiency report must not use uncovered framing"
        );
    }
}

#[test]
fn metrics_report_uses_only_emitted_vocabulary_strings() {
    let entries = vec![duplicate_entry(
        "x",
        10,
        "strong_discriminator",
        "exact assertion",
        "strong",
        &["pricing::p"],
        &[("function_argument", "1")],
    )];
    let json = test_efficiency_report_json(&entries, &[], &TestIntentReportSummary::default());

    // Aliases that must NOT appear in the metrics surface — ensures we
    // do not introduce a parallel vocabulary.
    for forbidden in [
        "\"broad\":",
        "\"circular\":",
        "\"vacuous\":",
        "\"smoke\":",
        "\"duplicate\":",
        "\"duplicate_discriminator\":",
        "\"discriminator\":",
    ] {
        assert!(
            !json.contains(forbidden),
            "metrics JSON must not contain alias `{forbidden}`"
        );
    }
}

#[test]
fn mutation_calibration_agreement_tables_disclose_sampling_when_truncated() -> Result<(), String> {
    let over_limit = MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT + 1;
    let static_seams = (0..over_limit)
        .map(|i| {
            let mut seam = targeted_static_seam(&format!("gap-{i}"), "ungripped");
            seam.file = "src/gap.rs".to_string();
            seam.line = i + 1;
            seam
        })
        .collect();
    let runtime_mutants = (0..over_limit)
        .map(|i| mutation_record_at(&format!("m{i}"), None, "src/other.rs", i + 1, "missed"))
        .collect();

    let report = build_mutation_calibration_report(static_seams, runtime_mutants);
    assert_eq!(
        report.agreement.runtime_signal_without_static_gap,
        over_limit
    );
    assert_eq!(
        report.agreement.static_gap_without_runtime_signal,
        over_limit
    );
    assert_eq!(
        report.missed_runtime_signals.len(),
        MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT
    );
    assert_eq!(
        report.static_only_findings.len(),
        MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT
    );
    assert_eq!(report.missed_runtime_signals_total, over_limit);
    assert_eq!(report.static_only_findings_total, over_limit);

    let markdown = mutation_calibration_report_markdown(&report);
    assert!(markdown.contains("Runtime signals without static gaps"));
    assert!(markdown.contains("Static gaps without runtime signals"));
    let expected = format!(
        "Sample only; showing {} of {over_limit}.",
        MUTATION_CALIBRATION_AGREEMENT_SAMPLE_LIMIT
    );
    assert_eq!(
        markdown.matches(&expected).count(),
        2,
        "both truncated agreement tables must disclose the pre-truncation count:\n{markdown}"
    );
    Ok(())
}

#[test]
fn mutation_calibration_args_parse_root_and_input_paths() -> Result<(), String> {
    let args = vec![
        "fixtures/demo".to_string(),
        "--mutants-json".to_string(),
        "target/mutants/outcomes.json".to_string(),
        "--repo-exposure-json".to_string(),
        "target/ripr/reports/repo-exposure.json".to_string(),
    ];

    let parsed = parse_mutation_calibration_args(&args)?;

    assert_eq!(parsed.root, "fixtures/demo");
    assert_eq!(
        parsed.mutants_json,
        PathBuf::from("target/mutants/outcomes.json")
    );
    assert_eq!(
        parsed.repo_exposure_json,
        Some(PathBuf::from("target/ripr/reports/repo-exposure.json"))
    );
    assert!(known_xtask_command("mutation-calibration"));
    Ok(())
}

#[test]
fn mutation_calibration_directory_input_combines_outcomes_and_mutants() -> Result<(), String> {
    let dir = temp_dir("mutation-calibration-dir");
    write(
        &dir.join("mutants.json"),
        r#"{
              "mutants": [
                {
                  "id": "m1",
                  "file": "src/pricing.rs",
                  "line": 42,
                  "operator": "replace >= with >"
                }
              ]
            }"#,
    );
    write(
        &dir.join("outcomes.json"),
        r#"{
              "outcomes": [
                {
                  "mutant_id": "m1",
                  "outcome": "caught",
                  "duration_ms": 123
                }
              ]
            }"#,
    );

    let input = read_mutation_input_json(&dir)?;
    let mutants = parse_mutation_outcomes_json(&input)?;
    let remove_result = fs::remove_dir_all(&dir);
    if let Err(err) = remove_result {
        return Err(format!(
            "failed to remove temp dir {}: {err}",
            dir.display()
        ));
    }

    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].file, Some("src/pricing.rs".to_string()));
    assert_eq!(mutants[0].runtime_outcome, "caught");
    Ok(())
}

/// Shapes copied from a cargo-mutants 27.1.0 `mutants.out` (rust-hex), combined
/// the way `read_mutation_input_json` combines `outcomes.json` and
/// `mutants.json`. The xtask command must not keep a second importer that
/// reads every outcome as `unknown` (#5374).
#[test]
fn mutation_calibration_imports_cargo_mutants_27_1_scenario_mutant_summaries() -> Result<(), String>
{
    let runtime_json = r#"[
  {
    "outcomes": [
      {"scenario": "Baseline", "summary": "Success"},
      {
        "scenario": {"Mutant": {
          "name": "src/lib.rs:101:9: replace next -> Option<Self::Item> with None",
          "package": "hex",
          "file": "src/lib.rs",
          "function": {"function_name": "next", "span": {"start": {"line": 99, "column": 5}, "end": {"line": 109, "column": 6}}},
          "span": {"start": {"line": 101, "column": 9}, "end": {"line": 108, "column": 10}},
          "replacement": "None",
          "genre": "FnValue"
        }},
        "summary": "CaughtMutant"
      },
      {
        "scenario": {"Mutant": {
          "name": "src/lib.rs:104:43: replace >> with << in next",
          "package": "hex",
          "file": "src/lib.rs",
          "span": {"start": {"line": 104, "column": 43}, "end": {"line": 104, "column": 45}},
          "replacement": "<<",
          "genre": "BinaryOperator"
        }},
        "summary": "MissedMutant"
      },
      {
        "scenario": {"Mutant": {
          "name": "src/lib.rs:110:1: replace timeout_mutant with ()",
          "package": "hex",
          "file": "src/lib.rs",
          "span": {"start": {"line": 110, "column": 1}, "end": {"line": 110, "column": 2}},
          "replacement": "()",
          "genre": "FnValue"
        }},
        "summary": "Timeout"
      },
      {
        "scenario": {"Mutant": {
          "name": "src/lib.rs:111:1: replace unviable_mutant with ()",
          "package": "hex",
          "file": "src/lib.rs",
          "span": {"start": {"line": 111, "column": 1}, "end": {"line": 111, "column": 2}},
          "replacement": "()",
          "genre": "FnValue"
        }},
        "summary": "Unviable"
      }
    ],
    "total_mutants": 4,
    "caught": 1,
    "missed": 1,
    "timeout": 1,
    "unviable": 1
  },
  [
    {
      "name": "src/lib.rs:101:9: replace next -> Option<Self::Item> with None",
      "package": "hex",
      "file": "src/lib.rs",
      "span": {"start": {"line": 101, "column": 9}, "end": {"line": 108, "column": 10}},
      "replacement": "None",
      "genre": "FnValue"
    },
    {
      "name": "src/lib.rs:104:43: replace >> with << in next",
      "package": "hex",
      "file": "src/lib.rs",
      "span": {"start": {"line": 104, "column": 43}, "end": {"line": 104, "column": 45}},
      "replacement": "<<",
      "genre": "BinaryOperator"
    },
    {
      "name": "src/lib.rs:110:1: replace timeout_mutant with ()",
      "package": "hex",
      "file": "src/lib.rs",
      "span": {"start": {"line": 110, "column": 1}, "end": {"line": 110, "column": 2}},
      "replacement": "()",
      "genre": "FnValue"
    },
    {
      "name": "src/lib.rs:111:1: replace unviable_mutant with ()",
      "package": "hex",
      "file": "src/lib.rs",
      "span": {"start": {"line": 111, "column": 1}, "end": {"line": 111, "column": 2}},
      "replacement": "()",
      "genre": "FnValue"
    }
  ]
]"#;

    let mutants = parse_mutation_outcomes_json(runtime_json)?;

    assert_eq!(
        mutants.len(),
        4,
        "Baseline must not become a record, and the four mutants must merge to one each: {mutants:?}"
    );
    assert!(
        mutants
            .iter()
            .all(|record| record.runtime_outcome != "unknown"),
        "xtask mutation-calibration must not import cargo-mutants 27.1 outcomes as unknown: {mutants:?}"
    );

    let caught = mutants
        .iter()
        .find(|record| record.line == Some(101))
        .ok_or_else(|| "line 101 mutant should be imported".to_string())?;
    assert_eq!(caught.file.as_deref(), Some("src/lib.rs"));
    assert_eq!(caught.runtime_outcome, "caught");
    assert_eq!(caught.mutation_operator, "FnValue");
    assert_eq!(
        caught.mutant_id.as_deref(),
        Some("src/lib.rs:101:9: replace next -> Option<Self::Item> with None")
    );

    let missed = mutants
        .iter()
        .find(|record| record.line == Some(104))
        .ok_or_else(|| "line 104 mutant should be imported".to_string())?;
    assert_eq!(missed.runtime_outcome, "missed");
    assert_eq!(missed.mutation_operator, "BinaryOperator");

    let timeout = mutants
        .iter()
        .find(|record| record.line == Some(110))
        .ok_or_else(|| "line 110 timeout mutant should be imported".to_string())?;
    assert_eq!(timeout.runtime_outcome, "timeout");

    let unviable = mutants
        .iter()
        .find(|record| record.line == Some(111))
        .ok_or_else(|| "line 111 unviable mutant should be imported".to_string())?;
    assert_eq!(unviable.runtime_outcome, "unviable");
    Ok(())
}

#[test]
fn mutation_calibration_imports_span_based_mutant_locations() -> Result<(), String> {
    let runtime_json = r#"{
          "mutants": [
            {
              "id": "m1",
              "operator": "replace >= with >",
              "span": {
                "file_name": "src/pricing.rs",
                "start": { "line": 42, "column": 13 },
                "end": { "line": 42, "column": 15 }
              }
            }
          ],
          "outcomes": [
            {
              "mutant_id": "m1",
              "outcome": "caught"
            }
          ]
        }"#;

    let mutants = parse_mutation_outcomes_json(runtime_json)?;

    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].file, Some("src/pricing.rs".to_string()));
    assert_eq!(mutants[0].line, Some(42));
    assert_eq!(mutants[0].runtime_outcome, "caught");
    Ok(())
}

#[test]
fn mutation_calibration_imports_static_seams_and_runtime_outcomes() -> Result<(), String> {
    let static_json = r#"{
          "schema_version": "0.2",
          "scope": "repo",
          "seams": [
            {
              "seam_id": "abc123",
              "kind": "predicate_boundary",
              "file": "src/pricing.rs",
              "line": 42,
              "grip_class": "weakly_gripped",
              "related_tests": [
                {
                  "oracle_kind": "broad_error",
                  "oracle_strength": "weak"
                },
                {
                  "oracle_kind": "exact_value",
                  "oracle_strength": "strong"
                }
              ],
              "observed_values": ["50", "10000"],
              "missing_discriminators": [
                {"value": "amount == discount_threshold", "reason": "equality boundary"}
              ]
            }
          ]
        }"#;
    let runtime_json = r#"{
          "outcomes": [
            {
              "mutant": {
                "id": "m1",
                "seam_id": "abc123",
                "file": "src/pricing.rs",
                "line": 42,
                "operator": "replace >= with >"
              },
              "outcome": "caught",
              "duration_ms": 123,
              "test_command": "cargo test pricing"
            }
          ]
        }"#;

    let seams = parse_repo_exposure_static_seams(static_json)?;
    let mutants = parse_mutation_outcomes_json(runtime_json)?;

    assert_eq!(seams.len(), 1);
    assert_eq!(seams[0].oracle_kind, "exact_value");
    assert_eq!(seams[0].oracle_strength, "strong");
    assert_eq!(
        seams[0].missing_discriminators,
        vec!["amount == discount_threshold (equality boundary)"]
    );
    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].mutation_operator, "replace >= with >");
    assert_eq!(mutants[0].runtime_outcome, "caught");
    assert_eq!(mutants[0].duration, Some("123".to_string()));
    Ok(())
}

#[test]
fn mutation_calibration_joins_by_seam_id_then_file_line() {
    let static_seams = vec![
        StaticSeamRecord {
            seam_id: "seam-a".to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: vec!["50".to_string()],
            missing_discriminators: vec!["amount == discount_threshold".to_string()],
        },
        StaticSeamRecord {
            seam_id: "seam-b".to_string(),
            seam_kind: "error_variant".to_string(),
            file: "src/auth.rs".to_string(),
            line: 11,
            seam_grip_class: "ungripped".to_string(),
            oracle_kind: "unknown".to_string(),
            oracle_strength: "unknown".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        },
    ];
    let runtime_mutants = vec![
        MutationOutcomeRecord {
            mutant_id: Some("m1".to_string()),
            seam_id: Some("seam-a".to_string()),
            file: None,
            line: None,
            mutation_operator: "replace >= with >".to_string(),
            runtime_outcome: "caught".to_string(),
            duration: Some("55".to_string()),
            test_command: Some("cargo test".to_string()),
        },
        MutationOutcomeRecord {
            mutant_id: Some("m2".to_string()),
            seam_id: None,
            file: Some(".\\src\\auth.rs".to_string()),
            line: Some(11),
            mutation_operator: "replace error variant".to_string(),
            runtime_outcome: "timeout".to_string(),
            duration: None,
            test_command: None,
        },
        MutationOutcomeRecord {
            mutant_id: Some("m3".to_string()),
            seam_id: None,
            file: Some("src/other.rs".to_string()),
            line: Some(99),
            mutation_operator: "replace value".to_string(),
            runtime_outcome: "caught".to_string(),
            duration: None,
            test_command: None,
        },
    ];

    let report = build_mutation_calibration_report(static_seams, runtime_mutants);

    assert_eq!(report.matched.len(), 2);
    assert_eq!(report.matched[0].join_method, "seam_id");
    assert_eq!(report.matched[1].join_method, "file_line");
    assert_eq!(report.unmatched_mutants.len(), 1);
    assert!(report.static_without_runtime.is_empty());
}

#[test]
fn mutation_calibration_merges_mutants_and_outcomes_by_mutant_id() -> Result<(), String> {
    let runtime_json = r#"{
          "mutants": [
            {
              "id": "m1",
              "file": "src/pricing.rs",
              "line": 42,
              "operator": "replace >= with >"
            }
          ],
          "outcomes": [
            {
              "mutant_id": "m1",
              "outcome": "caught",
              "duration_ms": 123,
              "test_command": "cargo test pricing"
            }
          ]
        }"#;

    let mutants = parse_mutation_outcomes_json(runtime_json)?;

    assert_eq!(mutants.len(), 1);
    assert_eq!(mutants[0].mutant_id, Some("m1".to_string()));
    assert_eq!(mutants[0].file, Some("src/pricing.rs".to_string()));
    assert_eq!(mutants[0].line, Some(42));
    assert_eq!(mutants[0].mutation_operator, "replace >= with >");
    assert_eq!(mutants[0].runtime_outcome, "caught");
    assert_eq!(mutants[0].duration, Some("123".to_string()));
    Ok(())
}

#[test]
fn mutation_calibration_reports_ambiguous_file_line_without_selecting_first() -> Result<(), String>
{
    let static_seams = vec![
        StaticSeamRecord {
            seam_id: "seam-a".to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        },
        StaticSeamRecord {
            seam_id: "seam-b".to_string(),
            seam_kind: "return_value".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "ungripped".to_string(),
            oracle_kind: "unknown".to_string(),
            oracle_strength: "unknown".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        },
    ];
    let runtime_mutants = vec![MutationOutcomeRecord {
        mutant_id: Some("m1".to_string()),
        seam_id: None,
        file: Some("src/pricing.rs".to_string()),
        line: Some(42),
        mutation_operator: "replace >= with >".to_string(),
        runtime_outcome: "caught".to_string(),
        duration: None,
        test_command: None,
    }];

    let report = build_mutation_calibration_report(static_seams, runtime_mutants);

    assert!(report.matched.is_empty());
    assert_eq!(report.ambiguous_file_line.len(), 1);
    assert_eq!(report.ambiguous_file_line[0].candidates.len(), 2);
    assert!(report.unmatched_mutants.is_empty());
    assert!(report.static_without_runtime.is_empty());
    let json = mutation_calibration_report_json(&report)?;
    assert!(json.contains(r#""confidence_label": "ambiguous_runtime_join""#));
    Ok(())
}

#[test]
fn mutation_calibration_reports_are_advisory_and_structured() -> Result<(), String> {
    let report = build_mutation_calibration_report(
        vec![StaticSeamRecord {
            seam_id: "seam-a".to_string(),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: 42,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: vec!["50".to_string()],
            missing_discriminators: vec!["amount == discount_threshold".to_string()],
        }],
        vec![MutationOutcomeRecord {
            mutant_id: Some("m1".to_string()),
            seam_id: Some("seam-a".to_string()),
            file: Some("src/pricing.rs".to_string()),
            line: Some(42),
            mutation_operator: "replace >= with >".to_string(),
            runtime_outcome: "caught".to_string(),
            duration: Some("55".to_string()),
            test_command: Some("cargo test pricing".to_string()),
        }],
    );

    let json = mutation_calibration_report_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("failed to parse calibration JSON: {err}"))?;
    assert_eq!(value["schema_version"], "0.1");
    assert_eq!(value["status"], "advisory");
    assert_eq!(value["metrics"]["matched_total"], 1);
    assert_eq!(value["metrics"]["ambiguous_file_line_total"], 0);
    assert_eq!(
        value["matches"][0]["static"]["missing_discriminators"][0],
        "amount == discount_threshold"
    );
    assert_eq!(
        value["matches"][0]["runtime"]["test_command"],
        "cargo test pricing"
    );

    let markdown = mutation_calibration_report_markdown(&report);
    assert!(markdown.contains("Status: advisory"));
    assert!(markdown.contains("weakly_gripped"));
    assert!(markdown.contains("replace >= with >"));
    Ok(())
}

#[test]
fn mutation_calibration_summarizes_static_runtime_agreement() -> Result<(), String> {
    let static_seams = vec![
        targeted_static_seam("gap-runtime-signal", "weakly_gripped"),
        targeted_static_seam("gap-runtime-clean", "ungripped"),
        targeted_static_seam("gap-inconclusive", "reachable_unrevealed"),
        targeted_static_seam("clean-runtime-clean", "strongly_gripped"),
        targeted_static_seam("clean-runtime-signal", "strongly_gripped"),
    ];
    let runtime_mutants = vec![
        mutation_record("m1", Some("gap-runtime-signal"), "missed"),
        mutation_record("m2", Some("gap-runtime-clean"), "caught"),
        mutation_record("m3", Some("gap-inconclusive"), "unviable"),
        mutation_record("m4", Some("clean-runtime-clean"), "caught"),
        mutation_record("m5", Some("clean-runtime-signal"), "missed"),
        mutation_record_at("m6", None, "src/other.rs", 99, "missed"),
    ];

    let report = build_mutation_calibration_report(static_seams, runtime_mutants);

    assert_eq!(report.agreement.static_gap_and_runtime_signal, 1);
    assert_eq!(report.agreement.static_gap_without_runtime_signal, 2);
    assert_eq!(report.agreement.runtime_signal_without_static_gap, 2);
    assert_eq!(report.agreement.static_clean_and_runtime_clean, 1);
    assert_eq!(report.agreement.runtime_inconclusive, 1);
    assert_eq!(report.static_only_findings.len(), 2);
    assert_eq!(report.missed_runtime_signals.len(), 2);

    let json = mutation_calibration_report_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("failed to parse calibration JSON: {err}"))?;
    assert_eq!(value["agreement"]["static_gap_and_runtime_signal"], 1);
    assert_eq!(value["agreement"]["runtime_signal_without_static_gap"], 2);
    assert_eq!(
        value["missed_runtime_signals"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["static_only_findings"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        value["matches"][0]["confidence_label"],
        "supports_static_gap"
    );
    assert!(json.contains(r#""confidence_label": "contradicts_static_gap""#));
    assert!(json.contains(r#""confidence_label": "supports_static_clean""#));
    assert!(json.contains(r#""confidence_label": "contradicts_static_clean""#));
    assert!(json.contains(r#""confidence_label": "runtime_only_signal""#));
    assert!(json.contains(r#""confidence_label": "no_runtime_data""#));

    let markdown = mutation_calibration_report_markdown(&report);
    assert!(markdown.contains("## Static/runtime agreement"));
    assert!(markdown.contains("static_gap_and_runtime_signal"));
    assert!(markdown.contains("Confidence label"));
    assert!(markdown.contains("Runtime signals without static gaps"));
    assert!(markdown.contains("Static gaps without runtime signals"));
    assert!(
        !markdown.contains("Sample only; showing"),
        "tables under the sample limit must not claim sampling:\n{markdown}"
    );
    Ok(())
}

#[test]
fn mutation_calibration_uses_same_static_without_runtime_sample_limit_for_json_and_markdown()
-> Result<(), String> {
    let seams = (0..51)
        .map(|idx| StaticSeamRecord {
            seam_id: format!("seam-{idx:02}"),
            seam_kind: "predicate_boundary".to_string(),
            file: "src/pricing.rs".to_string(),
            line: idx + 1,
            seam_grip_class: "weakly_gripped".to_string(),
            oracle_kind: "exact_value".to_string(),
            oracle_strength: "strong".to_string(),
            observed_values: Vec::new(),
            missing_discriminators: Vec::new(),
        })
        .collect::<Vec<_>>();
    let report = build_mutation_calibration_report(seams, Vec::new());

    let json = mutation_calibration_report_json(&report)?;
    let value: serde_json::Value = serde_json::from_str(&json)
        .map_err(|err| format!("failed to parse calibration JSON: {err}"))?;
    let Some(sample) = value["static_without_runtime_sample"].as_array() else {
        return Err("missing static_without_runtime_sample array".to_string());
    };
    let markdown = mutation_calibration_report_markdown(&report);
    let Some(static_without_runtime_section) = markdown
        .split("## Static Seams Without Runtime Data")
        .nth(1)
    else {
        return Err("missing Static Seams Without Runtime Data section".to_string());
    };
    let markdown_rows = static_without_runtime_section
        .lines()
        .filter(|line| line.starts_with("| `seam-"))
        .count();

    assert_eq!(
        sample.len(),
        MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT
    );
    assert_eq!(
        markdown_rows,
        MUTATION_CALIBRATION_STATIC_WITHOUT_RUNTIME_SAMPLE_LIMIT
    );
    Ok(())
}

#[test]
fn network_policy_candidate_extensions_cover_supported_script_sources() {
    for path in [
        "src/client.rs",
        "src/client.ts",
        "src/client.tsx",
        "src/client.py",
        "src/client.js",
        "src/client.jsx",
        "scripts/check.sh",
        "scripts/check.ps1",
        ".github/workflows/check.yml",
        ".github/workflows/check.yaml",
    ] {
        assert!(
            is_network_policy_candidate(path),
            "expected network-policy candidate: {path}"
        );
    }

    assert!(!is_network_policy_candidate("docs/network-policy.md"));
}

#[test]
fn network_policy_rejects_fetch_in_tsx_and_accepts_clean_jsx() -> Result<(), String> {
    with_temp_cwd("network-policy-jsx-tsx", |root| {
        write(&root.join("policy/network_allowlist.txt"), "");
        write(
            &root.join("src/networked.tsx"),
            concat!("export const load = () => fet", "ch('/api/data');\n"),
        );
        write(
            &root.join("src/presentational.jsx"),
            "export const Label = () => <span>ready</span>;\n",
        );

        let init = run("git", &["init", "--quiet"])?;
        if !init.success() {
            return Err(format!("temporary git init failed with {init}"));
        }
        let add = run(
            "git",
            &[
                "add",
                "policy/network_allowlist.txt",
                "src/networked.tsx",
                "src/presentational.jsx",
            ],
        )?;
        if !add.success() {
            return Err(format!("temporary git add failed with {add}"));
        }

        let failure = check_network_policy()
            .expect_err("an unallowlisted fetch in tracked TSX must fail network policy");
        assert!(
            failure.contains(concat!(
                "src/networked.tsx contains `fet",
                "ch(` 1 time(s), allowed 0"
            )),
            "unexpected network-policy failure: {failure}"
        );
        assert!(
            !failure.contains("src/presentational.jsx"),
            "clean JSX control must not be reported: {failure}"
        );
        Ok(())
    })
}

#[test]
fn non_rust_programming_policy_requires_retention_rule() {
    assert!(is_non_rust_programming_candidate("scripts/check.py"));
    assert!(is_non_rust_programming_candidate(
        "editors/vscode/src/extension.ts"
    ));
    assert!(!is_non_rust_programming_candidate(
        "policy/non_rust_allowlist.txt"
    ));
    assert!(!is_non_rust_programming_candidate("policy/ci-budget.toml"));

    assert!(non_rust_programming_retention_reason("editors/vscode/src/extension.ts").is_some());
    assert!(
        non_rust_programming_retention_reason("editors/vscode/test/suite/extension.test.ts")
            .is_some()
    );
    assert!(non_rust_programming_retention_reason("scripts/check.py").is_none());
    assert!(
        non_rust_programming_retention_reason("tools/python/portable-ripr-consumer/run.py")
            .is_some()
    );
}

#[test]
fn packet_coverage_report_accepts_exact_selected_names() -> Result<(), String> {
    with_temp_cwd("packet-coverage-exact-names", |root| {
        let path = root.join("target/ripr/reports/file-policy.md");
        let report = packet_coverage_report_fixture();
        write(&path, &report);
        assert_packet_coverage_report()?;
        let extra = report.replacen(
            "selected=2; tests=[",
            "selected=3; tests=[neighboring_control, ",
            1,
        );
        write(&path, &extra);
        assert_packet_coverage_report()
    })
}

#[test]
fn packet_coverage_report_rejects_invalid_selected_fields() -> Result<(), String> {
    with_temp_cwd("packet-coverage-invalid-fields", |root| {
        let path = root.join("target/ripr/reports/file-policy.md");
        let report = packet_coverage_report_fixture();
        write(&path, &report);
        assert_packet_coverage_report()?;
        for row in report.lines().filter(|row| row.contains("; tests=[")) {
            for malformed in [
                row.replace("selected=2", "selected=0"),
                row.replace("selected=2", "selected=1"),
                row.replace("selected=2", "selected=3"),
                row.replace("selected=2", "selected=invalid"),
                row.replace("; tests=[", "; subjects=["),
                row.replace("; tests=[", "; tests="),
                row.replace("]; `", "; `"),
                row.replace("; tests=[", "; tests=[, "),
                row.replace("]; `", ", ]; `"),
                row.replace("]; `", "]; tests=[]; `"),
            ] {
                write(&path, &report.replace(row, &malformed));
                if assert_packet_coverage_report().is_ok() {
                    return Err(format!("malformed selected field accepted: {malformed}"));
                }
            }
        }
        Ok(())
    })
}

#[test]
fn packet_coverage_report_rejects_neighboring_selected_names() -> Result<(), String> {
    with_temp_cwd("packet-coverage-neighbor-names", |root| {
        let path = root.join("target/ripr/reports/file-policy.md");
        let report = packet_coverage_report_fixture();
        write(&path, &report);
        assert_packet_coverage_report()?;
        for subject in [
            "portable_consumer::tests::consumer_source_does_not_search_path_or_open_a_network_client",
            "portable_consumer::tests::packet_digest_matches_the_producer_formula",
            "native_packet_analyzes_a_boundary_gap_without_path_or_compiler_fallback",
            "native_packet_pilot_consumes_the_summary_artifact",
        ] {
            // The native target is deliberately not enumerated on Windows.
            if !report.contains(subject) {
                continue;
            }
            for neighbor in [
                format!("neighbor::{subject}"),
                format!("{subject}_neighbor"),
            ] {
                write(&path, &report.replace(subject, &neighbor));
                if assert_packet_coverage_report().is_ok() {
                    return Err(format!(
                        "neighboring selected identity accepted: {neighbor}"
                    ));
                }
            }
        }
        Ok(())
    })
}

#[test]
fn packet_coverage_report_rejects_unpinned_or_wrong_common_binary() -> Result<(), String> {
    with_temp_cwd("packet-coverage-common-binary", |root| {
        let path = root.join("target/ripr/reports/file-policy.md");
        let report = packet_coverage_report_fixture();
        write(&path, &report);
        assert_packet_coverage_report()?;
        for target in ["", "--bin neighboring_binary "] {
            write(&path, &report.replacen("--bin xtask ", target, 1));
            if assert_packet_coverage_report().is_ok() {
                return Err(format!("wrong common packet target accepted: {target:?}"));
            }
        }
        Ok(())
    })
}

#[test]
fn parse_clippy_lints_ledger_separates_active_and_planned() {
    let ledger = r#"
schema_version = "0.1"

[policy]
unsafe = "forbid"

[[active.panic_family]]
name = "clippy::panic"
level = "deny"

[[active.silent_failure]]
name = "clippy::map_err_ignore"
level = "deny"

[[planned]]
name = "clippy::same_length_and_capacity"
level = "deny"
activate_when_msrv = "1.94"
blocked_by = "receipts"
"#;
    let (entries, violations) = super::super::parse_clippy_lints_ledger(ledger);
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
    let names: Vec<(&str, bool)> = entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.is_planned))
        .collect();
    assert_eq!(
        names,
        vec![
            ("clippy::panic", false),
            ("clippy::map_err_ignore", false),
            ("clippy::same_length_and_capacity", true),
        ]
    );
    assert_eq!(
        entries
            .iter()
            .find(|entry| entry.is_planned)
            .and_then(|entry| entry.blocked_by.as_deref()),
        Some("receipts")
    );
    assert!(
        entries
            .iter()
            .filter(|entry| !entry.is_planned)
            .all(|entry| entry.blocked_by.is_none())
    );
}

#[test]
fn parse_gh_pr_status_reads_view_json() -> Result<(), String> {
    let pr = parse_gh_pr_status_pull_request(
        r#"{
              "number": 42,
              "title": "devex: merge readiness",
              "isDraft": false,
              "mergeStateStatus": "CLEAN",
              "headRefName": "devex-gh-pr-status",
              "baseRefName": "main",
              "reviewDecision": "APPROVED",
              "latestReviews": [
                {"author": {"login": "droid"}, "state": "APPROVED"}
              ],
              "statusCheckRollup": [
                {"__typename": "CheckRun", "name": "rust", "status": "COMPLETED", "conclusion": "SUCCESS"},
                {"__typename": "StatusContext", "context": "droid/review", "state": "SUCCESS"}
              ]
            }"#,
    )?;
    assert_eq!(pr.number, 42);
    assert_eq!(pr.title, "devex: merge readiness");
    assert_eq!(pr.checks.len(), 2);
    assert_eq!(pr.reviews.len(), 1);
    assert_eq!(pr.reviews[0].author, "droid");
    assert_eq!(
        parse_required_status_contexts(r#"["rust","msrv","rust"]"#)?,
        vec!["msrv".to_string(), "rust".to_string()]
    );
    Ok(())
}

#[test]
fn parse_pr_triage_pull_requests_reads_gh_json() -> Result<(), String> {
    let prs = parse_pr_triage_pull_requests(
        r#"[
              {
                "number": 10,
                "title": "devex: report",
                "body": "Validation\n- cargo xtask check-pr",
                "isDraft": false,
                "createdAt": "2026-05-14T00:00:00Z",
                "updatedAt": "2026-05-14T01:00:00Z",
                "mergeStateStatus": "CLEAN",
                "headRefName": "devex-report",
                "baseRefName": "main",
                "reviewDecision": "",
                "labels": [{"name": "devex"}],
                "files": [{"path": "xtask/src/main.rs"}, {"path": "docs/PR_AUTOMATION.md"}],
                "statusCheckRollup": [
                  {"__typename": "CheckRun", "name": "rust", "status": "COMPLETED", "conclusion": "SUCCESS"},
                  {"__typename": "StatusContext", "context": "CodeRabbit", "state": "SUCCESS"}
                ]
              }
            ]"#,
    )?;
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].number, 10);
    assert_eq!(prs[0].labels, vec!["devex".to_string()]);
    assert_eq!(
        prs[0].files,
        vec![
            "docs/PR_AUTOMATION.md".to_string(),
            "xtask/src/main.rs".to_string()
        ]
    );
    assert_eq!(prs[0].checks.len(), 2);
    Ok(())
}

#[test]
fn parse_reason_accepts_flag_forms() {
    let spaced = vec![
        "--reason".to_string(),
        "RIPR-SPEC-0001: intentional update".to_string(),
    ];
    let equals = vec!["--reason=RIPR-SPEC-0001: intentional update".to_string()];

    assert_eq!(
        parse_reason(&spaced),
        Ok("RIPR-SPEC-0001: intentional update".to_string())
    );
    assert_eq!(
        parse_reason(&equals),
        Ok("RIPR-SPEC-0001: intentional update".to_string())
    );
    parse_reason(&[]).expect_err("empty args should fail to produce a reason");
}

#[test]
fn parse_reason_rejects_missing_or_malformed_spec_ids() {
    for reason in [
        "x",
        "updated output",
        "RIPR-SPEC-1: short id",
        "RIPR-SPEC-12345: too many digits",
        "RIPR-SPEC-0001x: invalid boundary",
    ] {
        let args = vec!["--reason".to_string(), reason.to_string()];
        assert!(
            parse_reason(&args).is_err(),
            "reason without a well-formed spec id should fail: {reason}"
        );
    }
}

#[test]
fn parse_repo_exposure_summary_counts_reads_class_table() -> Result<(), String> {
    let markdown = r#"# ripr repo exposure report

## Summary

| Class | Count |
| --- | --- |
| seams_total | 12 |
| headline_eligible | 7 |
| strongly_gripped | 5 |
| weakly_gripped | 2 |
| opaque | 1 |

## Top gaps

| Seam | Kind | Class |
| --- | --- | --- |
| a | b | c |
"#;
    let counts = parse_repo_exposure_summary_counts(markdown)?;
    assert_eq!(counts.get("seams_total"), Some(&12));
    assert_eq!(counts.get("headline_eligible"), Some(&7));
    assert_eq!(counts.get("weakly_gripped"), Some(&2));
    assert_eq!(counts.get("opaque"), Some(&1));
    assert!(!counts.contains_key("Seam"));
    Ok(())
}

#[test]
fn parse_ripr_plus_options_accepts_repo_exposure_summary_path() -> Result<(), String> {
    let options = parse_repo_badge_artifact_options(
        &[
            "--repo-exposure-summary".to_string(),
            "target/ripr/reports/repo-exposure-summary.json".to_string(),
        ],
        "ripr-plus",
    )?;

    assert_eq!(
        options.repo_exposure_summary,
        Some(PathBuf::from(
            "target/ripr/reports/repo-exposure-summary.json"
        ))
    );
    assert_eq!(options.gap_ledger, None);
    Ok(())
}

#[test]
fn parse_ripr_plus_options_rejects_ambiguous_receipt_sources() {
    let err = parse_repo_badge_artifact_options(
        &[
            "--gap-ledger".to_string(),
            "target/ripr/reports/gap-decision-ledger.json".to_string(),
            "--repo-exposure-summary".to_string(),
            "target/ripr/reports/repo-exposure-summary.json".to_string(),
        ],
        "ripr-plus",
    )
    .expect_err("ripr-plus must use exactly one receipt source override");

    assert!(err.contains("either --gap-ledger or --repo-exposure-summary"));
}

#[test]
fn parse_ripr_plus_options_rejects_missing_empty_and_unsupported_summary_path() {
    let missing =
        parse_repo_badge_artifact_options(&["--repo-exposure-summary".to_string()], "ripr-plus")
            .expect_err("missing summary path must fail");
    assert!(missing.contains("--repo-exposure-summary requires a path"));

    let empty = parse_repo_badge_artifact_options(
        &["--repo-exposure-summary".to_string(), "".to_string()],
        "ripr-plus",
    )
    .expect_err("empty summary path must fail");
    assert!(empty.contains("--repo-exposure-summary requires a non-empty path"));

    let unsupported = parse_repo_badge_artifact_options(
        &[
            "--repo-exposure-summary".to_string(),
            "target/ripr/reports/repo-exposure-summary.json".to_string(),
        ],
        "badges",
    )
    .expect_err("other badge artifact commands must reject summary reuse");
    assert!(unsupported.contains("does not support --repo-exposure-summary"));
}

#[test]
fn parse_workspace_lints_section_extracts_clippy_levels() {
    let cargo = r#"
[workspace]
members = ["crates/ripr"]

[workspace.lints.rust]
unsafe_code = "forbid"

[workspace.lints.clippy]
# leading comment
panic = "deny"
todo = "deny"
allow_attributes_without_reason = "deny" # trailing comment

[profile.release]
lto = true
"#;
    let clippy = super::super::parse_workspace_lints_section(cargo, "clippy");
    assert_eq!(clippy.get("panic").map(String::as_str), Some("deny"));
    assert_eq!(clippy.get("todo").map(String::as_str), Some("deny"));
    assert_eq!(
        clippy
            .get("allow_attributes_without_reason")
            .map(String::as_str),
        Some("deny")
    );
    assert!(!clippy.contains_key("unsafe_code"));

    let rust = super::super::parse_workspace_lints_section(cargo, "rust");
    assert_eq!(rust.get("unsafe_code").map(String::as_str), Some("forbid"));
    assert!(!rust.contains_key("panic"));
}

#[test]
fn parse_workspace_package_rust_version_reads_workspace_package_only() {
    let cargo = r#"
[workspace]
resolver = "2"

[workspace.package]
version = "0.11.0"
rust-version = "1.95"

[package]
rust-version = "1.70"
"#;
    assert_eq!(
        super::super::parse_workspace_package_rust_version(cargo).as_deref(),
        Some("1.95")
    );
    assert_eq!(super::super::parse_msrv_triple("1.95"), Some((1, 95, 0)));
    assert_eq!(super::super::parse_msrv_triple("1.95.1"), Some((1, 95, 1)));
    assert!(super::super::parse_msrv_triple("1").is_none());

    let single_quoted = r#"
[workspace.package]
rust-version = '1.95'
"#;
    assert_eq!(
        super::super::parse_workspace_package_rust_version(single_quoted).as_deref(),
        Some("1.95")
    );
}

// XnB: a deleted or hand-edited entry must not cause a duplicate heading —
// the first UNUSED number wins, not count+1.
#[test]
fn pending_changelog_headings_skip_used_numbers() {
    let gapped = "# Golden Output Changes\n\n## Pending — a (1)\n\nx\n\n## Pending — b (3)\n\ny\n";
    assert_eq!(
        next_pending_heading(gapped, "my_fixture"),
        "## Pending — my_fixture (2)"
    );
    let used_all =
        "# Golden Output Changes\n\n## Pending — a (1)\n\nx\n\n## Pending — b (2)\n\ny\n";
    assert_eq!(
        next_pending_heading(used_all, "my_fixture"),
        "## Pending — my_fixture (3)"
    );
}

// XQFg: pending changelog headings must be unique so the accumulated blessing
// log does not trip MD024 (no-duplicate-heading), including against legacy
// plain `## Pending` headings left by earlier blesses.
#[test]
fn pending_changelog_headings_stay_unique_across_blesses() {
    assert_eq!(
        next_pending_heading("# Golden Output Changes\n", "my_fixture"),
        "## Pending — my_fixture (1)"
    );
    let after_first = "# Golden Output Changes\n\n## Pending — my_fixture (1)\n\nReason:\nx\n";
    assert_eq!(
        next_pending_heading(after_first, "my_fixture"),
        "## Pending — my_fixture (2)"
    );
    let legacy = "# Golden Output Changes\n\n## Pending\n\nReason:\ny\n";
    assert_eq!(
        next_pending_heading(legacy, "my_fixture"),
        "## Pending — my_fixture (2)"
    );
}

// Guard the *live* corpus (#1729): the mechanism test above proves the gate
// catches a dishonest re-bless of a Perl advisory case, but only against a
// synthetic corpus. This pins the other failure mode #1729 raised — the
// real `perl_preview_card_advisory_no_repair_packet` charter case being
// silently demoted or removed so the gate stops consuming it. It must stay
// in the enforced path: tier `pure` (not clone/flag-gated), a byte-pinned
// `source_report`, and the `must_not_promote` + `expected_class:
// weakly_exposed` assertions. Complements CI's live gate run.
#[test]
fn perl_advisory_charter_case_stays_enforced_in_live_corpus() -> Result<(), String> {
    let corpus_path =
        super::super::repo_rooted_fixture_path(super::super::EVIDENCE_PROMOTION_HONESTY_CORPUS);
    let text = fs::read_to_string(&corpus_path)
        .map_err(|err| format!("read live honesty corpus: {err}"))?;
    let corpus: Value =
        serde_json::from_str(&text).map_err(|err| format!("parse live honesty corpus: {err}"))?;
    let cases = corpus
        .get("cases")
        .and_then(Value::as_array)
        .ok_or("live honesty corpus has no `cases` array")?;
    let perl_case = cases
        .iter()
        .find(|case| {
            case.get("id").and_then(Value::as_str)
                == Some("perl_preview_card_advisory_no_repair_packet")
        })
        .ok_or(
            "live honesty corpus dropped the Perl advisory charter case \
                 `perl_preview_card_advisory_no_repair_packet` — the gate would stop enforcing it",
        )?;

    assert_eq!(
        perl_case.get("tier").and_then(Value::as_str),
        Some("pure"),
        "Perl charter case must stay tier `pure` so it runs on the default gate (not clone/flag-gated)"
    );
    assert!(
        perl_case
            .get("source_report")
            .and_then(Value::as_str)
            .is_some_and(|report| !report.trim().is_empty()),
        "Perl charter case must keep a byte-pinned `source_report`"
    );
    let assertions = perl_case
        .get("assertions")
        .and_then(Value::as_array)
        .ok_or("Perl charter case lost its typed `assertions` array")?;
    assert!(
        assertions.iter().any(|assertion| {
            assertion.get("type").and_then(Value::as_str) == Some("must_not_promote")
        }),
        "Perl charter case must keep the `must_not_promote` assertion (cardinal-sin-seam invariant)"
    );
    assert!(
        assertions.iter().any(|assertion| {
            assertion.get("type").and_then(Value::as_str) == Some("expected_class")
                && assertion.get("class").and_then(Value::as_str) == Some("weakly_exposed")
        }),
        "Perl charter case must keep `expected_class: weakly_exposed`"
    );
    Ok(())
}

#[test]
fn perl_lsp_facts_exporter_fixture_accepts_supported_producers() -> Result<(), String> {
    with_temp_cwd("perl-lsp-facts-exporter-producer-names", |root| {
        for producer in ["perl-ripr-facts", "perllsp", "perl-lsp"] {
            let packet = root.join(format!("{producer}.json"));
            let packet_json = serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": "ripr-perl-facts-v1",
                "producer": {"name": producer},
                "files": [{"path": "lib/App.pm"}]
            }))
            .map_err(|err| err.to_string())?;
            write(&packet, &packet_json);

            let corpus = root.join(format!("{producer}-corpus.json"));
            let corpus_json = serde_json::to_string_pretty(&serde_json::json!({
                "kind": "perl_lsp_facts_exporter_corpus",
                "schema_version": "0.1",
                "spec": "RIPR-SPEC-0064",
                "cases": [
                    {
                        "id": producer,
                        "exporter": producer,
                        "packet_schema": "ripr-perl-facts-v1",
                        "authority_boundary": "preview_advisory_only",
                        "must_not_claim": [
                            "ripr_check_executes_perl_lsp",
                            "canonical_gap_id_emitted_by_perl_lsp",
                            "gap_state_emitted_by_perl_lsp",
                            "repair_packet_ready",
                            "default_gate_authority",
                            "public_badge_contribution",
                            "support_tier_promotion"
                        ],
                        "expected_packet": packet
                    }
                ]
            }))
            .map_err(|err| err.to_string())?;
            write(&corpus, &corpus_json);

            let mut violations = Vec::new();
            super::super::validate_perl_lsp_facts_exporter_fixture_corpus_at(
                &corpus,
                &mut violations,
            )?;
            assert_eq!(violations, Vec::<String>::new());
        }
        Ok(())
    })
}

#[test]
fn perl_lsp_facts_exporter_fixture_corpus_is_valid() -> Result<(), String> {
    with_repo_cwd(|| {
        let corpus = perl_lsp_facts_exporter_corpus_path()?;
        let mut violations = Vec::new();
        super::super::validate_perl_lsp_facts_exporter_fixture_corpus_at(&corpus, &mut violations)?;
        assert_eq!(violations, Vec::<String>::new());

        let mut root_violations = Vec::new();
        super::super::validate_perl_lsp_facts_exporter_fixture_corpus(&mut root_violations)?;
        assert_eq!(root_violations, Vec::<String>::new());
        Ok(())
    })
}

#[test]
fn perl_lsp_facts_exporter_fixture_guard_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("perl-lsp-facts-exporter-invalid");
    let missing = root.join("missing.json");
    let mut violations = Vec::new();
    super::super::validate_perl_lsp_facts_exporter_fixture_corpus_at(&missing, &mut violations)?;

    let no_cases = root.join("no-cases.json");
    write(
        &no_cases,
        r#"{
  "kind": "perl_lsp_facts_exporter_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0064"
}
"#,
    );
    super::super::validate_perl_lsp_facts_exporter_fixture_corpus_at(&no_cases, &mut violations)?;

    let empty_cases = root.join("empty-cases.json");
    write(
        &empty_cases,
        r#"{
  "kind": "perl_lsp_facts_exporter_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0064",
  "cases": []
}
"#,
    );
    super::super::validate_perl_lsp_facts_exporter_fixture_corpus_at(
        &empty_cases,
        &mut violations,
    )?;

    let bad_packet = root.join("bad-packet.json");
    let bad_packet_json = serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": "wrong",
        "producer": {"name": "not-perl-lsp"},
        "canonical_gap_id": "gap:bad",
        "gap_state": "actionable",
        "files": [
            {},
            {"path": "lib\\Bad.pm"},
            {"path": "/absolute.pm"},
            {"path": "host:bad.pm"}
        ]
    }))
    .map_err(|err| err.to_string())?;
    write(&bad_packet, &bad_packet_json);
    let missing_files_packet = root.join("missing-files-packet.json");
    let missing_files_packet_json = serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": "ripr-perl-facts-v1",
        "producer": {"name": "perl-lsp"}
    }))
    .map_err(|err| err.to_string())?;
    write(&missing_files_packet, &missing_files_packet_json);

    let corpus = root.join("corpus.json");
    let corpus_json = serde_json::to_string_pretty(&serde_json::json!({
        "kind": "wrong",
        "schema_version": "0.2",
        "spec": "RIPR-SPEC-9999",
        "cases": [
            {
                "id": "missing-expected",
                "exporter": "wrong",
                "packet_schema": "wrong",
                "authority_boundary": "wrong",
                "must_not_claim": []
            },
            {
                "id": "missing-packet",
                "exporter": "perl-lsp",
                "packet_schema": "ripr-perl-facts-v1",
                "authority_boundary": "preview_advisory_only",
                "must_not_claim": [
                    "ripr_check_executes_perl_lsp",
                    "canonical_gap_id_emitted_by_perl_lsp",
                    "gap_state_emitted_by_perl_lsp",
                    "repair_packet_ready",
                    "default_gate_authority",
                    "public_badge_contribution",
                    "support_tier_promotion"
                ],
                "expected_packet": root.join("does-not-exist.json")
            },
            {
                "id": "bad-packet",
                "exporter": "perl-lsp",
                "packet_schema": "ripr-perl-facts-v1",
                "authority_boundary": "preview_advisory_only",
                "must_not_claim": [
                    "default_gate_authority"
                ],
                "expected_packet": bad_packet
            },
            {
                "id": "bad-packet",
                "exporter": "perl-lsp",
                "packet_schema": "ripr-perl-facts-v1",
                "authority_boundary": "preview_advisory_only",
                "must_not_claim": [
                    "ripr_check_executes_perl_lsp",
                    "canonical_gap_id_emitted_by_perl_lsp",
                    "gap_state_emitted_by_perl_lsp",
                    "repair_packet_ready",
                    "default_gate_authority",
                    "public_badge_contribution",
                    "support_tier_promotion"
                ],
                "expected_packet": missing_files_packet
            }
        ]
    }))
    .map_err(|err| err.to_string())?;
    write(&corpus, &corpus_json);
    super::super::validate_perl_lsp_facts_exporter_fixture_corpus_at(&corpus, &mut violations)?;

    let report = violations.join("\n");
    assert!(report.contains("perl-lsp facts exporter corpus is missing"));
    assert!(report.contains("is missing cases array"));
    assert!(report.contains("cases array must not be empty"));
    assert!(report.contains("kind must be perl_lsp_facts_exporter_corpus"));
    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("spec must be RIPR-SPEC-0064"));
    assert!(report.contains("exporter must be perl-ripr-facts, perllsp, or perl-lsp"));
    assert!(report.contains("packet_schema must be ripr-perl-facts-v1"));
    assert!(report.contains("authority_boundary must be preview_advisory_only"));
    assert!(report.contains("must_not_claim is missing ripr_check_executes_perl_lsp"));
    assert!(report.contains("is missing expected_packet"));
    assert!(report.contains("missing packet"));
    assert!(report.contains("perl-lsp facts exporter case bad-packet is duplicated"));
    assert!(report.contains("packet schema_version must be ripr-perl-facts-v1"));
    assert!(report.contains("packet producer.name must be perl-ripr-facts, perllsp, or perl-lsp"));
    assert!(report.contains("packet must not emit RIPR-derived gap state"));
    assert!(report.contains("file fact is missing path"));
    assert!(report.contains("must be repo-relative"));
    assert!(report.contains("packet is missing files array"));
    Ok(())
}

#[test]
fn perl_lsp_facts_exporter_fixture_guard_reports_missing_root_files() -> Result<(), String> {
    with_temp_cwd("perl-lsp-facts-exporter-missing-root", |_| {
        let mut violations = Vec::new();
        super::super::validate_perl_lsp_facts_exporter_fixture_corpus(&mut violations)?;
        let report = violations.join("\n");

        assert!(report.contains("missing fixtures/perl_lsp_facts_exporter/SPEC.md"));
        assert!(report.contains("missing fixtures/perl_lsp_facts_exporter/corpus.json"));
        assert!(report.contains("perl-lsp facts exporter corpus is missing"));
        Ok(())
    })
}

#[test]
fn perl_packet_contract_migration_fixture_corpus_is_valid() -> Result<(), String> {
    with_repo_cwd(|| {
        let corpus = perl_packet_contract_migration_corpus_path()?;
        let mut violations = Vec::new();
        validate_perl_packet_contract_migration_corpus_at(&corpus, &mut violations)?;
        assert_eq!(violations, Vec::<String>::new());

        let mut root_violations = Vec::new();
        validate_perl_packet_contract_migration_corpus(&mut root_violations)?;
        assert_eq!(root_violations, Vec::<String>::new());
        Ok(())
    })
}

#[test]
fn perl_packet_contract_migration_fixture_guard_reports_disposition_authority_flip()
-> Result<(), String> {
    with_temp_cwd("perl-packet-contract-migration-authority-flip", |root| {
        let fixture_root = copy_perl_packet_contract_migration_fixture(root)?;
        let dispositions_path = fixture_root.join("expected/consumer-dispositions.v1.json");
        let mut dispositions: Value = serde_json::from_str(
            &fs::read_to_string(&dispositions_path).map_err(|err| err.to_string())?,
        )
        .map_err(|err| err.to_string())?;
        dispositions["pipeline"]["canonical_gap_emitted"] = Value::Bool(true);
        write(
            &dispositions_path,
            &serde_json::to_string_pretty(&dispositions).map_err(|err| err.to_string())?,
        );

        let mut violations = Vec::new();
        validate_perl_packet_contract_migration_corpus_at(
            &fixture_root.join("corpus.json"),
            &mut violations,
        )?;
        let report = violations.join("\n");
        assert!(report.contains("must stay false"));
        Ok(())
    })
}

#[test]
fn perl_packet_contract_migration_fixture_guard_reports_input_digest_drift() -> Result<(), String> {
    with_temp_cwd("perl-packet-contract-migration-input-drift", |root| {
        let fixture_root = copy_perl_packet_contract_migration_fixture(root)?;
        let input = fixture_root.join("producer-inputs/ordinary_discount/lib/App/Discount.pm");
        let mut bytes = fs::read(&input).map_err(|err| err.to_string())?;
        bytes.push(b' ');
        fs::write(&input, &bytes).map_err(|err| err.to_string())?;

        let mut violations = Vec::new();
        validate_perl_packet_contract_migration_corpus_at(
            &fixture_root.join("corpus.json"),
            &mut violations,
        )?;
        let report = violations.join("\n");
        assert!(report.contains("digest drift"));
        Ok(())
    })
}

#[test]
fn perl_packet_contract_migration_fixture_guard_reports_input_inventory_drift() -> Result<(), String>
{
    with_temp_cwd("perl-packet-contract-migration-inventory-drift", |root| {
        let fixture_root = copy_perl_packet_contract_migration_fixture(root)?;
        let unlisted = fixture_root.join("producer-inputs/ordinary_discount/lib/App/Extra.pm");
        write(
            &unlisted,
            "package App::Extra;
1;
",
        );

        let mut violations = Vec::new();
        validate_perl_packet_contract_migration_corpus_at(
            &fixture_root.join("corpus.json"),
            &mut violations,
        )?;
        let report = violations.join(
            "
",
        );
        assert!(report.contains("input inventory drift"));
        assert!(report.contains("no pinned digest"));
        Ok(())
    })
}

#[test]
fn perl_packet_contract_migration_fixture_guard_reports_packet_tamper() -> Result<(), String> {
    with_temp_cwd("perl-packet-contract-migration-packet-tamper", |root| {
        let fixture_root = copy_perl_packet_contract_migration_fixture(root)?;
        let packet = fixture_root.join("producer-packets/v1/ordinary_discount.json");
        let mut bytes = fs::read(&packet).map_err(|err| err.to_string())?;
        bytes.push(b' ');
        fs::write(&packet, &bytes).map_err(|err| err.to_string())?;

        let mut violations = Vec::new();
        validate_perl_packet_contract_migration_corpus_at(
            &fixture_root.join("corpus.json"),
            &mut violations,
        )?;
        let report = violations.join("\n");
        assert!(report.contains("packet digest drift"));
        Ok(())
    })
}

#[test]
fn perl_packet_contract_migration_fixture_guard_reports_row_removal() -> Result<(), String> {
    with_temp_cwd("perl-packet-contract-migration-row-removal", |root| {
        let fixture_root = copy_perl_packet_contract_migration_fixture(root)?;
        let contradictions_path = fixture_root.join("expected/contradictions.v1.json");
        let mut contradictions: Value = serde_json::from_str(
            &fs::read_to_string(&contradictions_path).map_err(|err| err.to_string())?,
        )
        .map_err(|err| err.to_string())?;
        // Remove a row whose id the corpus REQUIRES, not the first
        // serialized row: row order is not part of the corpus contract.
        let corpus: Value = serde_json::from_str(
            &fs::read_to_string(fixture_root.join("corpus.json")).map_err(|err| err.to_string())?,
        )
        .map_err(|err| err.to_string())?;
        let required = corpus
            .get("required_contradiction_ids")
            .and_then(Value::as_array)
            .ok_or_else(|| "required_contradiction_ids is missing".to_string())?;
        let required_first = required[0]
            .as_str()
            .ok_or_else(|| "required id is not a string".to_string())?;
        let rows = contradictions
            .get_mut("contradictions")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| "contradictions array is missing".to_string())?;
        let position = rows
            .iter()
            .position(|row| row.get("id").and_then(Value::as_str) == Some(required_first))
            .ok_or_else(|| format!("required row `{required_first}` not found"))?;
        rows.remove(position);
        write(
            &contradictions_path,
            &serde_json::to_string_pretty(&contradictions).map_err(|err| err.to_string())?,
        );

        let mut violations = Vec::new();
        validate_perl_packet_contract_migration_corpus_at(
            &fixture_root.join("corpus.json"),
            &mut violations,
        )?;
        let report = violations.join("\n");
        assert!(report.contains("missing required contradiction row"));
        Ok(())
    })
}

#[test]
fn perl_real_repo_eval_fixture_corpus_is_valid() -> Result<(), String> {
    with_repo_cwd(|| {
        let corpus = perl_real_repo_eval_corpus_path()?;
        let mut violations = Vec::new();
        super::super::validate_perl_real_repo_eval_fixture_corpus_at(&corpus, &mut violations)?;
        assert_eq!(violations, Vec::<String>::new());

        let mut root_violations = Vec::new();
        super::super::validate_perl_real_repo_eval_fixture_corpus(&mut root_violations)?;
        assert_eq!(root_violations, Vec::<String>::new());
        Ok(())
    })
}

#[test]
fn perl_real_repo_eval_fixture_guard_reports_contract_drift() -> Result<(), String> {
    let root = temp_dir("perl-real-repo-eval-invalid");
    let missing = root.join("missing.json");
    let mut violations = Vec::new();
    super::super::validate_perl_real_repo_eval_fixture_corpus_at(&missing, &mut violations)?;

    let no_cases = root.join("no-cases.json");
    write(
        &no_cases,
        r#"{
  "kind": "perl_real_repo_eval_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0064",
  "limits": [
    "producer_required_on_path",
    "no_five_repo_metrics",
    "no_public_repair_packet_authority",
    "no_support_tier_promotion"
  ]
}
"#,
    );
    super::super::validate_perl_real_repo_eval_fixture_corpus_at(&no_cases, &mut violations)?;

    let empty_cases = root.join("empty-cases.json");
    write(
        &empty_cases,
        r#"{
  "kind": "perl_real_repo_eval_corpus",
  "schema_version": "0.1",
  "spec": "RIPR-SPEC-0064",
  "limits": [
    "producer_required_on_path",
    "no_five_repo_metrics",
    "no_public_repair_packet_authority",
    "no_support_tier_promotion"
  ],
  "cases": []
}
"#,
    );
    super::super::validate_perl_real_repo_eval_fixture_corpus_at(&empty_cases, &mut violations)?;

    let bad_corpus = root.join("bad-corpus.json");
    let corpus_json = serde_json::to_string_pretty(&serde_json::json!({
            "kind": "wrong",
            "schema_version": "0.2",
            "spec": "RIPR-SPEC-9999",
            "limits": [],
            "cases": [
                {
                    "id": "bad-case",
                    "repo_shape": "",
                    "source_kind": "wrong",
                    "source_ref": "",
                    "command": "cargo test -p ripr",
                    "producer": "wrong",
                    "packet_schema": "wrong",
                    "diff": "fixtures/perl_cpan_alpha/input/not-a-diff.txt",
                    "oracle_shape": "",
                    "expected_outcome": "wrong",
                    "expected_classification": "",
                    "changed_owner": "python:wrong",
                    "missing_discriminator": "not_applicable_wrong",
                    "repair_packet_expected": true,
                    "agent_packet_expected": true,
                    "receipt_expected": true,
                    "harness_assertion": "",
                    "evidence_source": "none",
                    "claim_boundary": [],
                    "reason": ""
                },
                {
                    "id": "cpan_alpha_actionable_real_exporter_eval",
                    "repo_shape": "cpan_style_test_more",
                    "source_kind": "local_repo_fixture",
                    "source_ref": "fixtures/perl_cpan_alpha/input",
                    "command": "PERL_RIPR_FACTS=<path> cargo test -p ripr --features lang-perl --test perl_two_binary_harness -- --test-threads=1",
                    "producer": "perl-ripr-facts",
                    "packet_schema": "ripr-perl-facts-v1",
                    "diff": "fixtures/perl_cpan_alpha/input/dynamic_dispatch.diff",
                    "oracle_shape": "dynamic_method_dispatch",
                    "expected_outcome": "limited",
                    "expected_classification": "exposed",
                    "changed_owner": "perl:lib/Pricing.pm::Pricing::dynamic_method",
                    "missing_discriminator": "not_applicable_dynamic_dispatch",
                    "repair_packet_expected": false,
                    "agent_packet_expected": false,
                    "receipt_expected": false,
                    "harness_assertion": "asserts the limited outcome",
                    "evidence_source": "PR #1491",
                    "claim_boundary": [
                        "No >=5 real Perl repo evidence",
                        "No public repair-packet authority",
                        "No support-tier promotion"
                    ],
                    "reason": "required id with wrong outcome"
                },
                {
                    "id": "observed-bad-classification",
                    "repo_shape": "cpan_style_test_more",
                    "source_kind": "local_repo_fixture",
                    "source_ref": "fixtures/perl_cpan_alpha/input",
                    "command": "PERL_RIPR_FACTS=<path> cargo test -p ripr --features lang-perl --test perl_two_binary_harness -- --test-threads=1",
                    "producer": "perl-ripr-facts",
                    "packet_schema": "ripr-perl-facts-v1",
                    "diff": "fixtures/perl_cpan_alpha/input/boundary_change.diff",
                    "oracle_shape": "exact_test_more_is",
                    "expected_outcome": "already_observed",
                    "expected_classification": "weakly_exposed",
                    "changed_owner": "perl:lib/Pricing.pm::Pricing::calculate_discount",
                    "missing_discriminator": "not_applicable_already_observed",
                    "repair_packet_expected": false,
                    "agent_packet_expected": false,
                    "harness_assertion": "asserts the observed outcome",
                    "evidence_source": "PR #1491",
                    "claim_boundary": [
                        "No >=5 real Perl repo evidence",
                        "No public repair-packet authority",
                        "No support-tier promotion"
                    ],
                    "reason": "already observed case with wrong classification"
                },
                {
                    "id": "bad-case",
                    "repo_shape": "cpan_style_test_more",
                    "source_kind": "local_repo_fixture",
                    "source_ref": "fixtures/perl_cpan_alpha/input",
                    "command": "PERL_RIPR_FACTS=<path> cargo test -p ripr --features lang-perl --test perl_two_binary_harness -- --test-threads=1",
                    "producer": "perl-ripr-facts",
                    "packet_schema": "ripr-perl-facts-v1",
                    "diff": "fixtures/perl_cpan_alpha/input/diff.patch",
                    "oracle_shape": "weak_test_more_ok",
                    "expected_outcome": "actionable",
                    "expected_classification": "reachable_unrevealed",
                    "changed_owner": "perl:lib/Pricing.pm::Pricing::calculate_discount",
                    "missing_discriminator": "not_applicable_wrong",
                    "repair_packet_expected": false,
                    "agent_packet_expected": false,
                    "receipt_expected": false,
                    "harness_assertion": "asserts the actionable outcome",
                    "evidence_source": "PR #1491",
                    "claim_boundary": [
                        "No >=5 real Perl repo evidence",
                        "No public repair-packet authority",
                        "No support-tier promotion"
                    ],
                    "reason": "duplicate case"
                }
            ]
        }))
        .map_err(|err| err.to_string())?;
    write(&bad_corpus, &corpus_json);
    super::super::validate_perl_real_repo_eval_fixture_corpus_at(&bad_corpus, &mut violations)?;

    let report = violations.join("\n");
    assert!(report.contains("Perl real-repo eval corpus is missing"));
    assert!(report.contains("is missing cases array"));
    assert!(report.contains("cases array must not be empty"));
    assert!(report.contains("kind must be perl_real_repo_eval_corpus"));
    assert!(report.contains("schema_version must be 0.1"));
    assert!(report.contains("spec must be RIPR-SPEC-0064"));
    assert!(report.contains("limits is missing no_public_repair_packet_authority"));
    assert!(report.contains("source_kind must be local_repo_fixture"));
    assert!(report.contains("producer must be perl-ripr-facts"));
    assert!(report.contains("packet_schema must be ripr-perl-facts-v1"));
    assert!(report.contains("expected_outcome must be actionable"));
    assert!(report.contains("changed_owner must use perl: identity"));
    assert!(report.contains("command must run the perl_two_binary_harness"));
    assert!(report.contains("diff must point at a perl_cpan_alpha diff"));
    assert!(report.contains("repair_packet_expected must stay false"));
    assert!(report.contains("claim_boundary must include No public repair-packet authority"));
    assert!(report.contains("evidence_source must cite PR #1491"));
    assert!(report.contains("Perl real-repo eval case bad-case is duplicated"));
    assert!(report.contains("actionable outcome must name a missing discriminator"));
    assert!(report.contains("limited outcome must name a limitation classification"));
    assert!(report.contains("already_observed outcome must expect exposed"));
    assert!(report.contains("field receipt_expected must be present"));
    assert!(report.contains(
            "Perl real-repo eval case cpan_alpha_actionable_real_exporter_eval must have expected_outcome actionable, got limited"
        ));
    Ok(())
}

#[test]
fn perl_real_repo_eval_fixture_guard_reports_missing_root_files() -> Result<(), String> {
    with_temp_cwd("perl-real-repo-eval-missing-root", |_| {
        let mut violations = Vec::new();
        super::super::validate_perl_real_repo_eval_fixture_corpus(&mut violations)?;
        let report = violations.join("\n");

        assert!(report.contains("missing fixtures/perl-real-repo-evals/SPEC.md"));
        assert!(report.contains("missing fixtures/perl-real-repo-evals/corpus.json"));
        assert!(report.contains("Perl real-repo eval corpus is missing"));
        Ok(())
    })
}

#[test]
fn pin_report_packet_index_generated_at_replaces_only_the_stamp() {
    let rendered = "{\n  \"generated_at\": \"unix_ms:1758672000000\",\n  \"note\": \"unix_ms:1758672000000 elsewhere\"\n}";
    let pinned = pin_report_packet_index_generated_at(rendered, "unix_ms:1758672000000");
    assert!(
        pinned.contains("\"generated_at\": \"unix_ms:0\""),
        "the stamp should be pinned: {pinned}"
    );
    assert!(
        pinned.contains("unix_ms:1758672000000 elsewhere"),
        "only the generated_at field should move: {pinned}"
    );
}

#[test]
fn pinned_semver_honesty_patch_hunk_counts_match_declared_extents() -> Result<(), String> {
    let patch_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures/evidence-promotion-honesty-corpus/patches/semver-matches-greater.diff");
    let patch = std::fs::read_to_string(&patch_path).map_err(|err| {
        format!(
            "read pinned semver honesty patch {}: {err}",
            patch_path.display()
        )
    })?;
    if !patch.contains("@@ -69,8 +69,8 @@") {
        return Err(format!(
            "pinned semver patch must declare eight old/new lines, got:\n{patch}"
        ));
    }
    if !patch.contains("-            if ver.patch != patch {") {
        return Err("pinned semver patch must keep the intended != removal".to_string());
    }
    if !patch.contains("+            if ver.patch >= patch {") {
        return Err("pinned semver patch must keep the intended >= insertion".to_string());
    }

    let mut old = 0usize;
    let mut new = 0usize;
    let mut in_hunk = false;
    for line in patch.lines() {
        if line.starts_with("@@ ") {
            in_hunk = true;
            continue;
        }
        if !in_hunk {
            continue;
        }
        match line.as_bytes().first() {
            Some(b'-') => old += 1,
            Some(b'+') => new += 1,
            Some(b' ') | None => {
                old += 1;
                new += 1;
            }
            Some(b'\\') => {}
            other => {
                return Err(format!(
                    "unsupported pinned semver hunk line {other:?}: {line}"
                ));
            }
        }
    }
    if old != 8 {
        return Err(format!("old hunk body must match @@ -69,8, counted {old}"));
    }
    if new != 8 {
        return Err(format!("new hunk body must match @@ +69,8, counted {new}"));
    }
    Ok(())
}

#[test]
fn planned_blocker_text_is_msrv_only_strips_version_delays_only() {
    assert!(super::super::planned_blocker_text_is_msrv_only(""));
    assert!(super::super::planned_blocker_text_is_msrv_only("   "));
    assert!(super::super::planned_blocker_text_is_msrv_only(
        "waiting for Rust 1.97"
    ));
    assert!(super::super::planned_blocker_text_is_msrv_only(
        "requires Rust 1.97"
    ));
    assert!(super::super::planned_blocker_text_is_msrv_only("MSRV"));
    assert!(super::super::planned_blocker_text_is_msrv_only(
        "available since 1.93"
    ));
    assert!(!super::super::planned_blocker_text_is_msrv_only(
        "per-call expect receipts, not MSRV"
    ));
    assert!(!super::super::planned_blocker_text_is_msrv_only(
        "Rust 1.95.0 Clippy does not recognize this lint"
    ));
    assert!(!super::super::planned_blocker_text_is_msrv_only(
        "needs a reviewed clippy.toml disallowed-fields list"
    ));
}

#[test]
fn policy_checker_facade_runs_current_repo_checks() -> Result<(), String> {
    with_repo_cwd(|| {
        check_static_language()?;
        check_no_panic_family()?;
        check_allow_attributes()?;
        check_local_context()?;
        check_file_policy()?;
        assert_packet_coverage_report()?;
        check_executable_files()?;
        check_workflows()?;
        check_droid_review_config()?;
        check_process_policy()?;
        check_network_policy()
    })
}

/// An enforced `cargo xtask precommit` invocation must expand into every gate
/// precommit runs; an advisory invocation must not.
#[test]
fn precommit_ci_invocation_expansion_covers_precommit_gates() -> Result<(), String> {
    let workflow = r#"
jobs:
  rust:
    steps:
      - name: Required Rust gates
        run: |
          cargo xtask precommit
      - name: Advisory precommit
        continue-on-error: true
        run: |
          cargo xtask precommit
"#;
    let mut enforced = super::super::ci_enforced_xtask_invocations(workflow);
    super::super::expand_precommit_ci_invocations(&mut enforced);
    for gate in super::super::PRECOMMIT_GATE_COMMANDS {
        let invocation = ((*gate).to_string(), String::new());
        if !enforced.contains(&invocation) {
            return Err(format!(
                "enforced precommit invocation must expand to `{gate}`"
            ));
        }
    }
    let mut without_precommit: std::collections::BTreeSet<super::super::WorkflowXtaskInvocation> =
        std::collections::BTreeSet::new();
    super::super::expand_precommit_ci_invocations(&mut without_precommit);
    if !without_precommit.is_empty() {
        return Err("expansion must not add gates when precommit is not enforced".to_string());
    }
    Ok(())
}

/// Advisory-only `cargo xtask precommit` invocations (`continue-on-error`
/// steps and `|| true` shielding) must not put precommit or any of its gates
/// into the expanded enforced set.
#[test]
fn precommit_ci_invocation_expansion_ignores_advisory_invocations() -> Result<(), String> {
    let workflow = r#"
jobs:
  rust:
    steps:
      - name: Advisory step precommit
        continue-on-error: true
        run: |
          cargo xtask precommit
      - name: Shielded precommit
        run: |
          cargo xtask precommit || true
"#;
    let mut enforced = super::super::ci_enforced_xtask_invocations(workflow);
    super::super::expand_precommit_ci_invocations(&mut enforced);
    if enforced.contains(&("precommit".to_string(), String::new())) {
        return Err("advisory precommit invocations must not enter the enforced set".to_string());
    }
    for gate in super::super::PRECOMMIT_GATE_COMMANDS {
        let invocation = ((*gate).to_string(), String::new());
        if enforced.contains(&invocation) {
            return Err(format!(
                "advisory precommit invocations must not expand to `{gate}`"
            ));
        }
    }
    Ok(())
}

/// Every gate the drift-check expansion credits must be cataloged as
/// CI-enforced, so the expansion set and the catalog flags can never disagree
/// (precommit itself is enforced, so its whole table is enforced).
#[test]
fn precommit_gate_commands_are_catalog_ci_enforced() -> Result<(), String> {
    let catalog = command_catalog();
    for gate in super::super::PRECOMMIT_GATE_COMMANDS {
        // Catalog entries may carry argument suffixes (for example
        // `check-no-panic-family [--propose]`); match by command root, the
        // same way the drift check matches workflow invocations.
        let entry = catalog
            .iter()
            .find(|entry| entry.command.split_whitespace().next() == Some(*gate))
            .ok_or_else(|| format!("missing catalog entry for `{gate}`"))?;
        if !entry.ci_enforced {
            return Err(format!(
                "precommit gate `{gate}` must be cataloged ci_enforced=true because enforced precommit invocations expand to it"
            ));
        }
    }
    Ok(())
}

/// Pins PRECOMMIT_GATE_COMMANDS to the gate calls `precommit()` actually
/// executes, in order. The drift-check expansion trusts the const, so it must
/// match the real executed sequence, not only the report prose
/// (`precommit_gate_commands_match_report` covers the report direction).
#[test]
fn precommit_gate_commands_match_executed_precommit_source() -> Result<(), String> {
    let path = repo_root()?.join("xtask/src/main.rs");
    let source =
        std::fs::read_to_string(&path).map_err(|err| format!("read {}: {err}", path.display()))?;
    let expected: Vec<String> = super::super::PRECOMMIT_GATE_COMMANDS
        .iter()
        .map(|gate| (*gate).to_string())
        .collect();
    precommit_source_oracle::require_catalogue(&source, &expected)
}

/// Pins the drift-check expansion table to the precommit report so the two
/// cannot drift apart silently.
#[test]
fn precommit_gate_commands_match_report() -> Result<(), String> {
    let report = super::super::precommit_report_body();
    let report_gates: Vec<&str> = report
        .lines()
        .filter_map(|line| {
            line.strip_prefix("- `cargo xtask ")
                .and_then(|rest| rest.strip_suffix('`'))
        })
        .collect();
    let expected: Vec<&str> = super::super::PRECOMMIT_GATE_COMMANDS.to_vec();
    if report_gates != expected {
        return Err(format!(
            "precommit report gates {report_gates:?} must match PRECOMMIT_GATE_COMMANDS {expected:?}"
        ));
    }
    for required in [
        "check-dependencies",
        "check-process-policy",
        "check-network-policy",
    ] {
        if !expected.contains(&required) {
            return Err(format!(
                "precommit must include the policy gate `{required}`"
            ));
        }
    }
    Ok(())
}

#[test]
fn precommit_report_points_to_review_ready_gate() {
    let body = precommit_report_body();

    assert!(body.contains("cargo fmt --check"));
    assert!(body.contains("cargo xtask check-pr"));
}

#[test]
fn preview_editor_workflow_fixtures_pin_preview_boundaries() -> Result<(), String> {
    let report = with_repo_cwd(build_lsp_cockpit_report)?;
    let python = lsp_fixture(&report, "python_missing_import_graph_limit")?;
    assert_eq!(python.diagnostic_count, 1);
    assert_eq!(python.finding_diagnostic_count, 1);
    assert_eq!(python.seam_diagnostic_count, 0);
    assert!(
        python
            .action_titles
            .contains(&"Inspect finding: copy context packet".to_string())
    );
    assert!(python.context.refresh_available);
    assert!(!python.context.agent_packet_command_available);
    assert_preview_lsp_fixture(
        "python_missing_import_graph_limit",
        "python",
        "missing_import_graph",
    )?;

    let typescript = lsp_fixture(&report, "typescript_mocked_module_limit")?;
    assert_eq!(typescript.diagnostic_count, 1);
    assert_eq!(typescript.finding_diagnostic_count, 1);
    assert_eq!(typescript.seam_diagnostic_count, 0);
    assert!(
        typescript
            .action_titles
            .contains(&"Inspect finding: copy context packet".to_string())
    );
    assert!(typescript.context.refresh_available);
    assert!(!typescript.context.agent_packet_command_available);
    assert_preview_lsp_fixture(
        "typescript_mocked_module_limit",
        "typescript",
        "mocked_module",
    )?;

    let disabled = lsp_fixture(&report, "python_disabled")?;
    assert_eq!(disabled.diagnostic_count, 0);
    assert_eq!(disabled.finding_diagnostic_count, 0);
    assert_eq!(disabled.seam_diagnostic_count, 0);
    assert!(disabled.context.refresh_available);
    assert_preview_disabled_fixture()?;
    Ok(())
}

#[test]
fn proof_pack_command_known_set_accepts_xtask_roots_and_repo_commands() {
    assert!(super::super::proof_pack_command_is_known(
        "cargo xtask goldens check"
    ));
    assert!(super::super::proof_pack_command_is_known(
        "cargo test --workspace"
    ));
    assert!(!super::super::proof_pack_command_is_known("cargo xtask "));
    assert!(!super::super::proof_pack_command_is_known("rm -rf /"));
}

#[test]
fn public_badge_basis_guard_allows_explicit_seam_inventory_badge_copy() {
    let violations = public_badge_basis_violations(
        "README.md",
        "Internal seam inventory badge basis: seam_native\n",
    );
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn public_badge_basis_guard_ignores_policy_docs_seam_native_references() {
    assert!(!is_public_badge_basis_surface("docs/BADGE_POLICY.md"));
    let violations = public_badge_basis_violations(
        "docs/BADGE_POLICY.md",
        "`seam_native` is an internal inventory basis.\n",
    );
    assert!(
        violations.is_empty(),
        "unexpected violations: {violations:?}"
    );
}

#[test]
fn public_badge_basis_guard_rejects_seam_native_repair_badge_copy() {
    let violations =
        public_badge_basis_violations("README.md", "Public ripr badge basis: seam_native\n");
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("README.md:1"));
    assert!(violations[0].contains("canonical_actionable_gap"));
    assert!(violations[0].contains("seam inventory"));
}

#[test]
fn public_contract_rows_detect_json_and_lsp_surfaces() {
    let changes = vec![
        ChangedPath {
            path: "crates/ripr/src/output/json/report.rs".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
        ChangedPath {
            path: "editors/vscode/src/client.ts".to_string(),
            statuses: BTreeSet::from(["M".to_string()]),
        },
        ChangedPath {
            path: "crates/ripr/src/output/human/sections.rs".to_string(),
            statuses: BTreeSet::from(["A".to_string()]),
        },
    ];

    let rows = public_contract_rows(&changes);
    let json = rows
        .iter()
        .find(|(label, _)| *label == "JSON")
        .map(|(_, paths)| paths.clone())
        .unwrap_or_default();
    let lsp = rows
        .iter()
        .find(|(label, _)| *label == "LSP")
        .map(|(_, paths)| paths.clone())
        .unwrap_or_default();
    let human = rows
        .iter()
        .find(|(label, _)| *label == "Human output")
        .map(|(_, paths)| paths.clone())
        .unwrap_or_default();

    assert_eq!(json, vec!["crates/ripr/src/output/json/report.rs (M)"]);
    assert_eq!(lsp, vec!["editors/vscode/src/client.ts (M)"]);
    assert_eq!(human, vec!["crates/ripr/src/output/human/sections.rs (A)"]);
}

/// #4395(a): live workspace pin. Lives in unpublished xtask so `cargo test -p ripr`
/// from a crates.io package does not try to read workspace docs.
#[test]
fn public_docs_state_python_marker_auto_when_no_ripr_toml() -> Result<(), String> {
    let root = slice_test_repo_root();
    let configuration = std::fs::read_to_string(root.join("docs/CONFIGURATION.md"))
        .map_err(|err| format!("read CONFIGURATION.md: {err}"))?;
    let readme = std::fs::read_to_string(root.join("README.md"))
        .map_err(|err| format!("read README.md: {err}"))?;
    let support = std::fs::read_to_string(root.join("docs/status/SUPPORT_TIERS.md"))
        .map_err(|err| format!("read SUPPORT_TIERS.md: {err}"))?;
    let violations =
        super::super::python_marker_auto_docs_violations(&configuration, &readme, &support);
    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations.join("\n"))
    }
}

#[test]
fn python_marker_auto_docs_accept_required_phrases() {
    let (configuration, readme, support) = python_marker_auto_docs_passing_copy();
    assert!(
        super::super::python_marker_auto_docs_violations(configuration, readme, support).is_empty(),
        "required marker-auto phrases should pass"
    );
}

#[test]
fn python_marker_auto_docs_reject_opt_in_python_lumping() {
    let (_configuration, readme, support) = python_marker_auto_docs_passing_copy();
    let violations = super::super::python_marker_auto_docs_violations(
        "with no `ripr.toml` keep Python preview off; opt-in TypeScript, JavaScript, and Python evidence",
        readme,
        support,
    );
    assert!(
        violations
            .iter()
            .any(|violation| violation.contains("must not lump Python")),
        "lumping Python with opt-in TypeScript/JavaScript must fail: {violations:?}"
    );
}

#[test]
fn read_repo_exposure_summary_artifact_accepts_canonical_summary() -> Result<(), String> {
    let dir = badge_endpoint_tempdir("canonical-summary")?;
    let path = dir.join("repo-exposure-summary.json");
    let body = r#"{
            "schema_version": "0.1",
            "format": "repo-exposure-summary-json",
            "basis": "canonical_actionable_gap",
            "metrics": {
                "unsuppressed_exposure_gaps": 0
            },
            "top_files": []
        }"#;
    std::fs::write(&path, body).map_err(|err| format!("write summary fixture: {err}"))?;

    let read = read_repo_exposure_summary_artifact(&path)?;

    assert!(read.contains("canonical_actionable_gap"));
    Ok(())
}

#[test]
fn read_repo_exposure_summary_artifact_rejects_limited_runtime_status() -> Result<(), String> {
    let dir = badge_endpoint_tempdir("limited-summary")?;
    let path = dir.join("repo-exposure-summary.json");
    std::fs::write(
        &path,
        r#"{
                "schema_version": "0.1",
                "format": "repo-exposure-summary-json",
                "basis": "limited_runtime_status",
                "run_status": "limited_timeout",
                "runtime_status": {
                    "downstream_consumable": false
                },
                "metrics": {},
                "top_files": []
            }"#,
    )
    .map_err(|err| format!("write limited summary fixture: {err}"))?;

    let err = read_repo_exposure_summary_artifact(&path)
        .expect_err("limited summary artifacts must not feed ripr-plus receipts");
    assert!(err.contains("not downstream consumable"));
    assert!(err.contains("limited_runtime_status"));
    assert!(err.contains("limited_timeout"));
    Ok(())
}

#[test]
fn read_repo_exposure_summary_artifact_reports_missing_and_malformed_inputs() -> Result<(), String>
{
    let dir = badge_endpoint_tempdir("bad-summary")?;
    let missing = dir.join("missing.json");
    let missing_err = read_repo_exposure_summary_artifact(&missing)
        .expect_err("missing summary artifacts should fail clearly");
    assert!(missing_err.contains("failed to read"));

    let malformed = dir.join("malformed.json");
    std::fs::write(&malformed, "{not json")
        .map_err(|err| format!("write malformed summary fixture: {err}"))?;
    let malformed_err = read_repo_exposure_summary_artifact(&malformed)
        .expect_err("malformed summary artifacts should fail clearly");
    assert!(malformed_err.contains("failed to parse repo exposure summary artifact"));
    Ok(())
}

#[test]
fn receipt_json_escapes_and_includes_metadata() {
    let record = ReceiptRecord {
        file: "shape.json".to_string(),
        command: "cargo xtask shape".to_string(),
        status: "passed".to_string(),
        reports: vec![
            "target/ripr/reports/shape.md".to_string(),
            "target/ripr/reports/pr-summary.md".to_string(),
        ],
    };
    let mut git = BTreeMap::new();
    git.insert("branch".to_string(), "main".to_string());
    git.insert("commit".to_string(), "abc123".to_string());

    let json = receipt_json(&record, &git);

    assert!(json.contains("\"schema_version\": \"0.1\""));
    assert!(json.contains("\"command\": \"cargo xtask shape\""));
    assert!(json.contains("\"status\": \"passed\""));
    assert!(json.contains("\"branch\": \"main\""));
    assert!(json.contains("\"commit\": \"abc123\""));
    assert!(json.contains("target/ripr/reports/shape.md"));
}

#[test]
fn receipt_json_escapes_quotes_and_backslashes() {
    let record = ReceiptRecord {
        file: "report.json".to_string(),
        command: "cargo run -- check --diff \"path\\to\\file.diff\"".to_string(),
        status: "passed".to_string(),
        reports: vec![],
    };
    let git = BTreeMap::new();

    let json = receipt_json(&record, &git);

    assert!(json.contains("cargo run -- check --diff \\\"path\\\\to\\\\file.diff\\\""));
}

#[test]
fn receipt_specs_cover_required_gates() {
    let files = receipt_specs()
        .into_iter()
        .map(|spec| spec.file.to_string())
        .collect::<BTreeSet<_>>();

    assert!(files.contains("shape.json"));
    assert!(files.contains("fix-pr.json"));
    assert!(files.contains("ci-fast.json"));
    assert!(files.contains("check-pr.json"));
    assert!(files.contains("fixtures.json"));
    assert!(files.contains("goldens.json"));
    assert!(files.contains("test-oracles.json"));
    assert!(files.contains("badge-artifacts.json"));
    assert!(files.contains("repo-badge-artifacts.json"));
    assert!(files.contains("dogfood.json"));
    assert!(files.contains("metrics.json"));
    assert!(known_xtask_command("receipts"));
}

#[test]
fn receipt_specs_returns_expected_reports() {
    let specs = receipt_specs();
    assert!(!specs.is_empty());

    // Spot-check that we have some expected specs
    let spec_commands: Vec<&str> = specs.iter().map(|s| s.command).collect();
    assert!(spec_commands.contains(&"cargo xtask shape"));
    assert!(spec_commands.contains(&"cargo xtask check-pr"));
}

#[test]
fn receipt_status_aggregates_report_statuses() {
    assert_eq!(receipt_status_from_reports(&[]), "missing");
    assert_eq!(
        receipt_status_from_reports(&["target/ripr/reports/not-present.md".to_string()]),
        "missing"
    );

    let pass = ReceiptRecord {
        file: "check-pr.json".to_string(),
        command: "cargo xtask check-pr".to_string(),
        status: "passed".to_string(),
        reports: vec!["target/ripr/reports/check-pr.md".to_string()],
    };
    let git = BTreeMap::from([
        ("branch".to_string(), "test-branch".to_string()),
        ("commit".to_string(), "abc123".to_string()),
    ]);
    let json = receipt_json(&pass, &git);

    assert!(json.contains("\"schema_version\": \"0.1\""));
    assert!(json.contains("\"command\": \"cargo xtask check-pr\""));
    assert!(json.contains("\"status\": \"passed\""));
    assert!(json.contains("\"branch\": \"test-branch\""));
    assert!(json.contains("target/ripr/reports/check-pr.md"));
}

#[test]
fn receipt_status_from_reports_detects_missing_files() {
    with_temp_cwd("receipt-status-missing", |_root| {
        let result = receipt_status_from_reports(&["missing.md".to_string()]);
        assert_eq!(result, "missing");
    });
}

// ============================================================================
// Receipt and status utility tests
// ============================================================================

#[test]
fn receipt_status_vocabulary_is_locked() {
    assert!(is_receipt_status("passed"));
    assert!(is_receipt_status("warn"));
    assert!(is_receipt_status("failed"));
    assert!(is_receipt_status("missing"));

    assert!(!is_receipt_status("pass"));
    assert!(!is_receipt_status("warning"));
    assert!(!is_receipt_status("fail"));
    assert!(!is_receipt_status("unknown"));
    assert!(!is_receipt_status(""));
}

#[test]
fn receipt_statuses_accept_expected_values() {
    assert!(is_receipt_status("passed"));
    assert!(is_receipt_status("warn"));
    assert!(is_receipt_status("failed"));
    assert!(is_receipt_status("missing"));
    assert!(!is_receipt_status("unknown"));
}

#[test]
fn release_pin_ruleset_requires_fully_qualified_tag_ref() -> Result<(), String> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest should have a repository parent".to_string())?;
    let jq_root = temp_dir("release-pin-ruleset-jq");
    let result = release_pin_ruleset_contract(repo_root, &jq_root);
    ignore_remove_dir_all(&jq_root);
    result
}

#[test]
fn release_server_archive_rejects_format_before_repository_bound_staging() -> Result<(), String> {
    with_temp_cwd("release-server-archive-invalid-format", |_root| {
        let executable = if cfg!(windows) { "ripr.exe" } else { "ripr" };
        let args = vec![
            "--version".to_string(),
            "v1.2.3".to_string(),
            "--target".to_string(),
            "x86_64-unknown-linux-gnu".to_string(),
            "--executable".to_string(),
            executable.to_string(),
            "--archive".to_string(),
            "bad-format".to_string(),
        ];

        let Err(err) = super::super::release_server_archive(&args) else {
            return Err("unsupported archive format should fail".to_string());
        };
        assert!(err.contains("unsupported release server archive format"));
        Ok(())
    })
}

#[test]
fn release_server_assets_selects_versioned_archives() -> Result<(), String> {
    let root = temp_dir("release-server-assets");
    write(
        &root.join("ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"),
        "archive",
    );
    write(
        &root.join("ripr-server-v1.2.3-x86_64-pc-windows-msvc.zip"),
        "archive",
    );
    write(
        &root.join("ripr-server-v1.2.4-aarch64-apple-darwin.tar.gz"),
        "other",
    );
    write(
        &root.join("ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz.sha256"),
        "sha",
    );

    let assets = super::super::release_server_assets(&root, "1.2.3")?;

    assert_eq!(
        assets,
        vec![
            super::super::ReleaseServerAsset {
                target: "x86_64-pc-windows-msvc".to_string(),
                file_name: "ripr-server-v1.2.3-x86_64-pc-windows-msvc.zip".to_string(),
            },
            super::super::ReleaseServerAsset {
                target: "x86_64-unknown-linux-gnu".to_string(),
                file_name: "ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz".to_string(),
            },
        ]
    );
    Ok(())
}

#[test]
fn release_server_helpers_match_workflow_arguments() -> Result<(), String> {
    let args = vec![
        "--version=v1.2.3".to_string(),
        "--target".to_string(),
        "x86_64-unknown-linux-gnu".to_string(),
    ];

    assert_eq!(
        super::super::required_release_arg(&args, "version", "MISSING_ENV")?,
        "v1.2.3"
    );
    assert_eq!(
        super::super::required_release_arg(&args, "target", "MISSING_ENV")?,
        "x86_64-unknown-linux-gnu"
    );
    assert_eq!(super::super::normalize_release_version(" v1.2.3 "), "1.2.3");
    assert_eq!(
        super::super::release_server_readme("1.2.3"),
        "ripr server 1.2.3\n\nThis archive contains the ripr executable used by the VS Code/Open VSX\nextension. It is distributed under MIT OR Apache-2.0."
    );
    Ok(())
}

#[test]
fn release_server_manifest_embeds_the_editor_distribution_descriptor() -> Result<(), String> {
    with_temp_cwd("release-server-descriptor", |root| {
        let dist = root.join("dist");
        write(
            &dist.join("ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"),
            "linux",
        );
        write(
            &dist.join("ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz.sha256"),
            "linux-sha\n",
        );

        let args = vec![
            "--version".to_string(),
            "v1.2.3".to_string(),
            "--repository".to_string(),
            "EffortlessMetrics/ripr".to_string(),
        ];

        super::super::release_server_manifest(&args)?;

        let descriptor_path = root
            .join("editors")
            .join("vscode")
            .join("src")
            .join("serverDescriptor.ts");
        let descriptor = fs::read_to_string(&descriptor_path)
            .map_err(|err| format!("read editor distribution descriptor: {err}"))?;
        let manifest_sha256 =
            super::super::sha256_file(&dist.join("ripr-server-manifest-v1.2.3.json"))?;
        assert!(
            descriptor.contains(&format!(
                "{{ generation: \"1.2.3\", manifestSha256: \"{manifest_sha256}\" }}"
            )),
            "the descriptor must embed the exact generation and the manifest's raw-byte digest"
        );
        assert!(
            descriptor.contains("SERVER_DISTRIBUTION_DESCRIPTORS"),
            "the descriptor must declare the editor's admitted-descriptor list"
        );
        Ok(())
    })
}

#[test]
fn release_server_manifest_rejects_prerelease_version_before_descriptor() -> Result<(), String> {
    // The editor descriptor admits a distribution generation
    // (MAJOR.MINOR.PATCH) and the downloader only ever looks the generation
    // up, so a prerelease or build-metadata version would embed a descriptor
    // row no request can match and silently leave the packaged RC extension
    // without a fallback.
    // A four-component version can never match the downloader's
    // three-component generation lookup either.
    for version in ["1.2.3-rc.1", "1.2.3+meta", "1.2.3.4"] {
        with_temp_cwd("release-server-descriptor-prerelease", |root| {
            let dist = root.join("dist");
            let asset_name = format!("ripr-server-v{version}-x86_64-unknown-linux-gnu.tar.gz");
            write(&dist.join(&asset_name), "linux");
            write(&dist.join(format!("{asset_name}.sha256")), "linux-sha\n");

            let args = vec![
                "--version".to_string(),
                format!("v{version}"),
                "--repository".to_string(),
                "EffortlessMetrics/ripr".to_string(),
            ];

            let Err(err) = super::super::release_server_manifest(&args) else {
                return Err(format!(
                    "prerelease/build-metadata version `{version}` must not write the descriptor"
                ));
            };
            assert!(
                err.contains("MAJOR.MINOR.PATCH"),
                "the error must name the required distribution generation form; got: {err}"
            );

            // The guard sits exactly at the descriptor boundary: the
            // generation-keyed run artifacts exist, the descriptor does not.
            assert!(
                dist.join(format!("ripr-server-manifest-v{version}.json"))
                    .exists(),
                "the manifest write precedes the refused descriptor write"
            );
            let descriptor_path = root
                .join("editors")
                .join("vscode")
                .join("src")
                .join("serverDescriptor.ts");
            assert!(
                !descriptor_path.exists(),
                "a prerelease/build-metadata version must never write the editor descriptor"
            );
            Ok(())
        })?;
    }
    Ok(())
}

#[test]
fn release_server_manifest_writes_assets_and_checksums() -> Result<(), String> {
    with_temp_cwd("release-server-manifest", |root| {
        let dist = root.join("dist");
        write(
            &dist.join("ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"),
            "linux",
        );
        write(
            &dist.join("ripr-server-v1.2.3-x86_64-pc-windows-msvc.zip"),
            "windows",
        );
        write(
            &dist.join("ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz.sha256"),
            "linux-sha\n",
        );
        write(
            &dist.join("ripr-server-v1.2.3-x86_64-pc-windows-msvc.zip.sha256"),
            "windows-sha\n",
        );
        // Simulate a reused `dist/` from a pre-rename run: a stale legacy
        // `checksums.txt` must be cleaned up, never hashed into the new
        // sidecar, and never surfaced in the manifest.
        write(&dist.join("checksums.txt"), "stale-legacy-sidecar\n");

        let args = vec![
            "--version".to_string(),
            "v1.2.3".to_string(),
            "--repository".to_string(),
            "EffortlessMetrics/ripr".to_string(),
        ];

        super::super::release_server_manifest(&args)?;

        let manifest_path = dist.join("ripr-server-manifest-v1.2.3.json");
        let manifest_text = fs::read_to_string(&manifest_path)
            .map_err(|err| format!("read release manifest: {err}"))?;
        let manifest: Value = serde_json::from_str(&manifest_text)
            .map_err(|err| format!("parse release manifest: {err}"))?;
        assert_eq!(manifest["version"], "1.2.3");
        assert_eq!(
            manifest["assets"]["x86_64-unknown-linux-gnu"]["url"],
            "https://github.com/EffortlessMetrics/ripr/releases/download/v1.2.3/ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            manifest["assets"]["x86_64-pc-windows-msvc"]["sha256"],
            "windows-sha"
        );

        let checksums = fs::read_to_string(dist.join("SHA256SUMS"))
            .map_err(|err| format!("read SHA256SUMS: {err}"))?;
        assert!(checksums.contains("  ripr-server-v1.2.3-x86_64-unknown-linux-gnu.tar.gz"));
        assert!(checksums.contains("  ripr-server-v1.2.3-x86_64-pc-windows-msvc.zip"));
        assert!(checksums.contains("  ripr-server-manifest-v1.2.3.json"));
        assert!(!checksums.contains(".sha256"));
        // Neither sidecar hashes itself or the stale legacy file.
        assert!(
            !checksums.contains("SHA256SUMS"),
            "SHA256SUMS must not hash itself"
        );
        assert!(
            !checksums.contains("checksums.txt"),
            "the stale legacy checksums.txt must not be hashed into SHA256SUMS"
        );
        // The reused-dist upgrade path leaves only the new sidecar behind.
        assert!(
            !dist.join("checksums.txt").exists(),
            "stale legacy checksums.txt must be removed on a reused dist/ run"
        );
        // The JSON manifest keys are release targets only — never a sidecar.
        let assets = manifest["assets"]
            .as_object()
            .ok_or("manifest assets is not an object")?;
        assert_eq!(
            assets.len(),
            2,
            "only the two server targets are manifested"
        );
        assert!(!assets.contains_key("SHA256SUMS"));
        assert!(!assets.contains_key("checksums.txt"));
        Ok(())
    })
}

#[test]
fn release_workflows_publish_only_by_explicit_dispatch() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let read = |name: &str| {
        std::fs::read_to_string(root.join(".github/workflows").join(name))
            .map_err(|error| format!("failed to read {name}: {error}"))
    };
    for name in ["release-server-binaries.yml", "publish-extension.yml"] {
        let workflow = read(name)?;
        let triggers = workflow_trigger_keys(&workflow)
            .ok_or_else(|| format!("{name} has no block-form `on:` declaration"))?;
        if triggers != ["workflow_dispatch"] {
            return Err(format!(
                "{name} must run only on workflow_dispatch, found {triggers:?}"
            ));
        }
    }
    let extension = read("publish-extension.yml")?;
    for input in ["publish_vs_marketplace", "publish_open_vsx"] {
        let declared = extension
            .split_once(&format!("\n      {input}:\n"))
            .map(|(_, rest)| rest)
            .ok_or_else(|| format!("publish-extension.yml has no {input} input"))?;
        // Only this input's own mapping: stop at the first line indented no
        // deeper than the input key, so a later input's default cannot match.
        let default = declared
            .lines()
            .take_while(|line| line.trim().is_empty() || line.starts_with("        "))
            .find_map(|line| line.trim().strip_prefix("default: "))
            .ok_or_else(|| format!("{input} has no default"))?;
        if default != "\"false\"" {
            return Err(format!(
                "{input} must default to \"false\", found {default}"
            ));
        }
        if !extension.contains(&format!(
            "if: ${{{{ github.event.inputs.{input} == 'true' }}}}"
        )) {
            return Err(format!("{input} must gate its job on an explicit 'true'"));
        }
    }
    // One dispatch authorizes exactly one channel: zero or two must fail
    // before anything packages, publishes or touches the Release.
    if !extension.contains("true/false | false/true) ;;") {
        return Err("publish-extension.yml must admit exactly one marketplace channel".to_string());
    }
    for job in ["package", "server-assets-ready", "attach-release-asset"] {
        let needs = extension
            .split_once(&format!("\n  {job}:\n"))
            .map(|(_, rest)| {
                rest.lines()
                    .take_while(|line| {
                        line.is_empty()
                            || line.starts_with("    ")
                            || line.trim_start().starts_with('#')
                    })
                    .find_map(|line| line.trim().strip_prefix("needs: "))
                    .unwrap_or("")
                    .to_string()
            })
            .ok_or_else(|| format!("publish-extension.yml has no {job} job"))?;
        if !needs.contains("admit-dispatch") {
            return Err(format!("{job} must need admit-dispatch, found `{needs}`"));
        }
    }
    if extension
        .lines()
        .any(|line| line.contains("gh release") && line.contains("${{"))
    {
        return Err(
            "publish-extension.yml must pass the release ref to `gh release` through env"
                .to_string(),
        );
    }
    if extension.contains("gh release create") || extension.contains("--clobber") {
        return Err(
            "publish-extension.yml must not create the GitHub Release or replace an asset"
                .to_string(),
        );
    }
    Ok(())
}

#[test]
fn repository_owned_review_thread_mutation_policy_rejects_delegated_action() -> Result<(), String> {
    with_temp_cwd("workflow-delegated-review-resolver", |root| {
        let action = root.join(".github/actions/blind-review-resolver/action.yml");
        let script = root.join(".github/scripts/review-resolver.sh");
        write(
            &action,
            "name: blind resolver\nruns:\n  using: composite\n  steps:\n    - run: bash .github/scripts/review-resolver.sh\n",
        );
        write(
            &script,
            "#!/usr/bin/env bash\nresolver='resolve'+'ReviewThread'\ngh api graphql \"$resolver\"\n",
        );

        let violations = repository_owned_review_thread_mutation_violations()?;
        assert!(
            violations
                .iter()
                .any(|violation| { violation.contains(".github/scripts/review-resolver.sh") })
        );
        Ok(())
    })
}

#[test]
fn retired_command_message_keeps_scope_records_out_of_live_selection() {
    let message = unknown_command_message("goals");
    assert!(message.contains("GitHub issues, PRs, and checks"));
    assert!(message.contains("implementation slices are scope records"));
    assert!(!message.contains(".allow/spec-system/slices/"));
}

#[test]
fn reverse_direction_violation_is_silent_at_baseline() {
    let unregistered = [(
        "RIPR-SPEC-1234".to_string(),
        "docs/specs/RIPR-SPEC-1234-alpha.md".to_string(),
    )];
    assert_eq!(
        reverse_direction_unregistered_violation(&unregistered, 1, DOC_ARTIFACT_LEDGER),
        None,
        "the ratchet must not fire at the baseline count"
    );
}

#[test]
fn reverse_direction_violation_names_newest_ids_before_truncating() -> Result<(), String> {
    let unregistered: Vec<(String, String)> = (1..=UNREGISTERED_SPEC_MESSAGE_LIMIT + 2)
        .map(|n| {
            let id = format!("RIPR-SPEC-{n:04}");
            let path = format!("docs/specs/{id}-sample.md");
            (id, path)
        })
        .collect();
    let message = reverse_direction_unregistered_violation(&unregistered, 0, DOC_ARTIFACT_LEDGER)
        .ok_or_else(|| "any unregistered spec exceeds baseline 0".to_string())?;
    let newest = format!("RIPR-SPEC-{:04}", UNREGISTERED_SPEC_MESSAGE_LIMIT + 2);
    if !message.contains(&newest) {
        return Err(format!(
            "truncation must keep the newest spec id visible, got {message}"
        ));
    }
    if message.contains("RIPR-SPEC-0001 (") {
        return Err(format!(
            "oldest spec ids should fall into the truncation remainder, got {message}"
        ));
    }
    let remaining = 2;
    if !message.contains(&format!("and {remaining} more")) {
        return Err(format!("must mark truncated remainder, got {message}"));
    }
    Ok(())
}

#[test]
fn reverse_direction_violation_names_unregistered_spec_ids_and_paths() -> Result<(), String> {
    let unregistered = [
        (
            "RIPR-SPEC-1234".to_string(),
            "docs/specs/RIPR-SPEC-1234-alpha.md".to_string(),
        ),
        (
            "RIPR-SPEC-1235".to_string(),
            "docs/specs/RIPR-SPEC-1235-beta.md".to_string(),
        ),
    ];
    let message = reverse_direction_unregistered_violation(&unregistered, 1, DOC_ARTIFACT_LEDGER)
        .ok_or_else(|| "two unregistered specs exceed baseline 1".to_string())?;
    if !message.contains("RIPR-SPEC-1234 (docs/specs/RIPR-SPEC-1234-alpha.md)") {
        return Err(format!(
            "must name the first unregistered id and path, got {message}"
        ));
    }
    if !message.contains("RIPR-SPEC-1235 (docs/specs/RIPR-SPEC-1235-beta.md)") {
        return Err(format!(
            "must name the second unregistered id and path, got {message}"
        ));
    }
    if !message.contains("unregistered:") {
        return Err(format!(
            "must label the named ids, not only the excess count, got {message}"
        ));
    }
    if !(message.contains("baseline: 1") && message.contains("current: 2")) {
        return Err(format!("must retain the ratchet counts, got {message}"));
    }
    Ok(())
}

#[test]
fn route_quality_cross_validates_with_attempt_ledger_repair_route_quality() -> Result<(), String> {
    // Build a minimal attempt ledger value with two attempts and check that
    // route_quality.repair_route_quality_latest matches repair_route_quality in the
    // swarm-attempt-ledger, differing only by the added success_rate field.
    let ledger_value = serde_json::json!({
        "schema_version": "0.1",
        "tool": "ripr",
        "report": "swarm-attempt-ledger",
        "run_status": "full",
        "runtime_status": { "state": "full", "downstream_consumable": true },
        "repair_route_quality": [],
        "language_repair_route_quality": [],
        "historical_repair_route_quality": [],
        "historical_language_repair_route_quality": [],
        "attempts": [
            {
                "packet_id": "packet-a",
                "canonical_gap_id": "gap-a",
                "attempt_id": "attempt-a-1",
                "repair_kind": "add_assertion",
                "outcome": "evidence_improved",
                "actor_kind": "agent",
                "verify_command": "cargo test",
                "verify_result": "pass",
                "receipt_state": "receipt_present",
                "reason": "test"
            },
            {
                "packet_id": "packet-b",
                "canonical_gap_id": "gap-b",
                "attempt_id": "attempt-b-1",
                "repair_kind": "add_assertion",
                "outcome": "evidence_unchanged",
                "actor_kind": "agent",
                "verify_command": "cargo test",
                "verify_result": "pass",
                "receipt_state": "receipt_present",
                "reason": "test"
            }
        ],
        "latest_attempts": []
    });

    let route_quality_report = ripr_swarm_route_quality_from_ledger_value(
        "unix_ms:2".to_string(),
        "test".to_string(),
        "read".to_string(),
        None,
        Some(&ledger_value),
    );
    let attempts = super::super::ripr_swarm_attempt_ledger_entries_from_value(&ledger_value);
    let latest_attempts = ripr_swarm_attempt_ledger_latest_attempts(&attempts);
    let ledger_rows = ripr_swarm_attempt_ledger_repair_route_quality(&latest_attempts);

    // The route-quality report latest rows must match the ledger rows (same counts)
    assert_eq!(
        route_quality_report.repair_route_quality_latest.len(),
        ledger_rows.len()
    );
    for (rq_row, ledger_row) in route_quality_report
        .repair_route_quality_latest
        .iter()
        .zip(ledger_rows.iter())
    {
        assert_eq!(rq_row.repair_kind, ledger_row.repair_kind);
        assert_eq!(rq_row.attempted, ledger_row.attempted);
        assert_eq!(rq_row.improved, ledger_row.improved);
        assert_eq!(rq_row.unchanged, ledger_row.unchanged);
        assert_eq!(rq_row.resolved, ledger_row.resolved);
        assert_eq!(rq_row.regressed, ledger_row.regressed);
        assert_eq!(rq_row.attempted_no_receipt, ledger_row.attempted_no_receipt);
        assert_eq!(rq_row.receipt_present, ledger_row.receipt_present);
        assert_eq!(
            rq_row.missing_verify_result,
            ledger_row.missing_verify_result
        );
        assert_eq!(rq_row.expected_unchanged, ledger_row.expected_unchanged);
        assert_eq!(rq_row.unknown, ledger_row.unknown);
    }

    // Verify the JSON output includes success_rate for each row and no forbidden keys
    let json_str = ripr_swarm_route_quality_report_json(&route_quality_report)?;
    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|err| err.to_string())?;
    let rq_latest = value["repair_route_quality_latest"]
        .as_array()
        .ok_or("repair_route_quality_latest must be an array")?;
    assert!(
        !rq_latest.is_empty(),
        "expected non-empty route quality rows"
    );
    for row in rq_latest {
        assert!(
            row.get("repair_kind_success_rate").is_some(),
            "each row must have repair_kind_success_rate"
        );
    }
    assert!(value.get("top_orphan_receipt_sources").is_none());
    assert!(value.get("stale_receipt_count").is_none());
    assert!(value.get("top_limitation_routes").is_none());
    Ok(())
}

#[test]
fn route_quality_empty_input_produces_empty_not_zero_filled_report() -> Result<(), String> {
    let report = ripr_swarm_route_quality_from_ledger_value(
        "unix_ms:1".to_string(),
        "target/ripr/reports/swarm-attempt-ledger.json".to_string(),
        "missing".to_string(),
        Some("no ledger found".to_string()),
        None,
    );
    assert_eq!(report.repair_route_quality_latest, vec![]);
    assert_eq!(report.repair_route_quality_historical, vec![]);
    assert_eq!(report.language_repair_route_quality_latest, vec![]);
    assert_eq!(report.language_repair_route_quality_historical, vec![]);
    assert_eq!(report.status, "blocked");
    let json_str = ripr_swarm_route_quality_report_json(&report)?;
    let value: serde_json::Value =
        serde_json::from_str(&json_str).map_err(|err| err.to_string())?;
    assert_eq!(value["repair_route_quality_latest"], serde_json::json!([]));
    assert_eq!(
        value["repair_route_quality_historical"],
        serde_json::json!([])
    );
    assert_eq!(
        value["language_repair_route_quality_latest"],
        serde_json::json!([])
    );
    assert_eq!(
        value["language_repair_route_quality_historical"],
        serde_json::json!([])
    );
    // Confirm forbidden keys are absent
    assert!(value.get("top_orphan_receipt_sources").is_none());
    assert!(value.get("stale_receipt_count").is_none());
    assert!(value.get("top_limitation_routes").is_none());
    Ok(())
}

#[test]
fn route_quality_success_rate_computed_when_attempted_nonzero() {
    let row = super::super::RiprSwarmRepairRouteQualityRow {
        repair_kind: "add_call_observer".to_string(),
        attempted: 4,
        improved: 2,
        resolved: 1,
        expected_unchanged: 0,
        unchanged: 1,
        ..super::super::RiprSwarmRepairRouteQualityRow::default()
    };
    // success = improved(2) + resolved(1) + expected_unchanged(0) = 3; rate = 3/4 = 0.75
    let rate = ripr_swarm_repair_route_quality_success_rate(&row);
    assert_eq!(rate, serde_json::json!(0.75));
    assert!(rate != serde_json::Value::Null);
    // Confirm the JSON value is a number, not null
    assert!(rate.is_number());
}

// RIPR-SPEC-0080 route-quality standalone report tests

#[test]
fn route_quality_success_rate_is_null_when_attempted_is_zero() {
    let row = super::super::RiprSwarmRepairRouteQualityRow {
        repair_kind: "add_call_observer".to_string(),
        attempted: 0,
        improved: 0,
        resolved: 0,
        expected_unchanged: 0,
        ..super::super::RiprSwarmRepairRouteQualityRow::default()
    };
    assert_eq!(
        ripr_swarm_repair_route_quality_success_rate(&row),
        serde_json::Value::Null
    );
}

#[test]
fn routed_rust_contract_catches_deadline_bypass_with_stray_occurrences() {
    // #2230 review: a global `timeout-minutes:` occurrence count passes even
    // when a named job lost its deadline but stray tokens (comments, other
    // jobs, duplicates) keep the total at eight. The per-job check must fire
    // on the job that lost its deadline.
    let workflow = r#"
name: Routed Rust Small
jobs:
  route:
    runs-on: ubuntu-latest
    timeout-minutes: 10
  detect-docs-only:
    runs-on: ubuntu-latest
    timeout-minutes: 10
  rust-cx43:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    # timeout-minutes: 60 (stray occurrence in a comment)
  rust-cpx42:
    runs-on: ubuntu-latest
    timeout-minutes: 60
    timeout-minutes: 60
  rust-cx53:
    runs-on: ubuntu-latest
    timeout-minutes: 60
  rust-github:
    runs-on: ubuntu-latest
    timeout-minutes: 90
  docs-gate:
    runs-on: ubuntu-latest
    timeout-minutes: 20
  result:
    runs-on: ubuntu-latest
"#;
    let violations = routed_rust_workflow_contract_violations(workflow, None, None);
    assert!(
        violations.iter().any(|violation| violation
            .contains("job `result` must set an explicit `timeout-minutes` job deadline")),
        "missing per-job deadline must fire even with 8 stray occurrences: {violations:?}"
    );
    assert!(
        !violations
            .iter()
            .any(|violation| violation.contains("job `route` must set an explicit")),
        "a job that kept its deadline must not be flagged: {violations:?}"
    );
}
