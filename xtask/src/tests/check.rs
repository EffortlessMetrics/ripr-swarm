use super::*;

#[test]
fn check_pr_temp_env_points_to_repo_local_target_tmp() -> Result<(), String> {
    with_temp_cwd("check-pr-temp-env", |_root| -> Result<(), String> {
        let envs = super::super::check_pr_temp_env()?;
        let expected_path = std::env::current_dir()
            .map_err(|err| format!("current dir: {err}"))?
            .join("target")
            .join("tmp")
            .join("check-pr");
        if !expected_path.is_dir() {
            return Err(format!(
                "check-pr temp dir should exist at {}",
                expected_path.display()
            ));
        }
        let expected = expected_path.to_string_lossy().into_owned();

        for name in ["TEMP", "TMP", "TMPDIR"] {
            let Some((_, value)) = envs.iter().find(|(env_name, _)| env_name == name) else {
                return Err(format!("missing {name} in check-pr temp envs"));
            };
            if value != &expected {
                return Err(format!("{name} should point to {expected}, got {value}"));
            }
        }

        Ok(())
    })
}

#[test]
fn check_status_enum_distinguishes_pass_warn_fail() {
    let statuses = [CheckStatus::Pass, CheckStatus::Warn, CheckStatus::Fail];
    for status in &statuses {
        let debug_str = format!("{:?}", status);
        assert!(!debug_str.is_empty());
    }
}

#[test]
fn check_violation_structure_holds_check_path_and_message() {
    let violation = CheckViolation {
        check: "test-coverage".to_string(),
        path: Some(PathBuf::from("src/lib.rs")),
        line: Some(42),
        severity: CheckStatus::Fail,
        category: "coverage".to_string(),
        message: "Missing test".to_string(),
        why_it_matters: "Ensures code quality".to_string(),
        fix_kind: FixKind::AuthorDecisionRequired,
        suggested_commands: vec!["cargo test".to_string()],
        suggested_patch: Some("patch content".to_string()),
        exception_template: None,
    };
    assert_eq!(violation.check, "test-coverage");
    assert_eq!(violation.line, Some(42));
    assert_eq!(violation.message, "Missing test");
}

#[test]
fn check_report_aggregates_violations_and_status() {
    let report = CheckReport {
        check: "coverage".to_string(),
        status: CheckStatus::Fail,
        violations: vec![],
    };
    assert_eq!(report.check, "coverage");
    assert!(
        matches!(report.status, CheckStatus::Fail),
        "expected Fail status"
    );
}

#[test]
fn check_droid_action_refs_rejects_non_sha_refs() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      - uses: EffortlessMetrics/droid-action-safe@v5\n",
    );
    assert_eq!(violations.len(), 2);
    assert!(violations[0].contains("immutable commit SHA"));
}

#[test]
fn check_droid_action_refs_accepts_sha_refs() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      - uses: EffortlessMetrics/droid-action-safe@7c1377ccbacddc95560d1570547a5baa51de01ec\n",
    );
    assert!(violations.is_empty());
}

#[test]
fn check_droid_action_refs_rejects_unsafe_upstream_ref() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      - uses: Factory-AI/droid-action@e3d1f5e7861c36fe4a9c4dca3edec87b964b2bc4\n",
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("unsafe upstream Droid action"))
    );
}

#[test]
fn check_droid_action_refs_rejects_unapproved_safe_sha() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      - uses: EffortlessMetrics/droid-action-safe@e3d1f5e7861c36fe4a9c4dca3edec87b964b2bc4\n",
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("approved safe action ref"))
    );
}

#[test]
fn check_droid_action_refs_ignores_commented_uses() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      # - uses: Factory-AI/droid-action@v5\n",
    );
    assert!(violations.is_empty());
}

#[test]
fn check_droid_action_refs_rejects_main_ref() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      - uses: Factory-AI/droid-action@main\n",
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("immutable commit SHA"))
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("unsafe upstream Droid action"))
    );
}

#[test]
fn check_droid_common_flags_missing_same_repo_guard() {
    let mut violations = Vec::new();
    let yaml = "\
review_model: \"custom:MiniMax-M3-0\"
security_model: \"custom:MiniMax-M3-0\"
";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(violations.iter().any(|v| v.contains("same-repo guard")));
}

#[test]
fn check_droid_common_flags_anthropic_tokens_on_active_lines_only() {
    let mut violations = Vec::new();
    let yaml = "\
# ANTHROPIC_AUTH_TOKEN: something
ANTHROPIC_AUTH_TOKEN: \"\"
ANTHROPIC_BASE_URL: \"\"
review_model: \"custom:MiniMax-M3-0\"
security_model: \"custom:MiniMax-M3-0\"
";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(!violations.iter().any(|v| v.contains("ANTHROPIC")));
}

#[test]
fn check_droid_common_flags_active_anthropic_tokens() {
    let mut violations = Vec::new();
    let yaml = "\
ANTHROPIC_AUTH_TOKEN: something
review_model: \"custom:MiniMax-M3-0\"
security_model: \"custom:MiniMax-M3-0\"
";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(violations.iter().any(|v| v.contains("ANTHROPIC")));
}

#[test]
fn check_droid_common_flags_settings_input_only_when_active() {
    let mut violations = Vec::new();
    let yaml = "\
# settings: |
review_model: \"custom:MiniMax-M3-0\"
";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(!violations.iter().any(|v| v.contains("settings:")));
}

#[test]
fn check_droid_common_flags_active_settings_input() {
    let mut violations = Vec::new();
    let yaml = "\
settings: |
  some: config
";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(violations.iter().any(|v| v.contains("settings:")));
}

#[test]
fn check_droid_common_flags_show_full_output_only_when_active() {
    let mut violations = Vec::new();
    let yaml = "# show_full_output: true\nshow_full_output: false\n";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(!violations.iter().any(|v| v.contains("show_full_output")));
}

#[test]
fn check_droid_common_flags_missing_review_model() {
    let mut violations = Vec::new();
    let yaml = "security_model: \"custom:MiniMax-M3-0\"\n";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("review_model must be custom:MiniMax-M3-0"))
    );
}

#[test]
fn check_droid_common_flags_missing_security_model() {
    let mut violations = Vec::new();
    let yaml = "review_model: \"custom:MiniMax-M3-0\"\n";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("security_model must be custom:MiniMax-M3-0"))
    );
}

#[test]
fn check_droid_common_flags_missing_settings_local_json() {
    let mut violations = Vec::new();
    let yaml = "review_model: \"custom:MiniMax-M3-0\"\n";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(violations.iter().any(|v| v.contains("settings.json")));
}

#[test]
fn check_droid_common_flags_missing_literal_minimax_key() {
    let mut violations = Vec::new();
    let yaml = "review_model: \"custom:MiniMax-M3-0\"\n";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(violations.iter().any(|v| v.contains("${MINIMAX_API_KEY}")));
}

#[test]
fn check_droid_common_flags_active_anthropic_base_url() {
    let mut violations = Vec::new();
    let yaml = "ANTHROPIC_BASE_URL: https://example.com\n";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(violations.iter().any(|v| v.contains("ANTHROPIC")));
}

#[test]
fn check_droid_common_flags_active_show_full_output() {
    let mut violations = Vec::new();
    let yaml = "show_full_output: true\n";
    check_droid_common(&mut violations, "test.yml", yaml, true, true);
    assert!(violations.iter().any(|v| v.contains("show_full_output")));
}

#[test]
fn check_droid_common_flags_missing_show_full_output_false() {
    let mut violations = Vec::new();
    let yaml = "\
security_model: \"custom:MiniMax-M3-0\"
$HOME/.factory/settings.json
${MINIMAX_API_KEY}
";
    check_droid_common(&mut violations, "test.yml", yaml, false, false);
    assert!(violations.iter().any(|v| v.contains("show_full_output")));
}

#[test]
fn check_droid_common_flags_missing_debug_artifact_disable() {
    let mut violations = Vec::new();
    let yaml = "\
security_model: \"custom:MiniMax-M3-0\"
$HOME/.factory/settings.json
${MINIMAX_API_KEY}
show_full_output: false
";
    check_droid_common(&mut violations, "test.yml", yaml, false, false);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("upload_debug_artifacts: false"))
    );
}

#[test]
fn check_droid_common_flags_missing_github_cli_bootstrap() {
    let mut violations = Vec::new();
    let yaml = "\
security_model: \"custom:MiniMax-M3-0\"
$HOME/.factory/settings.json
${MINIMAX_API_KEY}
show_full_output: false
upload_debug_artifacts: false
";
    check_droid_common(&mut violations, "test.yml", yaml, false, false);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must check for GitHub CLI"))
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must install pinned GitHub CLI archive"))
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must verify pinned GitHub CLI SHA256"))
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must add the pinned GitHub CLI bin directory"))
    );
}

#[test]
fn check_droid_common_allows_security_scan_without_review_model_or_same_repo_guard() {
    let mut violations = Vec::new();
    let yaml = "\
security_model: \"custom:MiniMax-M3-0\"
$HOME/.factory/settings.json
${MINIMAX_API_KEY}
show_full_output: false
upload_debug_artifacts: false
";
    check_droid_common(&mut violations, "test.yml", yaml, false, false);
    assert!(
        !violations
            .iter()
            .any(|v| v.contains("review_model must be custom:MiniMax-M3-0"))
    );
    assert!(!violations.iter().any(|v| v.contains("same-repo guard")));
}

#[test]
fn check_droid_common_flags_missing_contents_read() {
    let mut violations = Vec::new();
    check_droid_common(
        &mut violations,
        "test.yml",
        "review_model: \"custom:MiniMax-M3-0\"\n",
        true,
        true,
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must set contents: read"))
    );
}

#[test]
fn check_droid_common_flags_contents_write() {
    let mut violations = Vec::new();
    check_droid_common(
        &mut violations,
        "test.yml",
        "contents: write\nreview_model: \"custom:MiniMax-M3-0\"\n",
        true,
        true,
    );
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must not grant contents: write"))
    );
}

#[test]
fn check_droid_security_scan_accepts_current_shape() {
    let violations = droid_security_scan_violations(valid_droid_security_scan_yaml());
    assert!(violations.is_empty());
}

#[test]
fn check_droid_security_scan_flags_non_repo_scoped_concurrency() {
    let yaml = valid_droid_security_scan_yaml().replace(
        "droid-security-scan-${{ github.repository }}",
        "droid-security-scan",
    );
    let violations = droid_security_scan_violations(&yaml);
    assert!(violations.iter().any(|v| v.contains("repository-scoped")));
}

#[test]
fn check_droid_security_scan_flags_cancel_in_progress() {
    let yaml = valid_droid_security_scan_yaml()
        .replace("cancel-in-progress: false", "cancel-in-progress: true");
    let violations = droid_security_scan_violations(&yaml);
    assert!(violations.iter().any(|v| v.contains("cancel-in-progress")));
}

#[test]
fn check_droid_security_scan_flags_missing_workflow_dispatch() {
    let yaml = valid_droid_security_scan_yaml().replace("  workflow_dispatch:\n", "");
    let violations = droid_security_scan_violations(&yaml);
    assert!(violations.iter().any(|v| v.contains("workflow_dispatch")));
}

#[test]
fn check_droid_security_scan_flags_wrong_weekly_schedule() {
    let yaml =
        valid_droid_security_scan_yaml().replace("cron: \"0 8 * * 1\"", "cron: \"0 8 * * *\"");
    let violations = droid_security_scan_violations(&yaml);
    assert!(violations.iter().any(|v| v.contains("weekly")));
}

#[test]
fn check_droid_security_scan_flags_missing_minimax_job_env() {
    let yaml = valid_droid_security_scan_yaml().replace(
        "      MINIMAX_API_KEY: ${{ secrets.MINIMAX_API_KEY }}\n",
        "",
    );
    let violations = droid_security_scan_violations(&yaml);
    assert!(violations.iter().any(|v| v.contains("MINIMAX_API_KEY")));
}

#[test]
fn check_droid_security_scan_flags_missing_schedule_mode() {
    let yaml = valid_droid_security_scan_yaml().replace(
        "security_scan_schedule: true",
        "security_scan_schedule: false",
    );
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("security_scan_schedule"))
    );
}

#[test]
fn check_droid_security_scan_flags_wrong_scan_days() {
    let yaml =
        valid_droid_security_scan_yaml().replace("security_scan_days: 7", "security_scan_days: 14");
    let violations = droid_security_scan_violations(&yaml);
    assert!(violations.iter().any(|v| v.contains("security_scan_days")));
}

#[test]
fn check_droid_security_scan_flags_wrong_threshold() {
    let yaml = valid_droid_security_scan_yaml().replace(
        "security_severity_threshold: medium",
        "security_severity_threshold: high",
    );
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("security_severity_threshold"))
    );
}

#[test]
fn check_droid_security_scan_flags_disabled_critical_block() {
    let yaml = valid_droid_security_scan_yaml().replace(
        "security_block_on_critical: true",
        "security_block_on_critical: false",
    );
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("security_block_on_critical"))
    );
}

#[test]
fn check_droid_security_scan_flags_enabled_high_block() {
    let yaml = valid_droid_security_scan_yaml().replace(
        "security_block_on_high: false",
        "security_block_on_high: true",
    );
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("security_block_on_high"))
    );
}

#[test]
fn check_droid_security_scan_flags_full_output() {
    let yaml = valid_droid_security_scan_yaml()
        .replace("show_full_output: false", "show_full_output: true");
    let violations = droid_security_scan_violations(&yaml);
    assert!(violations.iter().any(|v| v.contains("show_full_output")));
}

#[test]
fn check_droid_security_scan_flags_debug_artifact_upload() {
    let yaml = valid_droid_security_scan_yaml().replace(
        "upload_debug_artifacts: false",
        "upload_debug_artifacts: true",
    );
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("debug artifact upload"))
    );
}

#[test]
fn check_droid_security_scan_flags_contents_write() {
    let yaml = valid_droid_security_scan_yaml().replace("contents: read", "contents: write");
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must not grant contents: write"))
    );
}

#[test]
fn check_droid_security_scan_flags_id_token_write() {
    let yaml = valid_droid_security_scan_yaml()
        .replace("contents: read", "contents: read\n      id-token: write");
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must not grant id-token: write"))
    );
}

#[test]
fn check_droid_security_scan_flags_missing_contents_read() {
    let yaml = valid_droid_security_scan_yaml().replace("      contents: read\n", "");
    let violations = droid_security_scan_violations(&yaml);
    assert!(
        violations
            .iter()
            .any(|v| v.contains("must set contents: read"))
    );
}

#[test]
fn check_droid_action_refs_rejects_short_sha() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      - uses: actions/checkout@abc123\n",
    );
    assert_eq!(violations.len(), 1);
    assert!(violations[0].contains("immutable commit SHA"));
}

#[test]
fn check_droid_action_refs_handles_dash_uses_with_non_sha() {
    let mut violations = Vec::new();
    check_droid_action_refs(&mut violations, "test.yml", "- uses: some/action@v1.2.3\n");
    assert_eq!(violations.len(), 1);
}

#[test]
fn check_droid_action_refs_handles_bare_uses_with_sha() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "        uses: actions/checkout@93cb6efe18208431cddfb8368fd83d5badbf9bfd\n",
    );
    assert!(violations.is_empty());
}

#[test]
fn check_droid_action_refs_ignores_line_without_at_sign() {
    let mut violations = Vec::new();
    check_droid_action_refs(&mut violations, "test.yml", "      - uses: local-action\n");
    assert!(violations.is_empty());
}

#[test]
fn check_droid_action_refs_strips_inline_comment_before_checking() {
    let mut violations = Vec::new();
    check_droid_action_refs(
        &mut violations,
        "test.yml",
        "      - uses: actions/checkout@v5 # old ref\n",
    );
    assert_eq!(violations.len(), 1);
}

#[test]
fn check_badge_diff_policy_rejects_endpoint_diff_from_git_status() -> Result<(), String> {
    with_temp_cwd("badge-diff-policy-rejects", |root| {
        run("git", &["init"])?;
        std::fs::create_dir_all(root.join("badges")).map_err(|err| err.to_string())?;
        std::fs::write(root.join("badges/ripr.json"), "{\"message\":\"1\"}\n")
            .map_err(|err| err.to_string())?;
        run("git", &["add", "-N", "badges/ripr.json"])?;

        let message = check_badge_diff_policy_with_context(false)
            .expect_err("badge endpoint diff should fail");
        assert!(message.contains("generated badge endpoint changed"));
        assert!(message.contains("badges/ripr.json"));
        Ok(())
    })
}

#[test]
fn check_badge_diff_policy_rejects_public_surface_seam_native_from_git_status() -> Result<(), String>
{
    with_temp_cwd("badge-diff-policy-seam-native-public", |root| {
        run("git", &["init"])?;
        std::fs::write(root.join("README.md"), "ripr badge basis: seam_native\n")
            .map_err(|err| err.to_string())?;

        let message = check_badge_diff_policy_with_context(false)
            .expect_err("public seam_native repair badge copy should fail");
        assert!(message.contains("public badge surface uses `seam_native`"));
        assert!(message.contains("canonical_actionable_gap"));
        Ok(())
    })
}

#[test]
fn check_badge_diff_policy_allows_readme_only_git_status() -> Result<(), String> {
    with_temp_cwd("badge-diff-policy-readme", |root| {
        run("git", &["init"])?;
        std::fs::write(root.join("README.md"), "[![ripr](badge)](link)\n")
            .map_err(|err| err.to_string())?;

        check_badge_diff_policy_with_context(false)
    })
}

#[test]
fn check_ci_lane_whitelist_accepts_current_policy_ledgers() -> Result<(), String> {
    with_repo_cwd(super::super::check_ci_lane_whitelist_impl)
}

#[test]
fn check_ci_lane_whitelist_rejects_unknown_risk_pack_lane() -> Result<(), String> {
    let risk_packs =
        ci_risk_packs_fixture().replace(r#"required = ["docs"]"#, r#"required = ["missing-lane"]"#);
    let violations = ci_fixture_violations(
        ci_budget_fixture(),
        ci_lanes_fixture(),
        &risk_packs,
        ci_exceptions_fixture(),
    )?;
    if !violations
        .iter()
        .any(|violation| violation.contains("references unknown lane `missing-lane`"))
    {
        return Err(format!(
            "expected unknown lane violation, got {violations:?}"
        ));
    }
    Ok(())
}

#[test]
fn check_ci_lane_whitelist_rejects_unknown_artifact_family() -> Result<(), String> {
    let lanes = ci_lanes_fixture().replace(
        r#"artifact_families = ["ci-plan"]"#,
        r#"artifact_families = ["missing-family"]"#,
    );
    let violations = ci_fixture_violations(
        ci_budget_fixture(),
        &lanes,
        ci_risk_packs_fixture(),
        ci_exceptions_fixture(),
    )?;
    if !violations
        .iter()
        .any(|violation| violation.contains("unknown artifact family `missing-family`"))
    {
        return Err(format!(
            "expected unknown artifact family violation, got {violations:?}"
        ));
    }
    Ok(())
}

#[test]
fn check_ci_lane_whitelist_detects_budget_label_drift() -> Result<(), String> {
    let budget = ci_budget_fixture().replace(
        r#"budget_effect = "release""#,
        r#"budget_effect = "missing-band""#,
    );
    let violations = ci_fixture_violations(
        &budget,
        ci_lanes_fixture(),
        ci_risk_packs_fixture(),
        ci_exceptions_fixture(),
    )?;
    if !violations
        .iter()
        .any(|violation| violation.contains("unknown budget band `missing-band`"))
    {
        return Err(format!(
            "expected budget band drift violation, got {violations:?}"
        ));
    }
    Ok(())
}

#[test]
fn check_proof_packs_accepts_current_manifest() -> Result<(), String> {
    with_repo_cwd(super::super::check_proof_packs_impl)
}

#[test]
fn check_proof_packs_accepts_well_formed_fixture() -> Result<(), String> {
    let violations = proof_pack_fixture_violations(proof_packs_fixture())?;
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "expected no violations for well-formed fixture, got {violations:?}"
        ))
    }
}

#[test]
fn check_proof_packs_rejects_duplicate_pack_id() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"id = "docs-spec""#, r#"id = "release-package""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(&violations, "duplicate proof pack id `release-package`")
}

#[test]
fn check_proof_packs_rejects_empty_paths() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(r#"paths = ["docs/specs/**"]"#, r#"paths = []"#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `docs-spec` must cover at least one path",
    )
}

#[test]
fn check_proof_packs_rejects_empty_required_commands() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"required_commands = ["cargo xtask check-spec-format"]"#,
        r#"required_commands = []"#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `docs-spec` must name at least one required command",
    )
}

#[test]
fn check_proof_packs_rejects_unknown_command() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"required_commands = ["cargo xtask check-spec-format"]"#,
        r#"required_commands = ["cargo xtask not-a-real-command"]"#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "unknown repo command `cargo xtask not-a-real-command`",
    )
}

#[test]
fn check_proof_packs_rejects_unknown_non_xtask_command() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"advisory_commands = ["cargo xtask markdown-links"]"#,
        r#"advisory_commands = ["make proof"]"#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(&violations, "unknown repo command `make proof`")
}

#[test]
fn check_proof_packs_rejects_unknown_ci_lane() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"ci_lane = "docs""#, r#"ci_lane = "missing-lane""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(&violations, "references unknown ci_lane `missing-lane`")
}

#[test]
fn check_proof_packs_rejects_release_pack_without_never_routed() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace("never_routed = true", "never_routed = false");
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `release-package` must set `never_routed = true`",
    )
}

#[test]
fn check_proof_packs_rejects_release_pack_missing_release_command() -> Result<(), String> {
    // Dropping a release gate from the release-package pack must fail: the
    // pack has to name the full release proof (release-proof protection
    // slice, docs/PROOF_ROUTING.md).
    let manifest = proof_packs_fixture().replace(r#""cargo test --workspace", "#, "");
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `release-package` must require `cargo test --workspace`",
    )
}

#[test]
fn check_proof_packs_rejects_release_pack_missing_release_surface() -> Result<(), String> {
    // Dropping a release surface (here the changelog) from the
    // release-package pack must fail: every release surface has to trip
    // release_proof_required.
    let manifest = proof_packs_fixture().replace(r#", "CHANGELOG.md""#, "");
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "proof pack `release-package` must cover release surface `CHANGELOG.md`",
    )
}

#[test]
fn check_proof_packs_rejects_missing_release_pack() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"id = "release-package""#, r#"id = "other-pack""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "must define a `release-package` pack with `never_routed = true`",
    )
}

#[test]
fn check_proof_packs_rejects_unexpected_unknown_surface_policy() -> Result<(), String> {
    let manifest = proof_packs_fixture().replace(
        r#"unknown_surface_policy = "full-proof""#,
        r#"unknown_surface_policy = "cheapest-lane""#,
    );
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "`unknown_surface_policy` should be `full-proof`, got `cheapest-lane`",
    )
}

#[test]
fn check_proof_packs_rejects_non_manifest_only_state_drift() -> Result<(), String> {
    let manifest =
        proof_packs_fixture().replace(r#"state = "manifest-only""#, r#"state = "routing""#);
    let violations = proof_pack_fixture_violations(&manifest)?;
    require_proof_pack_violation(
        &violations,
        "`state` should be `manifest-only`, got `routing`",
    )
}

#[test]
fn check_lint_policy_detects_active_entry_missing_in_cargo_toml() {
    let cargo = r#"
[workspace.lints.clippy]
panic = "deny"
"#;
    let ledger = r#"
[[active.panic_family]]
name = "clippy::panic"
level = "deny"

[[active.silent_failure]]
name = "clippy::map_err_ignore"
level = "deny"
"#;
    let cargo_clippy = super::super::parse_workspace_lints_section(cargo, "clippy");
    let (entries, parse_violations) = super::super::parse_clippy_lints_ledger(ledger);
    assert!(parse_violations.is_empty());
    let mut violations = Vec::new();
    for entry in &entries {
        let (bare, _) = super::super::ledger_name_to_lookup(&entry.name);
        if !cargo_clippy.contains_key(bare) {
            violations.push(entry.name.clone());
        }
    }
    assert_eq!(violations, vec!["clippy::map_err_ignore".to_string()]);
}

#[test]
fn check_lint_policy_detects_planned_entry_already_active_in_cargo_toml() {
    let cargo = r#"
[workspace.lints.clippy]
indexing_slicing = "deny"
"#;
    let ledger = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
"#;
    let cargo_clippy = super::super::parse_workspace_lints_section(cargo, "clippy");
    let (entries, _violations) = super::super::parse_clippy_lints_ledger(ledger);
    let mut leaks = Vec::new();
    for entry in entries.iter().filter(|entry| entry.is_planned) {
        let (bare, _) = super::super::ledger_name_to_lookup(&entry.name);
        if cargo_clippy.contains_key(bare) {
            leaks.push(entry.name.clone());
        }
    }
    assert_eq!(leaks, vec!["clippy::indexing_slicing".to_string()]);
}

#[test]
fn check_lint_policy_requires_blocked_by_when_activate_when_msrv_already_met() {
    let cargo = r#"
[workspace.package]
rust-version = "1.95"
"#;
    let overdue = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
reason = "per-call expect receipts, not MSRV"
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, overdue);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy::indexing_slicing")
                && row.contains("already met by workspace rust-version `1.95`")
                && row.contains("`blocked_by`")
        }),
        "overdue activate_when_msrv without blocked_by must fail even when reason names a blocker: {violations:?}"
    );

    let recorded = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
blocked_by = "per-call expect receipts, not MSRV"
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, recorded);
    assert!(
        violations.is_empty(),
        "non-MSRV blocked_by must keep an overdue planned lint when reason is empty: {violations:?}"
    );

    let msrv_only = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
reason = "per-call expect receipts, not MSRV"
blocked_by = "waiting for Rust 1.97"
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, msrv_only);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy::indexing_slicing")
                && row.contains("`blocked_by`")
                && row.contains("is MSRV-only")
                && row.contains("waiting for Rust 1.97")
        }),
        "overdue activate_when_msrv with an MSRV-only blocked_by must fail even when reason names a blocker: {violations:?}"
    );

    let escaped_newline = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
blocked_by = "waiting for Rust 1.97\n"
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, escaped_newline);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy::indexing_slicing")
                && row.contains("is MSRV-only")
                && row.contains("waiting for Rust 1.97")
        }),
        "an escaped newline must not count as a non-MSRV token: {violations:?}"
    );

    let escaped_unicode_newline = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
blocked_by = "waiting for Rust 1.97\u000a"
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, escaped_unicode_newline);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy::indexing_slicing") && row.contains("is MSRV-only")
        }),
        "a unicode newline escape must not count as a non-MSRV token: {violations:?}"
    );

    let literal_newline = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
blocked_by = 'waiting for Rust 1.97\n'
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, literal_newline);
    assert!(
        violations.is_empty(),
        "a single-quoted backslash-n is literal TOML and stays a non-MSRV token: {violations:?}"
    );

    let clippy_unrecognized = r#"
[[planned]]
name = "clippy::manual_pop_if"
level = "warn"
activate_when_msrv = "1.95"
blocked_by = "Rust 1.95.0 Clippy does not recognize this lint; promote only after the pinned toolchain supports it."
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, clippy_unrecognized);
    assert!(
        violations.is_empty(),
        "Clippy-recognition blocked_by must count as non-MSRV: {violations:?}"
    );

    let future = r#"
[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.97"
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, future);
    assert!(
        violations.is_empty(),
        "future activate_when_msrv does not require a blocked_by yet: {violations:?}"
    );

    let single_quoted = r#"
[[planned]]
name = 'clippy::indexing_slicing'
level = 'deny'
activate_when_msrv = '1.93'
blocked_by = 'per-call expect receipts, not MSRV'
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, single_quoted);
    assert!(
        violations.is_empty(),
        "single-quoted planned fields must still bind the MSRV comparison: {violations:?}"
    );

    let single_quoted_overdue = r#"
[[planned]]
name = 'clippy::indexing_slicing'
level = 'deny'
activate_when_msrv = '1.93'
reason = 'per-call expect receipts, not MSRV'
"#;
    let violations = super::super::collect_lint_policy_violations(cargo, single_quoted_overdue);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy::indexing_slicing")
                && row.contains("already met by workspace rust-version `1.95`")
                && row.contains("`blocked_by`")
        }),
        "single-quoted overdue activate_when_msrv without blocked_by must fail: {violations:?}"
    );
}

#[test]
fn check_lint_policy_detects_level_drift() {
    let cargo = r#"
[workspace.lints.clippy]
panic = "warn"
"#;
    let ledger = r#"
[[active.panic_family]]
name = "clippy::panic"
level = "deny"
"#;
    let cargo_clippy = super::super::parse_workspace_lints_section(cargo, "clippy");
    let (entries, _violations) = super::super::parse_clippy_lints_ledger(ledger);
    let mut drift = Vec::new();
    for entry in entries.iter().filter(|entry| !entry.is_planned) {
        let (bare, _) = super::super::ledger_name_to_lookup(&entry.name);
        let level = cargo_clippy.get(bare).cloned().unwrap_or_default();
        if level != entry.level {
            drift.push((entry.name.clone(), entry.level.clone(), level));
        }
    }
    assert_eq!(
        drift,
        vec![(
            "clippy::panic".to_string(),
            "deny".to_string(),
            "warn".to_string()
        )]
    );
}

#[test]
fn check_lint_policy_parses_clippy_debt_and_rejects_stale_or_colliding_rows() {
    const TODAY: &str = "2026-09-21";
    let cargo = r#"
[workspace.lints.clippy]
unwrap_used = "deny"
"#;
    let lints = r#"
[[active.panic_family]]
name = "clippy::unwrap_used"
level = "deny"

[[planned]]
name = "clippy::indexing_slicing"
level = "deny"
activate_when_msrv = "1.93"
"#;
    let valid = r#"
[[debt]]
id = "clippy-debt-0001"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "pervasive let _ = cleanup"
blocked_by = "per-call review"
target = "2027-03-31"
"#;
    assert!(
        super::super::collect_clippy_debt_violations(valid, cargo, lints, TODAY).is_empty(),
        "live-shaped debt row must pass"
    );

    let missing = r#"
[[debt]]
id = "clippy-debt-0002"
lint = "clippy::let_underscore_must_use"
level = "deny"
"#;
    let violations = super::super::collect_clippy_debt_violations(missing, cargo, lints, TODAY);
    assert!(
        violations.iter().any(|row| row.contains("clippy-debt-0002")
            && row.contains("missing required field")
            && row.contains("owner")
            && row.contains("target")),
        "missing required fields must fail: {violations:?}"
    );

    let duplicate = format!("{valid}\n{valid}");
    let violations = super::super::collect_clippy_debt_violations(&duplicate, cargo, lints, TODAY);
    assert!(
        violations
            .iter()
            .any(|row| row.contains("duplicate debt id `clippy-debt-0001`")),
        "duplicate id must fail: {violations:?}"
    );

    let past = r#"
[[debt]]
id = "clippy-debt-0001"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "pervasive let _ = cleanup"
blocked_by = "per-call review"
target = "2026-09-01"
"#;
    let violations = super::super::collect_clippy_debt_violations(past, cargo, lints, TODAY);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy-debt-0001")
                && row.contains("target `2026-09-01`")
                && row.contains("before today `2026-09-21`")
        }),
        "past target must fail: {violations:?}"
    );

    let already_active = r#"
[[debt]]
id = "clippy-debt-0003"
lint = "clippy::unwrap_used"
level = "deny"
owner = "core/rust"
reason = "should not be debt"
blocked_by = "already active"
target = "2027-03-31"
"#;
    let violations =
        super::super::collect_clippy_debt_violations(already_active, cargo, lints, TODAY);
    assert!(
        violations
            .iter()
            .any(|row| row.contains("clippy::unwrap_used")
                && (row.contains("already `[[active]]`")
                    || row.contains("[workspace.lints.clippy]"))),
        "debt that is already active/in Cargo.toml must fail: {violations:?}"
    );

    let planned_dup = r#"
[[debt]]
id = "clippy-debt-0004"
lint = "clippy::indexing_slicing"
level = "deny"
owner = "core/rust"
reason = "planned, not debt"
blocked_by = "receipts"
target = "2027-03-31"
"#;
    let violations = super::super::collect_clippy_debt_violations(planned_dup, cargo, lints, TODAY);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy::indexing_slicing") && row.contains("already `[[planned]]`")
        }),
        "planned lint duplicated into debt must fail: {violations:?}"
    );

    let misspelled_table = r#"
[[debts]]
id = "clippy-debt-0001"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "pervasive let _ = cleanup"
blocked_by = "per-call review"
target = "2027-03-31"
"#;
    let violations =
        super::super::collect_clippy_debt_violations(misspelled_table, cargo, lints, TODAY);
    assert!(
        violations
            .iter()
            .any(|row| row.contains("debts") && row.contains("unknown field")),
        "misspelled [[debts]] must fail closed: {violations:?}"
    );

    let malformed_header = r#"
[[debt
id = "clippy-debt-0007"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "malformed header"
blocked_by = "schema"
target = "2027-03-31"
"#;
    let violations =
        super::super::collect_clippy_debt_violations(malformed_header, cargo, lints, TODAY);
    assert!(
        violations
            .iter()
            .any(|row| row.contains("policy/clippy-debt.toml:")),
        "unclosed [[debt header must fail closed: {violations:?}"
    );

    let unprefixed = r#"
[[debt]]
id = "clippy-debt-0005"
lint = "let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "missing clippy prefix"
blocked_by = "schema"
target = "2027-03-31"
"#;
    let violations = super::super::collect_clippy_debt_violations(unprefixed, cargo, lints, TODAY);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy-debt-0005")
                && row.contains("let_underscore_must_use")
                && row.contains("expected a `clippy::` lint name")
        }),
        "unprefixed debt lint must fail: {violations:?}"
    );

    let impossible_date = r#"
[[debt]]
id = "clippy-debt-0006"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "pervasive let _ = cleanup"
blocked_by = "per-call review"
target = "2027-02-31"
"#;
    let violations =
        super::super::collect_clippy_debt_violations(impossible_date, cargo, lints, TODAY);
    assert!(
        violations.iter().any(|row| {
            row.contains("clippy-debt-0006") && row.contains("malformed target `2027-02-31`")
        }),
        "impossible calendar date must fail: {violations:?}"
    );

    let whitespace_only = r#"
[[debt]]
id = "   "
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = " "
reason = "	"
blocked_by = "per-call review"
target = "2027-03-31"
"#;
    let violations =
        super::super::collect_clippy_debt_violations(whitespace_only, cargo, lints, TODAY);
    assert!(
        violations.iter().any(|row| {
            row.contains("missing required field")
                && row.contains("id")
                && row.contains("owner")
                && row.contains("reason")
        }),
        "whitespace-only required fields must fail: {violations:?}"
    );

    let duplicate_target = r#"
[[debt]]
id = "clippy-debt-0008"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "cleanup"
blocked_by = "review"
target = "2026-09-01"
target = "2027-03-31"
"#;
    let violations =
        super::super::collect_clippy_debt_violations(duplicate_target, cargo, lints, TODAY);
    assert!(
        violations
            .iter()
            .any(|row| row.contains("duplicate") && row.contains("target")),
        "duplicate target key must fail closed: {violations:?}"
    );

    let trailing_garbage = r#"
[[debt]]
id = "clippy-debt-0009"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "cleanup"
blocked_by = "review"
target = "2027-03-31" trailing garbage
"#;
    let violations =
        super::super::collect_clippy_debt_violations(trailing_garbage, cargo, lints, TODAY);
    assert!(
        violations
            .iter()
            .any(|row| row.contains("policy/clippy-debt.toml:")),
        "trailing garbage after a quoted value must fail closed: {violations:?}"
    );

    let unknown_field = r#"
[[debt]]
id = "clippy-debt-0010"
lint = "clippy::let_underscore_must_use"
level = "deny"
owner = "core/rust"
reason = "cleanup"
blocked_by = "review"
target = "2027-03-31"
owners = "typo"
"#;
    let violations =
        super::super::collect_clippy_debt_violations(unknown_field, cargo, lints, TODAY);
    assert!(
        violations
            .iter()
            .any(|row| row.contains("owners") && row.contains("unknown field")),
        "unknown field must fail closed: {violations:?}"
    );
}

// ---------------------------------------------------------------------------
// check-pr first-failure gate sequencing and report authority (#3036)
// ---------------------------------------------------------------------------

/// A deliberately wrong `run_check_pr_gates` that evaluates every closure
/// fails the invocation-count oracles below: later gates must record zero
/// invocations once an earlier gate has failed.
#[test]
fn check_pr_first_gate_failure_stops_later_gates() {
    let invocations = [std::cell::Cell::new(0), std::cell::Cell::new(0)];
    let run_a = || {
        invocations[0].set(invocations[0].get() + 1);
        Err("first gate exploded".to_string())
    };
    let run_b = || {
        invocations[1].set(invocations[1].get() + 1);
        Ok(())
    };
    let gates = [
        super::super::CheckPrGate {
            name: "gate-a",
            reproduce: "run a",
            run: &run_a,
        },
        super::super::CheckPrGate {
            name: "gate-b",
            reproduce: "run b",
            run: &run_b,
        },
    ];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::super::check_pr_report(Some(failure)));
        Ok(())
    });

    assert_eq!(
        [invocations[0].get(), invocations[1].get()],
        [1, 0],
        "later gates must not be invoked after the first failure"
    );
    assert!(
        matches!(result, Err(ref err) if err.contains("check-pr gate `gate-a` failed") && err.contains("reproduce: run a")),
        "the returned error must name the failed gate and reproduce command: {result:?}"
    );
    assert_eq!(
        published.borrow().len(),
        1,
        "exactly one failure report is published"
    );
    let report = published.borrow()[0].clone();
    assert!(report.contains("Status: fail"));
    assert!(
        !report.contains("Status: pass"),
        "a failure report must never carry the success status"
    );
    assert!(report.contains("Gate: `gate-a`"));
    assert!(report.contains("Reproduce: `run a`"));
    assert!(report.contains("first gate exploded"));
    assert!(
        report.contains("Not run after the first failure:\n- `gate-b`\n"),
        "the report must state that later gates were not run: {report}"
    );
}

#[test]
fn check_pr_middle_gate_failure_runs_earlier_and_skips_later() {
    let invocations = [
        std::cell::Cell::new(0),
        std::cell::Cell::new(0),
        std::cell::Cell::new(0),
    ];
    let run_a = || {
        invocations[0].set(invocations[0].get() + 1);
        Ok(())
    };
    let run_b = || {
        invocations[1].set(invocations[1].get() + 1);
        Err("middle gate exploded".to_string())
    };
    let run_c = || {
        invocations[2].set(invocations[2].get() + 1);
        Ok(())
    };
    let gates = [
        super::super::CheckPrGate {
            name: "gate-a",
            reproduce: "run a",
            run: &run_a,
        },
        super::super::CheckPrGate {
            name: "gate-b",
            reproduce: "run b",
            run: &run_b,
        },
        super::super::CheckPrGate {
            name: "gate-c",
            reproduce: "run c",
            run: &run_c,
        },
    ];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::super::check_pr_report(Some(failure)));
        Ok(())
    });

    assert_eq!(
        [
            invocations[0].get(),
            invocations[1].get(),
            invocations[2].get()
        ],
        [1, 1, 0],
        "earlier gates run, the failed gate runs once, later gates never run"
    );
    assert!(result.is_err());
    let report = published.borrow()[0].clone();
    assert!(report.contains("Gate: `gate-b`"));
    assert!(
        report.contains("Not run after the first failure:\n- `gate-c`\n"),
        "only the gates after the failure are listed as not run: {report}"
    );
}

#[test]
fn check_pr_all_gates_pass_publishes_no_failure_block() {
    let run_a = || Ok(());
    let gates = [super::super::CheckPrGate {
        name: "gate-a",
        reproduce: "run a",
        run: &run_a,
    }];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::super::check_pr_report(Some(failure)));
        Ok(())
    });

    assert!(matches!(result, Ok(())));
    assert!(
        published.borrow().is_empty(),
        "no failure report may be published when every gate passes"
    );
    // The success artifact is pinned byte-for-byte so the shared composer
    // cannot drift the long-standing pass report.
    assert_eq!(
        super::super::check_pr_report(None),
        "# ripr check-pr report\n\nStatus: pass\n\nChecks:\n\n- `cargo xtask ci-fast`\n- `cargo clippy --workspace --all-targets -- -D warnings`\n- `cargo doc --workspace --no-deps`\n- `cargo xtask pr-summary`\n\nReports:\n\n- `target/ripr/reports/pr-summary.md`\n- `target/ripr/reports/check-pr.md`\n\nRelease/package gates are intentionally left to `cargo xtask ci-full` or release-specific workflows.\n"
    );
}

#[test]
fn check_pr_failure_report_handles_empty_error_text() {
    let run_a = || Err(String::new());
    let gates = [super::super::CheckPrGate {
        name: "gate-a",
        reproduce: "run a",
        run: &run_a,
    }];
    let published = std::cell::RefCell::new(Vec::new());
    let result = super::super::run_check_pr_gates(&gates, &|failure| {
        published
            .borrow_mut()
            .push(super::super::check_pr_report(Some(failure)));
        Ok(())
    });
    assert!(
        result.is_err(),
        "an empty gate error is still a failure, not a pass: {result:?}"
    );

    let report = published.borrow()[0].clone();
    assert!(
        report.contains("(gate produced no error output)"),
        "an empty gate error must be explicit, not invisible: {report}"
    );
    assert!(
        report.contains("(none — the failed gate was the last)"),
        "a failure on the last gate has an explicit empty not-run list: {report}"
    );
    assert!(
        report.contains("pr-summary.md` (not refreshed"),
        "the failure path returns before pr_summary() runs, so the report must mark it stale: {report}"
    );
}

#[test]
fn check_pr_bounded_error_lines_are_crlf_normalized_and_capped() {
    let fewer = super::super::bound_first_failure_lines("one\ntwo");
    assert_eq!(
        fewer, "one\n  two",
        "fewer than five lines are all retained"
    );

    let more = super::super::bound_first_failure_lines("l1\nl2\nl3\nl4\nl5\nl6\nl7");
    assert_eq!(
        more,
        "l1\n  l2\n  l3\n  l4\n  l5\n  (+2 more lines truncated)"
    );
    assert!(
        !more.contains("l6"),
        "more than five lines are bounded to five: {more}"
    );
    assert!(
        more.contains("truncated"),
        "dropped lines must leave an explicit marker, not silent truncation: {more}"
    );

    let crlf = super::super::bound_first_failure_lines("alpha\r\nbeta\r\n");
    assert_eq!(crlf, "alpha\n  beta");
    assert!(!crlf.contains('\r'), "CRLF input is normalized: {crlf:?}");
}

#[test]
fn check_pr_failure_report_replaces_stale_success_report() -> Result<(), String> {
    with_repo_cwd(|| {
        // Preserve any pre-existing report so the test leaves no false
        // failure artifact behind for a real check-pr reader.
        let report_path = Path::new("target")
            .join("ripr")
            .join("reports")
            .join("check-pr.md");
        let original = fs::read(&report_path).ok();
        let outcome = (|| -> Result<(), String> {
            super::super::write_report("check-pr.md", &super::super::check_pr_report(None))?;
            let run_a = || Err("stale-replacement probe".to_string());
            let gates = [super::super::CheckPrGate {
                name: "gate-a",
                reproduce: "run a",
                run: &run_a,
            }];
            let result = super::super::run_check_pr_gates(&gates, &|failure| {
                super::super::write_report(
                    "check-pr.md",
                    &super::super::check_pr_report(Some(failure)),
                )
            });
            assert!(result.is_err());

            let text = fs::read_to_string("target/ripr/reports/check-pr.md")
                .map_err(|err| format!("read check-pr.md: {err}"))?;
            assert!(
                text.contains("Status: fail"),
                "the failed current run must replace the stale success report"
            );
            assert!(!text.contains("Status: pass"));
            Ok(())
        })();
        let restore = match &original {
            Some(bytes) => fs::write(&report_path, bytes).map_err(|err| {
                format!("failed to restore {}: {err}", normalize_path(&report_path))
            }),
            None => match fs::remove_file(&report_path) {
                Ok(()) => Ok(()),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(err) => Err(format!(
                    "failed to clear {}: {err}",
                    normalize_path(&report_path)
                )),
            },
        };
        restore?;
        outcome
    })
}

#[test]
fn check_pr_report_publication_failure_is_distinguishable() {
    let run_a = || Err("gate exploded".to_string());
    let gates = [super::super::CheckPrGate {
        name: "gate-a",
        reproduce: "run a",
        run: &run_a,
    }];
    let result = super::super::run_check_pr_gates(&gates, &|_failure| Err("disk full".to_string()));

    assert!(
        matches!(result, Err(ref err) if err.contains("gate-a")
            && err.contains("reproduce: run a")
            && err.contains("gate exploded")
            && err.contains("disk full")
            && err.contains("publishing the failure report also failed")),
        "a failed gate plus failed report publication must stay distinguishable and preserve the gate diagnostic and reproduce command: {result:?}"
    );
}
